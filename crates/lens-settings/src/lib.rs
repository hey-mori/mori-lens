//! Versioned TOML settings for Mori Lens.
//!
//! This crate owns the settings file format. It reads and writes
//! `settings.toml`, preserves comments and unknown keys through
//! round-trip editing, detects external modifications, and migrates
//! from the old `settings.conf` format.
//!
//! # Why a separate crate
//!
//! [`lens_core::settings::Settings`] is the in-memory type the product
//! uses. It has no file I/O and no parser dependency, because
//! `lens-core` must not depend on anything beyond its domain. This
//! crate bridges the gap: it reads TOML from disk and converts to and
//! from the core type.
//!
//! # The round-trip property
//!
//! A reader who adds their own comment or a key this version does not
//! understand must not lose it when Lens saves. The [`toml_edit`]
//! document model preserves formatting, comments, and unknown keys.
//! This crate loads the document, extracts known fields into a
//! [`Schema`], and writes known fields back into the same document
//! without touching anything else.
//!
//! # The atomic write property
//!
//! A crash mid-save must not leave a truncated file. Writes go to a
//! temporary file first, then rename over the original. An invalid
//! write rolls back. See [`write`] for the implementation.

pub mod migrate;
pub mod schema;
pub mod write;

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use schema::Schema;

/// A settings file on disk, with its preserved document for round-trip.
///
/// Loaded from `settings.toml`, or built from defaults. The document
/// is kept so that saving preserves comments, formatting, and keys
/// this version does not understand.
#[derive(Debug)]
pub struct SettingsFile {
    /// The structured settings, extracted from the document.
    schema: Schema,
    /// The original TOML document, kept for round-trip writes.
    /// `None` when built from defaults (no file was loaded).
    document: Option<toml_edit::DocumentMut>,
    /// SHA-256 of the file content at load time.
    loaded_hash: Option<[u8; 32]>,
    /// Where the file lives on disk.
    path: PathBuf,
}

/// What went wrong loading or saving a settings file.
#[derive(Debug)]
pub enum SettingsError {
    /// The file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file is not valid TOML.
    Parse { path: PathBuf, reason: String },
    /// The `schema_version` field is missing or not a number.
    MissingVersion { path: PathBuf },
    /// The `schema_version` is newer than this build understands.
    FutureVersion { path: PathBuf, version: u64 },
    /// The atomic write failed.
    Write(write::AtomicWriteError),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            Self::Parse { path, reason } => {
                write!(f, "{} is not valid TOML: {reason}", path.display())
            }
            Self::MissingVersion { path } => {
                write!(
                    f,
                    "{} has no schema_version; is it a Mori Lens settings file?",
                    path.display()
                )
            }
            Self::FutureVersion { path, version } => {
                write!(
                    f,
                    "{} uses schema version {version}, which is newer than this build (version {})",
                    path.display(),
                    schema::CURRENT_VERSION
                )
            }
            Self::Write(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Write(e) => Some(e),
            _ => None,
        }
    }
}

impl From<write::AtomicWriteError> for SettingsError {
    fn from(e: write::AtomicWriteError) -> Self {
        Self::Write(e)
    }
}

impl SettingsFile {
    /// Load a settings file from disk.
    ///
    /// Returns an error if the file does not exist, is not valid TOML,
    /// or has a schema version this build does not understand.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let content = fs::read_to_string(path).map_err(|e| SettingsError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let hash = content_hash(&content);

        let document =
            content
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| SettingsError::Parse {
                    path: path.to_path_buf(),
                    reason: e.to_string(),
                })?;

        let schema = extract_schema(&document, path)?;

        Ok(Self {
            schema,
            document: Some(document),
            loaded_hash: Some(hash),
            path: path.to_path_buf(),
        })
    }

    /// Build a settings file from defaults, with no file on disk yet.
    ///
    /// The first call to [`save`](Self::save) writes a fresh file.
    pub fn new_defaults(path: PathBuf) -> Self {
        Self {
            schema: Schema::default(),
            document: None,
            loaded_hash: None,
            path,
        }
    }

    /// The structured settings.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// A mutable reference to the schema, for editing before saving.
    pub fn schema_mut(&mut self) -> &mut Schema {
        &mut self.schema
    }

    /// Convert the schema to the in-memory settings type.
    pub fn to_settings(&self) -> lens_core::settings::Settings {
        self.schema.to_settings()
    }

    /// Whether the file was modified externally since it was loaded.
    ///
    /// Compares the current file content hash with the hash recorded
    /// at load time. Returns `false` if the file has not been loaded
    /// from disk (built from defaults).
    pub fn is_externally_modified(&self) -> bool {
        let Some(expected_hash) = self.loaded_hash else {
            return false;
        };
        let Ok(content) = fs::read_to_string(&self.path) else {
            return true;
        };
        content_hash(&content) != expected_hash
    }

    /// Save the settings to disk, preserving comments and unknown keys.
    ///
    /// If a document was loaded, the schema values are written back
    /// into it, keeping everything else untouched. If no document
    /// exists (first save), a fresh document is built from the schema.
    ///
    /// The write is atomic: a temporary file is written first, then
    /// renamed over the target. An invalid write rolls back.
    pub fn save(&mut self) -> Result<(), SettingsError> {
        let document = match &mut self.document {
            Some(doc) => {
                apply_schema_to_document(&self.schema, doc);
                doc.clone()
            }
            None => build_document_from_schema(&self.schema),
        };

        let content = document.to_string();
        write::atomic_write_verified(&self.path, &content)?;

        self.loaded_hash = Some(content_hash(&content));
        Ok(())
    }

    /// The path this file lives at.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Compute the SHA-256 hash of a string's content.
fn content_hash(content: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Extract a [`Schema`] from a parsed TOML document.
fn extract_schema(doc: &toml_edit::DocumentMut, path: &Path) -> Result<Schema, SettingsError> {
    let version = doc
        .get("schema_version")
        .and_then(|v| v.as_integer())
        .ok_or(SettingsError::MissingVersion {
            path: path.to_path_buf(),
        })?;

    let version = version as u64;
    if version > schema::CURRENT_VERSION {
        return Err(SettingsError::FutureVersion {
            path: path.to_path_buf(),
            version,
        });
    }

    let general = extract_general(doc);
    let quick = extract_engine(doc, "quick");
    let deep = extract_engine(doc, "deep");
    let applications = extract_applications(doc);

    Ok(Schema {
        version,
        general,
        quick,
        deep,
        applications,
    })
}

fn extract_general(doc: &toml_edit::DocumentMut) -> schema::General {
    let defaults = lens_core::settings::Settings::default();
    let table = doc
        .get("general")
        .and_then(|v| v.as_table_like())
        .unwrap_or_else(|| doc.as_table() as &dyn toml_edit::TableLike);

    let get_str = |key: &str| -> Option<String> {
        table
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
    };
    let get_int = |key: &str| -> Option<i64> { table.get(key).and_then(|v| v.as_integer()) };

    schema::General {
        language: get_str("language").unwrap_or(defaults.language),
        dwell_ms: get_int("dwell_ms")
            .filter(|v| (80..=3000).contains(v))
            .map(|v| v as u64)
            .unwrap_or(defaults.dwell_ms),
        theme: get_str("theme").unwrap_or(defaults.theme),
        translucency: get_str("translucency").unwrap_or(defaults.translucency),
        text_size: get_str("text_size").unwrap_or(defaults.text),
        default_tier: get_str("default_tier")
            .as_deref()
            .and_then(parse_tier)
            .unwrap_or(defaults.default_tier),
        ocr: get_str("ocr").unwrap_or(defaults.ocr),
        profile: get_str("profile").unwrap_or(defaults.profile),
    }
}

fn extract_engine(doc: &toml_edit::DocumentMut, section: &str) -> schema::EngineConfig {
    let defaults = lens_core::settings::Settings::default();
    let table = doc.get(section).and_then(|v| v.as_table_like());

    let Some(table) = table else {
        return schema::EngineConfig::default_ollama();
    };

    let get_str = |key: &str| -> Option<String> {
        table
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
    };

    let model = get_str("model").and_then(|m| if m == "auto" { None } else { Some(m) });

    schema::EngineConfig {
        engine: get_str("engine").unwrap_or_else(|| "ollama".to_owned()),
        model,
        endpoint: get_str("endpoint").unwrap_or(defaults.endpoint),
    }
}

fn extract_applications(doc: &toml_edit::DocumentMut) -> Vec<(String, lens_core::policy::AppTier)> {
    let Some(table) = doc.get("applications").and_then(|v| v.as_table_like()) else {
        return Vec::new();
    };

    let mut apps = Vec::new();
    for (key, value) in table.iter() {
        if let Some(tier_str) = value.as_str().and_then(parse_tier) {
            apps.push((key.to_owned(), tier_str));
        }
    }
    apps
}

fn parse_tier(value: &str) -> Option<lens_core::policy::AppTier> {
    match value {
        "allow" => Some(lens_core::policy::AppTier::Allow),
        "modifier" => Some(lens_core::policy::AppTier::Modifier),
        "deny" => Some(lens_core::policy::AppTier::Deny),
        _ => None,
    }
}

fn tier_str(tier: &lens_core::policy::AppTier) -> &'static str {
    match tier {
        lens_core::policy::AppTier::Allow => "allow",
        lens_core::policy::AppTier::Modifier => "modifier",
        lens_core::policy::AppTier::Deny => "deny",
    }
}

/// Write the schema values back into an existing document, preserving
/// comments, formatting, and unknown keys.
fn apply_schema_to_document(schema: &Schema, doc: &mut toml_edit::DocumentMut) {
    // Update schema_version.
    doc["schema_version"] = toml_edit::value(schema.version as i64);

    // [general]
    let general = doc["general"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    if let Some(table) = general.as_table_like_mut() {
        set_str(table, "language", &schema.general.language);
        set_int(table, "dwell_ms", schema.general.dwell_ms as i64);
        set_str(table, "theme", &schema.general.theme);
        set_str(table, "translucency", &schema.general.translucency);
        set_str(table, "text_size", &schema.general.text_size);
        set_str(
            table,
            "default_tier",
            tier_str(&schema.general.default_tier),
        );
        set_str(table, "ocr", &schema.general.ocr);
        set_str(table, "profile", &schema.general.profile);
    }

    // [quick]
    apply_engine_to_table(schema, "quick", doc);

    // [deep]
    apply_engine_to_table(schema, "deep", doc);

    // [applications] - rebuild entirely, because order and membership
    // may have changed.
    let mut apps_table = toml_edit::Table::new();
    for (name, tier) in &schema.applications {
        apps_table.insert(name, toml_edit::value(tier_str(tier)));
    }
    doc["applications"] = toml_edit::Item::Table(apps_table);
}

fn apply_engine_to_table(schema: &Schema, section: &str, doc: &mut toml_edit::DocumentMut) {
    let engine = if section == "quick" {
        &schema.quick
    } else {
        &schema.deep
    };

    let item = doc[section].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    if let Some(table) = item.as_table_like_mut() {
        set_str(table, "engine", &engine.engine);
        match &engine.model {
            Some(m) => set_str(table, "model", m),
            None => set_str(table, "model", "auto"),
        }
        set_str(table, "endpoint", &engine.endpoint);
    }
}

fn set_str(table: &mut dyn toml_edit::TableLike, key: &str, value: &str) {
    // Mutate the existing value in place, preserving any decoration
    // (comments, whitespace) attached to the key or value.
    if let Some(item) = table.get_mut(key) {
        if let Some(v) = item.as_value_mut() {
            *v = toml_edit::Value::from(value);
            return;
        }
    }
    table.insert(key, toml_edit::value(value));
}

fn set_int(table: &mut dyn toml_edit::TableLike, key: &str, value: i64) {
    if let Some(item) = table.get_mut(key) {
        if let Some(v) = item.as_value_mut() {
            *v = toml_edit::Value::from(value);
            return;
        }
    }
    table.insert(key, toml_edit::value(value));
}

/// Build a fresh TOML document from a schema, used when there is no
/// existing document to preserve.
fn build_document_from_schema(schema: &Schema) -> toml_edit::DocumentMut {
    let mut doc = toml_edit::DocumentMut::new();

    doc["schema_version"] = toml_edit::value(schema.version as i64);

    let mut general = toml_edit::Table::new();
    general.insert("language", toml_edit::value(&schema.general.language));
    general.insert("dwell_ms", toml_edit::value(schema.general.dwell_ms as i64));
    general.insert("theme", toml_edit::value(&schema.general.theme));
    general.insert(
        "translucency",
        toml_edit::value(&schema.general.translucency),
    );
    general.insert("text_size", toml_edit::value(&schema.general.text_size));
    general.insert(
        "default_tier",
        toml_edit::value(tier_str(&schema.general.default_tier)),
    );
    general.insert("ocr", toml_edit::value(&schema.general.ocr));
    general.insert("profile", toml_edit::value(&schema.general.profile));
    doc["general"] = toml_edit::Item::Table(general);

    let quick = engine_to_table(&schema.quick);
    doc["quick"] = toml_edit::Item::Table(quick);

    let deep = engine_to_table(&schema.deep);
    doc["deep"] = toml_edit::Item::Table(deep);

    let mut apps = toml_edit::Table::new();
    for (name, tier) in &schema.applications {
        apps.insert(name, toml_edit::value(tier_str(tier)));
    }
    doc["applications"] = toml_edit::Item::Table(apps);

    doc
}

fn engine_to_table(engine: &schema::EngineConfig) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table.insert("engine", toml_edit::value(&engine.engine));
    let model = engine.model.as_deref().unwrap_or("auto");
    table.insert("model", toml_edit::value(model));
    table.insert("endpoint", toml_edit::value(&engine.endpoint));
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use lens_core::policy::AppTier;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lens-settings-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_and_save_round_trip_preserves_comments() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("settings.toml");
        let content = "# My custom settings
schema_version = 1

[general]
# The reader's language
language = \"Japanese\"
dwell_ms = 250

[quick]
engine = \"ollama\"
model = \"auto\"
endpoint = \"127.0.0.1:11434\"

[deep]
engine = \"ollama\"
model = \"auto\"
endpoint = \"127.0.0.1:11434\"

[custom_section]
my_key = \"preserved\"

[applications]
chrome = \"allow\"
";
        fs::write(&path, content).unwrap();

        let mut file = SettingsFile::load(&path).unwrap();
        assert_eq!(file.schema().general.language, "Japanese");
        assert_eq!(file.schema().general.dwell_ms, 250);

        // Change a value and save.
        file.schema_mut().general.language = "English".to_owned();
        file.save().unwrap();

        let saved = fs::read_to_string(&path).unwrap();

        // The comment is preserved.
        assert!(saved.contains("# My custom settings"), "comment lost");
        assert!(
            saved.contains("# The reader's language"),
            "inline comment lost"
        );

        // The unknown section is preserved.
        assert!(saved.contains("[custom_section]"), "unknown section lost");
        assert!(saved.contains("my_key"), "unknown key lost");

        // The changed value is reflected.
        assert!(
            saved.contains("\"English\"") || saved.contains("English"),
            "change not saved"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_refuses_a_file_without_schema_version() {
        let dir = temp_dir("no_version");
        let path = dir.join("no_version.toml");
        fs::write(&path, "[general]\nlanguage = \"Korean\"\n").unwrap();

        let result = SettingsFile::load(&path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SettingsError::MissingVersion { .. }
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_refuses_a_future_version() {
        let dir = temp_dir("future");
        let path = dir.join("future.toml");
        fs::write(&path, "schema_version = 999\n").unwrap();

        let result = SettingsFile::load(&path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SettingsError::FutureVersion { version: 999, .. }
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_modification_is_detected() {
        let dir = temp_dir("detect");
        let path = dir.join("detect.toml");
        let content = "schema_version = 1

[general]
language = \"Korean\"

[quick]
engine = \"ollama\"
model = \"auto\"
endpoint = \"127.0.0.1:11434\"

[deep]
engine = \"ollama\"
model = \"auto\"
endpoint = \"127.0.0.1:11434\"
";
        fs::write(&path, content).unwrap();

        let file = SettingsFile::load(&path).unwrap();
        assert!(!file.is_externally_modified());

        // Modify the file externally.
        fs::write(&path, "schema_version = 1\n# changed\n").unwrap();
        assert!(file.is_externally_modified());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_defaults_saves_a_fresh_file() {
        let dir = temp_dir("fresh");
        let path = dir.join("fresh.toml");

        let mut file = SettingsFile::new_defaults(path.clone());
        file.schema_mut().general.language = "Japanese".to_owned();
        file.save().unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("schema_version = 1"));
        assert!(content.contains("Japanese"));

        // The saved file can be loaded back.
        let loaded = SettingsFile::load(&path).unwrap();
        assert_eq!(loaded.schema().general.language, "Japanese");
        assert_eq!(loaded.schema(), file.schema());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn applications_round_trip_through_save_and_load() {
        let dir = temp_dir("apps");
        let path = dir.join("apps.toml");

        let mut file = SettingsFile::new_defaults(path.clone());
        file.schema_mut().applications = vec![
            ("chrome".to_owned(), AppTier::Allow),
            ("code".to_owned(), AppTier::Deny),
        ];
        file.save().unwrap();

        let loaded = SettingsFile::load(&path).unwrap();
        assert_eq!(loaded.schema().applications.len(), 2);
        assert_eq!(
            loaded.schema().applications[0],
            ("chrome".to_owned(), AppTier::Allow)
        );
        assert_eq!(
            loaded.schema().applications[1],
            ("code".to_owned(), AppTier::Deny)
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
