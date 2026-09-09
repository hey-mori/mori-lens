//! AI runtime configuration for Quick and Deep translation paths.
//!
//! ADR-0005 requires Quick and Deep to have separate models, endpoints,
//! caches, and timeouts. This module provides the runtime configuration
//! layer that bridges the settings schema (which has separate `[quick]`
//! and `[deep]` sections) and the actual translator instances.

use lens_settings::schema::EngineConfig;

/// One engine's runtime configuration: where it is, and which model to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEngine {
    pub endpoint: String,
    pub model: Option<String>,
}

impl RuntimeEngine {
    /// Build from a schema engine config.
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
pub struct AiRuntimeConfig {
    pub quick: RuntimeEngine,
    pub deep: RuntimeEngine,
}

impl AiRuntimeConfig {
    /// Build from the settings schema.
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
    use lens_settings::schema::{General, Schema};
    use lens_core::policy::AppTier;

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
}
