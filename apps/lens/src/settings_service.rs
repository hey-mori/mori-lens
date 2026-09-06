//! Settings service: the only writer of settings.toml.
//!
//! The product's settings file was hand-written until #165. This module
//! makes the Tauri window the sole writer, with atomic save, external
//! edit detection, and a pending slot so changes take effect without
//! waiting for the next file poll.
//!
//! The existing `refresh::Watcher` still watches for *external* edits
//! (a user editing the file in a text editor). When it detects one,
//! it re-parses and the polling loop applies it.

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
/// If `settings.toml` exists, load it. If only `settings.conf` exists,
/// migrate it to TOML (keeping the old file as backup). Otherwise,
/// create defaults.
///
/// # Recovery
///
/// A corrupt or future-version file is backed up to `settings.toml.bak`
/// before defaults are written. A future-version file is also flagged
/// read-only so this build does not silently downgrade it.
pub fn load_or_migrate(data_dir: &std::path::Path) -> SettingsState {
    let toml_path = data_dir.join("settings.toml");
    let conf_path = data_dir.join("settings.conf");

    if toml_path.exists() {
        match SettingsFile::load(&toml_path) {
            Ok(file) => {
                let current = file.to_settings();
                return SettingsState {
                    file,
                    current,
                    pending: None,
                    generation: 0,
                };
            }
            Err(e) => {
                eprintln!("settings  cannot load {toml_path:?}: {e}");
                // Back up the corrupt or future-version file so the reader's
                // own configuration is not lost when defaults are written.
                let backup = toml_path.with_extension("toml.bak");
                if std::fs::rename(&toml_path, &backup).is_ok() {
                    eprintln!("settings  backed up to {backup:?}");
                }
                // Fall through to defaults.
            }
        }
    }

    if conf_path.exists() {
        if let Ok(text) = std::fs::read_to_string(&conf_path) {
            let report = lens_settings::migrate::migrate_from_conf(&text);
            let mut file = SettingsFile::new_defaults(toml_path.clone());
            *file.schema_mut() = report.schema;
            if let Err(e) = file.save() {
                eprintln!("settings  migration save failed: {e}");
            } else {
                println!("settings  migrated from settings.conf to settings.toml");
                for problem in &report.problems {
                    println!("settings  migration note: {}", problem.message);
                }
            }
            let current = file.to_settings();
            return SettingsState {
                file,
                current,
                pending: None,
                generation: 0,
            };
        }
    }

    // No file exists; create defaults.
    let file = SettingsFile::new_defaults(toml_path);
    let current = file.to_settings();
    SettingsState {
        file,
        current,
        pending: None,
        generation: 0,
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
    pub fn apply(&mut self, view: SettingsView) -> Result<(), String> {
        let schema = self.file.schema_mut();

        schema.general.language = view.language;
        schema.general.dwell_ms = view.dwell_ms.clamp(80, 3000);
        schema.general.theme = view.theme;
        schema.general.translucency = view.translucency;
        schema.general.text_size = view.text_size;
        schema.general.default_tier = parse_tier(&view.default_tier)
            .unwrap_or(lens_core::settings::Settings::default().default_tier);
        schema.general.ocr = view.ocr;
        schema.general.profile = view.profile;
        schema.general.hover_scope = view.hover_scope;
        schema.general.history = view.history;
        schema.general.history_days = view.history_days;
        schema.general.history_store_app = view.history_store_app;
        schema.general.history_excluded = view.history_excluded;

        schema.quick = EngineConfig {
            engine: view.quick_engine,
            model: view.quick_model,
            endpoint: view.quick_endpoint,
        };
        schema.deep = EngineConfig {
            engine: view.deep_engine,
            model: view.deep_model,
            endpoint: view.deep_endpoint,
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
            translucency: "on".into(),
            text_size: "medium".into(),
            default_tier: "allow".into(),
            ocr: "manual".into(),
            profile: "default".into(),
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
        let state = load_or_migrate(&dir);
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
        let _ = load_or_migrate(&dir);
        // Load again — should read the file.
        let state = load_or_migrate(&dir);
        assert_eq!(state.view().language, "Korean");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_sets_pending_and_persists() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir);
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
        let reloaded = load_or_migrate(&dir);
        assert_eq!(reloaded.current().language, "Japanese");
        assert_eq!(reloaded.current().dwell_ms, 500);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_clamps_dwell_ms() {
        let dir = temp_dir();
        let mut state = load_or_migrate(&dir);
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
        let mut state = load_or_migrate(&dir);
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
        let mut state = load_or_migrate(&dir);
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
}
