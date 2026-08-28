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
                };
            }
            Err(e) => {
                eprintln!("settings  cannot load {toml_path:?}: {e}");
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
        Ok(())
    }

    /// Take any pending settings, clearing the slot.
    pub fn take_pending(&mut self) -> Option<Settings> {
        self.pending.take()
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
