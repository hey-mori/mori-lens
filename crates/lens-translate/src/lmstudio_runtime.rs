//! LM Studio runtime: discovery, readiness, and status.
//!
//! Implements the [`ProviderRuntime`] trait for LM Studio, providing a
//! provider-neutral interface over LM Studio's native v1 API.
//!
//! LM Studio has two API surfaces:
//! - Native v1: `GET /api/v1/models` (management plane, this module)
//! - OpenAI-compatible: `POST /v1/chat/completions` (inference plane)
//!
//! The native v1 model list uses top-level key `"models"` (not `"data"`).
//! A 2xx response missing `"models"` is `TranslateError::Malformed`.
//! Model entries include:
//! - `key`: canonical model identifier (used as `ModelInfo.id`)
//! - `display_name`: human-readable label (used as `ModelInfo.display_name`)
//! - `type`: model type ("llm" for translation-capable models)
//! - `size_bytes`: model size in bytes (used as `ModelInfo.size_bytes`)
//! - `loaded_instances`: array of currently loaded instances (empty if not loaded)
//!
//! A model is considered "loaded" if its `loaded_instances` array is non-empty.
//! Only models with `type == "llm"` are included in translation-capable lists.
//!
//! # Authentication
//!
//! LM Studio supports optional API token authentication via the
//! `Authorization: Bearer <token>` header. When configured, the token is sent
//! with every request. When not configured, requests are sent without
//! authentication (suitable for loopback-only usage).
//!
//! # Model Loading
//!
//! LM Studio supports Just-In-Time (JIT) model loading, where downloaded models
//! are loaded on demand when inference is requested. Whether explicit loading
//! via `load_model()` is required depends on the server's JIT configuration:
//!
//! - **JIT enabled** (default): Downloaded models are loaded automatically when
//!   inference is requested. Explicit `load_model()` is optional but can be used
//!   to pre-load models for faster first inference.
//! - **JIT disabled**: Models must be explicitly loaded via `load_model()` before
//!   inference. Unloaded models will not be available for inference.
//!
//! The `load_model()` method returns an `instance_id` that must be used for
//! unloading. Only instances created by Mori should be unloaded (ownership
//! tracking is enforced by the operation manager, not this low-level runtime).

use lens_core::model_management::{DownloadProgress, DownloadStatus, ManagementError};
use lens_core::translate::{
    ModelInfo, ModelState, Provider, ProviderRuntime, Reachability, TranslateError,
};
use serde::Deserialize;

/// LM Studio native v1 models response.
///
/// Schema: `{"models": [...]}`
/// Per https://lmstudio.ai/docs/developer/rest/list
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    models: Vec<NativeModel>,
}

/// LM Studio OpenAI-compatible inference plane response.
///
/// Schema: `{"object": "list", "data": [...]}`
/// This is the /v1/models endpoint used for inference visibility.
#[derive(Debug, Deserialize)]
struct InferenceModelsResponse {
    data: Vec<InferenceModel>,
}

/// A model entry from LM Studio's inference plane /v1/models.
#[derive(Debug, Deserialize)]
struct InferenceModel {
    id: String,
}

/// A model entry from LM Studio's native v1 API.
///
/// Required fields for `type == "llm"`:
/// - `key`: non-empty canonical identifier
/// - `display_name`: non-empty human-readable label
/// - `size_bytes`: numeric model size
/// - `loaded_instances`: array (empty if not loaded)
///
/// Unknown optional fields are tolerated (serde ignores them by default).
#[derive(Debug, Deserialize)]
struct NativeModel {
    #[serde(rename = "type")]
    model_type: Option<String>,
    key: Option<serde_json::Value>,
    display_name: Option<serde_json::Value>,
    size_bytes: Option<serde_json::Value>,
    loaded_instances: Option<serde_json::Value>,
}

/// LM Studio runtime for provider management operations.
///
/// Wraps HTTP calls to LM Studio's native v1 `/api/v1/models` endpoint,
/// providing a provider-neutral interface over LM Studio's management API.
#[derive(Debug, Clone)]
pub struct LmStudioRuntime {
    endpoint: String,
    api_token: Option<String>,
}

impl Default for LmStudioRuntime {
    fn default() -> Self {
        // LM Studio default endpoint
        Self {
            endpoint: "127.0.0.1:1234".to_owned(),
            api_token: None,
        }
    }
}

impl LmStudioRuntime {
    /// Create a runtime pointed at a specific LM Studio endpoint.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_token: None,
        }
    }

    /// Create a runtime with API token authentication.
    pub fn with_token(endpoint: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_token: Some(token.into()),
        }
    }

    /// The endpoint this runtime is pointed at.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Whether this runtime has authentication configured.
    pub fn has_auth(&self) -> bool {
        self.api_token.is_some()
    }

    /// The API token, if configured.
    pub fn api_token(&self) -> Option<&str> {
        self.api_token.as_deref()
    }

    /// Check if a model is inference-visible via the OpenAI-compatible plane.
    ///
    /// This queries /v1/models to determine if the model is exposed for inference.
    /// For LM Studio with JIT enabled, downloaded-but-not-loaded models appear here.
    ///
    /// Returns:
    /// - Ok(true): model is inference-visible (JIT can load on demand)
    /// - Ok(false): model is not inference-visible (needs explicit load)
    /// - Err: inference plane query failed (fail closed, do not interpret as false)
    pub fn is_inference_visible(&self, model: &str) -> Result<bool, TranslateError> {
        let inference_models = list_inference_models(&self.endpoint, self.api_token.as_deref())?;
        Ok(inference_models.iter().any(|m| m == model))
    }

    /// Start downloading a model from LM Studio.
    ///
    /// Sends POST /api/v1/models/download with the model identifier.
    /// Returns a provider-specific result indicating whether the download
    /// started (with job_id), completed immediately, or was already present.
    ///
    /// The job_id (if present) can be used with `download_status()` to poll progress.
    pub fn start_model_download(
        &self,
        model: &str,
    ) -> Result<DownloadStartResult, ManagementError> {
        let body = crate::json::object(&[("model", crate::json::Value::Str(model))]);

        let (status, response_body) = if let Some(token) = &self.api_token {
            match crate::http::post_with_auth(
                &self.endpoint,
                "/api/v1/models/download",
                &body,
                std::time::Duration::from_secs(30),
                token,
            ) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        } else {
            match crate::http::post(
                &self.endpoint,
                "/api/v1/models/download",
                &body,
                std::time::Duration::from_secs(30),
            ) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        };

        // Map HTTP errors to ManagementError.
        if status == 401 || status == 403 {
            return Err(ManagementError::Unauthorized);
        }
        if !(200..=299).contains(&status) {
            return Err(ManagementError::DownloadFailed);
        }

        // Parse the response JSON.
        let response: DownloadStartResponse =
            serde_json::from_str(&response_body).map_err(|_| ManagementError::InvalidResponse)?;

        // Map provider status to our result type.
        match response.status.as_str() {
            "already_downloaded" => Ok(DownloadStartResult::AlreadyPresent),
            "completed" => Ok(DownloadStartResult::TerminalCompleted),
            "downloading" | "paused" => {
                // job_id is required for ongoing downloads.
                let job_id = response.job_id.ok_or(ManagementError::InvalidResponse)?;
                let initial_status = match response.status.as_str() {
                    "downloading" => DownloadStatus::Starting,
                    "paused" => DownloadStatus::Paused(DownloadProgress::unknown_total(0)),
                    _ => unreachable!(),
                };
                Ok(DownloadStartResult::Job {
                    job_id,
                    initial_status,
                })
            }
            "failed" => Err(ManagementError::DownloadFailed),
            _ => Err(ManagementError::InvalidResponse),
        }
    }

    /// Check the status of a download job.
    ///
    /// Sends GET /api/v1/models/download/status/:job_id.
    /// Returns normalized DownloadStatus with progress if available.
    pub fn download_status(&self, job_id: &str) -> Result<DownloadStatus, ManagementError> {
        // Validate job_id to prevent path injection.
        // LM Studio job IDs are opaque strings; permit only safe characters.
        if !job_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ManagementError::InvalidResponse);
        }

        let path = format!("/api/v1/models/download/status/{}", job_id);

        let (status, response_body) = if let Some(token) = &self.api_token {
            match crate::http::get_with_auth(
                &self.endpoint,
                &path,
                std::time::Duration::from_secs(10),
                token,
            ) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        } else {
            match crate::http::get(&self.endpoint, &path, std::time::Duration::from_secs(10)) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        };

        // Map HTTP errors.
        if status == 401 || status == 403 {
            return Err(ManagementError::Unauthorized);
        }
        if status == 404 {
            return Err(ManagementError::OperationNotFound);
        }
        if !(200..=299).contains(&status) {
            return Err(ManagementError::DownloadFailed);
        }

        // Parse the response.
        let response: DownloadStatusResponse =
            serde_json::from_str(&response_body).map_err(|_| ManagementError::InvalidResponse)?;

        // Map provider status to normalized DownloadStatus.
        match response.status.as_str() {
            "downloading" => {
                // Build progress if byte fields are present.
                let progress = match (response.downloaded_bytes, response.total_size_bytes) {
                    (Some(downloaded), Some(total)) => {
                        let mut p = DownloadProgress::new(downloaded, total);
                        if let Some(rate) = response.bytes_per_second {
                            p = p.with_rate(rate);
                        }
                        Some(p)
                    }
                    (Some(downloaded), None) => Some(DownloadProgress::unknown_total(downloaded)),
                    _ => None,
                };

                Ok(progress
                    .map(DownloadStatus::Downloading)
                    .unwrap_or(DownloadStatus::Starting))
            }
            "paused" => {
                let progress = match (response.downloaded_bytes, response.total_size_bytes) {
                    (Some(downloaded), Some(total)) => DownloadProgress::new(downloaded, total),
                    (Some(downloaded), None) => DownloadProgress::unknown_total(downloaded),
                    _ => DownloadProgress::unknown_total(0),
                };
                Ok(DownloadStatus::Paused(progress))
            }
            "completed" => Ok(DownloadStatus::Completed),
            "failed" => Err(ManagementError::DownloadFailed),
            _ => Err(ManagementError::InvalidResponse),
        }
    }

    /// Load a model into memory for inference.
    ///
    /// Sends POST /api/v1/models/load with the model identifier.
    /// Returns the instance_id assigned by LM Studio, which must be used
    /// for unloading. Mori Lens becomes the owner of this instance.
    ///
    /// LM Studio load response includes:
    /// - instance_id: the assigned instance identifier
    /// - status: must be "loaded" for success
    /// - type: model type (e.g., "llm")
    /// - load_time_seconds: time taken to load
    pub fn load_model(&self, model: &str) -> Result<String, ManagementError> {
        let body = crate::json::object(&[("model", crate::json::Value::Str(model))]);

        let (status, response_body) = if let Some(token) = &self.api_token {
            match crate::http::post_with_auth(
                &self.endpoint,
                "/api/v1/models/load",
                &body,
                std::time::Duration::from_secs(60),
                token,
            ) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        } else {
            match crate::http::post(
                &self.endpoint,
                "/api/v1/models/load",
                &body,
                std::time::Duration::from_secs(60),
            ) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        };

        // Map HTTP errors to ManagementError.
        if status == 401 || status == 403 {
            return Err(ManagementError::Unauthorized);
        }
        if status == 404 {
            return Err(ManagementError::ModelNotFound);
        }
        if !(200..=299).contains(&status) {
            return Err(ManagementError::LoadFailed);
        }

        // Parse the response to extract instance_id and validate status.
        let response: LoadResponse =
            serde_json::from_str(&response_body).map_err(|_| ManagementError::InvalidResponse)?;

        // Validate status is "loaded".
        if response.status.as_deref() != Some("loaded") {
            return Err(ManagementError::InvalidResponse);
        }

        // instance_id is required for unloading.
        response
            .instance_id
            .filter(|id| !id.is_empty())
            .ok_or(ManagementError::InvalidResponse)
    }

    /// Unload a model instance from memory.
    ///
    /// Sends POST /api/v1/models/unload with the instance_id.
    /// Only instances created by Mori Lens should be unloaded.
    /// Never unload instances that existed before Mori started.
    ///
    /// Note: instance_id is sent in JSON body, not URL path, so
    /// path-injection validation is not required. We still validate
    /// that it's non-empty for safety.
    pub fn unload_instance(&self, instance_id: &str) -> Result<(), ManagementError> {
        // Validate that instance_id is non-empty.
        // Character validation is not needed since it's in JSON body, not URL path.
        if instance_id.is_empty() {
            return Err(ManagementError::InvalidResponse);
        }

        let body = crate::json::object(&[("instance_id", crate::json::Value::Str(instance_id))]);

        let (status, _response_body) = if let Some(token) = &self.api_token {
            match crate::http::post_with_auth(
                &self.endpoint,
                "/api/v1/models/unload",
                &body,
                std::time::Duration::from_secs(30),
                token,
            ) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        } else {
            match crate::http::post(
                &self.endpoint,
                "/api/v1/models/unload",
                &body,
                std::time::Duration::from_secs(30),
            ) {
                Ok(result) => result,
                Err(e) => return Err(map_translate_error(e)),
            }
        };

        // Map HTTP errors.
        if status == 401 || status == 403 {
            return Err(ManagementError::Unauthorized);
        }
        if status == 404 {
            return Err(ManagementError::OperationNotFound);
        }
        if !(200..=299).contains(&status) {
            return Err(ManagementError::UnloadFailed);
        }

        Ok(())
    }
}

/// Map a TranslateError to a ManagementError.
///
/// This is a private helper that converts HTTP-level errors to domain errors
/// appropriate for model management operations.
fn map_translate_error(e: TranslateError) -> ManagementError {
    match e {
        TranslateError::Unavailable => ManagementError::EndpointUnreachable,
        TranslateError::Timeout => ManagementError::DownloadFailed,
        TranslateError::HttpFailed(401) | TranslateError::HttpFailed(403) => {
            ManagementError::Unauthorized
        }
        TranslateError::HttpFailed(404) => ManagementError::ModelNotFound,
        TranslateError::HttpFailed(_) => ManagementError::DownloadFailed,
        TranslateError::Malformed => ManagementError::InvalidResponse,
        _ => ManagementError::DownloadFailed,
    }
}

/// Result of starting a model download.
///
/// Provider-specific result that preserves the job_id for ongoing downloads.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStartResult {
    /// Model was already present locally.
    AlreadyPresent,
    /// Download completed immediately (rare, but possible for cached models).
    TerminalCompleted,
    /// Download started; job_id can be used to poll status.
    Job {
        job_id: String,
        initial_status: DownloadStatus,
    },
}

/// LM Studio download start response.
#[derive(Debug, Deserialize)]
struct DownloadStartResponse {
    status: String,
    job_id: Option<String>,
}

/// LM Studio download status response.
#[derive(Debug, Deserialize)]
struct DownloadStatusResponse {
    status: String,
    #[serde(rename = "downloaded_bytes")]
    downloaded_bytes: Option<u64>,
    #[serde(rename = "total_size_bytes")]
    total_size_bytes: Option<u64>,
    #[serde(rename = "bytes_per_second")]
    bytes_per_second: Option<f64>,
}

/// LM Studio load response.
///
/// Official v1 API response shape includes:
/// - instance_id: the assigned instance identifier
/// - status: "loaded" on success
/// - type: model type (e.g., "llm")
/// - load_time_seconds: time taken to load
#[derive(Debug, Deserialize)]
struct LoadResponse {
    instance_id: Option<String>,
    status: Option<String>,
    #[serde(rename = "type")]
    _model_type: Option<String>,
    #[allow(dead_code)]
    load_time_seconds: Option<f64>,
}

impl ProviderRuntime for LmStudioRuntime {
    fn provider_identity(&self) -> Provider {
        Provider::LmStudio
    }

    fn reachability(&self) -> Reachability {
        // Query /api/v1/models to check if LM Studio is reachable
        match list_models_with_auth(&self.endpoint, self.api_token.as_deref()) {
            Ok(_) => Reachability::Reachable,
            Err(_) => Reachability::Unreachable,
        }
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, TranslateError> {
        list_models_with_auth(&self.endpoint, self.api_token.as_deref())
    }

    fn model_state(&self, model: &str) -> Result<ModelState, TranslateError> {
        // Query native management plane for inventory and loaded state
        let raw_models = list_models_with_loaded(&self.endpoint, self.api_token.as_deref())?;

        let raw_model = raw_models
            .iter()
            .find(|m| m.key == model)
            .ok_or_else(|| TranslateError::ModelMissing(model.to_owned()))?;

        // If explicitly loaded in memory, it's Loaded (physical truth)
        if raw_model.is_loaded {
            return Ok(ModelState::Loaded {
                name: raw_model.key.clone(),
            });
        }

        // Model is present locally but not loaded in memory.
        // Return Available (physical truth: not loaded).
        // Inference visibility (JIT) is tracked separately for readiness evaluation.
        Ok(ModelState::Available {
            name: raw_model.key.clone(),
            size: raw_model.size_bytes,
        })
    }
}

/// Internal model info including loaded state from native v1 API.
struct RawLmStudioModel {
    key: String,
    display_name: String,
    size_bytes: u64,
    is_loaded: bool,
}

/// List models with loaded state from LM Studio's native v1 /api/v1/models endpoint.
///
/// Returns internal struct with loaded_instances information for model_state.
fn list_models_with_loaded(
    endpoint: &str,
    api_token: Option<&str>,
) -> Result<Vec<RawLmStudioModel>, TranslateError> {
    let (status, body) = if let Some(token) = api_token {
        crate::http::get_with_auth(
            endpoint,
            "/api/v1/models",
            std::time::Duration::from_secs(3),
            token,
        )?
    } else {
        crate::http::get(
            endpoint,
            "/api/v1/models",
            std::time::Duration::from_secs(3),
        )?
    };

    // Check for authentication errors using HTTP status code
    if status == 401 || status == 403 {
        return Err(TranslateError::HttpFailed(status));
    }

    // Any non-2xx status is an error, not a successful empty list.
    // This prevents 400, 404, 500, etc. from being treated as "no models".
    if !(200..300).contains(&status) {
        return Err(TranslateError::HttpFailed(status));
    }

    // Also check for authentication errors in the response body
    if let Some(error_type) = crate::json::string_value_of_key(&body, "type") {
        if error_type == "authentication_error" || error_type == "permission_error" {
            return Err(TranslateError::Unavailable);
        }
    }

    // Parse the LM Studio native v1 models response
    // Schema: {"models": [{"key": "...", "type": "llm", "size_bytes": 123, "loaded_instances": [...]}]}
    //
    // A 2xx response missing the "models" key is malformed, not an empty success.
    // Parse per-object to avoid index misalignment when fields are missing.
    // Flat extraction (string_values_of_key) would assign values by position
    // across the entire response, which breaks when one model lacks a field.
    let models = parse_model_objects(&body)?;

    Ok(models)
}

/// Parse model objects from the native v1 response body using serde_json.
///
/// The LM Studio native v1 API returns a nested JSON document with:
/// - top-level object with "models" array
/// - model objects with string, number, array, and nested object fields
/// - optional fields that must be tolerated
/// - required fields (for type=="llm") that must be present and correctly typed
///
/// A hand-written substring scanner cannot correctly validate this structure:
/// - Field names can appear inside string values
/// - JSON syntax (commas, brackets) must be validated
/// - Field types must be checked (string vs number vs array)
/// - Escaped strings must be handled correctly
///
/// Using serde_json provides correct JSON parsing with minimal code.
///
/// Returns `Err(TranslateError::Malformed)` when:
/// - The JSON is syntactically invalid (missing commas, unclosed brackets, etc.)
/// - The `"models"` key is absent or not an array
/// - An explicit `type == "llm"` entry has missing or wrong-typed required fields:
///   `key` (non-empty string), `display_name` (non-empty string),
///   `size_bytes` (number), `loaded_instances` (array)
fn parse_model_objects(body: &str) -> Result<Vec<RawLmStudioModel>, TranslateError> {
    // Parse the JSON document. Any syntax error is malformed.
    let response: ModelsResponse =
        serde_json::from_str(body).map_err(|_| TranslateError::Malformed)?;

    let mut models = Vec::new();

    for model in response.models {
        // Only process models with explicit type == "llm"
        if model.model_type.as_deref() != Some("llm") {
            continue;
        }

        // Required fields for an LLM model entry.
        // A type=="llm" entry missing identity or metadata is malformed.

        // key must be a non-empty string
        let key = match model.key {
            Some(serde_json::Value::String(s)) if !s.is_empty() => s,
            _ => return Err(TranslateError::Malformed),
        };

        // display_name must be a non-empty string
        let display_name = match model.display_name {
            Some(serde_json::Value::String(s)) if !s.is_empty() => s,
            _ => return Err(TranslateError::Malformed),
        };

        // size_bytes must be a number (u64)
        let size_bytes = match model.size_bytes {
            Some(serde_json::Value::Number(n)) => n.as_u64().ok_or(TranslateError::Malformed)?,
            _ => return Err(TranslateError::Malformed),
        };

        // loaded_instances must be an array
        let loaded_instances = match model.loaded_instances {
            Some(serde_json::Value::Array(arr)) => arr,
            _ => return Err(TranslateError::Malformed),
        };

        // A model is loaded if its loaded_instances array is non-empty
        let is_loaded = !loaded_instances.is_empty();

        models.push(RawLmStudioModel {
            key,
            display_name,
            size_bytes,
            is_loaded,
        });
    }

    Ok(models)
}

/// Query LM Studio's inference plane /v1/models endpoint.
///
/// Returns the list of model IDs visible to the inference interface.
/// This is authoritative for whether a model can be used for inference,
/// which depends on JIT configuration and loaded state.
///
/// Failures (transport, auth, malformed) are returned as errors and must
/// not be interpreted as "model not visible".
fn list_inference_models(
    endpoint: &str,
    api_token: Option<&str>,
) -> Result<Vec<String>, TranslateError> {
    let (status, body) = if let Some(token) = api_token {
        crate::http::get_with_auth(
            endpoint,
            "/v1/models",
            std::time::Duration::from_secs(3),
            token,
        )?
    } else {
        crate::http::get(endpoint, "/v1/models", std::time::Duration::from_secs(3))?
    };

    // Check for authentication errors
    if status == 401 || status == 403 {
        return Err(TranslateError::HttpFailed(status));
    }

    // Any non-2xx status is an error
    if !(200..=299).contains(&status) {
        return Err(TranslateError::HttpFailed(status));
    }

    // Parse the response
    let response: InferenceModelsResponse =
        serde_json::from_str(&body).map_err(|_| TranslateError::Malformed)?;

    // Extract model IDs
    Ok(response.data.into_iter().map(|m| m.id).collect())
}

/// List models from LM Studio's /api/v1/models endpoint.
///
/// Filters out embedding models and unknown types (only `type == "llm"` is
/// included). Returns only translation-capable LLM models.
fn list_models_with_auth(
    endpoint: &str,
    api_token: Option<&str>,
) -> Result<Vec<ModelInfo>, TranslateError> {
    let raw_models = list_models_with_loaded(endpoint, api_token)?;

    // Convert to ModelInfo, preserving all three fields:
    // - id: canonical key (used for API requests)
    // - display_name: human-readable label (for UI)
    // - size_bytes: model size on disk
    let models = raw_models
        .into_iter()
        .map(|m| ModelInfo {
            id: m.key,
            display_name: m.display_name,
            size_bytes: m.size_bytes,
        })
        .collect();

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_server::{FakeResponse, FakeServer};
    use crate::lmstudio_fixtures::*;

    #[test]
    fn lmstudio_runtime_reports_lmstudio_identity() {
        let runtime = LmStudioRuntime::default();
        assert_eq!(runtime.provider_identity(), Provider::LmStudio);
    }

    #[test]
    fn lmstudio_runtime_uses_default_endpoint() {
        let runtime = LmStudioRuntime::default();
        assert_eq!(runtime.endpoint(), "127.0.0.1:1234");
    }

    #[test]
    fn lmstudio_runtime_accepts_custom_endpoint() {
        let runtime = LmStudioRuntime::new("localhost:8080");
        assert_eq!(runtime.endpoint(), "localhost:8080");
    }

    #[test]
    fn lmstudio_runtime_without_token_has_no_auth() {
        let runtime = LmStudioRuntime::default();
        assert!(!runtime.has_auth());
    }

    #[test]
    fn lmstudio_runtime_with_token_has_auth() {
        let runtime = LmStudioRuntime::with_token("127.0.0.1:1234", "test-token");
        assert!(runtime.has_auth());
    }

    // Contract tests using deterministic fake HTTP server.
    // These are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.

    #[test]
    fn reachability_returns_reachable_when_endpoint_answers() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_EMPTY_MODELS).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        assert_eq!(runtime.reachability(), Reachability::Reachable);
    }

    #[test]
    fn reachability_returns_unreachable_when_endpoint_does_not_answer() {
        let runtime = LmStudioRuntime::new("127.0.0.1:1");
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    #[test]
    fn list_models_returns_empty_vec_when_no_models() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_EMPTY_MODELS).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn list_models_filters_out_embedding_models() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MODELS_RESPONSE).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        // Should only include llm models, not embedding models
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "llama-3.1-8b");
        assert_eq!(models[0].display_name, "LLaMA 3.1 8B");
        assert_eq!(models[0].size_bytes, 4700000000);
        assert_eq!(models[1].id, "qwen2.5-7b");
        assert_eq!(models[1].display_name, "Qwen 2.5 7B");
        assert_eq!(models[1].size_bytes, 4300000000);
    }

    #[test]
    fn list_models_returns_llm_models_only() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_LLM_MODELS_ONLY).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "llama-3.1-8b");
        assert_eq!(models[0].display_name, "LLaMA 3.1 8B");
        assert_eq!(models[0].size_bytes, 4700000000);
        assert_eq!(models[1].id, "qwen2.5-7b");
        assert_eq!(models[1].display_name, "Qwen 2.5 7B");
        assert_eq!(models[1].size_bytes, 4300000000);
    }

    #[test]
    fn list_models_returns_unavailable_when_endpoint_fails() {
        let runtime = LmStudioRuntime::new("127.0.0.1:1");
        let result = runtime.list_models();
        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn model_state_returns_available_when_model_exists() {
        // Model exists in native plane but not loaded.
        // Inference plane also does not contain it (JIT disabled or not visible).
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_LLM_MODELS_ONLY).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let state = runtime.model_state("llama-3.1-8b").unwrap();

        match state {
            ModelState::Available { name, size } => {
                assert_eq!(name, "llama-3.1-8b");
                assert_eq!(size, 4700000000);
            }
            _ => panic!("Expected ModelState::Available, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_model_missing_when_model_not_found() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_LLM_MODELS_ONLY).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.model_state("nonexistent-model");

        assert!(matches!(result, Err(TranslateError::ModelMissing(_))));
    }

    #[test]
    fn model_state_returns_model_missing_for_embedding_model() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MODELS_RESPONSE).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        // nomic-embed-text exists but is an embedding model, so it should be filtered out
        let result = runtime.model_state("nomic-embed-text");

        assert!(matches!(result, Err(TranslateError::ModelMissing(_))));
    }

    #[test]
    fn model_state_returns_unavailable_when_endpoint_fails() {
        let runtime = LmStudioRuntime::new("127.0.0.1:1");
        let result = runtime.model_state("llama-3.1-8b");

        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn model_state_returns_loaded_when_model_has_loaded_instances() {
        // LM_STUDIO_MODELS_RESPONSE has qwen2.5-7b with non-empty loaded_instances
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MODELS_RESPONSE).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let state = runtime.model_state("qwen2.5-7b").unwrap();

        match state {
            ModelState::Loaded { name } => {
                assert_eq!(name, "qwen2.5-7b");
            }
            _ => panic!("Expected ModelState::Loaded, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_available_when_model_has_no_loaded_instances() {
        // LM_STUDIO_MODELS_RESPONSE has llama-3.1-8b with empty loaded_instances
        // Inference plane does not contain it (JIT disabled or not visible)
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MODELS_RESPONSE).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let state = runtime.model_state("llama-3.1-8b").unwrap();

        match state {
            ModelState::Available { name, size } => {
                assert_eq!(name, "llama-3.1-8b");
                assert_eq!(size, 4700000000);
            }
            _ => panic!("Expected ModelState::Available, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_available_when_inference_plane_contains_model() {
        // Model exists in native plane but not explicitly loaded.
        // Inference plane contains it (JIT enabled, model visible for inference).
        // Physical state is still Available (not loaded in memory).
        // Inference visibility is tracked separately via is_inference_visible().
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_LLM_MODELS_ONLY).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let state = runtime.model_state("llama-3.1-8b").unwrap();

        // Physical state: Available (not loaded in memory)
        match state {
            ModelState::Available { name, size } => {
                assert_eq!(name, "llama-3.1-8b");
                assert_eq!(size, 4700000000);
            }
            _ => panic!(
                "Expected ModelState::Available (physical truth), got {:?}",
                state
            ),
        }
    }

    #[test]
    fn is_inference_visible_returns_true_when_model_in_v1_models() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_INFERENCE_MODELS_WITH_LLAMA,
        )
        .expect_get("/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let visible = runtime.is_inference_visible("llama-3.1-8b").unwrap();
        assert!(visible);
    }

    #[test]
    fn is_inference_visible_returns_false_when_model_not_in_v1_models() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_INFERENCE_MODELS_EMPTY,
        )
        .expect_get("/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let visible = runtime.is_inference_visible("llama-3.1-8b").unwrap();
        assert!(!visible);
    }

    #[test]
    fn is_inference_visible_returns_error_when_v1_models_fails() {
        let server = FakeServer::start(vec![FakeResponse::status(500).expect_get("/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.is_inference_visible("llama-3.1-8b");
        assert!(matches!(result, Err(TranslateError::HttpFailed(500))));
    }

    #[test]
    fn list_models_returns_http_failed_on_401_unauthorized() {
        let server = FakeServer::start(vec![FakeResponse::unauthorized()
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer invalid-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "invalid-token");
        let result = runtime.list_models();

        // 401 should be preserved as HttpFailed for diagnostic classification
        assert!(matches!(result, Err(TranslateError::HttpFailed(401))));
    }

    #[test]
    fn list_models_returns_http_failed_on_403_forbidden() {
        let server = FakeServer::start(vec![FakeResponse::forbidden()
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer forbidden-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "forbidden-token");
        let result = runtime.list_models();

        // 403 should be preserved as HttpFailed for diagnostic classification
        assert!(matches!(result, Err(TranslateError::HttpFailed(403))));
    }

    #[test]
    fn reachability_returns_unreachable_on_401_unauthorized() {
        let server = FakeServer::start(vec![FakeResponse::unauthorized()
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer invalid-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "invalid-token");
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    #[test]
    fn reachability_returns_unreachable_on_403_forbidden() {
        let server = FakeServer::start(vec![FakeResponse::forbidden()
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer forbidden-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "forbidden-token");
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    // Regression: identity uses canonical key, not display_name.
    // Two models share the display_name "My Model" but have different keys.
    // model_state must resolve by key only.
    #[test]
    fn model_state_uses_canonical_key_not_display_name() {
        // model_state calls list_models_with_loaded once and list_inference_models once per call.
        // Provide enough responses for all three model_state calls below.
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_AMBIGUOUS_DISPLAY_NAMES).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_AMBIGUOUS_DISPLAY_NAMES).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_AMBIGUOUS_DISPLAY_NAMES).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
        ]);

        let runtime = LmStudioRuntime::new(server.address());

        // Looking up by key works and is unambiguous
        let state = runtime.model_state("llama-3.1-8b").unwrap();
        assert!(matches!(state, ModelState::Available { ref name, .. } if name == "llama-3.1-8b"));

        // Looking up by the other key also works
        let state = runtime.model_state("qwen2.5-7b").unwrap();
        assert!(matches!(state, ModelState::Loaded { ref name } if name == "qwen2.5-7b"));

        // Looking up by display_name alone does NOT resolve (canonical ID only)
        let result = runtime.model_state("My Model");
        assert!(matches!(result, Err(TranslateError::ModelMissing(_))));
    }

    // Fail-closed: models with missing type field are excluded.
    #[test]
    fn list_models_excludes_models_with_missing_type() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MISSING_TYPE).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        // Only the model with explicit type == "llm" is included
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "known-llm");
    }

    // Fail-closed: models with unknown type are excluded.
    #[test]
    fn list_models_excludes_models_with_unknown_type() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_UNKNOWN_TYPE).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        // Only the model with explicit type == "llm" is included
        // The "multimodal" type is not recognized and must be excluded
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "known-llm");
    }

    // HTTP 500 must not become a successful empty list.
    #[test]
    fn list_models_returns_http_failed_on_500() {
        let server = FakeServer::start(vec![
            FakeResponse::server_error().expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        // 500 should be preserved as HttpFailed for diagnostic classification
        assert!(matches!(result, Err(TranslateError::HttpFailed(500))));
    }

    // HTTP 400 must not become a successful empty list.
    #[test]
    fn list_models_returns_http_failed_on_400() {
        let server = FakeServer::start(vec![
            FakeResponse::bad_request().expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        // 400 should be preserved as HttpFailed for diagnostic classification
        assert!(matches!(result, Err(TranslateError::HttpFailed(400))));
    }

    // HTTP 404 must not become a successful empty list.
    #[test]
    fn list_models_returns_http_failed_on_404() {
        let server =
            FakeServer::start(vec![FakeResponse::not_found().expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        // 404 should be preserved as HttpFailed for diagnostic classification
        assert!(matches!(result, Err(TranslateError::HttpFailed(404))));
    }

    // Reachability: 500 must NOT become Reachable.
    #[test]
    fn reachability_returns_unreachable_on_500() {
        let server = FakeServer::start(vec![
            FakeResponse::server_error().expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    // 2xx with empty object: no "models" key is malformed, not empty success.
    #[test]
    fn list_models_returns_malformed_on_empty_object() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_EMPTY_OBJECT,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // 2xx with stale "data" envelope: the native v1 API uses "models".
    #[test]
    fn list_models_returns_malformed_on_stale_data_envelope() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_STALE_ENVELOPE,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // 2xx with truncated JSON: unterminated object and array is malformed.
    // A truncated HTTP 2xx JSON document is not a valid empty model list.
    #[test]
    fn list_models_returns_malformed_on_truncated_response() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MALFORMED_TRUNCATED).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // 2xx with complete objects but unclosed array: structurally incomplete.
    #[test]
    fn list_models_returns_malformed_on_unclosed_array() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_UNCLOSED_ARRAY,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Fail-closed: an explicit type=="llm" entry missing key is malformed.
    #[test]
    fn list_models_returns_malformed_on_llm_missing_key() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_LLM_MISSING_KEY,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Fail-closed: an explicit type=="llm" entry missing size_bytes is malformed.
    #[test]
    fn list_models_returns_malformed_on_llm_missing_size() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_LLM_MISSING_SIZE,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Fail-closed: an explicit type=="llm" entry missing loaded_instances is malformed.
    #[test]
    fn list_models_returns_malformed_on_llm_missing_loaded_instances() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_LLM_MISSING_LOADED,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Parser defect: field name appearing only inside a string value must not
    // be mistaken for an actual field key. The display_name is "loaded_instances"
    // but there is no actual loaded_instances field.
    #[test]
    fn list_models_returns_malformed_when_field_name_appears_only_in_value() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_FIELD_NAME_IN_VALUE,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Parser defect: missing comma between objects. A structurally closed but
    // syntactically invalid JSON must not parse successfully.
    #[test]
    fn list_models_returns_malformed_on_missing_comma_between_objects() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_MISSING_COMMA,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Parser defect: size_bytes as string instead of number. Wrong JSON type
    // for required field is malformed.
    #[test]
    fn list_models_returns_malformed_on_size_bytes_as_string() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_SIZE_AS_STRING,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Parser defect: loaded_instances as object instead of array. Wrong JSON
    // type for required field is malformed.
    #[test]
    fn list_models_returns_malformed_on_loaded_instances_as_object() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_LOADED_AS_OBJECT,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Parser defect: key as null instead of string. Wrong JSON type for
    // required field is malformed.
    #[test]
    fn list_models_returns_malformed_on_key_as_null() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MALFORMED_KEY_NULL).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Malformed: models is not an array (it's an object).
    #[test]
    fn list_models_returns_malformed_when_models_is_not_array() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_MODELS_NOT_ARRAY,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Fail-closed: an explicit type=="llm" entry missing display_name is malformed.
    #[test]
    fn list_models_returns_malformed_on_llm_missing_display_name() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_LLM_MISSING_DISPLAY_NAME,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Malformed)));
    }

    // Valid: model with escaped characters in display_name.
    // JSON string escaping must be handled correctly by the parser.
    #[test]
    fn list_models_handles_escaped_characters_in_display_name() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_ESCAPED_DISPLAY_NAME).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "escaped-model");
        assert_eq!(
            models[0].display_name,
            r#"Model with "quotes" and \backslash"#
        );
        assert_eq!(models[0].size_bytes, 1000000000);
    }

    // Valid: model with unknown extra fields.
    // The parser must tolerate additional fields not in the minimal schema.
    #[test]
    fn list_models_tolerates_unknown_extra_fields() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_EXTRA_FIELDS).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "extra-model");
        assert_eq!(models[0].display_name, "Extra Model");
        assert_eq!(models[0].size_bytes, 1000000000);
    }

    // Official shape with all documented fields parses correctly.
    #[test]
    fn list_models_parses_official_shape_with_all_fields() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_OFFICIAL_SHAPE).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "llama-3.1-8b");
        assert_eq!(models[0].display_name, "LLaMA 3.1 8B");
        assert_eq!(models[0].size_bytes, 4700000000);
        assert_eq!(models[1].id, "qwen2.5-7b");
        assert_eq!(models[1].display_name, "Qwen 2.5 7B");
        assert_eq!(models[1].size_bytes, 4300000000);
    }

    // Official shape: loaded_instances detection with official entry shape.
    #[test]
    fn model_state_detects_loaded_from_official_shape() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_OFFICIAL_SHAPE).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_OFFICIAL_SHAPE).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        // llama-3.1-8b has non-empty loaded_instances in the official fixture
        let state = runtime.model_state("llama-3.1-8b").unwrap();
        assert!(matches!(state, ModelState::Loaded { .. }));

        // qwen2.5-7b has empty loaded_instances
        let state = runtime.model_state("qwen2.5-7b").unwrap();
        assert!(matches!(state, ModelState::Available { .. }));
    }

    // loaded_instances with whitespace: "[ ]" is empty, not loaded.
    #[test]
    fn loaded_instances_whitespace_is_detected_as_empty() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_LOADED_WHITESPACE).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_INFERENCE_MODELS_EMPTY).expect_get("/v1/models"),
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let state = runtime.model_state("whitespace-model").unwrap();

        // "[ ]" should be detected as empty (not loaded)
        assert!(matches!(state, ModelState::Available { .. }));
    }

    // Reachability: malformed 2xx must NOT be reported as reachable.
    #[test]
    fn reachability_returns_unreachable_on_malformed_response() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            LM_STUDIO_MALFORMED_EMPTY_OBJECT,
        )
        .expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    // ========== Download operations contract tests ==========

    #[test]
    fn start_model_download_returns_already_present_when_model_exists() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"status": "already_downloaded"}"#,
        )
        .expect_post("/api/v1/models/download")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.start_model_download("llama-3.1-8b").unwrap();

        assert_eq!(result, DownloadStartResult::AlreadyPresent);
    }

    #[test]
    fn start_model_download_returns_job_with_id_when_downloading() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"status": "downloading", "job_id": "dl-abc123"}"#,
        )
        .expect_post("/api/v1/models/download")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.start_model_download("llama-3.1-8b").unwrap();

        match result {
            DownloadStartResult::Job {
                job_id,
                initial_status,
            } => {
                assert_eq!(job_id, "dl-abc123");
                assert_eq!(initial_status, DownloadStatus::Starting);
            }
            _ => panic!("expected Job, got {:?}", result),
        }
    }

    #[test]
    fn start_model_download_returns_job_when_paused() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"status": "paused", "job_id": "dl-paused-456"}"#,
        )
        .expect_post("/api/v1/models/download")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.start_model_download("llama-3.1-8b").unwrap();

        match result {
            DownloadStartResult::Job {
                job_id,
                initial_status,
            } => {
                assert_eq!(job_id, "dl-paused-456");
                assert!(matches!(initial_status, DownloadStatus::Paused(_)));
            }
            _ => panic!("expected Job, got {:?}", result),
        }
    }

    #[test]
    fn start_model_download_returns_terminal_completed_when_immediate() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"status": "completed"}"#)
            .expect_post("/api/v1/models/download")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.start_model_download("llama-3.1-8b").unwrap();

        assert_eq!(result, DownloadStartResult::TerminalCompleted);
    }

    #[test]
    fn start_model_download_returns_error_when_failed() {
        let server =
            FakeServer::start(vec![FakeResponse::ok_json(r#"{"status": "failed"}"#)
                .expect_post("/api/v1/models/download")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.start_model_download("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::DownloadFailed));
    }

    #[test]
    fn start_model_download_returns_error_when_unauthorized() {
        let server = FakeServer::start(vec![
            FakeResponse::status(401).expect_post("/api/v1/models/download")
        ]);

        let runtime = LmStudioRuntime::with_token(server.address(), "wrong-token");
        let result = runtime.start_model_download("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::Unauthorized));
    }

    #[test]
    fn start_model_download_returns_error_when_missing_job_id() {
        // Provider says "downloading" but omits job_id; must be InvalidResponse.
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"status": "downloading"}"#)
            .expect_post("/api/v1/models/download")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.start_model_download("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn start_model_download_returns_error_when_malformed_json() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json("{broken").expect_post("/api/v1/models/download")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.start_model_download("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn start_model_download_returns_error_when_unreachable() {
        let runtime = LmStudioRuntime::new("127.0.0.1:1");
        let result = runtime.start_model_download("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::EndpointUnreachable));
    }

    #[test]
    fn start_model_download_sends_auth_header_when_token_configured() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"status": "already_downloaded"}"#,
        )
        .expect_post("/api/v1/models/download")
        .expect_header("Authorization", "Bearer secret-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "secret-token");
        let result = runtime.start_model_download("llama-3.1-8b").unwrap();

        assert_eq!(result, DownloadStartResult::AlreadyPresent);
    }

    #[test]
    fn download_status_returns_downloading_with_progress() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"status": "downloading", "downloaded_bytes": 1048576, "total_size_bytes": 4700000000, "bytes_per_second": 5242880.0}"#,
        )
        .expect_get("/api/v1/models/download/status/dl-abc123")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.download_status("dl-abc123").unwrap();

        match result {
            DownloadStatus::Downloading(progress) => {
                assert_eq!(progress.bytes_downloaded, 1048576);
                assert_eq!(progress.total_bytes, Some(4700000000));
                assert!((progress.bytes_per_second.unwrap() - 5242880.0).abs() < 1.0);
            }
            _ => panic!("expected Downloading, got {:?}", result),
        }
    }

    #[test]
    fn download_status_returns_paused_with_progress() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"status": "paused", "downloaded_bytes": 2097152, "total_size_bytes": 4700000000}"#,
        )
        .expect_get("/api/v1/models/download/status/dl-paused-789")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.download_status("dl-paused-789").unwrap();

        match result {
            DownloadStatus::Paused(progress) => {
                assert_eq!(progress.bytes_downloaded, 2097152);
                assert_eq!(progress.total_bytes, Some(4700000000));
            }
            _ => panic!("expected Paused, got {:?}", result),
        }
    }

    #[test]
    fn download_status_returns_completed() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"status": "completed"}"#)
            .expect_get("/api/v1/models/download/status/dl-done-999")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.download_status("dl-done-999").unwrap();

        assert_eq!(result, DownloadStatus::Completed);
    }

    #[test]
    fn download_status_returns_error_when_failed() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"status": "failed"}"#)
            .expect_get("/api/v1/models/download/status/dl-failed-000")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.download_status("dl-failed-000");

        assert_eq!(result, Err(ManagementError::DownloadFailed));
    }

    #[test]
    fn download_status_returns_error_when_not_found() {
        let server = FakeServer::start(vec![
            FakeResponse::status(404).expect_get("/api/v1/models/download/status/nonexistent")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.download_status("nonexistent");

        assert_eq!(result, Err(ManagementError::OperationNotFound));
    }

    #[test]
    fn download_status_returns_error_when_unauthorized() {
        let server = FakeServer::start(vec![
            FakeResponse::status(403).expect_get("/api/v1/models/download/status/dl-secret")
        ]);

        let runtime = LmStudioRuntime::with_token(server.address(), "bad-token");
        let result = runtime.download_status("dl-secret");

        assert_eq!(result, Err(ManagementError::Unauthorized));
    }

    #[test]
    fn download_status_rejects_invalid_job_id_characters() {
        let runtime = LmStudioRuntime::new("127.0.0.1:1");
        // Path injection attempt must be rejected before any HTTP call.
        let result = runtime.download_status("../etc/passwd");

        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn download_status_accepts_safe_job_id_characters() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"status": "completed"}"#)
            .expect_get("/api/v1/models/download/status/dl-abc_123_DEF")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.download_status("dl-abc_123_DEF").unwrap();

        assert_eq!(result, DownloadStatus::Completed);
    }

    #[test]
    fn download_status_sends_auth_header_when_token_configured() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"status": "completed"}"#)
            .expect_get("/api/v1/models/download/status/dl-auth")
            .expect_header("Authorization", "Bearer my-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "my-token");
        let result = runtime.download_status("dl-auth").unwrap();

        assert_eq!(result, DownloadStatus::Completed);
    }

    // ========== Load/unload operations contract tests ==========

    #[test]
    fn load_model_returns_instance_id_on_success() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"instance_id": "inst_abc123", "status": "loaded", "type": "llm", "load_time_seconds": 1.5}"#,
        )
        .expect_post("/api/v1/models/load")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.load_model("llama-3.1-8b").unwrap();

        assert_eq!(result, "inst_abc123");
    }

    #[test]
    fn load_model_returns_error_when_status_not_loaded() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"instance_id": "inst_abc123", "status": "loading"}"#,
        )
        .expect_post("/api/v1/models/load")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.load_model("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn load_model_returns_error_when_unauthorized() {
        let server = FakeServer::start(vec![
            FakeResponse::status(401).expect_post("/api/v1/models/load")
        ]);

        let runtime = LmStudioRuntime::with_token(server.address(), "bad-token");
        let result = runtime.load_model("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::Unauthorized));
    }

    #[test]
    fn load_model_returns_error_when_model_not_found() {
        let server = FakeServer::start(vec![
            FakeResponse::status(404).expect_post("/api/v1/models/load")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.load_model("nonexistent-model");

        assert_eq!(result, Err(ManagementError::ModelNotFound));
    }

    #[test]
    fn load_model_returns_error_when_missing_instance_id() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"status": "loaded", "type": "llm"}"#,
        )
        .expect_post("/api/v1/models/load")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.load_model("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn load_model_returns_error_when_empty_instance_id() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"instance_id": "", "status": "loaded", "type": "llm"}"#,
        )
        .expect_post("/api/v1/models/load")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.load_model("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn load_model_returns_error_when_unreachable() {
        let runtime = LmStudioRuntime::new("127.0.0.1:1");
        let result = runtime.load_model("llama-3.1-8b");

        assert_eq!(result, Err(ManagementError::EndpointUnreachable));
    }

    #[test]
    fn load_model_sends_auth_header_when_token_configured() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(
            r#"{"instance_id": "inst_auth", "status": "loaded", "type": "llm", "load_time_seconds": 1.0}"#,
        )
        .expect_post("/api/v1/models/load")
        .expect_header("Authorization", "Bearer secret-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "secret-token");
        let result = runtime.load_model("llama-3.1-8b").unwrap();

        assert_eq!(result, "inst_auth");
    }

    #[test]
    fn unload_instance_succeeds_on_2xx() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"success": true}"#).expect_post("/api/v1/models/unload")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.unload_instance("inst_abc123");

        assert!(result.is_ok());
    }

    #[test]
    fn unload_instance_returns_error_when_unauthorized() {
        let server = FakeServer::start(vec![
            FakeResponse::status(403).expect_post("/api/v1/models/unload")
        ]);

        let runtime = LmStudioRuntime::with_token(server.address(), "bad-token");
        let result = runtime.unload_instance("inst_abc123");

        assert_eq!(result, Err(ManagementError::Unauthorized));
    }

    #[test]
    fn unload_instance_returns_error_when_instance_not_found() {
        let server = FakeServer::start(vec![
            FakeResponse::status(404).expect_post("/api/v1/models/unload")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.unload_instance("nonexistent");

        assert_eq!(result, Err(ManagementError::OperationNotFound));
    }

    #[test]
    fn unload_instance_rejects_empty_instance_id() {
        let runtime = LmStudioRuntime::new("127.0.0.1:1");
        // Empty instance_id must be rejected before any HTTP call.
        let result = runtime.unload_instance("");

        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn unload_instance_accepts_opaque_instance_ids() {
        // instance_id is in JSON body, not URL path, so we accept opaque provider IDs.
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"success": true}"#).expect_post("/api/v1/models/unload")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        // LM Studio may use various formats for instance IDs.
        let result = runtime.unload_instance("inst-abc_123_DEF/xyz");

        assert!(result.is_ok());
    }

    #[test]
    fn unload_instance_sends_auth_header_when_token_configured() {
        let server = FakeServer::start(vec![FakeResponse::ok_json(r#"{"success": true}"#)
            .expect_post("/api/v1/models/unload")
            .expect_header("Authorization", "Bearer my-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "my-token");
        let result = runtime.unload_instance("inst_auth");

        assert!(result.is_ok());
    }
}
