//! LM Studio runtime: discovery, readiness, and status.
//!
//! Implements the [`ProviderRuntime`] trait for LM Studio, providing a
//! provider-neutral interface over LM Studio's native v1 API.
//!
//! LM Studio has two API surfaces:
//! - Native v1: `GET /api/v1/models` (management plane, this module)
//! - OpenAI-compatible: `POST /v1/chat/completions` (inference plane)
//!
//! The native v1 model schema uses `id`, `name`, `type`, `size`, and
//! `loaded_instances[]`. A model is considered "loaded" if its
//! `loaded_instances` array is non-empty.
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
        let raw_models = list_models_with_loaded(&self.endpoint, self.api_token.as_deref())?;

        let raw_model = raw_models
            .iter()
            .find(|m| m.key == model || m.display_name == model)
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

    // Also check for authentication errors in the response body
    if let Some(error_type) = crate::json::string_value_of_key(&body, "type") {
        if error_type == "authentication_error" || error_type == "permission_error" {
            return Err(TranslateError::Unavailable);
        }
    }

    // Parse the LM Studio native v1 models response
    // Schema: {"data": [{"key": "...", "display_name": "...", "type": "llm", "size_bytes": 123, "loaded_instances": [...]}]}
    let model_keys = crate::json::string_values_of_key(&body, "key");
    let model_display_names = crate::json::string_values_of_key(&body, "display_name");
    let model_types = crate::json::string_values_of_key(&body, "type");
    let model_sizes = crate::json::number_values_of_key(&body, "size_bytes");

    // Check loaded state by looking for non-empty loaded_instances arrays
    // "loaded_instances":[] means not loaded, "loaded_instances":[{...}] means loaded
    let is_loaded = check_loaded_instances(&body);

    // Filter to only llm models (exclude embedding models)
    let models: Vec<RawLmStudioModel> = model_keys
        .into_iter()
        .enumerate()
        .filter(|(i, _)| model_types.get(*i).map(|t| t == "llm").unwrap_or(true))
        .map(|(i, key)| RawLmStudioModel {
            key,
            display_name: model_display_names.get(i).cloned().unwrap_or_default(),
            size_bytes: model_sizes.get(i).copied().unwrap_or(0),
            is_loaded: is_loaded.get(i).copied().unwrap_or(false),
        })
        .collect();

    Ok(models)
}

/// Check which models have non-empty loaded_instances arrays.
///
/// Returns a vector of booleans, one per model in the response.
/// True means the model has at least one loaded instance.
fn check_loaded_instances(body: &str) -> Vec<bool> {
    let mut result = Vec::new();
    let needle = "\"loaded_instances\"";
    let mut from = 0usize;

    while let Some(at) = body[from..].find(needle) {
        let after_key = from + at + needle.len();
        from = after_key;
        let rest = &body[after_key..];
        let Some(colon) = rest.find(':') else { break };
        let after_colon = rest[colon + 1..].trim_start();

        // Check if it's an empty array [] or non-empty [{...}]
        let is_loaded = !after_colon.starts_with("[]");
        result.push(is_loaded);
    }

    result
}

/// List models from LM Studio's /api/v1/models endpoint.
///
/// Filters out embedding models (type != "llm") since they are not suitable
/// for translation. Returns only LLM models.
fn list_models_with_auth(
    endpoint: &str,
    api_token: Option<&str>,
) -> Result<Vec<ModelInfo>, TranslateError> {
    let raw_models = list_models_with_loaded(endpoint, api_token)?;

    // Convert to ModelInfo (discard loaded state and display_name)
    // ModelInfo.name receives the canonical key identifier
    let models = raw_models
        .into_iter()
        .map(|m| ModelInfo {
            name: m.key,
            size: m.size_bytes,
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
        assert_eq!(models[0].name, "llama-3.1-8b");
        assert_eq!(models[1].name, "qwen2.5-7b");
    }

    #[test]
    fn list_models_returns_llm_models_only() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_LLM_MODELS_ONLY).expect_get("/api/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama-3.1-8b");
        assert_eq!(models[1].name, "qwen2.5-7b");
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
}
