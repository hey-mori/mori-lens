//! Provider capability descriptors.
//!
//! Each provider (Ollama, LM Studio, etc.) has a set of capabilities
//! that determine what operations are supported.
//!
//! # Semantics
//!
//! `ProviderCapability` describes operations the **current Mori Lens adapter**
//! safely supports, not just what the provider API documents. If an operation
//! is not yet implemented in Mori Lens, the capability must be `false` even
//! if the provider supports it.
//!
//! This prevents UI from advertising actions that are not implemented.
//! Capabilities flip to `true` only after the adapter is implemented and tested.

use serde::{Deserialize, Serialize};

/// Authentication requirement for a provider.
///
/// Represents whether the provider requires authentication for API access.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationRequirement {
    /// Provider does not require authentication.
    /// Ollama: runs locally without auth.
    #[default]
    None,
    /// Provider supports authentication but does not require it by default.
    /// LM Studio: can be configured to require an API token.
    /// Mori Lens may or may not support token configuration.
    Optional,
    /// Provider requires authentication for all requests.
    Required,
}

/// Provider capability descriptor.
///
/// Describes what operations the **current Mori Lens adapter** supports for
/// Quick and Deep engines. These are derived from implemented adapter code,
/// not just provider API documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapability {
    /// Provider can serve Quick translation requests.
    pub can_quick: bool,
    /// Provider can serve Deep analysis requests.
    pub can_deep: bool,
    /// Mori Lens adapter supports listing available models via native API.
    /// Ollama: GET /api/tags (implemented)
    /// LM Studio: GET /api/v1/models (implemented)
    pub can_list_models: bool,
    /// Mori Lens adapter supports downloading models via native API.
    /// Ollama: POST /api/pull with NDJSON streaming (implemented in P20B)
    /// LM Studio: POST /api/v1/models/download with job polling (implemented in P20B)
    pub can_download_model: bool,
    /// Mori Lens adapter supports explicitly loading a model into memory.
    /// LM Studio: POST /api/v1/models/load, returns instance_id (implemented in P20B)
    /// Ollama: not applicable (auto-loads on inference)
    pub can_load_model: bool,
    /// Mori Lens adapter supports explicitly unloading a model from memory.
    /// LM Studio: POST /api/v1/models/unload, keyed by instance_id (implemented in P20B)
    /// Ollama: not applicable (manages memory automatically)
    pub can_unload_model: bool,
    /// Provider authentication requirement.
    pub auth_requirement: AuthenticationRequirement,
    /// Mori Lens adapter supports progress reporting for downloads.
    /// Ollama: NDJSON streaming from /api/pull (implemented in P20B)
    /// LM Studio: polling GET /api/v1/models/download/status (implemented in P20B)
    pub supports_progress: bool,
    /// Mori Lens adapter supports canceling an in-progress download.
    /// Currently `false` for both providers (no documented cancel endpoint).
    pub can_cancel_download: bool,
}

impl ProviderCapability {
    /// Ollama capabilities as currently supported by Mori Lens.
    ///
    /// Implemented:
    /// - Quick (translation): YES
    /// - Deep (analysis): YES
    /// - List models: YES (GET /api/tags)
    /// - Download model: YES (POST /api/pull with NDJSON progress streaming)
    /// - Progress reporting: YES (NDJSON streaming from /api/pull)
    ///
    /// Not applicable:
    /// - Load model: NO (Ollama auto-loads on inference)
    /// - Unload model: NO (Ollama manages memory automatically)
    ///
    /// Auth: None (Ollama runs locally without auth)
    /// Cancel: NO (no documented cancel endpoint)
    pub fn ollama() -> Self {
        Self {
            can_quick: true,
            can_deep: true,
            can_list_models: true,
            can_download_model: true,
            can_load_model: false,
            can_unload_model: false,
            auth_requirement: AuthenticationRequirement::None,
            supports_progress: true,
            can_cancel_download: false,
        }
    }

    /// LM Studio capabilities as currently supported by Mori Lens.
    ///
    /// Implemented:
    /// - Quick (translation): YES
    /// - List models: YES (GET /api/v1/models)
    /// - Download model: YES (POST /api/v1/models/download with job polling)
    /// - Load model: YES (POST /api/v1/models/load, returns instance_id)
    /// - Unload model: YES (POST /api/v1/models/unload, keyed by instance_id)
    /// - Progress reporting: YES (polling GET /api/v1/models/download/status)
    ///
    /// Not applicable:
    /// - Deep (analysis): NO (LM Studio does not support Deep)
    ///
    /// Auth: Optional (LM Studio can be configured to require API token)
    /// Cancel: NO (no documented cancel endpoint)
    pub fn lm_studio() -> Self {
        Self {
            can_quick: true,
            can_deep: false,
            can_list_models: true,
            can_download_model: true,
            can_load_model: true,
            can_unload_model: true,
            auth_requirement: AuthenticationRequirement::Optional,
            supports_progress: true,
            can_cancel_download: false,
        }
    }

    /// Return capabilities for a given provider.
    ///
    /// Convenience wrapper that dispatches to `ollama()` or `lm_studio()`.
    pub fn for_provider(provider: crate::translate::Provider) -> Self {
        match provider {
            crate::translate::Provider::Ollama => Self::ollama(),
            crate::translate::Provider::LmStudio => Self::lm_studio(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_capabilities_reflect_mori_support() {
        let caps = ProviderCapability::ollama();
        assert!(caps.can_quick, "Mori supports Quick via Ollama");
        assert!(caps.can_deep, "Mori supports Deep via Ollama");
        assert!(caps.can_list_models, "Mori supports listing Ollama models");
        assert!(
            caps.can_download_model,
            "Mori implements Ollama download via POST /api/pull"
        );
        assert!(
            !caps.can_load_model,
            "Ollama auto-loads on inference; explicit load not needed"
        );
        assert!(
            !caps.can_unload_model,
            "Ollama manages memory automatically; explicit unload not needed"
        );
        assert_eq!(
            caps.auth_requirement,
            AuthenticationRequirement::None,
            "Ollama does not require auth"
        );
        assert!(
            caps.supports_progress,
            "Mori implements Ollama progress via NDJSON streaming"
        );
        assert!(
            !caps.can_cancel_download,
            "Ollama has no documented cancel endpoint"
        );
    }

    #[test]
    fn lm_studio_capabilities_reflect_mori_support() {
        let caps = ProviderCapability::lm_studio();
        assert!(caps.can_quick, "Mori supports Quick via LM Studio");
        assert!(
            !caps.can_deep,
            "LM Studio does not support Deep (Mori or otherwise)"
        );
        assert!(
            caps.can_list_models,
            "Mori supports listing LM Studio models"
        );
        assert!(
            caps.can_download_model,
            "Mori implements LM Studio download via POST /api/v1/models/download"
        );
        assert!(
            caps.can_load_model,
            "Mori implements LM Studio load via POST /api/v1/models/load"
        );
        assert!(
            caps.can_unload_model,
            "Mori implements LM Studio unload via POST /api/v1/models/unload"
        );
        assert_eq!(
            caps.auth_requirement,
            AuthenticationRequirement::Optional,
            "LM Studio can be configured to require auth"
        );
        assert!(
            caps.supports_progress,
            "Mori implements LM Studio progress via polling download status"
        );
        assert!(
            !caps.can_cancel_download,
            "LM Studio has no documented cancel endpoint"
        );
    }

    #[test]
    fn authentication_requirement_serializes_to_snake_case() {
        let json = serde_json::to_string(&AuthenticationRequirement::None).unwrap();
        assert!(json.contains("\"none\""));

        let json = serde_json::to_string(&AuthenticationRequirement::Optional).unwrap();
        assert!(json.contains("\"optional\""));

        let json = serde_json::to_string(&AuthenticationRequirement::Required).unwrap();
        assert!(json.contains("\"required\""));
    }

    #[test]
    fn capabilities_serialize_to_json() {
        let caps = ProviderCapability::ollama();
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("\"can_quick\":true"));
        assert!(json.contains("\"can_deep\":true"));
        assert!(json.contains("\"can_download_model\":true"));
        assert!(json.contains("\"supports_progress\":true"));
        assert!(json.contains("\"can_load_model\":false"));
        assert!(json.contains("\"can_unload_model\":false"));
    }

    #[test]
    fn ollama_and_lm_studio_differ_on_deep() {
        let ollama = ProviderCapability::ollama();
        let lm_studio = ProviderCapability::lm_studio();
        assert!(ollama.can_deep);
        assert!(!lm_studio.can_deep);
    }

    #[test]
    fn ollama_and_lm_studio_differ_on_auth() {
        let ollama = ProviderCapability::ollama();
        let lm_studio = ProviderCapability::lm_studio();
        assert_ne!(ollama.auth_requirement, lm_studio.auth_requirement);
    }
}
