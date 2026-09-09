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
use lens_settings::SettingsFile;
use serde::{Deserialize, Serialize};

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
    pub applications: Vec<(String, String)>,
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
pub fn load_or_migrate(settings_path: &std::path::Path) -> SettingsState {
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
                lens_settings::LoadDisposition::InvalidPreserved { reason } => {
                    eprintln!("settings  {reason}");
                }
            }

            SettingsState {
                file,
                current,
                pending: None,
                generation: 0,
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
                .map(|(name, tier)| (name.clone(), tier_to_string(tier)))
                .collect(),
        }
    }

    /// Apply a settings view from the webview, save to disk, and set pending.
    ///
    /// String enum fields are validated before writing. An invalid value is
    /// replaced with the default and a warning is printed, because a setting
    /// the user cannot see being ignored is worse than one that is refused.
    pub fn apply(&mut self, view: SettingsView) -> Result<(), String> {
        let schema = self.file.schema_mut();
        let defaults = lens_core::settings::Settings::default();

        schema.general.language = view.language;
        schema.general.dwell_ms = view.dwell_ms.clamp(80, 3000);
        schema.general.theme =
            validate_or_default(&view.theme, is_valid_theme, &defaults.theme, "theme");
        schema.general.translucency = validate_or_default(
            &view.translucency,
            is_valid_translucency,
            &defaults.translucency,
            "translucency",
        );
        schema.general.text_size = validate_or_default(
            &view.text_size,
            is_valid_text_size,
            &defaults.text,
            "text_size",
        );
        schema.general.default_tier =
            parse_tier(&view.default_tier).unwrap_or(defaults.default_tier);
        schema.general.ocr = validate_or_default(&view.ocr, is_valid_ocr, &defaults.ocr, "ocr");
        schema.general.profile = validate_or_default(
            &view.profile,
            is_valid_profile,
            &defaults.profile,
            "profile",
        );
        schema.general.hover_scope = validate_or_default(
            &view.hover_scope,
            is_valid_hover_scope,
            &defaults.hover_scope,
            "hover_scope",
        );
        schema.general.history = view.history;
        schema.general.history_days = if [0, 1, 7, 30, 90].contains(&view.history_days) {
            view.history_days
        } else {
            defaults.history_days
        };
        schema.general.history_store_app = view.history_store_app;
        schema.general.history_excluded = view.history_excluded;

        schema.quick = EngineConfig {
            engine: validate_engine(&view.quick_engine),
            model: view.quick_model,
            endpoint: validate_endpoint(&view.quick_endpoint, &defaults.endpoint, "quick_endpoint"),
        };
        schema.deep = EngineConfig {
            engine: validate_engine(&view.deep_engine),
            model: view.deep_model,
            endpoint: validate_endpoint(&view.deep_endpoint, &defaults.endpoint, "deep_endpoint"),
        };

        schema.applications = view
            .applications
            .into_iter()
            .filter_map(|(name, tier_str)| parse_tier(&tier_str).map(|tier| (name, tier)))
            .collect();

        if let Err(e) = self.file.save() {
            return Err(format!("save failed: {e}"));
        }

        self.current = self.file.to_settings();
        self.pending = Some(self.current.clone());
        self.generation += 1;
        Ok(())
    }

    /// Take any pending settings, clearing the slot.
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

fn validate_engine(value: &str) -> String {
    if value == "ollama" {
        value.to_owned()
    } else {
        eprintln!("settings  engine = {value:?} is not recognized, using \"ollama\"");
        "ollama".to_owned()
    }
}

fn validate_endpoint(value: &str, default: &str, field_name: &str) -> String {
    if lens_core::translate::engine_for(value) == lens_core::translate::EngineId::LocalOllama {
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
        }
    }

    #[test]
    fn load_or_migrate_creates_defaults_when_no_file() {
        let dir = temp_dir();
        let state = load_or_migrate(&dir.join("settings.toml"));
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
        let _ = load_or_migrate(&dir.join("settings.toml"));
        // Load again — should read the file.
        let state = load_or_migrate(&dir.join("settings.toml"));
        assert_eq!(state.view().language, "Korean");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_sets_pending_and_persists() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
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
        let reloaded = load_or_migrate(&dir.join("settings.toml"));
        assert_eq!(reloaded.current().language, "Japanese");
        assert_eq!(reloaded.current().dwell_ms, 500);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_clamps_dwell_ms() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
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
        let mut state = load_or_migrate(&dir.join("settings.toml"));
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
        let mut state = load_or_migrate(&dir.join("settings.toml"));
        let mut view = default_view();
        view.language = "Spanish".into();
        view.hover_scope = "word".into();
        view.history = true;
        view.history_days = 90;
        view.applications = vec![("Safari".into(), "deny".into())];
        state.apply(view.clone()).unwrap();

        let read_back = state.view();
        assert_eq!(read_back.language, "Spanish");
        assert_eq!(read_back.hover_scope, "word");
        assert!(read_back.history);
        assert_eq!(read_back.history_days, 90);
        assert_eq!(
            read_back.applications,
            vec![("Safari".into(), "deny".into())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_theme_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
        let mut view = default_view();
        view.theme = "purple".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().theme, Settings::default().theme);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_translucency_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
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
        let mut state = load_or_migrate(&dir.join("settings.toml"));
        let mut view = default_view();
        view.text_size = "huge".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().text, Settings::default().text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_ocr_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
        let mut view = default_view();
        view.ocr = "always".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().ocr, Settings::default().ocr);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_hover_scope_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
        let mut view = default_view();
        view.hover_scope = "paragraph".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().hover_scope, Settings::default().hover_scope);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_invalid_endpoint_falls_back_to_default() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
        let mut view = default_view();
        view.quick_endpoint = "192.168.1.100:11434".into();
        state.apply(view).unwrap();
        assert_eq!(state.current().endpoint, Settings::default().endpoint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_valid_enum_values_are_kept() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir.join("settings.toml"));
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
        let state = load_or_migrate(&custom_path);
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
        let reloaded = load_or_migrate(&custom_path);
        assert_eq!(reloaded.view().language, "Japanese");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
