//! AI runtime configuration for Quick and Deep translation paths.
//!
//! ADR-0005 requires Quick and Deep to have separate models, endpoints,
//! caches, and timeouts. This module provides the runtime configuration
//! layer that bridges the settings schema (which has separate `[quick]`
//! and `[deep]` sections) and the actual translator instances.

use lens_settings::schema::EngineConfig;

/// One engine's runtime configuration: where it is, and which model to use.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct RuntimeEngine {
    pub endpoint: String,
    pub model: Option<String>,
}

impl RuntimeEngine {
    /// Build from a schema engine config.
    #[allow(dead_code)]
    pub fn from_schema(config: &EngineConfig) -> Self {
        Self {
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
        }
    }
}

/// The complete AI runtime configuration for both translation paths.
///
/// Quick and Deep are separate per ADR-0005: separate models, separate
/// endpoints, separate caches, separate timeouts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct AiRuntimeConfig {
    pub quick: RuntimeEngine,
    pub deep: RuntimeEngine,
}

impl AiRuntimeConfig {
    /// Build from the settings schema.
    #[allow(dead_code)]
    pub fn from_schema(schema: &lens_settings::schema::Schema) -> Self {
        Self {
            quick: RuntimeEngine::from_schema(&schema.quick),
            deep: RuntimeEngine::from_schema(&schema.deep),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lens_core::policy::AppTier;
    use lens_settings::schema::{General, Schema};

    fn test_schema() -> Schema {
        Schema {
            version: 1,
            general: General {
                language: "Korean".to_owned(),
                dwell_ms: 320,
                theme: "system".to_owned(),
                translucency: "clear".to_owned(),
                text_size: "normal".to_owned(),
                default_tier: AppTier::Modifier,
                ocr: "manual".to_owned(),
                profile: "reader".to_owned(),
                hover_scope: "sentence".to_owned(),
                history: false,
                history_days: 30,
                history_store_app: false,
                history_excluded: Vec::new(),
            },
            quick: EngineConfig {
                engine: "ollama".to_owned(),
                model: Some("qwen2.5:3b".to_owned()),
                endpoint: "127.0.0.1:11434".to_owned(),
            },
            deep: EngineConfig {
                engine: "ollama".to_owned(),
                model: Some("llama3.1:8b".to_owned()),
                endpoint: "127.0.0.1:11435".to_owned(),
            },
            applications: Vec::new(),
        }
    }

    #[test]
    fn quick_and_deep_are_separate() {
        let schema = test_schema();
        let config = AiRuntimeConfig::from_schema(&schema);

        // Quick and Deep should have different models
        assert_ne!(config.quick.model, config.deep.model);
        assert_eq!(config.quick.model, Some("qwen2.5:3b".to_owned()));
        assert_eq!(config.deep.model, Some("llama3.1:8b".to_owned()));

        // Quick and Deep should have different endpoints
        assert_ne!(config.quick.endpoint, config.deep.endpoint);
        assert_eq!(config.quick.endpoint, "127.0.0.1:11434");
        assert_eq!(config.deep.endpoint, "127.0.0.1:11435");
    }

    #[test]
    fn auto_model_is_none() {
        let mut schema = test_schema();
        schema.quick.model = None;
        schema.deep.model = None;

        let config = AiRuntimeConfig::from_schema(&schema);

        assert_eq!(config.quick.model, None);
        assert_eq!(config.deep.model, None);
    }

    #[test]
    fn cache_keys_differ_by_model() {
        use lens_core::analyse::DeepRequest;
        use lens_core::cache::CacheKey;
        use lens_core::translate::QuickRequest;

        // Prove that the same request with different models produces different cache keys.
        // This is critical for ADR-0005: Quick and Deep must have separate cache namespaces.

        let quick_request = QuickRequest::new("Hello world", "Korean");
        let deep_request = DeepRequest::new("Hello world", "Hello 세계", "Korean");

        // Quick path: same request, different models -> different keys
        let key_q1 = CacheKey::of(&quick_request, "qwen2.5:3b", 1);
        let key_q2 = CacheKey::of(&quick_request, "llama3.1:8b", 1);
        assert_ne!(key_q1, key_q2, "Quick cache keys must differ by model");

        // Deep path: same request, different models -> different keys
        let key_d1 = CacheKey::for_analysis(&deep_request, "qwen2.5:3b", 1);
        let key_d2 = CacheKey::for_analysis(&deep_request, "llama3.1:8b", 1);
        assert_ne!(key_d1, key_d2, "Deep cache keys must differ by model");

        // Quick and Deep with same model still produce different keys (different request types)
        let key_quick = CacheKey::of(&quick_request, "qwen2.5:3b", 1);
        let key_deep = CacheKey::for_analysis(&deep_request, "qwen2.5:3b", 1);
        assert_ne!(
            key_quick, key_deep,
            "Quick and Deep cache keys must differ even with same model"
        );
    }
}
