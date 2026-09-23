//! Provider capability descriptors.
//!
//! Each provider (Ollama, LM Studio, etc.) has a set of capabilities
//! that determine what operations are supported.

use serde::{Deserialize, Serialize};

/// Provider capability descriptor.
///
/// Describes what operations a provider supports for Quick and Deep engines.
/// These are derived from the official API documentation for each provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapability {
    /// Provider can serve Quick translation requests.
    pub can_quick: bool,
    /// Provider can serve Deep analysis requests.
    pub can_deep: bool,
    /// Provider supports listing available models via native API.
    /// Ollama: GET /api/tags
    /// LM Studio: GET /api/v1/models
    pub can_list_models: bool,
    /// Provider supports downloading models via native API.
    /// Ollama: POST /api/pull (streaming progress)
    /// LM Studio: POST /api/v1/models/download (job status)
    pub can_download_model: bool,
    /// Provider supports explicitly loading a model into memory.
    /// Ollama: false (auto-loads on inference request)
    /// LM Studio: true (POST /api/v1/models/load)
    pub can_load_model: bool,
    /// Provider supports explicitly unloading a model from memory.
    /// Ollama: false (manages memory automatically)
    /// LM Studio: true (POST /api/v1/models/unload)
    pub can_unload_model: bool,
    /// Provider requires authentication (API key or token).
    /// Both Ollama and LM Studio run locally and do not require auth.
    pub requires_auth: bool,
    /// Provider supports progress reporting for downloads.
    /// Ollama: streaming JSON with "completed" and "total" fields
    /// LM Studio: job status endpoint with progress field
    pub supports_progress: bool,
    /// Provider supports canceling an in-progress download.
    /// Ollama: false (no documented cancel endpoint)
    /// LM Studio: false (no documented cancel endpoint)
    pub can_cancel_download: bool,
}

impl ProviderCapability {
    /// Ollama capabilities.
    ///
    /// Ollama supports both Quick (translation) and Deep (analysis) engines.
    /// Native API: list models (/api/tags), download (/api/pull).
    /// No explicit load/unload: Ollama auto-loads on inference and manages memory.
    /// Progress: streaming JSON with completed/total fields.
    /// No cancel: no documented cancel endpoint.
    /// No auth: runs locally on http://localhost:11434.
    pub fn ollama() -> Self {
        Self {
            can_quick: true,
            can_deep: true,
            can_list_models: true,
            can_download_model: true,
            can_load_model: false,
            can_unload_model: false,
            requires_auth: false,
            supports_progress: true,
            can_cancel_download: false,
        }
    }

    /// LM Studio capabilities.
    ///
    /// LM Studio supports Quick (translation) but not Deep (analysis).
    /// Native API: list models (/api/v1/models), download (/api/v1/models/download),
    /// load (/api/v1/models/load), unload (/api/v1/models/unload).
    /// Progress: job status endpoint with progress field.
    /// No cancel: no documented cancel endpoint.
    /// No auth: runs locally on http://localhost:1234.
    pub fn lm_studio() -> Self {
        Self {
            can_quick: true,
            can_deep: false,
            can_list_models: true,
            can_download_model: true,
            can_load_model: true,
            can_unload_model: true,
            requires_auth: false,
            supports_progress: true,
            can_cancel_download: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_capabilities_match_api_truth() {
        let caps = ProviderCapability::ollama();
        assert!(caps.can_quick, "Ollama supports Quick (translation)");
        assert!(caps.can_deep, "Ollama supports Deep (analysis)");
        assert!(
            caps.can_list_models,
            "Ollama supports GET /api/tags for listing"
        );
        assert!(
            caps.can_download_model,
            "Ollama supports POST /api/pull for downloading"
        );
        assert!(
            !caps.can_load_model,
            "Ollama does NOT have explicit load API (auto-loads on inference)"
        );
        assert!(
            !caps.can_unload_model,
            "Ollama does NOT have explicit unload API (manages memory automatically)"
        );
        assert!(
            !caps.requires_auth,
            "Ollama runs locally and does not require auth"
        );
        assert!(
            caps.supports_progress,
            "Ollama streaming pull provides progress"
        );
        assert!(
            !caps.can_cancel_download,
            "Ollama has no documented cancel endpoint"
        );
    }

    #[test]
    fn lm_studio_capabilities_match_api_truth() {
        let caps = ProviderCapability::lm_studio();
        assert!(caps.can_quick, "LM Studio supports Quick (translation)");
        assert!(!caps.can_deep, "LM Studio does NOT support Deep (analysis)");
        assert!(
            caps.can_list_models,
            "LM Studio supports GET /api/v1/models for listing"
        );
        assert!(
            caps.can_download_model,
            "LM Studio supports POST /api/v1/models/download"
        );
        assert!(
            caps.can_load_model,
            "LM Studio supports POST /api/v1/models/load"
        );
        assert!(
            caps.can_unload_model,
            "LM Studio supports POST /api/v1/models/unload"
        );
        assert!(
            !caps.requires_auth,
            "LM Studio runs locally and does not require auth"
        );
        assert!(
            caps.supports_progress,
            "LM Studio download job status provides progress"
        );
        assert!(
            !caps.can_cancel_download,
            "LM Studio has no documented cancel endpoint"
        );
    }

    #[test]
    fn ollama_and_lm_studio_differ_on_load_unload() {
        let ollama = ProviderCapability::ollama();
        let lm_studio = ProviderCapability::lm_studio();
        assert_ne!(ollama.can_load_model, lm_studio.can_load_model);
        assert_ne!(ollama.can_unload_model, lm_studio.can_unload_model);
    }

    #[test]
    fn ollama_and_lm_studio_differ_on_deep() {
        let ollama = ProviderCapability::ollama();
        let lm_studio = ProviderCapability::lm_studio();
        assert!(ollama.can_deep);
        assert!(!lm_studio.can_deep);
    }

    #[test]
    fn capabilities_serialize_to_json() {
        let caps = ProviderCapability::ollama();
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("\"can_quick\":true"));
        assert!(json.contains("\"can_deep\":true"));
        assert!(json.contains("\"can_load_model\":false"));
        assert!(json.contains("\"can_unload_model\":false"));
    }
}
