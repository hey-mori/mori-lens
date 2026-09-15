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
    let body = crate::http::get(endpoint, "/api/ps", std::time::Duration::from_secs(2))
        .map_err(|_| TranslateError::Unavailable)?;

    // Check if the model name appears in the JSON response
    // The response has a "models" array, and we look for the model name in it
    Ok(body.contains(&format!("\"name\":\"{}\"", model)))
}

/// Prewarm a model by sending a tiny request with a keep-alive lease.
///
/// This is an Ollama-specific lifecycle operation. It sends a minimal generate
/// request to force the model to load, with a lease that keeps it in memory
/// for the specified duration.
///
/// **Not part of the ProviderRuntime trait** because lifecycle operations are
/// provider-specific. LM Studio requires explicit load/unload with instance
/// ownership tracking, which is a different shape.
pub fn prewarm(endpoint: &str, model: &str) -> Result<std::time::Duration, TranslateError> {
    // Build JSON manually to avoid pulling in serde_json
    let request = format!(
        r#"{{"model":"{}","prompt":"","stream":false,"keep_alive":"30m"}}"#,
        model
    );

    let start = std::time::Instant::now();
    crate::http::post(
        endpoint,
        "/api/generate",
        &request,
        std::time::Duration::from_secs(30),
    )
    .map_err(|_| TranslateError::Unavailable)?;
    let elapsed = start.elapsed();

    Ok(elapsed)
}

/// Release a model by sending a request with keep_alive=0.
///
/// This is an Ollama-specific lifecycle operation. It tells Ollama to unload
/// the model immediately.
///
/// **Not part of the ProviderRuntime trait** because lifecycle operations are
/// provider-specific, and this has no ownership tracking. In a multi-client
/// scenario, another client may have loaded the model, and releasing it here
/// would break their lease. This is an existing weakness in the Ollama adapter
/// that is documented in the P4 spike.
pub fn release(endpoint: &str, model: &str) {
    // Build JSON manually to avoid pulling in serde_json
    let request = format!(
        r#"{{"model":"{}","prompt":"","stream":false,"keep_alive":0}}"#,
        model
    );

    // Best-effort: if the release fails, the model will expire naturally
    let _ = crate::http::post(
        endpoint,
        "/api/generate",
        &request,
        std::time::Duration::from_secs(2),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
