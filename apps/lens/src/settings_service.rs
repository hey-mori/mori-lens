//! Settings service: runtime owner of settings.toml.
//!
//! This module owns the runtime settings state, accessed by Tauri commands
//! and the polling loop. The canonical file-lifecycle logic (loading,
//! migration, initialization) lives in [`lens_settings::load_or_initialize`].
//!
//! The existing `refresh::Watcher` watches for *external* edits (a user
//! editing the file in a text editor). When it detects one, it re-parses
//! and the polling loop applies it.

use std::sync::{Arc, Mutex};

use lens_core::policy::AppTier;
use lens_core::settings::Settings;
use lens_settings::schema::EngineConfig;
#[cfg(target_os = "macos")]
use lens_settings::schema::Schema;
use lens_settings::SettingsFile;
use serde::{Deserialize, Serialize};

/// The active engine settings at startup, used to detect restart-required changes.
///
/// These fields are captured from the initial settings and compared against
/// saved settings to determine if a restart is needed.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveEngineSettings {
    pub quick_engine: String,
    pub quick_model: Option<String>,
    pub quick_endpoint: String,
    pub deep_engine: String,
    pub deep_model: Option<String>,
    pub deep_endpoint: String,
    pub ocr_mode: String,
    pub shortcuts: lens_core::shortcut::ShortcutConfig,
}

/// The outcome of saving settings, indicating whether a restart is required.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsSaveOutcome {
    pub restart_required: bool,
    pub reasons: Vec<String>,
    pub settings: SettingsView,
}

/// Per-application configuration for IPC/SettingsView.
///
/// Structured DTO replacing the tuple `[name, tier]` representation.
/// Supports optional per-app OCR override (Phase 1: backend only, no UI).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationConfigDto {
    /// Application name (process-name substring, case-insensitive).
    pub name: String,
    /// Tier: "allow", "modifier", or "deny".
    pub tier: String,
    /// Optional OCR override: null (inherit global), "off", "manual", or "automatic".
    pub ocr: Option<String>,
}

/// A single shortcut binding for the webview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutBindingDto {
    pub action: String,
    pub shortcut: Option<String>,
}

/// Shortcut configuration for the webview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutsView {
    pub bindings: Vec<ShortcutBindingDto>,
}

/// Shared settings state, accessed by Tauri commands and the polling loop.
pub type SharedSettings = Arc<Mutex<SettingsState>>;

/// The service's state: the file handle, current settings, and a pending slot.
///
/// When a Tauri command saves new settings, it writes to disk and sets
/// `pending`. The polling loop checks `pending` each tick; if set, it
/// applies the new settings through the same path as an external edit.
pub struct SettingsState {
    /// The settings file, kept for round-trip preservation.
    file: SettingsFile,
    /// The current in-memory settings, derived from the file.
    current: Settings,
    /// Settings waiting to be applied by the polling loop.
    /// Set by `save_settings`, cleared after application.
    pending: Option<Settings>,
    /// Monotonically increasing counter. Incremented on every `apply`.
    /// The product loop checks this to avoid cloning Settings every tick.
    generation: u64,
    /// The engine settings at startup, used to detect restart-required changes.
    active_baseline: ActiveEngineSettings,
}

/// A serde-friendly view of the settings, for IPC with the webview.
///
/// This is separate from [`Schema`] because the webview needs flat
/// strings and numbers, not the typed Rust enums.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsView {
    pub language: String,
    pub dwell_ms: u64,
    pub theme: String,
    pub translucency: String,
    pub text_size: String,
    pub default_tier: String,
    pub ocr: String,
    pub profile: String,
    pub hover_scope: String,
    pub history: bool,
    pub history_days: u32,
    pub history_store_app: bool,
    pub history_excluded: Vec<String>,
    pub quick_engine: String,
    pub quick_model: Option<String>,
    pub quick_endpoint: String,
    pub deep_engine: String,
    pub deep_model: Option<String>,
    pub deep_endpoint: String,
    /// Per-application configuration (structured, replacing tuple representation).
    pub applications: Vec<ApplicationConfigDto>,
    /// Global keyboard shortcuts.
    pub shortcuts: ShortcutsView,
}

/// Load or migrate the settings file.
///
/// This uses the canonical [`lens_settings::load_or_initialize`] which
/// handles all file-lifecycle decisions: loading valid files, migrating
/// legacy format, creating defaults on first run, and preserving invalid
/// files without destructive overwrites.
///
/// # Recovery
///
/// A corrupt or future-version file is preserved without modification.
/// The reader's own configuration is not lost.
pub fn load_or_migrate(
    settings_path: &std::path::Path,
    active_baseline: ActiveEngineSettings,
) -> SettingsState {
    let toml_path = settings_path.to_path_buf();

    // Use canonical loader
    match lens_settings::load_or_initialize(&toml_path) {
        Ok((file, disposition)) => {
            let current = file.to_settings();

            // Report what happened
            match disposition {
                lens_settings::LoadDisposition::Loaded => {
                    // Silent success
                }
                lens_settings::LoadDisposition::Created => {
                    println!("settings  created defaults at {toml_path:?}");
                }
                lens_settings::LoadDisposition::Migrated { problems } => {
                    println!("settings  migrated to canonical format");
                    for problem in &problems {
                        eprintln!("settings  migration note: {}", problem.message);
                    }
                }
                lens_settings::LoadDisposition::SchemaVersionMigrated {
                    from_version,
                    to_version,
                } => {
                    println!("settings  migrated schema v{from_version} to v{to_version}");
                }
                lens_settings::LoadDisposition::InvalidPreserved { reason } => {
                    eprintln!("settings  {reason}");
                }
            }

            SettingsState {
                file,
                current,
                pending: None,
                generation: 0,
                active_baseline,
            }
        }
        Err(e) => {
            // IO error: use defaults
            eprintln!("settings  cannot load {toml_path:?}: {e}");
            let file = lens_settings::SettingsFile::new_defaults(toml_path);
            let current = file.to_settings();
            SettingsState {
                file,
                current,
                pending: None,
                generation: 0,
                active_baseline,
            }
        }
    }
}

impl SettingsState {
    /// Get the current settings view for the webview.
    pub fn view(&self) -> SettingsView {
        let schema = self.file.schema();
        SettingsView {
            language: schema.general.language.clone(),
            dwell_ms: schema.general.dwell_ms,
            theme: schema.general.theme.clone(),
            translucency: schema.general.translucency.clone(),
            text_size: schema.general.text_size.clone(),
            default_tier: tier_to_string(&schema.general.default_tier),
            ocr: schema.general.ocr.clone(),
            profile: schema.general.profile.clone(),
            hover_scope: schema.general.hover_scope.clone(),
            history: schema.general.history,
            history_days: schema.general.history_days,
            history_store_app: schema.general.history_store_app,
            history_excluded: schema.general.history_excluded.clone(),
            quick_engine: schema.quick.engine.clone(),
            quick_model: schema.quick.model.clone(),
            quick_endpoint: schema.quick.endpoint.clone(),
            deep_engine: schema.deep.engine.clone(),
            deep_model: schema.deep.model.clone(),
            deep_endpoint: schema.deep.endpoint.clone(),
            applications: schema
                .applications
                .iter()
                .map(|app| ApplicationConfigDto {
                    name: app.name.clone(),
                    tier: tier_to_string(&app.tier),
                    ocr: app.ocr.clone(),
                })
                .collect(),
            shortcuts: ShortcutsView {
                bindings: schema
                    .shortcuts
                    .bindings
                    .iter()
                    .map(|(action, shortcut)| ShortcutBindingDto {
                        action: action.as_str().to_string(),
                        shortcut: shortcut.map(|s| s.to_string()),
                    })
                    .collect(),
            },
        }
    }

    /// Apply a settings view from the webview, save to disk, and set pending.
    ///
    /// String enum fields are validated before writing. An invalid value is
    /// replaced with the default and a warning is printed, because a setting
    /// the user cannot see being ignored is worse than one that is refused.
    ///
    /// Engine changes clear the model (models are provider-specific) and reset
    /// the endpoint to the provider default.
    ///
    /// Returns a [`SettingsSaveOutcome`] indicating whether a restart is required.
    /// A restart is required if any engine-related settings (model, endpoint, OCR mode)
    /// differ from the active baseline captured at startup.
    ///
    /// # Transactionality
    ///
    /// The entire apply operation is atomic: if any validation fails (including
    /// shortcut validation), no part of the schema is mutated and the disk file
    /// remains unchanged. This is achieved by building a complete candidate schema,
    /// validating it, and only then committing it.
    pub fn apply(&mut self, view: SettingsView) -> Result<SettingsSaveOutcome, String> {
        let defaults = lens_core::settings::Settings::default();

        // Build a complete candidate schema without mutating the current one.
        // This ensures transactionality: if validation fails, nothing changes.
        let mut candidate = self.file.schema().clone();

        // Capture old engine values before modification to detect provider changes.
        let old_quick_engine = candidate.quick.engine.clone();
        let old_deep_engine = candidate.deep.engine.clone();

        candidate.general.language = view.language;
        candidate.general.dwell_ms = view.dwell_ms.clamp(80, 3000);
        candidate.general.theme =
            validate_or_default(&view.theme, is_valid_theme, &defaults.theme, "theme");
        candidate.general.translucency = validate_or_default(
            &view.translucency,
            is_valid_translucency,
            &defaults.translucency,
            "translucency",
        );
        candidate.general.text_size = validate_or_default(
            &view.text_size,
            is_valid_text_size,
            &defaults.text,
            "text_size",
        );
        candidate.general.default_tier =
            parse_tier(&view.default_tier).unwrap_or(defaults.default_tier);
        candidate.general.ocr = validate_or_default(&view.ocr, is_valid_ocr, &defaults.ocr, "ocr");
        candidate.general.profile = validate_or_default(
            &view.profile,
            is_valid_profile,
            &defaults.profile,
            "profile",
        );
        candidate.general.hover_scope = validate_or_default(
            &view.hover_scope,
            is_valid_hover_scope,
            &defaults.hover_scope,
            "hover_scope",
        );
        candidate.general.history = view.history;
        candidate.general.history_days = if [0, 1, 7, 30, 90].contains(&view.history_days) {
            view.history_days
        } else {
            defaults.history_days
        };
        candidate.general.history_store_app = view.history_store_app;
        candidate.general.history_excluded = view.history_excluded;

        let new_quick_engine = validate_quick_engine(&view.quick_engine);
        let quick_engine_changed = new_quick_engine != old_quick_engine;
        candidate.quick = EngineConfig {
            engine: new_quick_engine.clone(),
            model: if quick_engine_changed {
                None // Clear model when provider changes
            } else {
                view.quick_model
            },
            endpoint: if quick_engine_changed {
                // Preserve custom endpoint if user has entered one
                let current_endpoint = &view.quick_endpoint;
                let old_default = default_endpoint_for_engine(&old_quick_engine);
                if current_endpoint == old_default {
                    // User hasn't customized endpoint, reset to new provider's default
                    default_endpoint_for_engine(&new_quick_engine).to_owned()
                } else {
                    // User has custom endpoint, but still validate it (privacy constraint)
                    validate_endpoint(
                        current_endpoint,
                        default_endpoint_for_engine(&new_quick_engine),
                        "quick_endpoint",
                    )
                }
            } else {
                validate_endpoint(&view.quick_endpoint, &defaults.endpoint, "quick_endpoint")
            },
        };

        let new_deep_engine = validate_deep_engine(&view.deep_engine);
        let deep_engine_changed = new_deep_engine != old_deep_engine;
        candidate.deep = EngineConfig {
            engine: new_deep_engine.clone(),
            model: if deep_engine_changed {
                None // Clear model when provider changes
            } else {
                view.deep_model
            },
            endpoint: if deep_engine_changed {
                // Preserve custom endpoint if user has entered one
                let current_endpoint = &view.deep_endpoint;
                let old_default = default_endpoint_for_engine(&old_deep_engine);
                if current_endpoint == old_default {
                    // User hasn't customized endpoint, reset to new provider's default
                    default_endpoint_for_engine(&new_deep_engine).to_owned()
                } else {
                    // User has custom endpoint, but still validate it (privacy constraint)
                    validate_endpoint(
                        current_endpoint,
                        default_endpoint_for_engine(&new_deep_engine),
                        "deep_endpoint",
                    )
                }
            } else {
                validate_endpoint(&view.deep_endpoint, &defaults.endpoint, "deep_endpoint")
            },
        };

        candidate.applications = view
            .applications
            .into_iter()
            .filter_map(|app_dto| {
                // Skip empty application names
                if app_dto.name.trim().is_empty() {
                    return None;
                }
                // Parse tier, skip if invalid (fail-closed)
                let tier = parse_tier(&app_dto.tier)?;
                // Validate OCR override if present
                let ocr = app_dto
                    .ocr
                    .filter(|s| lens_core::session::OcrMode::named(s).is_some());
                Some(lens_settings::schema::ApplicationConfig {
                    name: app_dto.name,
                    tier,
                    ocr,
                })
            })
            .collect();

        // Apply shortcuts - validate before mutating
        let mut new_bindings = Vec::new();
        for binding_dto in &view.shortcuts.bindings {
            let action = match binding_dto.action.as_str() {
                "pause" => lens_core::shortcut::ShortcutAction::Pause,
                "manual_ocr" => lens_core::shortcut::ShortcutAction::ManualOcr,
                "deep" => lens_core::shortcut::ShortcutAction::Deep,
                "copy" => lens_core::shortcut::ShortcutAction::Copy,
                "save" => lens_core::shortcut::ShortcutAction::Save,
                "pin" => lens_core::shortcut::ShortcutAction::Pin,
                unknown => {
                    return Err(format!("unknown shortcut action: {unknown}"));
                }
            };
            let shortcut = match &binding_dto.shortcut {
                Some(s) if !s.trim().is_empty() => {
                    match s.parse::<lens_core::shortcut::Shortcut>() {
                        Ok(parsed) => Some(parsed),
                        Err(_) => {
                            return Err(format!(
                                "invalid shortcut for {}: {}",
                                binding_dto.action, s
                            ));
                        }
                    }
                }
                _ => None,
            };
            new_bindings.push((action, shortcut));
        }

        // Build candidate shortcuts config and validate before committing
        let candidate_shortcuts = lens_core::shortcut::ShortcutConfig::new(new_bindings);
        if let Err(errors) = candidate_shortcuts.validate() {
            let messages: Vec<String> = errors
                .iter()
                .map(|e| match e {
                    lens_core::shortcut::ShortcutValidationError::RequiredActionUnbound { action } => {
                        format!("{} must have a shortcut", action)
                    }
                    lens_core::shortcut::ShortcutValidationError::CannotBeUnbound { action } => {
                        format!("{} cannot be unbound", action)
                    }
                    lens_core::shortcut::ShortcutValidationError::DuplicateBinding { action1, action2 } => {
                        format!("{} and {} use the same shortcut", action1, action2)
                    }
                    lens_core::shortcut::ShortcutValidationError::InsufficientModifiers => {
                        "global shortcuts require Ctrl/Alt/Meta".to_string()
                    }
                })
                .collect();
            return Err(messages.join("; "));
        }

        candidate.shortcuts = candidate_shortcuts;

        // All validation passed. Now commit the candidate schema.
        *self.file.schema_mut() = candidate;

        if let Err(e) = self.file.save() {
            return Err(format!("save failed: {e}"));
        }

        self.current = self.file.to_settings();
        self.pending = Some(self.current.clone());
        self.generation += 1;

        // Determine if restart is required by comparing against active baseline.
        let (restart_required, reasons) = self.restart_status();

        let canonical_settings = self.view();

        Ok(SettingsSaveOutcome {
            restart_required,
            reasons,
            settings: canonical_settings,
        })
    }

    /// Determine if a restart is required by comparing current settings against
    /// the active baseline (process-start settings).
    ///
    /// Returns a tuple of (restart_required, reasons) where reasons lists the
    /// specific settings that changed.
    pub fn restart_status(&self) -> (bool, Vec<String>) {
        let saved_schema = self.file.schema();
        let mut restart_required = false;
        let mut reasons = Vec::new();

        if saved_schema.quick.engine != self.active_baseline.quick_engine {
            restart_required = true;
            reasons.push("quick_engine".to_owned());
        }
        if saved_schema.quick.model != self.active_baseline.quick_model {
            restart_required = true;
            reasons.push("quick_model".to_owned());
        }
        if saved_schema.quick.endpoint != self.active_baseline.quick_endpoint {
            restart_required = true;
            reasons.push("quick_endpoint".to_owned());
        }
        if saved_schema.deep.engine != self.active_baseline.deep_engine {
            restart_required = true;
            reasons.push("deep_engine".to_owned());
        }
        if saved_schema.deep.model != self.active_baseline.deep_model {
            restart_required = true;
            reasons.push("deep_model".to_owned());
        }
        if saved_schema.deep.endpoint != self.active_baseline.deep_endpoint {
            restart_required = true;
            reasons.push("deep_endpoint".to_owned());
        }
        if saved_schema.general.ocr != self.active_baseline.ocr_mode {
            restart_required = true;
            reasons.push("ocr_mode".to_owned());
        }
        if saved_schema.shortcuts != self.active_baseline.shortcuts {
            restart_required = true;
            reasons.push("shortcuts".to_owned());
        }

        (restart_required, reasons)
    }

    /// Take any pending settings, clearing the slot.
    #[allow(dead_code)]
    pub fn take_pending(&mut self) -> Option<Settings> {
        self.pending.take()
    }

    /// The current settings generation. Increments on every `apply`.
    /// The product loop checks this to avoid cloning Settings every tick.
    #[allow(dead_code)] // Used by macOS; Windows uses `current()` directly.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The current in-memory settings.
    pub fn current(&self) -> &Settings {
        &self.current
    }

    /// The structured schema, with separate `[quick]` and `[deep]` engine
    /// sections. Used by macOS for per-lane configuration (ADR-0005).
    #[cfg(target_os = "macos")]
    pub fn schema(&self) -> &Schema {
        self.file.schema()
    }
}

fn tier_to_string(tier: &AppTier) -> String {
    match tier {
        AppTier::Allow => "allow".to_owned(),
        AppTier::Modifier => "modifier".to_owned(),
        AppTier::Deny => "deny".to_owned(),
    }
}

fn parse_tier(s: &str) -> Option<AppTier> {
    match s {
        "allow" => Some(AppTier::Allow),
        "modifier" => Some(AppTier::Modifier),
        "deny" => Some(AppTier::Deny),
        _ => None,
    }
}

fn validate_or_default(
    value: &str,
    is_valid: fn(&str) -> bool,
    default: &str,
    field_name: &str,
) -> String {
    if is_valid(value) {
        value.to_owned()
    } else {
        eprintln!("settings  {field_name} = {value} is not valid, using {default}");
        default.to_owned()
    }
}

fn is_valid_theme(s: &str) -> bool {
    matches!(s, "system" | "forest" | "night-lake" | "paper")
}

fn is_valid_translucency(s: &str) -> bool {
    matches!(s, "clear" | "balanced" | "solid")
}

fn is_valid_text_size(s: &str) -> bool {
    matches!(s, "small" | "normal" | "large" | "larger")
}

fn is_valid_ocr(s: &str) -> bool {
    matches!(s, "off" | "manual" | "automatic")
}

fn is_valid_profile(s: &str) -> bool {
    lens_core::analyse::Profile::named(s).is_some()
}

fn is_valid_hover_scope(s: &str) -> bool {
    matches!(s, "sentence" | "word")
}

fn validate_quick_engine(value: &str) -> String {
    if matches!(value, "ollama" | "lmstudio") {
        value.to_owned()
    } else {
        eprintln!("settings  quick_engine = {value:?} is not recognized, using \"ollama\"");
        "ollama".to_owned()
    }
}

fn validate_deep_engine(value: &str) -> String {
    if value == "ollama" {
        value.to_owned()
    } else {
        eprintln!(
            "settings  deep_engine = {value:?} is not supported in Phase 3, using \"ollama\""
        );
        "ollama".to_owned()
    }
}

fn default_endpoint_for_engine(engine: &str) -> &'static str {
    match engine {
        "lmstudio" => "127.0.0.1:1234",
        _ => "127.0.0.1:11434", // Ollama is the default
    }
}

fn validate_endpoint(value: &str, default: &str, field_name: &str) -> String {
    if lens_core::translate::engine_for(value) == lens_core::translate::EngineId::Local {
        value.to_owned()
    } else {
        eprintln!("settings  {field_name} = {value} is not a loopback address, using {default}");
        default.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("lens-settings-test-{}-{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn default_view() -> SettingsView {
        SettingsView {
            language: "Korean".into(),
            dwell_ms: 320,
            theme: "forest".into(),
            translucency: "clear".into(),
            text_size: "normal".into(),
            default_tier: "allow".into(),
            ocr: "manual".into(),
            profile: "reader".into(),
            hover_scope: "sentence".into(),
            history: false,
            history_days: 30,
            history_store_app: false,
            history_excluded: vec![],
            quick_engine: "local".into(),
            quick_model: None,
            quick_endpoint: "127.0.0.1:11434".into(),
            deep_engine: "local".into(),
            deep_model: None,
            deep_endpoint: "127.0.0.1:11434".into(),
            applications: vec![],
            shortcuts: ShortcutsView {
                bindings: vec![
                    ShortcutBindingDto {
                        action: "pause".into(),
                        shortcut: Some("Ctrl+Alt+L".into()),
                    },
                    ShortcutBindingDto {
                        action: "manual_ocr".into(),
                        shortcut: Some("Ctrl+Alt+O".into()),
                    },
                    ShortcutBindingDto {
                        action: "deep".into(),
                        shortcut: Some("Ctrl+Alt+D".into()),
                    },
                    ShortcutBindingDto {
                        action: "copy".into(),
                        shortcut: Some("Ctrl+Alt+C".into()),
                    },
                    ShortcutBindingDto {
                        action: "save".into(),
                        shortcut: Some("Ctrl+Alt+S".into()),
                    },
                    ShortcutBindingDto {
                        action: "pin".into(),
                        shortcut: Some("Ctrl+Alt+P".into()),
                    },
                ],
            },
        }
    }

    fn default_baseline() -> ActiveEngineSettings {
        ActiveEngineSettings {
            quick_engine: "ollama".into(),
            quick_model: None,
            quick_endpoint: "127.0.0.1:11434".into(),
            deep_engine: "ollama".into(),
            deep_model: None,
            deep_endpoint: "127.0.0.1:11434".into(),
            ocr_mode: "manual".into(),
            shortcuts: lens_core::shortcut::default_windows_shortcuts(),
        }
    }

    #[test]
    fn load_or_migrate_creates_defaults_when_no_file() {
        let dir = temp_dir();
        let state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        assert!(state.pending.is_none());
        let view = state.view();
        assert_eq!(view.language, "Korean");
        assert_eq!(view.dwell_ms, 320);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_migrate_loads_existing_file() {
        let dir = temp_dir();
        // Create defaults first.
        let _ = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        // Load again — should read the file.
        let state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        assert_eq!(state.view().language, "Korean");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_sets_pending_and_persists() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        assert!(state.take_pending().is_none());

        let mut view = default_view();
        view.language = "Japanese".into();
        view.dwell_ms = 500;
        state.apply(view).unwrap();

        // Pending is set after apply.
        let pending = state.take_pending().unwrap();
        assert_eq!(pending.language, "Japanese");
        assert_eq!(pending.dwell_ms, 500);

        // Second take returns None.
        assert!(state.take_pending().is_none());

        // Current reflects the applied settings.
        assert_eq!(state.current().language, "Japanese");

        // File persists: reload and check.
        let reloaded = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        assert_eq!(reloaded.current().language, "Japanese");
        assert_eq!(reloaded.current().dwell_ms, 500);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_clamps_dwell_ms() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.dwell_ms = 10; // Below minimum (80).
        state.apply(view).unwrap();
        assert_eq!(state.current().dwell_ms, 80);

        let mut view = default_view();
        view.dwell_ms = 9999; // Above maximum (3000).
        state.apply(view).unwrap();
        assert_eq!(state.current().dwell_ms, 3000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_tier_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.default_tier = "bogus".into();
        state.apply(view).unwrap();
        // Invalid tier falls back to the Settings default.
        assert_eq!(
            state.current().default_tier,
            Settings::default().default_tier
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn view_roundtrip_preserves_values() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.language = "Spanish".into();
        view.hover_scope = "word".into();
        view.history = true;
        view.history_days = 90;
        view.applications = vec![ApplicationConfigDto {
            name: "Safari".into(),
            tier: "deny".into(),
            ocr: None,
        }];
        state.apply(view.clone()).unwrap();

        let read_back = state.view();
        assert_eq!(read_back.language, "Spanish");
        assert_eq!(read_back.hover_scope, "word");
        assert!(read_back.history);
        assert_eq!(read_back.history_days, 90);
        assert_eq!(
            read_back.applications,
            vec![ApplicationConfigDto {
                name: "Safari".into(),
                tier: "deny".into(),
                ocr: None,
            }]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_theme_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.theme = "purple".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().theme, Settings::default().theme);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_translucency_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.translucency = "invisible".into();
        state.apply(view).unwrap();
        assert_eq!(
            state.current().translucency,
            Settings::default().translucency
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_text_size_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.text_size = "huge".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().text, Settings::default().text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_ocr_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.ocr = "always".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().ocr, Settings::default().ocr);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_hover_scope_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.hover_scope = "paragraph".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().hover_scope, Settings::default().hover_scope);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_endpoint_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_endpoint = "192.168.1.100:11434".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().endpoint, Settings::default().endpoint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_valid_enum_values_are_kept() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.theme = "night-lake".into();
        view.translucency = "solid".into();
        view.text_size = "larger".into();
        view.ocr = "automatic".into();
        view.profile = "developer".into();
        view.hover_scope = "word".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().theme, "night-lake");
        assert_eq!(state.current().translucency, "solid");
        assert_eq!(state.current().text, "larger");
        assert_eq!(state.current().ocr, "automatic");
        assert_eq!(state.current().profile, "developer");
        assert_eq!(state.current().hover_scope, "word");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_migrate_respects_custom_settings_file_name() {
        // Regression test: --settings custom-profile.toml should NOT rename to settings.toml
        let dir = temp_dir();
        let custom_path = dir.join("custom-profile.toml");

        // Load from custom path
        let state = load_or_migrate(&custom_path, default_baseline());
        assert_eq!(state.view().language, "Korean");

        // Verify the file was created with the custom name, not "settings.toml"
        assert!(custom_path.exists(), "custom-profile.toml should exist");
        assert!(
            !dir.join("settings.toml").exists(),
            "settings.toml should NOT be created when custom name is used"
        );

        // Modify and save — should write to custom path
        let mut view = default_view();
        view.language = "Japanese".into();
        let mut state = state;
        state.apply(view).unwrap();

        // Reload from the same custom path — should see the change
        let reloaded = load_or_migrate(&custom_path, default_baseline());
        assert_eq!(reloaded.view().language, "Japanese");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_no_restart_required_when_engine_settings_unchanged() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        // Change only non-engine settings
        view.language = "Japanese".into();
        view.dwell_ms = 500;
        view.theme = "night-lake".into();
        let outcome = state.apply(view).unwrap();
        assert!(!outcome.restart_required);
        assert!(outcome.reasons.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restart_required_when_quick_model_changed() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_model = Some("llama2".into());
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert!(outcome.reasons.contains(&"quick_model".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restart_required_when_quick_endpoint_changed() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_endpoint = "127.0.0.1:11435".into();
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert!(outcome.reasons.contains(&"quick_endpoint".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restart_required_when_deep_model_changed() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.deep_model = Some("codellama".into());
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert!(outcome.reasons.contains(&"deep_model".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restart_required_when_deep_endpoint_changed() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.deep_endpoint = "127.0.0.1:11436".into();
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert!(outcome.reasons.contains(&"deep_endpoint".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restart_required_when_ocr_mode_changed() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.ocr = "automatic".into();
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert!(outcome.reasons.contains(&"ocr_mode".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restart_status_returns_same_result_as_apply() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_model = Some("llama2".into());
        view.deep_endpoint = "127.0.0.1:11436".into();

        // Apply the settings
        let outcome = state.apply(view.clone()).unwrap();

        // Query restart_status() and verify it matches
        let (restart_required, reasons) = state.restart_status();
        assert_eq!(restart_required, outcome.restart_required);
        assert_eq!(reasons, outcome.reasons);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restart_status_no_restart_when_unchanged() {
        let dir = temp_dir();
        let state = load_or_migrate(&dir.join("settings.toml"), default_baseline());

        let (restart_required, reasons) = state.restart_status();
        assert!(!restart_required);
        assert!(reasons.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restart_required_multiple_reasons() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_model = Some("llama2".into());
        view.deep_endpoint = "127.0.0.1:11436".into();
        view.ocr = "automatic".into();
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert_eq!(outcome.reasons.len(), 3);
        assert!(outcome.reasons.contains(&"quick_model".to_owned()));
        assert!(outcome.reasons.contains(&"deep_endpoint".to_owned()));
        assert!(outcome.reasons.contains(&"ocr_mode".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_quick_engine_accepts_ollama_and_lmstudio() {
        assert_eq!(validate_quick_engine("ollama"), "ollama");
        assert_eq!(validate_quick_engine("lmstudio"), "lmstudio");
        assert_eq!(validate_quick_engine("unknown"), "ollama"); // fallback
        assert_eq!(validate_quick_engine(""), "ollama"); // fallback
    }

    #[test]
    fn validate_deep_engine_accepts_only_ollama() {
        assert_eq!(validate_deep_engine("ollama"), "ollama");
        assert_eq!(validate_deep_engine("lmstudio"), "ollama"); // Phase 3: Deep is Ollama-only
        assert_eq!(validate_deep_engine("unknown"), "ollama"); // fallback
        assert_eq!(validate_deep_engine(""), "ollama"); // fallback
    }

    #[test]
    fn apply_restart_required_when_quick_engine_changed() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_engine = "lmstudio".into();
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert!(outcome.reasons.contains(&"quick_engine".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deep_lmstudio_is_canonicalized_to_ollama() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        // Try to set Deep to LM Studio
        view.deep_engine = "lmstudio".into();
        let outcome = state.apply(view).unwrap();
        // Phase 3: Deep is Ollama-only, so LM Studio is canonicalized to Ollama
        assert_eq!(outcome.settings.deep_engine, "ollama");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_deep_ollama_does_not_trigger_restart() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.deep_engine = "ollama".into(); // same as baseline
        let outcome = state.apply(view).unwrap();
        // No restart because value did not change
        assert!(!outcome.reasons.contains(&"deep_engine".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_no_restart_required_when_engine_unchanged() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_engine = "ollama".into(); // same as baseline
        view.deep_engine = "ollama".into(); // same as baseline
        let outcome = state.apply(view).unwrap();
        assert!(!outcome.restart_required);
        assert!(!outcome.reasons.contains(&"quick_engine".to_owned()));
        assert!(!outcome.reasons.contains(&"deep_engine".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_persists_lmstudio_engine() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_engine = "lmstudio".into();
        // Phase 3: Deep is Ollama-only, so deep_engine must remain ollama
        view.deep_engine = "ollama".into();
        let outcome = state.apply(view).unwrap();
        assert_eq!(outcome.settings.quick_engine, "lmstudio");
        assert_eq!(outcome.settings.deep_engine, "ollama");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_clears_model_and_resets_default_endpoint_when_quick_engine_changes() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        // First, set up with ollama and the default endpoint
        let mut view = default_view();
        view.quick_engine = "ollama".into();
        view.quick_model = Some("llama2".into());
        view.quick_endpoint = "127.0.0.1:11434".into(); // Ollama default
        state.apply(view).unwrap();

        // Now switch to lmstudio, still with Ollama's default endpoint
        let mut view = default_view();
        view.quick_engine = "lmstudio".into();
        view.quick_model = Some("some-lmstudio-model".into());
        view.quick_endpoint = "127.0.0.1:11434".into(); // Still Ollama default
        let outcome = state.apply(view).unwrap();

        // Model should be cleared because engine changed
        assert_eq!(outcome.settings.quick_model, None);
        // Endpoint should be reset to lmstudio default because user hadn't customized it
        assert_eq!(outcome.settings.quick_endpoint, "127.0.0.1:1234");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_validates_custom_endpoint_when_quick_engine_changes() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        // First, set up with ollama and a loopback custom endpoint
        let mut view = default_view();
        view.quick_engine = "ollama".into();
        view.quick_model = Some("llama2".into());
        view.quick_endpoint = "127.0.0.2:11434".into(); // Loopback custom endpoint (different from default)
        state.apply(view).unwrap();

        // Now switch to lmstudio, keep the loopback custom endpoint
        let mut view = default_view();
        view.quick_engine = "lmstudio".into();
        view.quick_model = Some("some-lmstudio-model".into());
        view.quick_endpoint = "127.0.0.2:11434".into(); // Still the loopback custom endpoint
        let outcome = state.apply(view).unwrap();

        // Model should be cleared because engine changed
        assert_eq!(outcome.settings.quick_model, None);
        // Loopback custom endpoint should be preserved
        assert_eq!(outcome.settings.quick_endpoint, "127.0.0.2:11434");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_rejects_non_loopback_endpoint_when_quick_engine_changes() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        // First, set up with ollama
        let mut view = default_view();
        view.quick_engine = "ollama".into();
        view.quick_endpoint = "127.0.0.1:11434".into(); // Ollama default
        state.apply(view).unwrap();

        // Now switch to lmstudio with a non-loopback endpoint
        let mut view = default_view();
        view.quick_engine = "lmstudio".into();
        view.quick_endpoint = "192.168.1.100:1234".into(); // Non-loopback (privacy violation)
        let outcome = state.apply(view).unwrap();

        // Non-loopback endpoint should be rejected and reset to new provider's default
        assert_eq!(outcome.settings.quick_endpoint, "127.0.0.1:1234"); // LM Studio default
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_empty_endpoint_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_endpoint = "".into(); // Empty endpoint
        state.apply(view).unwrap();
        // Empty endpoint is not a loopback address, so it falls back to default
        assert_eq!(state.current().endpoint, Settings::default().endpoint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_keeps_model_when_quick_engine_unchanged() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        view.quick_engine = "ollama".into();
        view.quick_model = Some("llama2".into());
        let outcome = state.apply(view).unwrap();

        // Model should be kept because engine did not change
        assert_eq!(outcome.settings.quick_model, Some("llama2".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_rejects_malformed_shortcut() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        // Try to set an invalid shortcut syntax
        view.shortcuts.bindings[0].shortcut = Some("InvalidShortcut".into());
        let result = state.apply(view);
        assert!(result.is_err(), "malformed shortcut should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.contains("invalid shortcut"),
            "error should mention invalid shortcut: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_rejects_unbinding_pause() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        // Try to unbind Pause (set to None)
        view.shortcuts.bindings[0].action = "pause".into();
        view.shortcuts.bindings[0].shortcut = None;
        let result = state.apply(view);
        assert!(result.is_err(), "unbinding Pause should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.contains("pause") && err.contains("must have a shortcut"),
            "error should mention Pause must have a shortcut: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_rejects_duplicate_shortcuts() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        // Set two actions to the same shortcut
        view.shortcuts.bindings[0].shortcut = Some("Ctrl+Alt+X".into());
        view.shortcuts.bindings[1].shortcut = Some("Ctrl+Alt+X".into());
        let result = state.apply(view);
        assert!(result.is_err(), "duplicate shortcuts should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.contains("same shortcut"),
            "error should mention duplicate binding: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_rejects_insufficient_modifiers() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        // Try to set a shortcut with insufficient modifiers (no Ctrl/Alt/Meta)
        view.shortcuts.bindings[0].shortcut = Some("L".into());
        let result = state.apply(view);
        assert!(result.is_err(), "shortcut without modifiers should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.contains("Ctrl/Alt/Meta"),
            "error should mention required modifiers: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restart_required_when_shortcuts_changed() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let mut view = default_view();
        // Change Pause shortcut to a different key
        view.shortcuts.bindings[0].shortcut = Some("Ctrl+Alt+X".into());
        let outcome = state.apply(view).unwrap();
        assert!(outcome.restart_required);
        assert!(outcome.reasons.contains(&"shortcuts".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_no_restart_when_shortcuts_unchanged() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());
        let view = default_view();
        // Apply with unchanged shortcuts
        let outcome = state.apply(view).unwrap();
        assert!(!outcome.restart_required);
        assert!(!outcome.reasons.contains(&"shortcuts".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression test: apply() must be transactional.
    ///
    /// If shortcut validation fails, no other setting should be mutated.
    /// This prevents partial state corruption where the user changes language
    /// and enters a malformed shortcut, and the language is changed despite
    /// the save failing.
    #[test]
    fn apply_is_transactional_failed_shortcut_validation_does_not_mutate_other_settings() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());

        // Capture initial state
        let initial_language = state.view().language.clone();
        let initial_generation = state.generation;
        let initial_pending = state.pending.clone();
        let initial_shortcuts = state.view().shortcuts.clone();

        // Read disk content before failed apply
        let disk_before = std::fs::read_to_string(state.file.path()).unwrap();

        // Try to apply: change language AND enter malformed shortcut
        let mut view = default_view();
        view.language = "Japanese".into();
        view.shortcuts.bindings[0].shortcut = Some("MalformedShortcut".into());

        // Apply should fail
        let result = state.apply(view);
        assert!(result.is_err(), "malformed shortcut should cause apply to fail");

        // Verify transactionality: nothing should have changed
        assert_eq!(
            state.view().language, initial_language,
            "language should not be mutated when shortcut validation fails"
        );
        assert_eq!(
            state.generation, initial_generation,
            "generation should not increment when apply fails"
        );
        assert_eq!(
            state.pending, initial_pending,
            "pending should not change when apply fails"
        );
        assert_eq!(
            state.view().shortcuts, initial_shortcuts,
            "shortcuts should not change when validation fails"
        );

        // Verify disk is unchanged
        let disk_after = std::fs::read_to_string(state.file.path()).unwrap();
        assert_eq!(
            disk_before, disk_after,
            "disk file should not be modified when apply fails"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression test: apply() must be transactional for duplicate shortcuts too.
    #[test]
    fn apply_is_transactional_duplicate_shortcut_does_not_mutate_engine() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"), default_baseline());

        // Capture initial engine state
        let initial_quick_engine = state.view().quick_engine.clone();
        let initial_quick_model = state.view().quick_model.clone();

        // Try to apply: change engine AND enter duplicate shortcuts
        let mut view = default_view();
        view.quick_engine = "lmstudio".into();
        view.quick_model = Some("test-model".into());
        // Create duplicate shortcuts
        view.shortcuts.bindings[0].shortcut = Some("Ctrl+Alt+X".into());
        view.shortcuts.bindings[1].shortcut = Some("Ctrl+Alt+X".into());

        // Apply should fail
        let result = state.apply(view);
        assert!(result.is_err(), "duplicate shortcuts should cause apply to fail");

        // Verify transactionality: engine should not have changed
        assert_eq!(
            state.view().quick_engine, initial_quick_engine,
            "quick_engine should not be mutated when shortcut validation fails"
        );
        assert_eq!(
            state.view().quick_model, initial_quick_model,
            "quick_model should not be mutated when shortcut validation fails"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
