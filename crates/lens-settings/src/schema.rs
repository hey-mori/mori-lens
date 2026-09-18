//! The version 2 settings schema.
//!
//! This is the structured representation of `settings.toml`. Each field
//! corresponds to a key in the file, grouped into sections. The schema
//! converts to and from [`lens_core::settings::Settings`], which is the
//! in-memory type the rest of the product uses.

use lens_core::policy::AppTier;

/// The current schema version, written at the top of every file this crate
/// produces.
pub const CURRENT_VERSION: u64 = 2;

/// The complete settings file, structured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    pub version: u64,
    pub general: General,
    pub quick: EngineConfig,
    pub deep: EngineConfig,
    pub applications: Vec<ApplicationConfig>,
}

impl Default for Schema {
    fn default() -> Self {
        let settings = lens_core::settings::Settings::default();
        Self::from_settings(&settings)
    }
}

/// General, non-engine settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct General {
    pub language: String,
    pub dwell_ms: u64,
    pub theme: String,
    pub translucency: String,
    pub text_size: String,
    pub default_tier: AppTier,
    pub ocr: String,
    pub profile: String,
    pub hover_scope: String,
    /// Whether persistent history is enabled (ADR-0014 Tier 3).
    pub history: bool,
    /// How many days to keep history entries. 0 means forever.
    pub history_days: u32,
    /// Whether to store the source application name in history entries.
    pub history_store_app: bool,
    /// Applications excluded from history recording.
    pub history_excluded: Vec<String>,
}

/// One engine's configuration: where it is, and which model to use.
///
/// `model` is `None` when the user chose `auto`. For Ollama, this means
/// the product picks the best installed model at startup. For LM Studio,
/// explicit model selection is required (the UI shows "Select a model"
/// instead of "Auto").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub engine: String,
    pub model: Option<String>,
    pub endpoint: String,
}

impl EngineConfig {
    /// The default engine configuration, matching what the product ships with.
    pub fn default_ollama() -> Self {
        let settings = lens_core::settings::Settings::default();
        Self {
            engine: "ollama".to_owned(),
            model: None,
            endpoint: settings.endpoint,
        }
    }
}

/// One application's configuration: what tier it has, and optionally an OCR override.
///
/// `ocr` is `None` when the application inherits the global OCR setting.
/// `Some(mode)` overrides the global setting for this application only.
///
/// The tier governs whether acquisition is allowed at all. OCR mode governs
/// what OCR behavior applies if acquisition reaches the OCR decision. These
/// are independent: an application with `tier = Allow` and `ocr = Off` allows
/// accessibility reading but never runs OCR, even if the global OCR is
/// `automatic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationConfig {
    /// The application name (process-name substring, case-insensitive).
    pub name: String,
    /// Whether acquisition is allowed: Allow, Modifier, or Deny.
    pub tier: AppTier,
    /// Optional per-app OCR override. `None` means inherit global OCR.
    pub ocr: Option<String>,
}

impl ApplicationConfig {
    /// Build an application config with no OCR override (inherits global).
    pub fn new(name: String, tier: AppTier) -> Self {
        Self {
            name,
            tier,
            ocr: None,
        }
    }

    /// Build an application config with an explicit OCR override.
    pub fn with_ocr(name: String, tier: AppTier, ocr: String) -> Self {
        Self {
            name,
            tier,
            ocr: Some(ocr),
        }
    }
}

impl Schema {
    /// Build a schema from the existing in-memory settings type.
    ///
    /// Both the quick and deep engine sections start with the same values,
    /// because the old format has only one model and one endpoint. The
    /// reader can separate them later from the settings window.
    pub fn from_settings(settings: &lens_core::settings::Settings) -> Self {
        let engine = EngineConfig {
            engine: "ollama".to_owned(),
            model: settings.model.clone(),
            endpoint: settings.endpoint.clone(),
        };
        Self {
            version: CURRENT_VERSION,
            general: General {
                language: settings.language.clone(),
                dwell_ms: settings.dwell_ms,
                theme: settings.theme.clone(),
                translucency: settings.translucency.clone(),
                text_size: settings.text.clone(),
                default_tier: settings.default_tier,
                ocr: settings.ocr.clone(),
                profile: settings.profile.clone(),
                hover_scope: settings.hover_scope.clone(),
                history: settings.history,
                history_days: settings.history_days,
                history_store_app: settings.history_store_app,
                history_excluded: settings.history_excluded.clone(),
            },
            quick: engine.clone(),
            deep: engine,
            applications: settings
                .applications
                .iter()
                .map(|(name, tier)| ApplicationConfig::new(name.clone(), *tier))
                .collect(),
        }
    }

    /// Convert the schema back to the in-memory settings type.
    ///
    /// The flat [`Settings`] view carries only the quick engine's
    /// endpoint and model. Callers that need per-lane configuration
    /// should use [`AiRuntimeConfig::from_schema`] instead
    /// (ADR-0005).
    ///
    /// Note: Per-app OCR overrides are NOT projected into the flat
    /// [`Settings`] view. Callers that need per-app OCR resolution
    /// should use the structured [`ApplicationConfig`] directly.
    pub fn to_settings(&self) -> lens_core::settings::Settings {
        lens_core::settings::Settings {
            language: self.general.language.clone(),
            model: self.quick.model.clone(),
            dwell_ms: self.general.dwell_ms,
            theme: self.general.theme.clone(),
            translucency: self.general.translucency.clone(),
            endpoint: self.quick.endpoint.clone(),
            ocr: self.general.ocr.clone(),
            profile: self.general.profile.clone(),
            text: self.general.text_size.clone(),
            hover_scope: self.general.hover_scope.clone(),
            history: self.general.history,
            history_days: self.general.history_days,
            history_store_app: self.general.history_store_app,
            history_excluded: self.general.history_excluded.clone(),
            default_tier: self.general.default_tier,
            applications: self
                .applications
                .iter()
                .map(|app| (app.name.clone(), app.tier))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_schema_matches_default_settings() {
        let schema = Schema::default();
        let settings = schema.to_settings();
        assert_eq!(settings, lens_core::settings::Settings::default());
    }

    #[test]
    fn from_settings_and_back_preserves_values() {
        let original = lens_core::settings::Settings {
            language: "Japanese".to_owned(),
            model: Some("gemma3:4b".to_owned()),
            dwell_ms: 250,
            theme: "night-lake".to_owned(),
            translucency: "clear".to_owned(),
            endpoint: "127.0.0.1:11434".to_owned(),
            ocr: "automatic".to_owned(),
            profile: "developer".to_owned(),
            text: "large".to_owned(),
            hover_scope: "word".to_owned(),
            default_tier: AppTier::Allow,
            applications: vec![("chrome".to_owned(), AppTier::Allow)],
            history: false,
            history_days: 30,
            history_store_app: false,
            history_excluded: vec![],
        };
        let schema = Schema::from_settings(&original);
        let round_tripped = schema.to_settings();
        assert_eq!(round_tripped, original);
    }
}
