//! Provider capability descriptors.
//!
//! Each provider (Ollama, LM Studio, etc.) has a set of capabilities
//! that determine what operations are supported.

use serde::{Deserialize, Serialize};

/// Provider capability descriptor.
///
/// Describes what operations a provider supports for Quick and Deep engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapability {
    /// Provider can serve Quick translation requests.
    pub can_quick: bool,
    /// Provider can serve Deep analysis requests.
    pub can_deep: bool,
    /// Provider supports listing available models.
    pub can_list_models: bool,
    /// Provider supports downloading models.
    pub can_download_model: bool,
    /// Provider supports loading models into memory.
    pub can_load_model: bool,
    /// Provider supports unloading models from memory.
    pub can_unload_model: bool,
    /// Provider requires authentication (API key or token).
    pub requires_auth: bool,
    /// Provider supports progress reporting for downloads.
    pub supports_progress: bool,
}

impl ProviderCapability {
    /// Ollama capabilities.
    ///
    /// Ollama supports both Quick and Deep, can list/download/load/unload models,
    /// does not require auth, and supports progress reporting.
    pub fn ollama() -> Self {
        Self {
            can_quick: true,
            can_deep: true,
            can_list_models: true,
            can_download_model: true,
            can_load_model: true,
            can_unload_model: true,
            requires_auth: false,
            supports_progress: true,
        }
    }

    /// LM Studio capabilities.
    ///
    /// LM Studio supports Quick only, can list models, does not support
    /// downloading/loading/unloading via API, does not require auth,
    /// and does not support progress reporting.
    pub fn lm_studio() -> Self {
        Self {
            can_quick: true,
            can_deep: false,
            can_list_models: true,
            can_download_model: false,
            can_load_model: false,
            can_unload_model: false,
            requires_auth: false,
            supports_progress: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_capabilities_are_correct() {
        let caps = ProviderCapability::ollama();
        assert!(caps.can_quick);
        assert!(caps.can_deep);
        assert!(caps.can_list_models);
        assert!(caps.can_download_model);
        assert!(caps.can_load_model);
        assert!(caps.can_unload_model);
        assert!(!caps.requires_auth);
        assert!(caps.supports_progress);
    }

    #[test]
    fn lm_studio_capabilities_are_correct() {
        let caps = ProviderCapability::lm_studio();
        assert!(caps.can_quick);
        assert!(!caps.can_deep);
        assert!(caps.can_list_models);
        assert!(!caps.can_download_model);
        assert!(!caps.can_load_model);
        assert!(!caps.can_unload_model);
        assert!(!caps.requires_auth);
        assert!(!caps.supports_progress);
    }

    #[test]
    fn capabilities_serialize_to_json() {
        let caps = ProviderCapability::ollama();
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("\"can_quick\":true"));
        assert!(json.contains("\"can_deep\":true"));
    }
}
