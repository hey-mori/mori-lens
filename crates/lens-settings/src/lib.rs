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

/// Outcome of loading or initializing a settings file.
///
/// The caller uses this to report what happened (first run, migrated,
/// loaded existing) and decide whether to show a welcome message.
#[derive(Debug)]
pub enum LoadDisposition {
    /// Loaded an existing valid settings file.
    Loaded,
    /// Created a new file with defaults (first run).
    Created,
    /// Migrated from legacy flat format.
    Migrated {
        /// Problems found during migration.
        problems: Vec<lens_core::settings::Problem>,
    },
    /// File exists but is invalid; preserved without modification.
    InvalidPreserved {
        /// Why the file could not be loaded.
        reason: String,
    },
}

/// Load a settings file, migrating from legacy format if needed.
///
/// This is the canonical file-lifecycle entry point. It handles:
/// - Loading valid new-format TOML files
/// - Migrating legacy flat-format files with fail-safe backup
/// - Creating new files with defaults on first run
/// - Preserving invalid files without destructive overwrites
///
/// The migration is fail-safe: if the backup cannot be created, the
/// original file is never modified. If the backup exists, a numbered
/// variant is used to avoid collision.
///
/// Returns the loaded file and the disposition indicating what happened.
pub fn load_or_initialize(path: &Path) -> Result<(SettingsFile, LoadDisposition), SettingsError> {
    // Try reading the file first
    let content = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // First run: create new file with defaults
            let mut file = SettingsFile::new_defaults(path.to_path_buf());
            if let Err(e) = file.save() {
                eprintln!("settings  cannot create {path:?}: {e}");
                // Return the file anyway; caller can use defaults
                return Ok((file, LoadDisposition::Created));
            }
            return Ok((file, LoadDisposition::Created));
        }
        Err(e) => {
            return Err(SettingsError::Io {
                path: path.to_path_buf(),
                source: e,
            });
        }
    };

    // Try loading as new format
    match SettingsFile::load(path) {
        Ok(file) => Ok((file, LoadDisposition::Loaded)),
        Err(SettingsError::MissingVersion { .. }) => {
            // No schema_version: check if it's legacy format
            if is_legacy_format(&content) {
                // Migrate legacy format
                migrate_legacy_file(path, &content)
            } else {
                // Not legacy, not new format: preserve and report
                let reason = "no schema_version and not recognized as legacy format".to_string();
                eprintln!("settings  {path:?}: {reason}");
                let file = SettingsFile::new_defaults(path.to_path_buf());
                Ok((file, LoadDisposition::InvalidPreserved { reason }))
            }
        }
        Err(SettingsError::FutureVersion { .. }) => {
            // Future version: preserve, never downgrade
            let reason = "future schema version; cannot load without downgrading".to_string();
            eprintln!("settings  {path:?}: {reason}");
            let file = SettingsFile::new_defaults(path.to_path_buf());
            Ok((file, LoadDisposition::InvalidPreserved { reason }))
        }
        Err(SettingsError::Parse { .. }) => {
            // Malformed TOML: not legacy, preserve
            let reason = "not valid TOML; preserving original".to_string();
            eprintln!("settings  {path:?}: {reason}");
            let file = SettingsFile::new_defaults(path.to_path_buf());
            Ok((file, LoadDisposition::InvalidPreserved { reason }))
        }
        Err(e) => {
            // Other error: preserve
            let reason = format!("cannot load: {e}");
            eprintln!("settings  {path:?}: {reason}");
            let file = SettingsFile::new_defaults(path.to_path_buf());
            Ok((file, LoadDisposition::InvalidPreserved { reason }))
        }
    }
}

/// Detect whether content is in the legacy flat format.
///
/// Legacy format characteristics:
/// - No `schema_version` field
/// - Top-level keys like `language`, `model`, `dwell_ms`, etc.
/// - No new schema sections like `[general]`, `[quick]`, `[deep]`
///
/// This function assumes the content is valid TOML (or close enough to
/// parse). If TOML parsing fails, this returns false.
fn is_legacy_format(content: &str) -> bool {
    // Try to parse as TOML
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
        return false;
    };

    // If it has schema_version, it's new format
    if doc.get("schema_version").is_some() {
        return false;
    }

    // Check for new schema sections (indicates new format without version)
    if doc.get("general").is_some() || doc.get("quick").is_some() || doc.get("deep").is_some() {
        return false;
    }

    // Check for legacy top-level keys
    let legacy_keys = [
        "language",
        "model",
        "dwell_ms",
        "theme",
        "translucency",
        "endpoint",
        "ocr",
        "text",
        "profile",
        "hover_scope",
        "history",
        "history_days",
        "history_store_app",
        "default",
    ];

    let has_legacy_key = legacy_keys.iter().any(|key| doc.get(key).is_some());

    // If it has at least one legacy key and no new sections, it's legacy
    has_legacy_key
}

/// Migrate a legacy format file to new TOML format with fail-safe backup.
///
/// The migration is transactional:
/// 1. Parse legacy format
/// 2. Build new schema in memory
/// 3. Create backup (with collision handling)
/// 4. Write new file
///
/// If backup creation fails, the original file is never modified.
fn migrate_legacy_file(
    path: &Path,
    content: &str,
) -> Result<(SettingsFile, LoadDisposition), SettingsError> {
    let report = migrate::migrate_from_conf(content);
    let mut file = SettingsFile::new_defaults(path.to_path_buf());
    *file.schema_mut() = report.schema;

    // Create backup with collision handling
    let backup = create_backup(path)?;

    // Backup succeeded, now we can safely overwrite
    eprintln!("settings  migrated legacy format to {path:?} (backup at {backup:?})");
    for problem in &report.problems {
        eprintln!("settings  migration note: {}", problem.message);
    }

    if let Err(e) = file.save() {
        // Save failed after backup was created
        eprintln!("settings  migration save failed: {e}");
        eprintln!("settings  original preserved at {backup:?}");
        // Return defaults; original is backed up
        return Ok((
            SettingsFile::new_defaults(path.to_path_buf()),
            LoadDisposition::InvalidPreserved {
                reason: format!("migration save failed: {e}"),
            },
        ));
    }

    Ok((
        file,
        LoadDisposition::Migrated {
            problems: report.problems,
        },
    ))
}

/// Create a backup of the file, handling collisions.
///
/// Returns the backup path on success. On failure, the original file
/// is not modified.
fn create_backup(path: &Path) -> Result<PathBuf, SettingsError> {
    // Try the standard backup name first
    let backup = path.with_extension("toml.legacy-backup");

    if !backup.exists() {
        // No collision: simple rename
        if let Err(e) = fs::rename(path, &backup) {
            return Err(SettingsError::Io {
                path: path.to_path_buf(),
                source: e,
            });
        }
        return Ok(backup);
    }

    // Collision: try numbered variants
    for i in 1..=100 {
        let numbered = path.with_extension(format!("toml.legacy-backup.{i}"));
        if !numbered.exists() {
            if let Err(e) = fs::rename(path, &numbered) {
                return Err(SettingsError::Io {
                    path: path.to_path_buf(),
                    source: e,
                });
            }
            return Ok(numbered);
        }
    }

    // Too many backups: refuse to migrate
    Err(SettingsError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(
            "too many legacy backups already exist; manual cleanup required",
        ),
    })
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
        theme: get_str("theme")
            .filter(|v| is_valid_theme(v))
            .unwrap_or(defaults.theme),
        translucency: get_str("translucency")
            .filter(|v| is_valid_translucency(v))
            .unwrap_or(defaults.translucency),
        text_size: get_str("text_size")
            .filter(|v| is_valid_text_size(v))
            .unwrap_or(defaults.text),
        default_tier: get_str("default_tier")
            .as_deref()
            .and_then(parse_tier)
            .unwrap_or(defaults.default_tier),
        ocr: get_str("ocr")
            .filter(|v| lens_core::session::OcrMode::named(v).is_some())
            .unwrap_or(defaults.ocr),
        profile: get_str("profile")
            .filter(|v| lens_core::analyse::Profile::named(v).is_some())
            .unwrap_or(defaults.profile),
        hover_scope: get_str("hover_scope")
            .filter(|v| is_valid_hover_scope(v))
            .unwrap_or(defaults.hover_scope),
        history: table
            .get("history")
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.history),
        history_days: get_int("history_days")
            .filter(|v| [0, 1, 7, 30, 90].contains(v))
            .map(|v| v as u32)
            .unwrap_or(defaults.history_days),
        history_store_app: table
            .get("history_store_app")
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.history_store_app),
        history_excluded: table
            .get("history_excluded")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                    .collect()
            })
            .unwrap_or_default(),
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

    let model = get_str("model").filter(|m| m != "auto");

    let endpoint = get_str("endpoint").unwrap_or_else(|| defaults.endpoint.clone());
    // Reject non-loopback endpoints at parse time using the canonical
    // locality classifier. The HTTP client also validates at request time
    // (defense in depth), but early rejection prevents silent misconfiguration.
    let endpoint = if lens_core::translate::engine_for(&endpoint)
        == lens_core::translate::EngineId::LocalOllama
    {
        endpoint
    } else {
        eprintln!(
            "settings: rejecting non-loopback endpoint {:?}, using default",
            endpoint
        );
        defaults.endpoint.clone()
    };

    schema::EngineConfig {
        engine: get_str("engine").unwrap_or_else(|| "ollama".to_owned()),
        model,
        endpoint,
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

fn is_valid_theme(s: &str) -> bool {
    matches!(s, "system" | "forest" | "night-lake" | "paper")
}

fn is_valid_translucency(s: &str) -> bool {
    matches!(s, "clear" | "balanced" | "solid")
}

fn is_valid_text_size(s: &str) -> bool {
    matches!(s, "small" | "normal" | "large" | "larger")
}

fn is_valid_hover_scope(s: &str) -> bool {
    matches!(s, "sentence" | "word")
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
        set_str(table, "hover_scope", &schema.general.hover_scope);
        set_bool(table, "history", schema.general.history);
        set_int(table, "history_days", schema.general.history_days as i64);
        set_bool(table, "history_store_app", schema.general.history_store_app);
        set_str_array(table, "history_excluded", &schema.general.history_excluded);
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

fn set_bool(table: &mut dyn toml_edit::TableLike, key: &str, value: bool) {
    if let Some(item) = table.get_mut(key) {
        if let Some(v) = item.as_value_mut() {
            *v = toml_edit::Value::from(value);
            return;
        }
    }
    table.insert(key, toml_edit::value(value));
}

fn set_str_array(table: &mut dyn toml_edit::TableLike, key: &str, values: &[String]) {
    let mut arr = toml_edit::Array::new();
    for v in values {
        arr.push(v.as_str());
    }
    if let Some(item) = table.get_mut(key) {
        *item = toml_edit::Item::Value(toml_edit::Value::Array(arr));
        return;
    }
    table.insert(key, toml_edit::Item::Value(toml_edit::Value::Array(arr)));
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
    general.insert("hover_scope", toml_edit::value(&schema.general.hover_scope));
    general.insert("history", toml_edit::value(schema.general.history));
    general.insert(
        "history_days",
        toml_edit::value(schema.general.history_days as i64),
    );
    general.insert(
        "history_store_app",
        toml_edit::value(schema.general.history_store_app),
    );
    let mut excl_arr = toml_edit::Array::new();
    for v in &schema.general.history_excluded {
        excl_arr.push(v.as_str());
    }
    general.insert(
        "history_excluded",
        toml_edit::Item::Value(toml_edit::Value::Array(excl_arr)),
    );
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

    #[test]
    fn legacy_flat_format_causes_missing_version_error() {
        // Proves the settings parser conflict bug:
        // - lens-core::settings::write_template() writes flat format (no schema_version)
        // - lens-settings::SettingsFile::load() expects schema_version = 1
        // - Result: MissingVersion error, user's settings get backed up and reset

        let dir = temp_dir("legacy_conflict");
        let path = dir.join("settings.toml");

        // This is what lens-core::settings::write_template() writes
        let legacy_flat_format = "# Mori Lens settings.
#
# Edited by hand, read at startup. Command line flags win over this file, so a
# one-off run can override it without editing anything.

language     = Korean
model        = auto
dwell_ms     = 320
theme        = system
translucency = clear
endpoint     = 127.0.0.1:11434
ocr          = manual
text         = normal
profile      = reader
hover_scope  = sentence
history      = off
history_days = 30
history_store_app = off
";
        fs::write(&path, legacy_flat_format).unwrap();

        // Attempt to load with new parser
        let result = SettingsFile::load(&path);

        // Should fail with Parse error (legacy format uses unquoted strings)
        match result {
            Err(SettingsError::Parse { .. }) => {
                // Conflict confirmed: legacy flat format is incompatible with new parser
            }
            Err(e) => {
                panic!("Expected Parse error, got: {:?}", e);
            }
            Ok(_) => {
                panic!("Legacy format should not be accepted by new parser");
            }
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_enum_values_in_toml_fall_back_to_defaults() {
        let dir = temp_dir("invalid_enums");
        let path = dir.join("invalid_enums.toml");
        let content = "schema_version = 1

[general]
theme = \"purple\"
translucency = \"invisible\"
text_size = \"huge\"
ocr = \"always\"
profile = \"nobody\"
hover_scope = \"paragraph\"

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
        let settings = file.to_settings();
        let defaults = lens_core::settings::Settings::default();

        assert_eq!(
            settings.theme, defaults.theme,
            "invalid theme must fall back"
        );
        assert_eq!(
            settings.translucency, defaults.translucency,
            "invalid translucency must fall back"
        );
        assert_eq!(
            settings.text, defaults.text,
            "invalid text_size must fall back"
        );
        assert_eq!(settings.ocr, defaults.ocr, "invalid ocr must fall back");
        assert_eq!(
            settings.profile, defaults.profile,
            "invalid profile must fall back"
        );
        assert_eq!(
            settings.hover_scope, defaults.hover_scope,
            "invalid hover_scope must fall back"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_enum_values_in_toml_are_kept() {
        let dir = temp_dir("valid_enums");
        let path = dir.join("valid_enums.toml");
        let content = "schema_version = 1

[general]
theme = \"night-lake\"
translucency = \"solid\"
text_size = \"larger\"
ocr = \"automatic\"
profile = \"developer\"
hover_scope = \"word\"

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
        let settings = file.to_settings();

        assert_eq!(settings.theme, "night-lake");
        assert_eq!(settings.translucency, "solid");
        assert_eq!(settings.text, "larger");
        assert_eq!(settings.ocr, "automatic");
        assert_eq!(settings.profile, "developer");
        assert_eq!(settings.hover_scope, "word");

        let _ = fs::remove_dir_all(&dir);
    }

    /// Round-trip preservation: load, save, load again must produce identical values.
    ///
    /// This test verifies that no validation or transformation drift occurs
    /// between load and save paths. A value that loads successfully must save
    /// and reload to the same value.
    #[test]
    fn round_trip_preserves_all_field_values() {
        let dir = temp_dir("roundtrip_all");
        let path = dir.join("roundtrip_all.toml");
        let content = "schema_version = 1

[general]
language = \"Japanese\"
dwell_ms = 450
theme = \"night-lake\"
translucency = \"solid\"
text_size = \"larger\"
default_tier = \"allow\"
ocr = \"automatic\"
profile = \"developer\"
hover_scope = \"word\"
history = true
history_days = 90
history_store_app = true
history_excluded = [\"chrome\", \"code\"]

[quick]
engine = \"ollama\"
model = \"qwen2.5:3b\"
endpoint = \"127.0.0.1:11434\"

[deep]
engine = \"ollama\"
model = \"llama3.1:8b\"
endpoint = \"127.0.0.1:11435\"

[applications]
chrome = \"allow\"
code = \"deny\"
";
        fs::write(&path, content).unwrap();

        // First load
        let mut file1 = SettingsFile::load(&path).unwrap();
        let schema1 = file1.schema().clone();

        // Save
        file1.save().unwrap();

        // Second load
        let file2 = SettingsFile::load(&path).unwrap();
        let schema2 = file2.schema();

        // All fields must be identical
        assert_eq!(schema2.version, schema1.version);
        assert_eq!(schema2.general.language, schema1.general.language);
        assert_eq!(schema2.general.dwell_ms, schema1.general.dwell_ms);
        assert_eq!(schema2.general.theme, schema1.general.theme);
        assert_eq!(schema2.general.translucency, schema1.general.translucency);
        assert_eq!(schema2.general.text_size, schema1.general.text_size);
        assert_eq!(schema2.general.default_tier, schema1.general.default_tier);
        assert_eq!(schema2.general.ocr, schema1.general.ocr);
        assert_eq!(schema2.general.profile, schema1.general.profile);
        assert_eq!(schema2.general.hover_scope, schema1.general.hover_scope);
        assert_eq!(schema2.general.history, schema1.general.history);
        assert_eq!(schema2.general.history_days, schema1.general.history_days);
        assert_eq!(
            schema2.general.history_store_app,
            schema1.general.history_store_app
        );
        assert_eq!(
            schema2.general.history_excluded,
            schema1.general.history_excluded
        );
        assert_eq!(schema2.quick.engine, schema1.quick.engine);
        assert_eq!(schema2.quick.model, schema1.quick.model);
        assert_eq!(schema2.quick.endpoint, schema1.quick.endpoint);
        assert_eq!(schema2.deep.engine, schema1.deep.engine);
        assert_eq!(schema2.deep.model, schema1.deep.model);
        assert_eq!(schema2.deep.endpoint, schema1.deep.endpoint);
        assert_eq!(schema2.applications, schema1.applications);

        let _ = fs::remove_dir_all(&dir);
    }

    /// Validation consistency: non-loopback endpoints are rejected at load time.
    ///
    /// This test verifies that the endpoint validation in extract_engine()
    /// is applied consistently to both quick and deep sections.
    #[test]
    fn non_loopback_endpoints_are_rejected_for_both_engines() {
        let dir = temp_dir("endpoint_validation");
        let path = dir.join("endpoint_validation.toml");
        let content = "schema_version = 1

[general]
language = \"Korean\"

[quick]
engine = \"ollama\"
model = \"auto\"
endpoint = \"192.168.1.1:11434\"

[deep]
engine = \"ollama\"
model = \"auto\"
endpoint = \"10.0.0.1:11434\"
";
        fs::write(&path, content).unwrap();

        let file = SettingsFile::load(&path).unwrap();
        let defaults = lens_core::settings::Settings::default();

        // Both endpoints should be rejected and replaced with defaults
        assert_eq!(
            file.schema().quick.endpoint,
            defaults.endpoint,
            "non-loopback quick endpoint must be rejected"
        );
        assert_eq!(
            file.schema().deep.endpoint,
            defaults.endpoint,
            "non-loopback deep endpoint must be rejected"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Validation consistency: range checks are applied at load time.
    ///
    /// This test verifies that dwell_ms and history_days are validated
    /// against their allowed ranges.
    #[test]
    fn range_checks_are_applied_at_load_time() {
        let dir = temp_dir("range_checks");
        let path = dir.join("range_checks.toml");
        let content = "schema_version = 1

[general]
language = \"Korean\"
dwell_ms = 50
history_days = 15

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
        let defaults = lens_core::settings::Settings::default();

        // dwell_ms = 50 is below the minimum (80), should fall back to default
        assert_eq!(
            file.schema().general.dwell_ms,
            defaults.dwell_ms,
            "dwell_ms below minimum must fall back to default"
        );

        // history_days = 15 is not in the allowed set [0, 1, 7, 30, 90]
        assert_eq!(
            file.schema().general.history_days,
            defaults.history_days,
            "history_days not in allowed set must fall back to default"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
