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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationRequirement {
    /// Provider does not require authentication.
    /// Ollama: runs locally without auth.
    None,
    /// Provider supports authentication but does not require it by default.
    /// LM Studio: can be configured to require an API token.
    /// Mori Lens may or may not support token configuration.
    Optional,
    /// Provider requires authentication for all requests.
    Required,
}

impl Default for AuthenticationRequirement {
    fn default() -> Self {
        Self::None
    }
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
    /// Must be `false` until the download adapter is implemented and tested.
    /// Will flip to `true` in P20B after adapter implementation.
    pub can_download_model: bool,
    /// Mori Lens adapter supports explicitly loading a model into memory.
    /// Must be `false` until the load adapter is implemented and tested.
    /// Will flip to `true` in P20B for LM Studio after adapter implementation.
    pub can_load_model: bool,
    /// Mori Lens adapter supports explicitly unloading a model from memory.
    /// Must be `false` until the unload adapter is implemented and tested.
    /// Will flip to `true` in P20B for LM Studio after adapter implementation.
    pub can_unload_model: bool,
    /// Provider authentication requirement.
    pub auth_requirement: AuthenticationRequirement,
    /// Mori Lens adapter supports progress reporting for downloads.
    /// Must be `false` until download adapter with progress parsing is implemented.
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
    ///
    /// Not yet implemented (P20B):
    /// - Download model: NO (POST /api/pull not yet implemented)
    /// - Load model: NO (Ollama auto-loads on inference anyway)
    /// - Unload model: NO (Ollama manages memory automatically)
    ///
    /// Auth: None (Ollama runs locally without auth)
    /// Progress: NO (not implemented yet; will flip after P20B)
    /// Cancel: NO (no documented cancel endpoint)
    pub fn ollama() -> Self {
        Self {
            can_quick: true,
            can_deep: true,
            can_list_models: true,
            can_download_model: false, // Not implemented in P20A
            can_load_model: false,
            can_unload_model: false,
            auth_requirement: AuthenticationRequirement::None,
            supports_progress: false, // Not implemented in P20A
            can_cancel_download: false,
        }
    }

    /// LM Studio capabilities as currently supported by Mori Lens.
    ///
    /// Implemented:
    /// - Quick (translation): YES
    /// - List models: YES (GET /api/v1/models)
    ///
    /// Not yet implemented (P20B):
    /// - Deep (analysis): NO (LM Studio does not support Deep)
    /// - Download model: NO (POST /api/v1/models/download not yet implemented)
    /// - Load model: NO (POST /api/v1/models/load not yet implemented)
    /// - Unload model: NO (POST /api/v1/models/unload not yet implemented)
    ///
    /// Auth: Optional (LM Studio can be configured to require API token)
    /// Progress: NO (not implemented yet; will flip after P20B)
    /// Cancel: NO (no documented cancel endpoint)
    pub fn lm_studio() -> Self {
        Self {
            can_quick: true,
            can_deep: false,
            can_list_models: true,
            can_download_model: false, // Not implemented in P20A
            can_load_model: false,     // Not implemented in P20A
            can_unload_model: false,   // Not implemented in P20A
            auth_requirement: AuthenticationRequirement::Optional,
            supports_progress: false, // Not implemented in P20A
            can_cancel_download: false,
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
            !caps.can_download_model,
            "Mori has NOT implemented Ollama download in P20A"
        );
        assert!(
            !caps.can_load_model,
            "Mori has NOT implemented Ollama load (not needed anyway)"
        );
        assert!(
            !caps.can_unload_model,
            "Mori has NOT implemented Ollama unload (not needed anyway)"
        );
        assert_eq!(
            caps.auth_requirement,
            AuthenticationRequirement::None,
            "Ollama does not require auth"
        );
        assert!(
            !caps.supports_progress,
            "Mori has NOT implemented Ollama progress in P20A"
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
            !caps.can_download_model,
            "Mori has NOT implemented LM Studio download in P20A"
        );
        assert!(
            !caps.can_load_model,
            "Mori has NOT implemented LM Studio load in P20A"
        );
        assert!(
            !caps.can_unload_model,
            "Mori has NOT implemented LM Studio unload in P20A"
        );
        assert_eq!(
            caps.auth_requirement,
            AuthenticationRequirement::Optional,
            "LM Studio can be configured to require auth"
        );
        assert!(
            !caps.supports_progress,
            "Mori has NOT implemented LM Studio progress in P20A"
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
        assert!(json.contains("\"can_download_model\":false"));
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
