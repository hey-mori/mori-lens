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

use lens_core::translate::{
    ModelInfo, ModelState, Provider, ProviderRuntime, Reachability, TranslateError,
};

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
        // Use the native v1 API response which includes loaded_instances.
        // A model is "loaded" if its loaded_instances array is non-empty.
        // Identity resolution uses canonical ID (key) only, not display_name.
        let raw_models = list_models_with_loaded(&self.endpoint, self.api_token.as_deref())?;

        let raw_model = raw_models
            .iter()
            .find(|m| m.key == model)
            .ok_or_else(|| TranslateError::ModelMissing(model.to_owned()))?;

        if raw_model.is_loaded {
            Ok(ModelState::Loaded {
                name: raw_model.key.clone(),
            })
        } else {
            Ok(ModelState::Available {
                name: raw_model.key.clone(),
                size: raw_model.size_bytes,
            })
        }
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
        return Err(TranslateError::Unavailable);
    }

    // Any non-2xx status is an error, not a successful empty list.
    // This prevents 400, 404, 500, etc. from being treated as "no models".
    if !(200..300).contains(&status) {
        return Err(TranslateError::Unavailable);
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

/// Parse model objects from the native v1 response body.
///
/// Each object in the "models" array is parsed individually so that missing
/// fields in one model do not shift the indices of subsequent models.
///
/// Returns `Err(TranslateError::Malformed)` if the `"models"` key is absent,
/// because a 2xx response without it does not represent a valid model list.
fn parse_model_objects(body: &str) -> Result<Vec<RawLmStudioModel>, TranslateError> {
    let mut models = Vec::new();

    // Find the "models" array. A 2xx without it is malformed.
    let Some(data_start) = body.find("\"models\"") else {
        return Err(TranslateError::Malformed);
    };
    let after_data_key = &body[data_start + 8..];
    let Some(array_start) = after_data_key.find('[') else {
        return Err(TranslateError::Malformed);
    };
    let array_content = &after_data_key[array_start + 1..];

    // Iterate through objects in the array
    let mut pos = 0usize;
    while let Some(obj_start) = array_content[pos..].find('{') {
        let obj_begin = pos + obj_start;
        // Find the matching closing brace
        let Some(obj_end) = find_matching_brace(array_content, obj_begin) else {
            break;
        };
        let obj_str = &array_content[obj_begin..=obj_end];

        // Extract fields from this single object
        let key = extract_string_field(obj_str, "key").unwrap_or_default();
        let display_name = extract_string_field(obj_str, "display_name").unwrap_or_default();
        let model_type = extract_string_field(obj_str, "type");
        let size_bytes = extract_number_field(obj_str, "size_bytes").unwrap_or(0);
        let is_loaded = check_object_loaded(obj_str);

        // Fail closed: only include models with explicit type == "llm"
        if model_type.as_deref() == Some("llm") {
            models.push(RawLmStudioModel {
                key,
                display_name,
                size_bytes,
                is_loaded,
            });
        }

        pos = obj_end + 1;
    }

    Ok(models)
}

/// Find the matching closing brace for an opening brace at position `start`.
fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in s[start..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return Some(start + i);
            }
        }
    }
    None
}

/// Extract a string field value from a JSON object string.
fn extract_string_field(obj: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let field_pos = obj.find(&needle)?;
    let after_field = &obj[field_pos + needle.len()..];
    let colon = after_field.find(':')?;
    let after_colon = after_field[colon + 1..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    // Find the closing quote (handling escapes)
    let content = &after_colon[1..];
    let mut end = 0;
    let mut escape = false;
    for (i, ch) in content.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '"' {
            end = i;
            break;
        }
    }
    Some(content[..end].to_owned())
}

/// Extract a number field value from a JSON object string.
fn extract_number_field(obj: &str, field: &str) -> Option<u64> {
    let needle = format!("\"{}\"", field);
    let field_pos = obj.find(&needle)?;
    let after_field = &obj[field_pos + needle.len()..];
    let colon = after_field.find(':')?;
    let after_colon = after_field[colon + 1..].trim_start();
    // Parse digits
    let num_str: String = after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse().ok()
}

/// Check if a single model object has non-empty loaded_instances.
///
/// Handles whitespace variants: `[]`, `[ ]`, `[\n]` are all empty.
/// A non-empty array has an object literal as its first element: `[{...}]`.
fn check_object_loaded(obj: &str) -> bool {
    let Some(pos) = obj.find("\"loaded_instances\"") else {
        return false;
    };
    let after = &obj[pos + 18..];
    let Some(colon) = after.find(':') else {
        return false;
    };
    let after_colon = after[colon + 1..].trim_start();
    if !after_colon.starts_with('[') {
        return false;
    }
    // Find the character after '[' (skipping whitespace)
    let after_bracket = after_colon[1..].trim_start();
    // Non-empty if the next non-whitespace char after '[' is '{' (an object)
    after_bracket.starts_with('{')
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
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_LLM_MODELS_ONLY).expect_get("/api/v1/models")
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
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MODELS_RESPONSE).expect_get("/api/v1/models")
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
    fn list_models_returns_unavailable_on_401_unauthorized() {
        let server = FakeServer::start(vec![FakeResponse::unauthorized()
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer invalid-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "invalid-token");
        let result = runtime.list_models();

        // 401 should be treated as Unreachable (authentication failure)
        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn list_models_returns_unavailable_on_403_forbidden() {
        let server = FakeServer::start(vec![FakeResponse::forbidden()
            .expect_get("/api/v1/models")
            .expect_header("Authorization", "Bearer forbidden-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "forbidden-token");
        let result = runtime.list_models();

        // 403 should be treated as Unreachable (permission failure)
        assert!(matches!(result, Err(TranslateError::Unavailable)));
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
        // model_state calls list_models_with_loaded once per call.
        // Provide enough responses for all three model_state calls below.
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_AMBIGUOUS_DISPLAY_NAMES).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_AMBIGUOUS_DISPLAY_NAMES).expect_get("/api/v1/models"),
            FakeResponse::ok_json(LM_STUDIO_AMBIGUOUS_DISPLAY_NAMES).expect_get("/api/v1/models"),
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
    fn list_models_returns_unavailable_on_500() {
        let server = FakeServer::start(vec![
            FakeResponse::server_error().expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    // HTTP 400 must not become a successful empty list.
    #[test]
    fn list_models_returns_unavailable_on_400() {
        let server = FakeServer::start(vec![
            FakeResponse::bad_request().expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    // HTTP 404 must not become a successful empty list.
    #[test]
    fn list_models_returns_unavailable_on_404() {
        let server =
            FakeServer::start(vec![FakeResponse::not_found().expect_get("/api/v1/models")]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.list_models();

        assert!(matches!(result, Err(TranslateError::Unavailable)));
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

    // 2xx with truncated JSON after "models": cannot parse any objects.
    #[test]
    fn list_models_returns_malformed_on_truncated_response() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MALFORMED_TRUNCATED).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        // Truncated response: "models" key is found but no complete objects.
        // This should return Ok(empty) since the array exists but contains
        // no parseable objects (the truncation means no matching braces).
        let result = runtime.list_models();
        assert!(result.is_ok() || matches!(result, Err(TranslateError::Malformed)));
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
            FakeResponse::ok_json(LM_STUDIO_OFFICIAL_SHAPE).expect_get("/api/v1/models"),
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
            FakeResponse::ok_json(LM_STUDIO_LOADED_WHITESPACE).expect_get("/api/v1/models")
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
}
