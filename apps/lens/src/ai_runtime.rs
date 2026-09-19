//! AI runtime configuration for Quick and Deep translation paths.
//!
//! ADR-0005 requires Quick and Deep to have separate models, endpoints,
//! caches, and timeouts. This module provides the runtime configuration
//! layer that bridges the settings schema (which has separate `[quick]`
//! and `[deep]` sections) and the actual translator instances.

use lens_core::translate::Provider;
use lens_settings::schema::EngineConfig;

/// One engine's runtime configuration: which provider, where it is, and which model to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEngine {
    pub provider: Provider,
    pub endpoint: String,
    pub model: Option<String>,
}

impl RuntimeEngine {
    /// Build from a schema engine config.
    ///
    /// Unknown provider strings are logged and fall back to `Provider::Ollama`
    /// for safety. The settings boundary should validate provider tokens before
    /// this point for stricter behavior.
    pub fn from_schema(config: &EngineConfig) -> Self {
        let provider = match Provider::from_token(&config.engine) {
            Some(p) => p,
            None => {
                eprintln!(
                    "ai_runtime  unknown provider {:?}, falling back to Ollama",
                    config.engine
                );
                Provider::Ollama
            }
        };
        Self {
            provider,
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
        }
    }

    /// Build from a schema engine config for Deep lane.
    ///
    /// Deep is Ollama-only. Any non-Ollama provider is forced to Ollama,
    /// and the endpoint and model are reset to Ollama defaults.
    /// This enforces the Deep capability boundary at load time as a WHOLE-lane invariant.
    pub fn from_schema_deep(config: &EngineConfig) -> Self {
        // Deep is Ollama-only
        if config.engine == "ollama" {
            // Already Ollama, preserve all fields
            Self {
                provider: Provider::Ollama,
                endpoint: config.endpoint.clone(),
                model: config.model.clone(),
            }
        } else {
            // Not Ollama: force to Ollama and reset endpoint and model
            // Log the override for diagnostics
            eprintln!(
                "ai_runtime  Deep engine {:?} is not supported in Phase 3, forcing Ollama with defaults",
                config.engine
            );
            Self {
                provider: Provider::Ollama,
                endpoint: Provider::Ollama.default_endpoint().to_owned(),
                model: None,
            }
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
    ///
    /// Quick lane accepts both Ollama and LM Studio.
    /// Deep lane is Ollama-only.
    pub fn from_schema(schema: &lens_settings::schema::Schema) -> Self {
        Self {
            quick: RuntimeEngine::from_schema(&schema.quick),
            deep: RuntimeEngine::from_schema_deep(&schema.deep),
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
            shortcuts: if cfg!(target_os = "macos") {
                lens_core::shortcut::default_macos_shortcuts()
            } else {
                lens_core::shortcut::default_windows_shortcuts()
            },
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

        // Quick and Deep should preserve provider identity
        assert_eq!(config.quick.provider, Provider::Ollama);
        assert_eq!(config.deep.provider, Provider::Ollama);
    }

    #[test]
    fn deep_lmstudio_forced_to_ollama_at_load() {
        // Deep is Ollama-only. If schema has Deep=LM Studio,
        // AiRuntimeConfig::from_schema must force it to Ollama AND reset
        // endpoint and model to Ollama defaults (WHOLE-lane invariant).
        let mut schema = test_schema();
        schema.quick.engine = "ollama".to_owned();
        schema.deep.engine = "lmstudio".to_owned();
        schema.deep.endpoint = "127.0.0.1:1234".to_owned();
        schema.deep.model = Some("lm-studio-model".to_owned());

        let config = AiRuntimeConfig::from_schema(&schema);

        assert_eq!(config.quick.provider, Provider::Ollama);
        assert_eq!(
            config.deep.provider,
            Provider::Ollama,
            "Deep must be forced to Ollama"
        );
        assert_eq!(
            config.deep.endpoint, "127.0.0.1:11434",
            "Deep endpoint must be reset to Ollama default"
        );
        assert_eq!(
            config.deep.model, None,
            "Deep model must be reset to None (auto)"
        );
    }

    #[test]
    fn provider_combinations() {
        // Quick can be Ollama or LM Studio, but Deep is always Ollama.
        // Only 2 valid combinations exist:
        //   1. Quick=Ollama, Deep=Ollama
        //   2. Quick=LM Studio, Deep=Ollama
        // When Deep is forced to Ollama, endpoint and model must also be reset (WHOLE-lane).

        // Quick=Ollama, Deep=Ollama (valid)
        let mut schema = test_schema();
        schema.quick.engine = "ollama".to_owned();
        schema.deep.engine = "ollama".to_owned();
        let config = AiRuntimeConfig::from_schema(&schema);
        assert_eq!(config.quick.provider, Provider::Ollama);
        assert_eq!(config.deep.provider, Provider::Ollama);

        // Quick=LM Studio, Deep=Ollama (valid)
        schema.quick.engine = "lmstudio".to_owned();
        schema.deep.engine = "ollama".to_owned();
        let config = AiRuntimeConfig::from_schema(&schema);
        assert_eq!(config.quick.provider, Provider::LmStudio);
        assert_eq!(config.deep.provider, Provider::Ollama);

        // Quick=Ollama, Deep=LM Studio (invalid, forced to Ollama with defaults)
        schema.quick.engine = "ollama".to_owned();
        schema.deep.engine = "lmstudio".to_owned();
        schema.deep.endpoint = "127.0.0.1:1234".to_owned();
        schema.deep.model = Some("lm-model".to_owned());
        let config = AiRuntimeConfig::from_schema(&schema);
        assert_eq!(config.quick.provider, Provider::Ollama);
        assert_eq!(
            config.deep.provider,
            Provider::Ollama,
            "Deep LM Studio must be forced to Ollama"
        );
        assert_eq!(
            config.deep.endpoint, "127.0.0.1:11434",
            "Deep endpoint must reset to Ollama default"
        );
        assert_eq!(config.deep.model, None, "Deep model must reset to None");

        // Quick=LM Studio, Deep=LM Studio (invalid, forced to Ollama with defaults)
        schema.quick.engine = "lmstudio".to_owned();
        schema.deep.engine = "lmstudio".to_owned();
        schema.deep.endpoint = "127.0.0.1:1234".to_owned();
        schema.deep.model = Some("lm-model".to_owned());
        let config = AiRuntimeConfig::from_schema(&schema);
        assert_eq!(config.quick.provider, Provider::LmStudio);
        assert_eq!(
            config.deep.provider,
            Provider::Ollama,
            "Deep LM Studio must be forced to Ollama"
        );
        assert_eq!(
            config.deep.endpoint, "127.0.0.1:11434",
            "Deep endpoint must reset to Ollama default"
        );
        assert_eq!(config.deep.model, None, "Deep model must reset to None");
    }

    #[test]
    fn unknown_provider_falls_back_to_ollama() {
        let mut schema = test_schema();
        schema.quick.engine = "unknown".to_owned();
        schema.deep.engine = "invalid".to_owned();

        let config = AiRuntimeConfig::from_schema(&schema);

        // Unknown providers should fall back to Ollama for safety
        assert_eq!(config.quick.provider, Provider::Ollama);
        assert_eq!(config.deep.provider, Provider::Ollama);
    }

    #[test]
    fn lmstudio_quick_round_trip() {
        // Critical Phase 3 contract: Quick LM Studio must survive load/save/reload
        // without being silently converted to Ollama.
        let mut schema = test_schema();
        schema.quick.engine = "lmstudio".to_owned();
        schema.quick.model = Some("lm-model-7b".to_owned());
        schema.quick.endpoint = "127.0.0.1:1234".to_owned();
        schema.deep.engine = "ollama".to_owned();
        schema.deep.model = Some("llama3.1:8b".to_owned());
        schema.deep.endpoint = "127.0.0.1:11434".to_owned();

        let config = AiRuntimeConfig::from_schema(&schema);

        // Quick must preserve LM Studio identity
        assert_eq!(config.quick.provider, Provider::LmStudio);
        assert_eq!(config.quick.model, Some("lm-model-7b".to_owned()));
        assert_eq!(config.quick.endpoint, "127.0.0.1:1234");

        // Deep must preserve Ollama identity
        assert_eq!(config.deep.provider, Provider::Ollama);
        assert_eq!(config.deep.model, Some("llama3.1:8b".to_owned()));
        assert_eq!(config.deep.endpoint, "127.0.0.1:11434");

        // Quick and Deep must remain independent
        assert_ne!(config.quick.provider, config.deep.provider);
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
        use lens_core::translate::{Provider, QuickRequest};

        // Prove that the same request with different models produces different cache keys.
        // This is critical for ADR-0005: Quick and Deep must have separate cache namespaces.

        let quick_request = QuickRequest::new("Hello world", "Korean");
        let deep_request = DeepRequest::new("Hello world", "Hello 세계", "Korean");

        // Quick path: same request, different models -> different keys
        let key_q1 = CacheKey::of(&quick_request, Provider::Ollama, "qwen2.5:3b", 1);
        let key_q2 = CacheKey::of(&quick_request, Provider::Ollama, "llama3.1:8b", 1);
        assert_ne!(key_q1, key_q2, "Quick cache keys must differ by model");

        // Deep path: same request, different models -> different keys
        let key_d1 = CacheKey::for_analysis(&deep_request, Provider::Ollama, "qwen2.5:3b", 1);
        let key_d2 = CacheKey::for_analysis(&deep_request, Provider::Ollama, "llama3.1:8b", 1);
        assert_ne!(key_d1, key_d2, "Deep cache keys must differ by model");

        // Quick and Deep with same model still produce different keys (different request types)
        let key_quick = CacheKey::of(&quick_request, Provider::Ollama, "qwen2.5:3b", 1);
        let key_deep = CacheKey::for_analysis(&deep_request, Provider::Ollama, "qwen2.5:3b", 1);
        assert_ne!(
            key_quick, key_deep,
            "Quick and Deep cache keys must differ even with same model"
        );
    }
}
