//! Ollama runtime: discovery, readiness, and status.
//!
//! Implements the [`ProviderRuntime`] trait for Ollama, wrapping the existing
//! Ollama-specific logic for model listing and readiness checks.

use lens_core::translate::{
    ModelInfo, ModelState, Provider, ProviderRuntime, Reachability, TranslateError,
};

use crate::DEFAULT_ENDPOINT;

/// Ollama runtime for provider management operations.
///
/// Wraps the HTTP calls to Ollama's `/api/tags` and `/api/ps` endpoints,
/// providing a provider-neutral interface over Ollama-specific logic.
#[derive(Debug, Clone)]
pub struct OllamaRuntime {
    endpoint: String,
}

impl Default for OllamaRuntime {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_owned(),
        }
    }
}

impl OllamaRuntime {
    /// Create a runtime pointed at a specific Ollama endpoint.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// The endpoint this runtime is pointed at.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl ProviderRuntime for OllamaRuntime {
    fn provider_identity(&self) -> Provider {
        Provider::Ollama
    }

    fn reachability(&self) -> Reachability {
        // Use the existing check_reachable logic, which hits /api/tags
        if crate::check_reachable(&self.endpoint) {
            Reachability::Reachable
        } else {
            Reachability::Unreachable
        }
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>, TranslateError> {
        crate::list_models(&self.endpoint)
    }

    fn model_state(&self, model: &str) -> Result<ModelState, TranslateError> {
        // Check if the model is in the installed list
        let models = self.list_models()?;
        let model_info = models
            .iter()
            .find(|m| m.name == model)
            .ok_or_else(|| TranslateError::ModelMissing(model.to_owned()))?;

        // Check if the model is currently loaded (via /api/ps)
        let loaded = is_model_loaded(&self.endpoint, model)?;

        if loaded {
            Ok(ModelState::Loaded {
                name: model.to_owned(),
            })
        } else {
            Ok(ModelState::Available {
                name: model_info.name.clone(),
                size: model_info.size,
            })
        }
    }
}

/// Whether a model is currently loaded in Ollama's memory.
///
/// Queries `/api/ps` and checks if the model name appears in the response.
/// This is the same check used by the settings UI and the worker.
fn is_model_loaded(endpoint: &str, model: &str) -> Result<bool, TranslateError> {
    let (_status, body) = crate::http::get(endpoint, "/api/ps", std::time::Duration::from_secs(2))
        .map_err(|_| TranslateError::Unavailable)?;

    // Use canonical JSON parsing path (same as lib.rs)
    let loaded_models = crate::json::string_values_of_key(&body, "name");
    Ok(loaded_models.iter().any(|name| name == model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_server::{FakeResponse, FakeServer};

    #[test]
    fn ollama_runtime_reports_ollama_identity() {
        let runtime = OllamaRuntime::default();
        assert_eq!(runtime.provider_identity(), Provider::Ollama);
    }

    #[test]
    fn ollama_runtime_uses_default_endpoint() {
        let runtime = OllamaRuntime::default();
        assert_eq!(runtime.endpoint(), DEFAULT_ENDPOINT);
    }

    #[test]
    fn ollama_runtime_accepts_custom_endpoint() {
        let runtime = OllamaRuntime::new("localhost:9999");
        assert_eq!(runtime.endpoint(), "localhost:9999");
    }

    // Contract tests using deterministic fake HTTP server.
    // These are AUTOMATED CONTRACT PASS, not RUNTIME VERIFIED.

    #[test]
    fn reachability_returns_reachable_when_endpoint_answers() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"models":[]}"#).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        assert_eq!(runtime.reachability(), Reachability::Reachable);
    }

    #[test]
    fn reachability_returns_unreachable_when_endpoint_does_not_answer() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        assert_eq!(runtime.reachability(), Reachability::Unreachable);
    }

    #[test]
    fn list_models_returns_empty_vec_when_no_models_installed() {
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(r#"{"models":[]}"#).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let models = runtime.list_models().unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn list_models_returns_models_with_name_and_size() {
        let response_body = r#"{
            "models": [
                {"name": "llama3.1:8b", "size": 4700000000},
                {"name": "qwen2.5:7b", "size": 4300000000}
            ]
        }"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(response_body).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let models = runtime.list_models().unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "llama3.1:8b");
        assert_eq!(models[0].size, 4700000000);
        assert_eq!(models[1].name, "qwen2.5:7b");
        assert_eq!(models[1].size, 4300000000);
    }

    #[test]
    fn list_models_returns_unavailable_when_endpoint_fails() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        let result = runtime.list_models();
        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn model_state_returns_available_when_model_is_installed_but_not_loaded() {
        // First call: list_models via /api/tags
        let tags_response = r#"{"models":[{"name":"llama3.1:8b","size":4700000000}]}"#;
        // Second call: is_model_loaded via /api/ps (model not loaded)
        let ps_response = r#"{"models":[]}"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(tags_response).expect_get("/api/tags"),
            FakeResponse::ok_json(ps_response).expect_get("/api/ps"),
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let state = runtime.model_state("llama3.1:8b").unwrap();

        match state {
            ModelState::Available { name, size } => {
                assert_eq!(name, "llama3.1:8b");
                assert_eq!(size, 4700000000);
            }
            _ => panic!("Expected ModelState::Available, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_loaded_when_model_is_in_memory() {
        // First call: list_models via /api/tags
        let tags_response = r#"{"models":[{"name":"llama3.1:8b","size":4700000000}]}"#;
        // Second call: is_model_loaded via /api/ps (model is loaded)
        let ps_response = r#"{"models":[{"name":"llama3.1:8b"}]}"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(tags_response).expect_get("/api/tags"),
            FakeResponse::ok_json(ps_response).expect_get("/api/ps"),
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let state = runtime.model_state("llama3.1:8b").unwrap();

        match state {
            ModelState::Loaded { name } => {
                assert_eq!(name, "llama3.1:8b");
            }
            _ => panic!("Expected ModelState::Loaded, got {:?}", state),
        }
    }

    #[test]
    fn model_state_returns_model_missing_when_model_not_installed() {
        let tags_response = r#"{"models":[]}"#;
        let server = FakeServer::start(vec![
            FakeResponse::ok_json(tags_response).expect_get("/api/tags")
        ]);

        let runtime = OllamaRuntime::new(server.address());
        let result = runtime.model_state("nonexistent-model");

        assert!(matches!(result, Err(TranslateError::ModelMissing(_))));
    }

    #[test]
    fn model_state_returns_unavailable_when_tags_endpoint_fails() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        let result = runtime.model_state("llama3.1:8b");

        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }

    #[test]
    fn model_state_returns_unavailable_when_ps_endpoint_fails() {
        // Use an endpoint that nothing listens on
        let runtime = OllamaRuntime::new("127.0.0.1:1");
        let result = runtime.model_state("llama3.1:8b");

        assert!(matches!(result, Err(TranslateError::Unavailable)));
    }
}
