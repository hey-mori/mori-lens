//! LM Studio runtime: discovery, readiness, and status.
//!
//! Implements the [`ProviderRuntime`] trait for LM Studio, providing a
//! provider-neutral interface over LM Studio's OpenAI-compatible API.
//!
//! LM Studio uses the OpenAI-compatible `/v1/models` endpoint for model listing
//! and `/v1/chat/completions` for inference. This module handles the management
//! plane (model listing and status), while [`LmStudioTranslator`] handles the
//! inference plane.
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
/// Wraps HTTP calls to LM Studio's `/v1/models` endpoint, providing a
/// provider-neutral interface over LM Studio's OpenAI-compatible API.
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
}

impl ProviderRuntime for LmStudioRuntime {
    fn provider_identity(&self) -> Provider {
        Provider::LmStudio
    }

    fn reachability(&self) -> Reachability {
        // Query /v1/models to check if LM Studio is reachable
        match list_models_with_auth(&self.endpoint, self.api_token.as_deref()) {
            Ok(_) => Reachability::Reachable,
            Err(_) => Reachability::Unreachable,
        }
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, TranslateError> {
        list_models_with_auth(&self.endpoint, self.api_token.as_deref())
    }

    fn model_state(&self, model: &str) -> Result<ModelState, TranslateError> {
        // LM Studio doesn't expose a separate "loaded models" endpoint like Ollama's /api/ps.
        // All models in /v1/models are available, and we don't distinguish loaded vs available.
        // So we return Available if the model exists, ModelMissing if it doesn't.
        let models = self.list_models()?;
        let model_info = models
            .iter()
            .find(|m| m.name == model)
            .ok_or_else(|| TranslateError::ModelMissing(model.to_owned()))?;

        Ok(ModelState::Available {
            name: model_info.name.clone(),
            size: model_info.size,
        })
    }
}

/// List models from LM Studio's /v1/models endpoint.
///
/// Filters out embedding models (type != "text") since they are not suitable
/// for translation. Returns only text models.
fn list_models_with_auth(
    endpoint: &str,
    api_token: Option<&str>,
) -> Result<Vec<ModelInfo>, TranslateError> {
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

    // Check for authentication errors using HTTP status code
    if status == 401 || status == 403 {
        return Err(TranslateError::Unavailable);
    }

    // Also check for authentication errors in the response body
    // LM Studio returns {"error": {"type": "authentication_error"}} for 401
    // and {"error": {"type": "permission_error"}} for 403
    if let Some(error_type) = crate::json::string_value_of_key(&body, "type") {
        if error_type == "authentication_error" || error_type == "permission_error" {
            return Err(TranslateError::Unavailable);
        }
    }

    // Parse the LM Studio v1 models response
    // Schema: {"object": "list", "data": [{"id": "...", "type": "...", ...}]}
    let model_ids = crate::json::string_values_of_key(&body, "id");
    let model_types = crate::json::string_values_of_key(&body, "type");

    // Filter to only text models (exclude embedding models)
    // LM Studio size is not in the v1 schema, so we use 0 as a placeholder
    // If model_types is shorter than model_ids, assume missing types are "text"
    let models: Vec<ModelInfo> = model_ids
        .into_iter()
        .enumerate()
        .filter(|(i, _)| {
            model_types.get(*i).map(|t| t == "text").unwrap_or(true) // Default to text if type is missing
        })
        .map(|(_, id)| ModelInfo {
            name: id,
            size: 0, // LM Studio v1 schema doesn't include size
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
            FakeResponse::ok_json(LM_STUDIO_EMPTY_MODELS).expect_get("/v1/models")
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
            FakeResponse::ok_json(LM_STUDIO_EMPTY_MODELS).expect_get("/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn list_models_filters_out_embedding_models() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MODELS_RESPONSE).expect_get("/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        // Should only include text models, not embedding models
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama-3.1-8b");
        assert_eq!(models[1].name, "qwen2.5-7b");
    }

    #[test]
    fn list_models_returns_text_models_only() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_TEXT_MODELS_ONLY).expect_get("/v1/models")
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
            FakeResponse::ok_json(LM_STUDIO_TEXT_MODELS_ONLY).expect_get("/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let state = runtime.model_state("llama-3.1-8b").unwrap();

        match state {
            ModelState::Available { name, size } => {
                assert_eq!(name, "llama-3.1-8b");
                assert_eq!(size, 0); // LM Studio v1 doesn't include size
            }
            _ => panic!("Expected ModelState::Available, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_model_missing_when_model_not_found() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_TEXT_MODELS_ONLY).expect_get("/v1/models")
        ]);

        let runtime = LmStudioRuntime::new(server.address());
        let result = runtime.model_state("nonexistent-model");

        assert!(matches!(result, Err(TranslateError::ModelMissing(_))));
    }

    #[test]
    fn model_state_returns_model_missing_for_embedding_model() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(LM_STUDIO_MODELS_RESPONSE).expect_get("/v1/models")
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
    fn list_models_returns_unavailable_on_401_unauthorized() {
        let server = FakeServer::start(vec![FakeResponse::unauthorized()
            .expect_get("/v1/models")
            .expect_header("Authorization", "Bearer invalid-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "invalid-token");
        let result = runtime.list_models();

        // 401 should be treated as Unreachable (authentication failure)
        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn list_models_returns_unavailable_on_403_forbidden() {
        let server = FakeServer::start(vec![FakeResponse::forbidden()
            .expect_get("/v1/models")
            .expect_header("Authorization", "Bearer forbidden-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "forbidden-token");
        let result = runtime.list_models();

        // 403 should be treated as Unreachable (permission failure)
        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn reachability_returns_unreachable_on_401_unauthorized() {
        let server = FakeServer::start(vec![FakeResponse::unauthorized()
            .expect_get("/v1/models")
            .expect_header("Authorization", "Bearer invalid-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "invalid-token");
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    #[test]
    fn reachability_returns_unreachable_on_403_forbidden() {
        let server = FakeServer::start(vec![FakeResponse::forbidden()
            .expect_get("/v1/models")
            .expect_header("Authorization", "Bearer forbidden-token")]);

        let runtime = LmStudioRuntime::with_token(server.address(), "forbidden-token");
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }
}
