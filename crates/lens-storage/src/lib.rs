//! SQLite storage for items a reader has kept.
//!
//! A self-contained persistence layer: open a database, insert items, search
//! them, list them, delete them. No dependency on `lens-core` or any OS crate.
//!
//! # Design
//!
//! `library.db` is the structured source of truth
//! ([ADR-0013](../../docs/decisions/ADR-0013-the-local-library.md)).
//! `saved.md` survives as the human-readable export. On first activation the
//! existing `saved.md` is imported best effort, and the file is never
//! overwritten or removed.
//!
//! Migrations are numbered and tracked in a `schema_migrations` table. Each
//! migration runs once and is never re-applied.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One item in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Stable identity. Zero before insertion.
    pub id: i64,
    /// The text on the screen.
    pub original: String,
    /// The translation or explanation.
    pub answer: String,
    /// The caveat, when the answer carries one.
    pub caveat: Option<String>,
    /// The source application's process name.
    pub app: Option<String>,
    /// When the item was kept, seconds since the Unix epoch.
    pub created_at: i64,
    /// `"translation"` or `"analysis"`.
    pub kind: String,
    /// Source language code (e.g. `"en"`, `"ko"`).
    pub source_language: Option<String>,
    /// Target language code (e.g. `"ko"`, `"en"`).
    pub target_language: Option<String>,
    /// The model that produced the answer.
    pub model: Option<String>,
    /// The reader's own note.
    pub note: Option<String>,
    /// Tags for this item, populated on demand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Manual review status (Later / Known / Archive).
    /// New items default to Later. No automatic scheduling.
    #[serde(default = "default_review_status")]
    pub review_status: ReviewStatus,
}

fn default_review_status() -> ReviewStatus {
    ReviewStatus::Later
}

/// A page of items returned by [`Library::list`] or [`Library::search`].
#[derive(Debug, Clone)]
pub struct Page {
    /// The items in this page, newest first.
    pub items: Vec<Item>,
    /// Whether more items exist beyond this page.
    pub has_more: bool,
}

/// One entry in the persistent history (ADR-0014 Tier 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Stable identity. Zero before insertion.
    pub id: i64,
    /// The text on the screen.
    pub original: String,
    /// The translation or explanation.
    pub answer: String,
    /// `"translation"` or `"analysis"`.
    pub kind: String,
    /// Source language code.
    pub source_language: Option<String>,
    /// Target language code.
    pub target_language: Option<String>,
    /// The model that produced the answer.
    pub model: Option<String>,
    /// The source application's process name. Only stored when
    /// `history_store_app` is on.
    pub app: Option<String>,
    /// How the text was acquired: `"accessibility"` or `"ocr"`.
    pub provenance: String,
    /// Where the inference ran: `"local"`, `"external"`, or `"unknown"`.
    /// Existing rows before migration 6 have `"unknown"`.
    pub provider_scope: String,
    /// When the card was shown, seconds since the Unix epoch.
    pub created_at: i64,
}

/// A page of history entries.
#[derive(Debug, Clone)]
pub struct HistoryPage {
    /// The entries in this page, newest first.
    pub entries: Vec<HistoryEntry>,
    /// Whether more entries exist beyond this page.
    pub has_more: bool,
    /// Total number of entries in the history.
    pub total: i64,
}

/// The kind of a kept item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A quick translation.
    Translation,
    /// A deep analysis.
    Analysis,
}

impl Kind {
    /// The string stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Translation => "translation",
            Self::Analysis => "analysis",
        }
    }
}

/// Filter criteria for listing items.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    /// Filter by kind: "translation" or "analysis".
    pub kind: Option<String>,
    /// Filter by source application name.
    pub app: Option<String>,
    /// Filter by source language code.
    pub source_language: Option<String>,
    /// Filter by target language code.
    pub target_language: Option<String>,
    /// Filter by review status.
    pub review_status: Option<ReviewStatus>,
}

/// A page size for listing and searching.
const PAGE_SIZE: i64 = 50;

/// The default maximum number of history entries (ADR-0014).
///
/// When the count exceeds this, the oldest entries are removed. Ordering is
/// deterministic: `created_at ASC, id ASC`.
const DEFAULT_HISTORY_MAX_ENTRIES: i64 = 1000;

/// Manual review status (Issue #141 owner-approved v1 contract).
///
/// - `Later`: reader wants to revisit manually. Visible in Later queue.
///   No due date, no timer, no schedule.
/// - `Known`: retained in library, searchable, not in active Later queue.
/// - `Archive`: retained, searchable/filterable, not in active Later queue.
///   Not a delete.
///
/// Transitions are manual and reversible. No automatic scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Later,
    Known,
    Archive,
}

impl ReviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Later => "later",
            Self::Known => "known",
            Self::Archive => "archive",
        }
    }

    /// Map a legacy SRS state to the manual review equivalent.
    ///
    /// Recognized legacy values: `mastered` → Known; `new`, `learning` → Later.
    /// Recognized current values: `later`, `known`, `archive` pass through.
    /// Unknown values return `None` (fail-closed: never silently classified).
    pub fn from_legacy(s: &str) -> Option<Self> {
        match s {
            "later" => Some(Self::Later),
            "known" => Some(Self::Known),
            "archive" => Some(Self::Archive),
            "mastered" => Some(Self::Known),
            "new" => Some(Self::Later),
            "learning" => Some(Self::Later),
            _ => None,
        }
    }
}

impl std::fmt::Display for ReviewStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ReviewStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "later" => Ok(Self::Later),
            "known" => Ok(Self::Known),
            "archive" => Ok(Self::Archive),
            _ => Err(()),
        }
    }
}

/// Errors from library operations.
#[derive(Debug)]
pub enum Error {
    /// A SQLite error.
    Sqlite(rusqlite::Error),
    /// The database file could not be created or accessed.
    Io(std::io::Error),
    /// The database was written by a newer Lens build. This build must
    /// not mutate it: an older binary touching a newer schema corrupts
    /// data silently.
    FutureSchema { have: i64, understand: i64 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "library database: {e}"),
            Self::Io(e) => write!(f, "library file: {e}"),
            Self::FutureSchema { have, understand } => write!(
                f,
                "library schema version {have} is newer than this build (version {understand}); upgrade Lens or do not open this database",
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// A handle to the library database.
///
/// The connection is opened in WAL mode for better concurrent read
/// performance and to avoid locking the file during reads.
#[derive(Debug)]
pub struct Library {
    conn: Connection,
}

impl Library {
    /// Open (or create) the library at the given path.
    ///
    /// The parent directory must exist. Runs all pending migrations before
    /// returning, so a caller always sees the current schema.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        let lib = Self { conn };
        lib.migrate()?;
        Ok(lib)
    }

    /// Insert one item. Returns the new item's id.
    ///
    /// The `id` field on the item is ignored; the database assigns one.
    /// Review status defaults to `Later` (manual review queue, no schedule).
    pub fn insert(&self, item: &Item) -> Result<i64, Error> {
        let status = item.review_status.as_str();
        self.conn.execute(
            "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                item.original,
                item.answer,
                item.caveat,
                item.app,
                item.created_at,
                item.kind,
                item.source_language,
                item.target_language,
                item.model,
                item.note,
                status,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List items, newest first.
    pub fn list(&self, offset: i64) -> Result<Page, Error> {
        let limit = PAGE_SIZE + 1;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state FROM library ORDER BY created_at DESC, id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let items: Vec<Item> = stmt
            .query_map(params![limit, offset], row_to_item)?
            .collect::<Result<_, _>>()?;
        let has_more = items.len() as i64 > PAGE_SIZE;
        let mut items = items;
        items.truncate(PAGE_SIZE as usize);
        Ok(Page { items, has_more })
    }

    /// Get one item by id.
    pub fn get(&self, id: i64) -> Result<Option<Item>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state FROM library WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_item)?;
        match rows.next() {
            Some(Ok(item)) => Ok(Some(item)),
            Some(Err(e)) => Err(Error::Sqlite(e)),
            None => Ok(None),
        }
    }

    /// Check whether an item with the same original text and kind already exists.
    ///
    /// Used by the keep flow to avoid storing the same sentence twice. The
    /// file remains append-only; this is for the database only.
    pub fn has_duplicate(&self, original: &str, kind: &str) -> Result<bool, Error> {
        let exists: bool = self
            .conn
            .prepare("SELECT 1 FROM library WHERE original = ?1 AND kind = ?2")?
            .exists(params![original, kind])?;
        Ok(exists)
    }

    /// Delete one item.
    ///
    /// A plain DELETE removes the row; it is not forensic erasure. See
    /// ADR-0013 for what `secure_delete` and `VACUUM` additionally provide.
    pub fn delete(&self, id: i64) -> Result<bool, Error> {
        let affected = self
            .conn
            .execute("DELETE FROM library WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Update the reader's note on an item.
    ///
    /// Returns whether the item existed.
    pub fn update_note(&self, id: i64, note: &str) -> Result<bool, Error> {
        let affected = self.conn.execute(
            "UPDATE library SET note = ?1 WHERE id = ?2",
            params![note, id],
        )?;
        Ok(affected > 0)
    }

    /// Set the tags for an item, replacing any existing tags.
    ///
    /// Tags are created if they don't exist. Returns whether the item existed.
    pub fn set_tags(&self, id: i64, tags: &[String]) -> Result<bool, Error> {
        // Check if the item exists.
        let exists: bool = self
            .conn
            .prepare("SELECT 1 FROM library WHERE id = ?1")?
            .exists(params![id])?;
        if !exists {
            return Ok(false);
        }

        // Remove existing tags for this item.
        self.conn.execute(
            "DELETE FROM library_tags WHERE library_id = ?1",
            params![id],
        )?;

        // Insert each tag, creating it if needed.
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() {
                continue;
            }

            // Insert or get the tag id.
            self.conn.execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                params![tag],
            )?;
            let tag_id: i64 = self.conn.query_row(
                "SELECT id FROM tags WHERE name = ?1",
                params![tag],
                |row| row.get(0),
            )?;

            // Link the tag to the item.
            self.conn.execute(
                "INSERT OR IGNORE INTO library_tags (library_id, tag_id) VALUES (?1, ?2)",
                params![id, tag_id],
            )?;
        }

        Ok(true)
    }

    /// Get the tags for an item.
    pub fn get_tags(&self, id: i64) -> Result<Vec<String>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM tags t
             JOIN library_tags lt ON lt.tag_id = t.id
             WHERE lt.library_id = ?1
             ORDER BY t.name",
        )?;
        let tags = stmt
            .query_map(params![id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(tags)
    }

    /// Search across original and answer fields.
    ///
    /// Uses SQLite LIKE with the query as a substring match. Works
    /// uniformly across all languages, including CJK text where word
    /// boundaries are ambiguous.
    pub fn search(&self, query: &str, offset: i64) -> Result<Page, Error> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list(offset);
        }
        let pattern = format!("%{trimmed}%");
        let limit = PAGE_SIZE + 1;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state FROM library WHERE original LIKE ?1 OR answer LIKE ?1 ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
        )?;
        let items: Vec<Item> = stmt
            .query_map(params![pattern, limit, offset], row_to_item)?
            .collect::<Result<_, _>>()?;
        let has_more = items.len() as i64 > PAGE_SIZE;
        let mut items = items;
        items.truncate(PAGE_SIZE as usize);
        Ok(Page { items, has_more })
    }

    /// List items matching the given filter, newest first.
    pub fn list_filtered(&self, filter: &Filter, offset: i64) -> Result<Page, Error> {
        let mut conditions = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        if let Some(ref kind) = filter.kind {
            param_values.push(kind.clone());
            conditions.push(format!("kind = ?{}", param_values.len()));
        }
        if let Some(ref app) = filter.app {
            param_values.push(app.clone());
            conditions.push(format!("app = ?{}", param_values.len()));
        }
        if let Some(ref src) = filter.source_language {
            param_values.push(src.clone());
            conditions.push(format!("source_language = ?{}", param_values.len()));
        }
        if let Some(ref tgt) = filter.target_language {
            param_values.push(tgt.clone());
            conditions.push(format!("target_language = ?{}", param_values.len()));
        }
        if let Some(ref status) = filter.review_status {
            param_values.push(status.as_str().to_owned());
            conditions.push(format!("review_state = ?{}", param_values.len()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let limit = PAGE_SIZE + 1;
        let sql = format!(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state FROM library{where_clause} ORDER BY created_at DESC, id DESC LIMIT ?{} OFFSET ?{}",
            param_values.len() + 1,
            param_values.len() + 2
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|p| p as &dyn rusqlite::types::ToSql)
            .collect();
        params.push(&limit);
        params.push(&offset);

        let items: Vec<Item> = stmt
            .query_map(params.as_slice(), row_to_item)?
            .collect::<Result<_, _>>()?;
        let has_more = items.len() as i64 > PAGE_SIZE;
        let mut items = items;
        items.truncate(PAGE_SIZE as usize);
        Ok(Page { items, has_more })
    }

    /// Total number of items in the library.
    pub fn count(&self) -> Result<i64, Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM library", [], |row| row.get(0))
            .map_err(Error::Sqlite)
    }

    /// Get distinct source applications, sorted alphabetically.
    pub fn distinct_apps(&self) -> Result<Vec<String>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT app FROM library WHERE app IS NOT NULL AND app != '' ORDER BY app",
        )?;
        let apps = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(apps)
    }

    /// Get distinct source languages, sorted alphabetically.
    pub fn distinct_source_languages(&self) -> Result<Vec<String>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT source_language FROM library WHERE source_language IS NOT NULL AND source_language != '' ORDER BY source_language",
        )?;
        let langs = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(langs)
    }

    /// Get distinct target languages, sorted alphabetically.
    pub fn distinct_target_languages(&self) -> Result<Vec<String>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT target_language FROM library WHERE target_language IS NOT NULL AND target_language != '' ORDER BY target_language",
        )?;
        let langs = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(langs)
    }

    /// Whether the library has been used (any imports tracked).
    pub fn is_imported(&self) -> Result<bool, Error> {
        let exists: bool = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='meta'")?
            .exists([])?;
        if !exists {
            return Ok(false);
        }
        let val: Option<String> = self
            .conn
            .prepare("SELECT value FROM meta WHERE key = 'imported'")?
            .query_row([], |row| row.get(0))
            .optional();
        Ok(val.as_deref() == Some("true"))
    }

    /// Mark the saved.md import as completed.
    pub fn mark_imported(&self) -> Result<(), Error> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('imported', 'true')",
            [],
        )?;
        Ok(())
    }

    /// Export the library as a Markdown string in the saved.md format.
    /// Items are sorted newest first, separated by `---`.
    pub fn export_markdown(&self) -> Result<String, Error> {
        // Fetch all items by iterating through pages.
        let mut all_items = Vec::new();
        let mut offset = 0i64;
        loop {
            let page = self.list(offset)?;
            if page.items.is_empty() {
                break;
            }
            all_items.extend(page.items);
            if !page.has_more {
                break;
            }
            offset += PAGE_SIZE;
        }

        if all_items.is_empty() {
            return Ok(String::new());
        }

        let mut out = String::new();
        for (i, item) in all_items.iter().enumerate() {
            if i > 0 {
                out.push_str("\n---\n\n");
            }

            // Format timestamp as "YYYY-MM-DD HH:MM"
            let datetime = format_datetime(item.created_at);
            out.push_str("## ");
            out.push_str(&datetime);
            if let Some(ref app) = item.app {
                if !app.is_empty() {
                    out.push_str(" · ");
                    out.push_str(app);
                }
            }
            out.push('\n');

            out.push_str("\n> ");
            out.push_str(&item.original.replace('\n', "\n> "));

            out.push_str("\n\n> ");
            out.push_str(&item.answer.replace('\n', "\n> "));

            if let Some(ref caveat) = item.caveat {
                if !caveat.is_empty() {
                    out.push_str("\n\n");
                    out.push_str(caveat);
                }
            }
        }

        Ok(out)
    }

    /// Export the library as a JSON string, including all fields and tags.
    /// This is the lossless export format.
    pub fn export_json(&self) -> Result<String, Error> {
        // Fetch all items by iterating through pages.
        let mut all_items = Vec::new();
        let mut offset = 0i64;
        loop {
            let page = self.list(offset)?;
            if page.items.is_empty() {
                break;
            }
            let mut items = page.items;
            // Populate tags for each item.
            for item in &mut items {
                item.tags = Some(self.get_tags(item.id)?);
            }
            all_items.extend(items);
            if !page.has_more {
                break;
            }
            offset += PAGE_SIZE;
        }

        serde_json::to_string_pretty(&all_items)
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
    }

    /// Export the library as a versioned JSONL string.
    ///
    /// The first line is a version header (`{"version": 1}`). Each
    /// subsequent line is one JSON object representing an item, including
    /// all fields and tags. This is the lossless round-trip format
    /// described by [ADR-0013](../../docs/decisions/ADR-0013-the-local-library.md):
    /// line-oriented, append-friendly, and resilient to partial corruption
    /// (a damaged line loses one record, not the whole file).
    pub fn export_jsonl(&self) -> Result<String, Error> {
        let mut all_items = Vec::new();
        let mut offset = 0i64;
        loop {
            let page = self.list(offset)?;
            if page.items.is_empty() {
                break;
            }
            let mut items = page.items;
            for item in &mut items {
                item.tags = Some(self.get_tags(item.id)?);
            }
            all_items.extend(items);
            if !page.has_more {
                break;
            }
            offset += PAGE_SIZE;
        }

        let mut out = String::from("{\"version\":1}\n");
        for item in &all_items {
            let line = serde_json::to_string(item)
                .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }

    /// Import items from a versioned JSONL string.
    ///
    /// The first line must be a version header (`{"version": 1}`). Each
    /// subsequent line is one JSON object. Items with the same original
    /// text and kind as an existing row are skipped (duplicate detection).
    ///
    /// Returns the number of items imported (not counting skipped
    /// duplicates).
    pub fn import_jsonl(&self, jsonl: &str) -> Result<i64, Error> {
        let mut imported = 0i64;
        for (i, line) in jsonl.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // First line is the version header.
            if i == 0 {
                let header: serde_json::Value = serde_json::from_str(trimmed)
                    .map_err(|e| Error::Io(std::io::Error::other(format!("line 1: {e}"))))?;
                let version = header.get("version").and_then(|v| v.as_i64()).unwrap_or(0);
                if version != 1 {
                    return Err(Error::Io(std::io::Error::other(format!(
                        "unsupported JSONL version: {version}"
                    ))));
                }
                continue;
            }

            let item: Item = serde_json::from_str(trimmed)
                .map_err(|e| Error::Io(std::io::Error::other(format!("line {}: {e}", i + 1))))?;

            // Skip duplicates.
            if self
                .has_duplicate(&item.original, &item.kind)
                .unwrap_or(false)
            {
                continue;
            }

            // Insert preserving the imported item's review status (or Later
            // if the legacy format has no status field).
            let insert_item = Item {
                id: 0,
                original: item.original,
                answer: item.answer,
                caveat: item.caveat,
                app: item.app,
                created_at: item.created_at,
                kind: item.kind,
                source_language: item.source_language,
                target_language: item.target_language,
                model: item.model,
                note: item.note,
                tags: None,
                review_status: item.review_status,
            };
            let id = self.insert(&insert_item)?;

            // Restore tags.
            if let Some(tags) = item.tags {
                if !tags.is_empty() {
                    self.set_tags(id, &tags)?;
                }
            }

            imported += 1;
        }
        Ok(imported)
    }

    /// The database connection, for callers that need to run their own
    /// queries (e.g. schema introspection in tests).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // -- manual review ------------------------------------------------------

    /// Set the review status for an item. Manual, reversible.
    ///
    /// Returns whether the item existed.
    pub fn set_review_status(&self, id: i64, status: ReviewStatus) -> Result<bool, Error> {
        let affected = self.conn.execute(
            "UPDATE library SET review_state = ?1 WHERE id = ?2",
            params![status.as_str(), id],
        )?;
        Ok(affected > 0)
    }

    /// List items with the given review status, newest first.
    pub fn list_by_status(&self, status: ReviewStatus, offset: i64) -> Result<Page, Error> {
        let limit = PAGE_SIZE + 1;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state FROM library WHERE review_state = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
        )?;
        let items: Vec<Item> = stmt
            .query_map(params![status.as_str(), limit, offset], row_to_item)?
            .collect::<Result<_, _>>()?;
        let has_more = items.len() as i64 > PAGE_SIZE;
        let mut items = items;
        items.truncate(PAGE_SIZE as usize);
        Ok(Page { items, has_more })
    }

    /// Count items with the given review status.
    pub fn count_by_status(&self, status: ReviewStatus) -> Result<i64, Error> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM library WHERE review_state = ?1",
                params![status.as_str()],
                |row| row.get(0),
            )
            .map_err(Error::Sqlite)
    }

    /// Securely wipe all data from the database.
    ///
    /// A plain SQLite `DELETE` frees pages but does not erase their content;
    /// the bytes remain until the pages are reused. This method goes further,
    /// following [ADR-0013](../../docs/decisions/ADR-0013-the-local-library.md):
    ///
    /// 1. Checkpoint the WAL so all pending writes are in the main database.
    /// 2. Run `secure_delete = ON` so future deletes overwrite with zeros.
    /// 3. Delete all rows from every content table.
    /// 4. `VACUUM` to rebuild the database file, which reclaims freed pages
    ///    and removes residual data from the file.
    ///
    /// This removes data **inside the SQLite file**. It does not reach
    /// filesystem snapshots, drive controller remapped blocks, backups, or
    /// exports the reader made. Each is named in the interface rather than
    /// covered by a promise that sounds larger than it is.
    pub fn secure_wipe(&self) -> Result<(), Error> {
        // Checkpoint the WAL so all pending writes land in the main file.
        // Ignore errors: a database without WAL mode returns an error that
        // is harmless (there is nothing to checkpoint).
        let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

        // Enable secure_delete so the DELETE below overwrites freed pages
        // with zeros rather than leaving the old content in place.
        self.conn.execute_batch("PRAGMA secure_delete = ON;")?;

        // Remove all content. The order matters: child tables first because
        // foreign keys reference library rows. History is also user-owned
        // persistent data and must be wiped.
        self.conn.execute_batch(
            "DELETE FROM library_tags;
             DELETE FROM tags;
             DELETE FROM history;
             DELETE FROM library;
             DELETE FROM meta;",
        )?;

        // Rebuild the database file from scratch. VACUUM creates a new file,
        // copies live pages into it, and replaces the old one. Any residual
        // data in freed pages is gone after this.
        self.conn.execute_batch("VACUUM;")?;

        Ok(())
    }

    // -- migrations ---------------------------------------------------------

    fn migrate(&self) -> Result<(), Error> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )?;

        let applied = self.applied_versions()?;

        // The highest version this build understands. If the database has
        // migrations beyond this, a newer build wrote them and this build
        // must not mutate the schema.
        const CURRENT_SCHEMA: i64 = 7;
        let max_applied = applied.iter().copied().max().unwrap_or(0);
        if max_applied > CURRENT_SCHEMA {
            return Err(Error::FutureSchema {
                have: max_applied,
                understand: CURRENT_SCHEMA,
            });
        }

        if !applied.contains(&1) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "CREATE TABLE library (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    original TEXT NOT NULL,
                    answer TEXT NOT NULL,
                    caveat TEXT,
                    app TEXT,
                    created_at INTEGER NOT NULL,
                    kind TEXT NOT NULL DEFAULT 'translation',
                    source_language TEXT,
                    target_language TEXT,
                    model TEXT,
                    note TEXT
                );

                CREATE INDEX library_created_at ON library (created_at DESC);
                CREATE INDEX library_kind ON library (kind);

                INSERT INTO schema_migrations (version) VALUES (1);",
            )?;
            tx.commit()?;
        }

        // The `meta` table for import tracking. Added as a separate
        // migration so an existing library from migration 1 gains it.
        if !applied.contains(&2) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "CREATE TABLE meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                INSERT INTO schema_migrations (version) VALUES (2);",
            )?;
            tx.commit()?;
        }

        // Tags: a many-to-many relationship between items and tags.
        if !applied.contains(&3) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "CREATE TABLE tags (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE
                );

                CREATE TABLE library_tags (
                    library_id INTEGER NOT NULL,
                    tag_id INTEGER NOT NULL,
                    PRIMARY KEY (library_id, tag_id),
                    FOREIGN KEY (library_id) REFERENCES library(id) ON DELETE CASCADE,
                    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
                );

                CREATE INDEX library_tags_tag_id ON library_tags(tag_id);

                INSERT INTO schema_migrations (version) VALUES (3);",
            )?;
            tx.commit()?;
        }

        // Review state: spaced repetition scheduling for saved items.
        if !applied.contains(&4) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "ALTER TABLE library ADD COLUMN review_state TEXT NOT NULL DEFAULT 'new';
                 ALTER TABLE library ADD COLUMN next_review_at INTEGER;
                 ALTER TABLE library ADD COLUMN review_count INTEGER NOT NULL DEFAULT 0;

                 CREATE INDEX library_review_due ON library (next_review_at)
                     WHERE review_state != 'mastered';

                 INSERT INTO schema_migrations (version) VALUES (4);",
            )?;
            tx.commit()?;
        }

        // Persistent history (ADR-0014 Tier 3).
        // Off by default; nothing is written until the reader turns it on.
        if !applied.contains(&5) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "CREATE TABLE history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    original TEXT NOT NULL,
                    answer TEXT NOT NULL,
                    kind TEXT NOT NULL DEFAULT 'translation',
                    source_language TEXT,
                    target_language TEXT,
                    model TEXT,
                    app TEXT,
                    provenance TEXT NOT NULL DEFAULT 'accessibility',
                    created_at INTEGER NOT NULL
                );

                CREATE INDEX history_created_at ON history (created_at DESC);

                INSERT INTO schema_migrations (version) VALUES (5);",
            )?;
            tx.commit()?;
        }

        // Provider scope: whether the inference ran locally or externally.
        // ADR-0014 requires both acquisition and provider provenance to be visible.
        if !applied.contains(&6) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "ALTER TABLE history ADD COLUMN provider_scope TEXT NOT NULL DEFAULT 'unknown';

                 INSERT INTO schema_migrations (version) VALUES (6);",
            )?;
            tx.commit()?;
        }

        // Manual review contract (Issue #141).
        //
        // Replaces the automatic spaced-repetition scheduling from migration 4
        // with a manual Later / Known / Archive lifecycle. No due dates, no
        // timers, no automatic resurfacing.
        //
        // Legacy state mapping:
        //   mastered  → known
        //   new       → later
        //   learning  → later
        //   (other)   → preserved in legacy_review_state for audit
        //
        // Drops the SRS-only columns (next_review_at, review_count) and the
        // due-date index. The review_state column is repurposed for the
        // manual lifecycle values.
        if !applied.contains(&7) {
            let tx = self.conn.unchecked_transaction()?;

            // Preserve any unrecognized legacy states before mutation.
            tx.execute_batch(
                "ALTER TABLE library ADD COLUMN legacy_review_state TEXT;

                 UPDATE library SET legacy_review_state = review_state
                     WHERE review_state NOT IN ('later', 'known', 'archive',
                                                 'mastered', 'new', 'learning');

                 -- Map known legacy states to the manual contract.
                 UPDATE library SET review_state = 'known'
                     WHERE review_state = 'mastered';
                 UPDATE library SET review_state = 'later'
                     WHERE review_state IN ('new', 'learning');

                 -- Drop SRS-only columns and index.
                 DROP INDEX IF EXISTS library_review_due;
                 ALTER TABLE library DROP COLUMN next_review_at;
                 ALTER TABLE library DROP COLUMN review_count;

                 INSERT INTO schema_migrations (version) VALUES (7);",
            )?;
            tx.commit()?;
        }

        Ok(())
    }

    /// Record one entry in the persistent history.
    ///
    /// Only written when the reader has turned history on (ADR-0014 Tier 3).
    /// The caller is responsible for checking the setting; this method always
    /// writes if called. After insertion, enforces the default maximum entry
    /// count by removing the oldest entries if needed.
    pub fn record_history(&self, entry: &HistoryEntry) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO history (original, answer, kind, source_language, target_language, model, app, provenance, provider_scope, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                entry.original,
                entry.answer,
                entry.kind,
                entry.source_language,
                entry.target_language,
                entry.model,
                entry.app,
                entry.provenance,
                entry.provider_scope,
                entry.created_at,
            ],
        )?;
        let id = self.conn.last_insert_rowid();

        // Enforce the default maximum entry count (ADR-0014).
        // Remove the oldest entries if the count exceeds the limit.
        // Ordering: created_at ASC, id ASC (deterministic).
        self.conn.execute(
            "DELETE FROM history WHERE id NOT IN (
                SELECT id FROM history ORDER BY created_at DESC, id DESC LIMIT ?1
            )",
            params![DEFAULT_HISTORY_MAX_ENTRIES],
        )?;

        Ok(id)
    }

    /// List history entries, newest first.
    pub fn history_list(&self, offset: i64) -> Result<HistoryPage, Error> {
        let limit = PAGE_SIZE + 1;
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, kind, source_language, target_language, model, app, provenance, provider_scope, created_at FROM history ORDER BY created_at DESC, id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let entries: Vec<HistoryEntry> = stmt
            .query_map(params![limit, offset], row_to_history_entry)?
            .collect::<Result<_, _>>()?;
        let has_more = entries.len() as i64 > PAGE_SIZE;
        let mut entries = entries;
        entries.truncate(PAGE_SIZE as usize);
        Ok(HistoryPage {
            entries,
            has_more,
            total,
        })
    }

    /// Delete one history entry.
    pub fn history_delete(&self, id: i64) -> Result<bool, Error> {
        let changed = self
            .conn
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    /// Prune history entries older than the retention period.
    ///
    /// `retention_secs` is the maximum age in seconds. Entries older than this
    /// are deleted. Pass 0 to skip pruning (forever retention). `now` is the
    /// current time in seconds since the Unix epoch.
    pub fn history_prune(&self, now: i64, retention_secs: i64) -> Result<usize, Error> {
        if retention_secs <= 0 {
            return Ok(0);
        }
        let cutoff = now - retention_secs;
        let changed = self
            .conn
            .execute("DELETE FROM history WHERE created_at < ?1", params![cutoff])?;
        Ok(changed)
    }

    /// Promote a history entry to the library.
    ///
    /// Creates a library item from the history entry, then deletes the history
    /// entry. The promoted item survives history expiry because it is now in
    /// the library (ADR-0014).
    pub fn history_promote(&self, history_id: i64) -> Result<i64, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, kind, source_language, target_language, model, app, provenance, provider_scope, created_at FROM history WHERE id = ?1",
        )?;
        let entry = stmt
            .query_row(params![history_id], row_to_history_entry)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    Error::Sqlite(rusqlite::Error::QueryReturnedNoRows)
                }
                other => Error::Sqlite(other),
            })?;

        self.conn.execute(
            "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, NULL, 'later')",
            params![
                entry.original,
                entry.answer,
                entry.app,
                entry.created_at,
                entry.kind,
                entry.source_language,
                entry.target_language,
                entry.model,
            ],
        )?;
        let library_id = self.conn.last_insert_rowid();

        self.conn
            .execute("DELETE FROM history WHERE id = ?1", params![history_id])?;

        Ok(library_id)
    }

    /// Count all history entries.
    pub fn history_count(&self) -> Result<i64, Error> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?;
        Ok(count)
    }

    fn applied_versions(&self) -> Result<Vec<i64>, Error> {
        let exists: bool = self
            .conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            )?
            .exists([])?;
        if !exists {
            return Ok(vec![]);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut versions = Vec::new();
        for v in rows {
            versions.push(v?);
        }
        Ok(versions)
    }

    /// The highest applied migration version, or 0 for a fresh database.
    ///
    /// Useful for diagnostics: a reader reporting a problem can paste the
    /// schema version so the developer knows which migrations have run.
    pub fn schema_version(&self) -> Result<i64, Error> {
        let exists: bool = self
            .conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            )?
            .exists([])?;
        if !exists {
            return Ok(0);
        }
        let max: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;
        Ok(max)
    }
}

/// Map a result row to an [`Item`].
fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<Item> {
    let status_str: String = row
        .get::<_, String>(11)
        .unwrap_or_else(|_| "later".to_owned());
    let review_status = status_str.parse().unwrap_or(ReviewStatus::Later);
    Ok(Item {
        id: row.get(0)?,
        original: row.get(1)?,
        answer: row.get(2)?,
        caveat: row.get(3)?,
        app: row.get(4)?,
        created_at: row.get(5)?,
        kind: row.get(6)?,
        source_language: row.get(7)?,
        target_language: row.get(8)?,
        model: row.get(9)?,
        note: row.get(10)?,
        review_status,
        tags: None,
    })
}

/// Map a result row to a [`HistoryEntry`].
fn row_to_history_entry(row: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        original: row.get(1)?,
        answer: row.get(2)?,
        kind: row.get(3)?,
        source_language: row.get(4)?,
        target_language: row.get(5)?,
        model: row.get(6)?,
        app: row.get(7)?,
        provenance: row.get(8)?,
        provider_scope: row.get(9)?,
        created_at: row.get(10)?,
    })
}

/// Extension trait for optional query results.
trait OptionalExt {
    fn optional(self) -> Option<String>;
}

impl OptionalExt for Result<String, rusqlite::Error> {
    fn optional(self) -> Option<String> {
        match self {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => panic!("unexpected database error: {e}"),
        }
    }
}

// -- saved.md import parser -------------------------------------------------

/// One parsed block from a `saved.md` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedEntry {
    /// The original text (first blockquote block).
    pub original: String,
    /// The answer text (second blockquote block).
    pub answer: String,
    /// A caveat line after the blockquotes.
    pub caveat: Option<String>,
    /// Seconds since the Unix epoch.
    pub timestamp: i64,
    /// The application name from the heading.
    pub app: Option<String>,
}

/// Parse the content of a `saved.md` file into its entries.
///
/// Best effort: blocks that do not match the expected format are skipped.
/// The file itself is never modified.
pub fn parse_saved_md(content: &str) -> Vec<SavedEntry> {
    let mut entries = Vec::new();

    let blocks: Vec<&str> = content.split("\n---\n").collect();
    for block in blocks {
        let trimmed = block.trim();
        if trimmed.is_empty() || !trimmed.contains("## ") {
            continue;
        }
        if let Some(entry) = parse_one_block(trimmed) {
            entries.push(entry);
        }
    }
    entries
}

/// Parse one `---`-delimited block.
fn parse_one_block(block: &str) -> Option<SavedEntry> {
    let mut lines = block.lines().peekable();

    // Skip preamble lines until the heading.
    let mut heading_line = None;
    while let Some(line) = lines.peek() {
        if let Some(rest) = line.strip_prefix("## ") {
            heading_line = Some(rest.trim().to_owned());
            lines.next();
            break;
        }
        lines.next();
    }
    let heading = heading_line?;

    // Parse heading: "YYYY-MM-DD HH:MM" optionally followed by " · AppName".
    let (timestamp, app) = parse_heading(&heading)?;

    // Collect quoted blocks, separated by blank lines.
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("> ") {
            current.push(rest.to_owned());
        } else if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
            // Non-blank, non-quoted lines after the blockquotes are the
            // caveat. Gather this line and any remaining lines.
            let mut caveat_lines = vec![line.to_owned()];
            for remaining in lines.by_ref() {
                caveat_lines.push(remaining.to_owned());
            }
            let trimmed = caveat_lines.join("\n");
            let trimmed = trimmed.trim();
            if !trimmed.is_empty() && !blocks.is_empty() {
                return Some(SavedEntry {
                    original: blocks[0].join("\n"),
                    answer: if blocks.len() > 1 {
                        blocks[1].join("\n")
                    } else {
                        String::new()
                    },
                    caveat: Some(trimmed.to_owned()),
                    timestamp,
                    app: app.clone(),
                });
            }
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    if blocks.len() >= 2 {
        Some(SavedEntry {
            original: blocks[0].join("\n"),
            answer: blocks[1].join("\n"),
            caveat: None,
            timestamp,
            app,
        })
    } else {
        None
    }
}

/// Parse a heading into a UTC timestamp and an optional app name.
fn parse_heading(heading: &str) -> Option<(i64, Option<String>)> {
    let (date_part, app) = if let Some(idx) = heading.find(" · ") {
        (&heading[..idx], Some(heading[idx + 3..].trim().to_owned()))
    } else {
        (heading, None)
    };

    let parts: Vec<&str> = date_part.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if date_parts.len() < 3 || time_parts.len() < 2 {
        return None;
    }

    let year: i64 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;
    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;

    let days = days_from_civil(year, month as i64, day as i64);
    let timestamp = days * 86_400 + hour as i64 * 3600 + minute as i64 * 60;

    Some((timestamp, app))
}

/// Calendar date to days since the Unix epoch.
///
/// Howard Hinnant's `days_from_civil`, the inverse of the function in
/// [`lens_core::keep`](../../crates/lens-core/src/keep.rs).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Format a Unix timestamp as "YYYY-MM-DD HH:MM" in UTC.
fn format_datetime(seconds: i64) -> String {
    let days = seconds / 86_400;
    let time_of_day = seconds % 86_400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Reverse civil_from_days (from keep.rs)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year}-{month:02}-{day:02} {hours:02}:{minutes:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mori-lens-storage-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("library.db")
    }

    fn cleanup(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    fn a_translation() -> Item {
        Item {
            id: 0,
            original: "The pointer must rest before anything is read.".to_owned(),
            answer: "포인터가 멈춘 뒤에야 무엇이든 읽습니다.".to_owned(),
            caveat: None,
            app: Some("Code".to_owned()),
            created_at: 1_786_075_140,
            kind: "translation".to_owned(),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            model: None,
            note: None,
            tags: None,
            review_status: ReviewStatus::Later,
        }
    }

    fn an_analysis() -> Item {
        Item {
            id: 0,
            original: "RowQuery는 멱등성을 보장하지 않습니다.".to_owned(),
            answer: "원형 · 멱등성\n품사 · 명사".to_owned(),
            caveat: Some("AI 설명 · 사실 확인 필요".to_owned()),
            app: None,
            created_at: 1_786_075_200,
            kind: "analysis".to_owned(),
            source_language: Some("ko".to_owned()),
            target_language: Some("ko".to_owned()),
            model: Some("qwen2.5:7b".to_owned()),
            note: None,
            tags: None,
            review_status: ReviewStatus::Later,
        }
    }

    // -- schema and migration -----------------------------------------------

    #[test]
    fn opening_a_fresh_database_creates_the_schema() {
        let path = temp_db("fresh");
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.count().unwrap(), 0);

        // All migration versions are recorded.
        let versions: Vec<i64> = lib
            .conn()
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7]);
        cleanup(&path);
    }

    #[test]
    fn reopening_runs_no_new_migrations() {
        let path = temp_db("reopen");
        {
            let _lib = Library::open(&path).unwrap();
        }
        // Second open must succeed and see the same schema.
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.count().unwrap(), 0);
        cleanup(&path);
    }

    // -- insert and get -----------------------------------------------------

    #[test]
    fn inserting_an_item_assigns_an_id_and_stores_every_field() {
        let path = temp_db("insert");
        let lib = Library::open(&path).unwrap();
        let item = a_translation();

        let id = lib.insert(&item).unwrap();
        assert!(id > 0, "assigned a positive id");

        let fetched = lib.get(id).unwrap().unwrap();
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.original, item.original);
        assert_eq!(fetched.answer, item.answer);
        assert_eq!(fetched.caveat, item.caveat);
        assert_eq!(fetched.app, item.app);
        assert_eq!(fetched.created_at, item.created_at);
        assert_eq!(fetched.kind, "translation");
        assert_eq!(fetched.source_language.as_deref(), Some("en"));
        assert_eq!(fetched.target_language.as_deref(), Some("ko"));
        cleanup(&path);
    }

    #[test]
    fn an_analysis_item_keeps_its_caveat_and_model() {
        let path = temp_db("analysis");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&an_analysis()).unwrap();

        let fetched = lib.get(id).unwrap().unwrap();
        assert_eq!(fetched.kind, "analysis");
        assert!(fetched.caveat.unwrap().contains("AI"));
        assert_eq!(fetched.model.as_deref(), Some("qwen2.5:7b"));
        cleanup(&path);
    }

    // -- list ---------------------------------------------------------------

    #[test]
    fn listing_returns_newest_first() {
        let path = temp_db("list-order");
        let lib = Library::open(&path).unwrap();

        let mut first = a_translation();
        first.created_at = 100;
        let mut second = a_translation();
        second.created_at = 200;
        second.original = "second".to_owned();

        lib.insert(&first).unwrap();
        lib.insert(&second).unwrap();

        let page = lib.list(0).unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].original, "second");
        assert!(!page.has_more);
        cleanup(&path);
    }

    #[test]
    fn listing_pages_past_the_first_batch() {
        let path = temp_db("list-page");
        let lib = Library::open(&path).unwrap();

        for i in 0..55 {
            let mut item = a_translation();
            item.created_at = 1_000 + i;
            item.original = format!("item {i}");
            lib.insert(&item).unwrap();
        }

        let first = lib.list(0).unwrap();
        assert_eq!(first.items.len(), 50);
        assert!(first.has_more);

        let second = lib.list(50).unwrap();
        assert_eq!(second.items.len(), 5);
        assert!(!second.has_more);
        cleanup(&path);
    }

    // -- filtered list ------------------------------------------------------

    #[test]
    fn filtering_by_kind_returns_only_matching_items() {
        let path = temp_db("filter-kind");
        let lib = Library::open(&path).unwrap();

        lib.insert(&a_translation()).unwrap();
        lib.insert(&an_analysis()).unwrap();

        let filter = Filter {
            kind: Some("translation".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].kind, "translation");

        let filter = Filter {
            kind: Some("analysis".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].kind, "analysis");

        cleanup(&path);
    }

    #[test]
    fn filtering_by_app_returns_only_matching_items() {
        let path = temp_db("filter-app");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.app = Some("Chrome".to_owned());
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.app = Some("VSCode".to_owned());
        lib.insert(&item2).unwrap();

        let filter = Filter {
            app: Some("Chrome".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].app.as_deref(), Some("Chrome"));

        cleanup(&path);
    }

    // -- tags ---------------------------------------------------------------

    #[test]
    fn setting_and_getting_tags() {
        let path = temp_db("tags");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        // Initially no tags.
        assert!(lib.get_tags(id).unwrap().is_empty());

        // Set some tags.
        assert!(lib
            .set_tags(id, &["grammar".to_owned(), "vocabulary".to_owned()])
            .unwrap());
        let tags = lib.get_tags(id).unwrap();
        assert_eq!(tags, vec!["grammar", "vocabulary"]);

        // Replace tags.
        assert!(lib.set_tags(id, &["review".to_owned()]).unwrap());
        let tags = lib.get_tags(id).unwrap();
        assert_eq!(tags, vec!["review"]);

        // Setting tags on a nonexistent item returns false.
        assert!(!lib.set_tags(999, &["orphan".to_owned()]).unwrap());

        cleanup(&path);
    }

    #[test]
    fn empty_tags_are_ignored() {
        let path = temp_db("tags-empty");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        lib.set_tags(id, &["".to_owned(), "  ".to_owned(), "valid".to_owned()])
            .unwrap();
        let tags = lib.get_tags(id).unwrap();
        assert_eq!(tags, vec!["valid"]);

        cleanup(&path);
    }

    // -- search -------------------------------------------------------------

    #[test]
    fn search_finds_items_by_original_text() {
        let path = temp_db("search-orig");
        let lib = Library::open(&path).unwrap();

        lib.insert(&a_translation()).unwrap();
        let mut other = a_translation();
        other.original = "unrelated text".to_owned();
        other.created_at += 1;
        lib.insert(&other).unwrap();

        let page = lib.search("pointer", 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].original.contains("pointer"));
        cleanup(&path);
    }

    #[test]
    fn search_finds_items_by_answer_text() {
        let path = temp_db("search-answer");
        let lib = Library::open(&path).unwrap();

        lib.insert(&a_translation()).unwrap();

        // The answer contains the Korean word for "pointer".
        let page = lib.search("포인터", 0).unwrap();
        assert_eq!(page.items.len(), 1);
        cleanup(&path);
    }

    #[test]
    fn an_empty_search_falls_back_to_listing() {
        let path = temp_db("search-empty");
        let lib = Library::open(&path).unwrap();

        lib.insert(&a_translation()).unwrap();

        let page = lib.search("", 0).unwrap();
        assert_eq!(page.items.len(), 1);
        cleanup(&path);
    }

    // -- delete -------------------------------------------------------------

    #[test]
    fn deleting_removes_the_item_and_its_search_entry() {
        let path = temp_db("delete");
        let lib = Library::open(&path).unwrap();

        let id = lib.insert(&a_translation()).unwrap();
        assert_eq!(lib.count().unwrap(), 1);

        let deleted = lib.delete(id).unwrap();
        assert!(deleted);
        assert_eq!(lib.count().unwrap(), 0);
        assert!(lib.get(id).unwrap().is_none());

        // Search finds nothing after deletion.
        let page = lib.search("pointer", 0).unwrap();
        assert!(page.items.is_empty());
        cleanup(&path);
    }

    #[test]
    fn deleting_a_nonexistent_item_returns_false() {
        let path = temp_db("delete-miss");
        let lib = Library::open(&path).unwrap();
        assert!(!lib.delete(999).unwrap());
        cleanup(&path);
    }

    #[test]
    fn updating_a_note_changes_it_and_returns_true() {
        let path = temp_db("update-note");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        assert!(lib.update_note(id, "my note").unwrap());
        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.note.as_deref(), Some("my note"));

        assert!(lib.update_note(id, "").unwrap());
        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.note.as_deref(), Some(""));

        assert!(!lib.update_note(999, "no such item").unwrap());
        cleanup(&path);
    }

    #[test]
    fn exporting_generates_saved_md_format() {
        let path = temp_db("export");
        let lib = Library::open(&path).unwrap();

        // Empty library exports to empty string.
        assert_eq!(lib.export_markdown().unwrap(), "");

        let id = lib.insert(&a_translation()).unwrap();
        let md = lib.export_markdown().unwrap();

        // Should contain the heading with timestamp and app.
        assert!(md.contains("## 2026-08-07 03:59 · Code"), "md was: {md}");
        // Should contain quoted original.
        assert!(md.contains("> The pointer must rest"));
        // Should contain quoted translation.
        assert!(md.contains("> 포인터가 멈춘"));

        // Parse it back and verify it round-trips.
        let entries = parse_saved_md(&md);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].original,
            "The pointer must rest before anything is read."
        );
        assert_eq!(entries[0].answer, "포인터가 멈춘 뒤에야 무엇이든 읽습니다.");
        assert_eq!(entries[0].app.as_deref(), Some("Code"));

        let _ = id;
        cleanup(&path);
    }

    #[test]
    fn export_includes_caveat_when_present() {
        let path = temp_db("export-caveat");
        let lib = Library::open(&path).unwrap();

        let item = Item {
            id: 0,
            original: "The output files came back empty.".to_owned(),
            answer: "출력 파일이 비어 있었습니다.".to_owned(),
            caveat: Some("This is an AI explanation.".to_owned()),
            app: Some("TestApp".to_owned()),
            created_at: 1_786_075_140,
            kind: "translation".to_owned(),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            model: None,
            note: None,
            tags: None,
            review_status: ReviewStatus::Later,
        };
        lib.insert(&item).unwrap();

        let md = lib.export_markdown().unwrap();
        assert!(md.contains("This is an AI explanation."));

        let entries = parse_saved_md(&md);
        assert_eq!(
            entries[0].caveat.as_deref(),
            Some("This is an AI explanation.")
        );

        cleanup(&path);
    }

    #[test]
    fn exporting_json_includes_all_fields_and_tags() {
        let path = temp_db("export-json");
        let lib = Library::open(&path).unwrap();

        let item = Item {
            id: 0,
            original: "test".to_owned(),
            answer: "답".to_owned(),
            caveat: Some("note".to_owned()),
            app: Some("App".to_owned()),
            created_at: 1_786_075_140,
            kind: "translation".to_owned(),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            model: Some("qwen".to_owned()),
            note: Some("my note".to_owned()),
            tags: None,
            review_status: ReviewStatus::Later,
        };
        let id = lib.insert(&item).unwrap();
        lib.set_tags(id, &["tag1".to_owned(), "tag2".to_owned()])
            .unwrap();

        let json = lib.export_json().unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);

        let obj = &parsed[0];
        assert_eq!(obj["original"], "test");
        assert_eq!(obj["answer"], "답");
        assert_eq!(obj["caveat"], "note");
        assert_eq!(obj["app"], "App");
        assert_eq!(obj["kind"], "translation");
        assert_eq!(obj["note"], "my note");
        assert_eq!(obj["tags"], serde_json::json!(["tag1", "tag2"]));

        cleanup(&path);
    }

    #[test]
    fn jsonl_export_has_a_version_header_and_one_item_per_line() {
        let path = temp_db("export-jsonl");
        let lib = Library::open(&path).unwrap();

        let mut item = Item {
            id: 0,
            original: "first".to_owned(),
            answer: "첫째".to_owned(),
            caveat: None,
            app: None,
            created_at: 1_786_075_140,
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            note: None,
            tags: None,
            review_status: ReviewStatus::Later,
        };
        lib.insert(&item).unwrap();
        item.original = "second".to_owned();
        item.answer = "둘째".to_owned();
        item.created_at = 1_786_075_200;
        lib.insert(&item).unwrap();

        let jsonl = lib.export_jsonl().unwrap();
        let lines: Vec<&str> = jsonl.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header["version"], 1);

        let first: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        let second: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(first["original"], "second");
        assert_eq!(second["original"], "first");

        cleanup(&path);
    }

    #[test]
    fn jsonl_round_trip_preserves_items_and_tags() {
        let src = temp_db("jsonl-src");
        let dst = temp_db("jsonl-dst");
        let lib_a = Library::open(&src).unwrap();

        let mut item = Item {
            id: 0,
            original: "hello world".to_owned(),
            answer: "안녕 세상".to_owned(),
            caveat: None,
            app: Some("TestApp".to_owned()),
            created_at: 1_786_075_140,
            kind: "translation".to_owned(),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            model: None,
            note: Some("a note".to_owned()),
            tags: None,
            review_status: ReviewStatus::Later,
        };
        let id = lib_a.insert(&item).unwrap();
        lib_a
            .set_tags(id, &["greeting".to_owned(), "test".to_owned()])
            .unwrap();

        item.original = "second item".to_owned();
        item.answer = "둘째 항목".to_owned();
        item.created_at = 1_786_075_200;
        item.kind = "analysis".to_owned();
        lib_a.insert(&item).unwrap();

        let jsonl = lib_a.export_jsonl().unwrap();

        // Import into a fresh database.
        let lib_b = Library::open(&dst).unwrap();
        let count = lib_b.import_jsonl(&jsonl).unwrap();
        assert_eq!(count, 2);

        // Verify the imported data.
        assert_eq!(lib_b.count().unwrap(), 2);
        let page = lib_b.search("hello world", 0).unwrap();
        assert_eq!(page.items.len(), 1);
        let tags = lib_b.get_tags(page.items[0].id).unwrap();
        assert!(tags.contains(&"greeting".to_owned()));
        assert!(tags.contains(&"test".to_owned()));

        // Re-importing the same data skips duplicates.
        let count2 = lib_b.import_jsonl(&jsonl).unwrap();
        assert_eq!(count2, 0);
        assert_eq!(lib_b.count().unwrap(), 2);

        cleanup(&src);
        cleanup(&dst);
    }

    // -- import tracking ----------------------------------------------------

    #[test]
    fn import_tracking_starts_false_and_flips_to_true() {
        let path = temp_db("import-track");
        let lib = Library::open(&path).unwrap();

        assert!(!lib.is_imported().unwrap());
        lib.mark_imported().unwrap();
        assert!(lib.is_imported().unwrap());
        cleanup(&path);
    }

    // -- saved.md parser ----------------------------------------------------

    #[test]
    fn parsing_a_translation_block() {
        let content = concat!(
            "# Saved from Mori Lens\n",
            "\n",
            "---\n",
            "\n",
            "## 2026-08-07 03:59 · Code\n",
            "\n",
            "> The pointer must rest before anything is read.\n",
            "\n",
            "> 포인터가 멈춘 뒤에야 무엇이든 읽습니다.\n",
        );
        let entries = parse_saved_md(content);
        assert_eq!(entries.len(), 1);

        let e = &entries[0];
        assert_eq!(e.original, "The pointer must rest before anything is read.");
        assert_eq!(e.answer, "포인터가 멈춘 뒤에야 무엇이든 읽습니다.");
        assert_eq!(e.app.as_deref(), Some("Code"));
        assert!(e.caveat.is_none());
        assert_eq!(e.timestamp, 1_786_075_140);
    }

    #[test]
    fn parsing_an_analysis_block_with_caveat() {
        let content = concat!(
            "---\n",
            "\n",
            "## 2026-08-07 03:59\n",
            "\n",
            "> RowQuery는 멱등성을 보장하지 않습니다.\n",
            "\n",
            "> 원형 · 멱등성\n",
            "> 품사 · 명사\n",
            "\n",
            "AI 설명 · 사실 확인 필요\n",
        );
        let entries = parse_saved_md(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].caveat.as_deref(),
            Some("AI 설명 · 사실 확인 필요")
        );
    }

    #[test]
    fn multiple_blocks_are_parsed_in_order() {
        let content = concat!(
            "---\n",
            "\n",
            "## 2026-08-07 03:59\n",
            "\n",
            "> first\n",
            "\n",
            "> 첫 번째\n",
            "\n",
            "---\n",
            "\n",
            "## 2026-08-07 04:00\n",
            "\n",
            "> second\n",
            "\n",
            "> 둘째\n",
        );
        let entries = parse_saved_md(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].original, "first");
        assert_eq!(entries[1].original, "second");
        assert!(entries[0].timestamp < entries[1].timestamp);
    }

    #[test]
    fn an_empty_file_produces_no_entries() {
        assert!(parse_saved_md("").is_empty());
        assert!(parse_saved_md("# Saved from Mori Lens\n").is_empty());
    }

    #[test]
    fn the_preamble_is_skipped() {
        let content = concat!(
            "# Saved from Mori Lens\n",
            "\n",
            "One block per thing you kept.\n",
            "\n",
            "---\n",
            "\n",
            "## 2026-08-07 03:59\n",
            "\n",
            "> hello\n",
            "\n",
            "> 안녕\n",
        );
        let entries = parse_saved_md(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].original, "hello");
    }

    // -- date arithmetic ----------------------------------------------------

    #[test]
    fn days_from_civil_matches_known_dates() {
        // 1970-01-01 is day 0.
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        // 2026-08-07: verified against the keep.rs date_of(1_786_075_140).
        let d = days_from_civil(2026, 8, 7);
        assert_eq!(d * 86_400 + 3 * 3600 + 59 * 60, 1_786_075_140);
    }

    #[test]
    fn days_from_civil_round_trips_with_keep_rs() {
        // Verify our days_from_civil produces the same timestamps that
        // keep.rs's date_of formats. The pair (timestamp, expected_date)
        // is taken from keep.rs's own test.
        for (seconds, expected_date) in [
            (0u64, "1970-01-01"),
            (951_782_400, "2000-02-29"),
            (1_709_164_800, "2024-02-29"),
            (1_786_075_140, "2026-08-07"),
        ] {
            let days = (seconds / 86_400) as i64;
            // Reverse civil_from_days (from keep.rs) to get y/m/d.
            let z = days + 719_468;
            let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
            let day_of_era = (z - era * 146_097) as u64;
            let year_of_era =
                (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
            let year = year_of_era as i64 + era * 400;
            let day_of_year =
                day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
            let shifted_month = (5 * day_of_year + 2) / 153;
            let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
            let month = if shifted_month < 10 {
                shifted_month + 3
            } else {
                shifted_month - 9
            } as u32;
            let year = if month <= 2 { year + 1 } else { year };

            // Format the date string from civil_from_days's output.
            let formatted = format!("{year:04}-{month:02}-{day:02}");
            assert_eq!(formatted, expected_date, "at {seconds}");

            // Our days_from_civil must produce the same day count.
            assert_eq!(
                days_from_civil(year, month as i64, day as i64),
                days,
                "round-trip at {seconds}"
            );
        }
    }

    // -- distinct queries ---------------------------------------------------

    #[test]
    fn distinct_apps_returns_sorted_unique_values() {
        let path = temp_db("distinct-apps");
        let lib = Library::open(&path).unwrap();

        let mut item = a_translation();
        item.app = Some("Firefox".to_owned());
        lib.insert(&item).unwrap();

        item.app = Some("Code".to_owned());
        lib.insert(&item).unwrap();

        item.app = Some("Firefox".to_owned());
        lib.insert(&item).unwrap();

        item.app = None;
        lib.insert(&item).unwrap();

        let apps = lib.distinct_apps().unwrap();
        assert_eq!(apps, vec!["Code", "Firefox"]);
        cleanup(&path);
    }

    #[test]
    fn distinct_source_languages_returns_sorted_unique_values() {
        let path = temp_db("distinct-sources");
        let lib = Library::open(&path).unwrap();

        let mut item = a_translation();
        item.source_language = Some("en".to_owned());
        lib.insert(&item).unwrap();

        item.source_language = Some("ja".to_owned());
        lib.insert(&item).unwrap();

        item.source_language = None;
        lib.insert(&item).unwrap();

        let langs = lib.distinct_source_languages().unwrap();
        assert_eq!(langs, vec!["en", "ja"]);
        cleanup(&path);
    }

    #[test]
    fn distinct_target_languages_returns_sorted_unique_values() {
        let path = temp_db("distinct-targets");
        let lib = Library::open(&path).unwrap();

        let mut item = a_translation();
        item.target_language = Some("ko".to_owned());
        lib.insert(&item).unwrap();

        item.target_language = Some("ja".to_owned());
        lib.insert(&item).unwrap();

        let langs = lib.distinct_target_languages().unwrap();
        assert_eq!(langs, vec!["ja", "ko"]);
        cleanup(&path);
    }

    #[test]
    fn has_duplicate_finds_matching_original_and_kind() {
        let path = temp_db("has-duplicate");
        let lib = Library::open(&path).unwrap();

        let item = a_translation();
        lib.insert(&item).unwrap();

        assert!(lib.has_duplicate(&item.original, "translation").unwrap());
        assert!(!lib.has_duplicate(&item.original, "analysis").unwrap());
        assert!(!lib.has_duplicate("different text", "translation").unwrap());
        cleanup(&path);
    }

    /// A secure wipe removes all rows and leaves a usable database.
    #[test]
    fn secure_wipe_removes_everything_and_leaves_a_working_database() {
        let path = temp_db("secure-wipe");
        let lib = Library::open(&path).unwrap();

        // Insert a few items with tags.
        let mut item = a_translation();
        lib.insert(&item).unwrap();
        item.original = "A second sentence.".to_owned();
        lib.insert(&item).unwrap();
        lib.set_tags(1, &["vocab".to_owned(), "important".to_owned()])
            .unwrap();

        assert_eq!(lib.count().unwrap(), 2);

        // Wipe.
        lib.secure_wipe().unwrap();

        // Everything is gone.
        assert_eq!(lib.count().unwrap(), 0);
        assert!(lib.get(1).unwrap().is_none());
        assert!(lib.search("sentence", 0).unwrap().items.is_empty());

        // The database is still usable.
        item.original = "After the wipe.".to_owned();
        lib.insert(&item).unwrap();
        assert_eq!(lib.count().unwrap(), 1);

        cleanup(&path);
    }

    /// Secure wipe also removes persistent history (CORRECTNESS fix).
    #[test]
    fn secure_wipe_removes_history() {
        let path = temp_db("wipe-history");
        let lib = Library::open(&path).unwrap();

        // Add library items
        let item = a_translation();
        lib.insert(&item).unwrap();

        // Add history entries
        let entry = HistoryEntry {
            id: 0,
            original: "history original".to_owned(),
            answer: "history answer".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: Some("chrome.exe".to_owned()),
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        lib.record_history(&entry).unwrap();
        assert_eq!(lib.history_count().unwrap(), 1);

        // Wipe
        lib.secure_wipe().unwrap();

        // Verify library is empty
        assert_eq!(lib.count().unwrap(), 0);

        // Verify history is also empty
        assert_eq!(lib.history_count().unwrap(), 0);
        assert_eq!(lib.history_list(0).unwrap().entries.len(), 0);

        cleanup(&path);
    }

    /// Secure wipe persists across database restart.
    #[test]
    fn secure_wipe_survives_reopen() {
        let path = temp_db("wipe-reopen");
        let lib = Library::open(&path).unwrap();

        // Add data
        let item = a_translation();
        lib.insert(&item).unwrap();

        let entry = HistoryEntry {
            id: 0,
            original: "test".to_owned(),
            answer: "test".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        lib.record_history(&entry).unwrap();

        lib.secure_wipe().unwrap();
        drop(lib);

        // Reopen
        let lib2 = Library::open(&path).unwrap();
        assert_eq!(lib2.count().unwrap(), 0);
        assert_eq!(lib2.history_count().unwrap(), 0);

        cleanup(&path);
    }

    /// Fresh database reaches latest schema with all tables and indexes.
    #[test]
    fn fresh_database_reaches_latest_schema() {
        let path = temp_db("fresh-schema");
        let lib = Library::open(&path).unwrap();

        // Verify all tables exist
        let tables: Vec<String> = lib
            .conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"library".to_owned()));
        assert!(tables.contains(&"meta".to_owned()));
        assert!(tables.contains(&"tags".to_owned()));
        assert!(tables.contains(&"library_tags".to_owned()));
        assert!(tables.contains(&"history".to_owned()));
        assert!(tables.contains(&"schema_migrations".to_owned()));

        // Verify schema version is 7
        let version: i64 = lib
            .conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 7);

        // Verify library table has manual review column (migration 7
        // dropped next_review_at and review_count, added legacy_review_state).
        let columns: Vec<String> = lib
            .conn
            .prepare("PRAGMA table_info(library)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.contains(&"review_state".to_owned()));
        assert!(columns.contains(&"legacy_review_state".to_owned()));
        assert!(!columns.contains(&"next_review_at".to_owned()));
        assert!(!columns.contains(&"review_count".to_owned()));

        cleanup(&path);
    }

    /// Reopening latest schema is idempotent (no re-migration).
    #[test]
    fn latest_schema_reopen_is_idempotent() {
        let path = temp_db("idempotent-reopen");
        let lib = Library::open(&path).unwrap();

        // Add some data
        let item = a_translation();
        lib.insert(&item).unwrap();
        lib.set_tags(1, &["test".to_owned()]).unwrap();

        let entry = HistoryEntry {
            id: 0,
            original: "test".to_owned(),
            answer: "test".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        lib.record_history(&entry).unwrap();

        // Record counts before reopen
        let count_before = lib.count().unwrap();
        let history_before = lib.history_count().unwrap();

        drop(lib);

        // Reopen
        let lib2 = Library::open(&path).unwrap();

        // Verify no data duplication or loss
        assert_eq!(lib2.count().unwrap(), count_before);
        assert_eq!(lib2.history_count().unwrap(), history_before);

        // Verify schema version is still 7
        let version: i64 = lib2
            .conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 7);

        cleanup(&path);
    }

    /// JSONL semantic round-trip preserves user-visible content.
    #[test]
    fn jsonl_semantic_round_trip() {
        let path_a = temp_db("jsonl-export");
        let lib_a = Library::open(&path_a).unwrap();

        // Create diverse items
        let mut item1 = a_translation();
        item1.original = "plain item".to_owned();
        lib_a.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.original = "item with note".to_owned();
        item2.note = Some("my note".to_owned());
        let id2 = lib_a.insert(&item2).unwrap();
        lib_a
            .set_tags(id2, &["tag1".to_owned(), "tag2".to_owned()])
            .unwrap();

        let mut item3 = a_translation();
        item3.original = "Unicode: 안녕하세요 세계".to_owned();
        item3.answer = "Hello world".to_owned();
        lib_a.insert(&item3).unwrap();

        let mut item4 = a_translation();
        item4.original = "quotes \"and\" newlines\n".to_owned();
        lib_a.insert(&item4).unwrap();

        // Export
        let jsonl = lib_a.export_jsonl().unwrap();
        drop(lib_a);

        // Import to fresh DB
        let path_b = temp_db("jsonl-import");
        let lib_b = Library::open(&path_b).unwrap();
        let imported = lib_b.import_jsonl(&jsonl).unwrap();
        assert_eq!(imported, 4);

        // Verify semantic equality (not ID equality)
        let items = lib_b.list(0).unwrap().items;
        assert_eq!(items.len(), 4);

        // Check specific items
        let plain = items.iter().find(|i| i.original == "plain item").unwrap();
        assert!(plain.note.is_none());

        let with_note = items
            .iter()
            .find(|i| i.original == "item with note")
            .unwrap();
        assert_eq!(with_note.note.as_deref(), Some("my note"));
        let tags = lib_b.get_tags(with_note.id).unwrap();
        assert_eq!(tags, vec!["tag1".to_owned(), "tag2".to_owned()]);

        let unicode = items
            .iter()
            .find(|i| i.original.contains("안녕하세요"))
            .unwrap();
        assert_eq!(unicode.answer, "Hello world");

        cleanup(&path_a);
        cleanup(&path_b);
    }

    /// JSONL round-trip preserves review status.
    #[test]
    fn jsonl_round_trip_preserves_review_status() {
        let path_a = temp_db("jsonl-review-export");
        let lib_a = Library::open(&path_a).unwrap();

        let item = a_translation();
        let id = lib_a.insert(&item).unwrap();
        lib_a.set_review_status(id, ReviewStatus::Known).unwrap();

        let item_before = lib_a.get(id).unwrap().unwrap();
        assert_eq!(item_before.review_status, ReviewStatus::Known);

        let jsonl = lib_a.export_jsonl().unwrap();
        let path_b = temp_db("jsonl-review-import");
        let lib_b = Library::open(&path_b).unwrap();
        lib_b.import_jsonl(&jsonl).unwrap();

        let items = lib_b.list(0).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].review_status, ReviewStatus::Known);

        cleanup(&path_a);
        cleanup(&path_b);
    }

    /// Malformed JSONL does not corrupt database.
    #[test]
    fn malformed_jsonl_does_not_corrupt_database() {
        let path = temp_db("jsonl-malformed");
        let lib = Library::open(&path).unwrap();

        // Add a valid item first
        let item = a_translation();
        lib.insert(&item).unwrap();
        assert_eq!(lib.count().unwrap(), 1);

        // Try to import malformed JSONL
        let malformed = r#"{"version":1}
{"id":0,"original":"valid","answer":"valid","kind":"translation","created_at":1000}
{invalid json here}
{"id":0,"original":"after","answer":"after","kind":"translation","created_at":2000}
"#;

        // Import should fail
        let result = lib.import_jsonl(malformed);
        assert!(result.is_err());

        // But the first valid item should have been imported (partial import)
        // and the database should not be corrupted
        assert_eq!(lib.count().unwrap(), 2); // original + first valid line

        // Database is still usable
        let new_item = a_translation();
        lib.insert(&new_item).unwrap();
        assert_eq!(lib.count().unwrap(), 3);

        cleanup(&path);
    }

    /// Missing required field in JSONL causes error.
    #[test]
    fn jsonl_missing_required_field() {
        let path = temp_db("jsonl-missing-field");
        let lib = Library::open(&path).unwrap();

        let missing_original = r#"{"version":1}
{"id":0,"answer":"has answer but no original","kind":"translation","created_at":1000}
"#;

        let result = lib.import_jsonl(missing_original);
        assert!(result.is_err());
        assert_eq!(lib.count().unwrap(), 0); // Nothing imported

        cleanup(&path);
    }

    /// Unknown extra fields in JSONL are ignored.
    #[test]
    fn jsonl_unknown_extra_fields_ignored() {
        let path = temp_db("jsonl-extra-fields");
        let lib = Library::open(&path).unwrap();

        let extra_fields = r#"{"version":1}
{"id":0,"original":"test","answer":"test","kind":"translation","created_at":1000,"unknown_field":"value","another":123}
"#;

        let result = lib.import_jsonl(extra_fields);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
        assert_eq!(lib.count().unwrap(), 1);

        cleanup(&path);
    }

    /// Blank lines in JSONL are skipped.
    #[test]
    fn jsonl_blank_lines_skipped() {
        let path = temp_db("jsonl-blank-lines");
        let lib = Library::open(&path).unwrap();

        let with_blanks = r#"{"version":1}

{"id":0,"original":"test","answer":"test","kind":"translation","created_at":1000}

"#;

        let result = lib.import_jsonl(with_blanks);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        cleanup(&path);
    }

    /// Duplicate items are silently skipped.
    #[test]
    fn jsonl_duplicates_silently_skipped() {
        let path = temp_db("jsonl-duplicates");
        let lib = Library::open(&path).unwrap();

        let duplicates = r#"{"version":1}
{"id":0,"original":"same","answer":"first","kind":"translation","created_at":1000}
{"id":0,"original":"same","answer":"second","kind":"translation","created_at":2000}
{"id":0,"original":"different","answer":"third","kind":"translation","created_at":3000}
"#;

        let result = lib.import_jsonl(duplicates);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2); // First "same" + "different"
        assert_eq!(lib.count().unwrap(), 2);

        // Verify first answer was kept
        let items = lib.list(0).unwrap().items;
        let same_item = items.iter().find(|i| i.original == "same").unwrap();
        assert_eq!(same_item.answer, "first");

        cleanup(&path);
    }

    /// History → Library promotion creates independent item.
    #[test]
    fn history_promoted_to_library_is_independent() {
        let path = temp_db("history-to-library");
        let lib = Library::open(&path).unwrap();

        // Create history entry
        let entry = HistoryEntry {
            id: 0,
            original: "test original".to_owned(),
            answer: "test answer".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: Some("chrome.exe".to_owned()),
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        let history_id = lib.record_history(&entry).unwrap();
        assert_eq!(lib.history_count().unwrap(), 1);

        // Promote to library
        let library_id = lib.history_promote(history_id).unwrap();
        assert!(library_id > 0);

        // Verify history entry is deleted
        assert_eq!(lib.history_count().unwrap(), 0);

        // Verify library item exists
        let item = lib.get(library_id).unwrap().unwrap();
        assert_eq!(item.original, "test original");
        assert_eq!(item.answer, "test answer");
        assert_eq!(item.review_status, ReviewStatus::Later);

        cleanup(&path);
    }

    /// Deleting library item does not affect history (they're independent).
    #[test]
    fn library_delete_does_not_affect_history() {
        let path = temp_db("library-delete-independence");
        let lib = Library::open(&path).unwrap();

        // Create library item
        let item = a_translation();
        let lib_id = lib.insert(&item).unwrap();

        // Create history entry with same content
        let entry = HistoryEntry {
            id: 0,
            original: item.original.clone(),
            answer: item.answer.clone(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        let history_id = lib.record_history(&entry).unwrap();

        // Delete library item
        lib.delete(lib_id).unwrap();
        assert_eq!(lib.count().unwrap(), 0);

        // History should still exist
        assert_eq!(lib.history_count().unwrap(), 1);
        let history_item = lib.history_list(0).unwrap().entries[0].clone();
        assert_eq!(history_item.id, history_id);

        cleanup(&path);
    }

    /// Promoting same history entry twice: second attempt fails.
    #[test]
    fn history_promote_twice_fails() {
        // After promotion, the history entry is deleted, so promoting again fails.
        let path = temp_db("history-promote-twice");
        let lib = Library::open(&path).unwrap();

        let entry = HistoryEntry {
            id: 0,
            original: "test".to_owned(),
            answer: "test".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        let history_id = lib.record_history(&entry).unwrap();

        // Promote once
        let _lib_id_1 = lib.history_promote(history_id).unwrap();
        assert_eq!(lib.count().unwrap(), 1);
        assert_eq!(lib.history_count().unwrap(), 0); // History entry deleted

        // Try to promote again - should fail because history entry is gone
        let result = lib.history_promote(history_id);
        assert!(result.is_err());

        // Still only one library item
        assert_eq!(lib.count().unwrap(), 1);

        cleanup(&path);
    }

    /// Promoted history items enter the manual review queue as Later.
    #[test]
    fn history_promote_sets_later_status() {
        let path = temp_db("history-promote-review");
        let lib = Library::open(&path).unwrap();

        let entry = HistoryEntry {
            id: 0,
            original: "test".to_owned(),
            answer: "test".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        let history_id = lib.record_history(&entry).unwrap();
        let library_id = lib.history_promote(history_id).unwrap();

        let item = lib.get(library_id).unwrap().unwrap();
        assert_eq!(item.review_status, ReviewStatus::Later);

        cleanup(&path);
    }

    // -- manual review (Issue #141) -----------------------------------------

    #[test]
    fn new_items_default_to_later() {
        let path = temp_db("review-default");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.review_status, ReviewStatus::Later);

        cleanup(&path);
    }

    #[test]
    fn set_review_status_transitions_and_is_reversible() {
        let path = temp_db("review-transitions");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        // Later → Known
        assert!(lib.set_review_status(id, ReviewStatus::Known).unwrap());
        assert_eq!(
            lib.get(id).unwrap().unwrap().review_status,
            ReviewStatus::Known
        );

        // Known → Later (reversible)
        assert!(lib.set_review_status(id, ReviewStatus::Later).unwrap());
        assert_eq!(
            lib.get(id).unwrap().unwrap().review_status,
            ReviewStatus::Later
        );

        // Later → Archive
        assert!(lib.set_review_status(id, ReviewStatus::Archive).unwrap());
        assert_eq!(
            lib.get(id).unwrap().unwrap().review_status,
            ReviewStatus::Archive
        );

        // Archive → Known
        assert!(lib.set_review_status(id, ReviewStatus::Known).unwrap());
        assert_eq!(
            lib.get(id).unwrap().unwrap().review_status,
            ReviewStatus::Known
        );

        cleanup(&path);
    }

    #[test]
    fn set_review_status_on_missing_item_returns_false() {
        let path = temp_db("review-missing");
        let lib = Library::open(&path).unwrap();
        assert!(!lib.set_review_status(999, ReviewStatus::Known).unwrap());
        cleanup(&path);
    }

    #[test]
    fn list_by_status_returns_only_matching_items() {
        let path = temp_db("review-list-by-status");
        let lib = Library::open(&path).unwrap();

        let mut later = a_translation();
        later.original = "later item".to_owned();
        let later_id = lib.insert(&later).unwrap();

        let mut known = a_translation();
        known.original = "known item".to_owned();
        known.created_at += 1;
        let known_id = lib.insert(&known).unwrap();
        lib.set_review_status(known_id, ReviewStatus::Known)
            .unwrap();

        let mut archive = a_translation();
        archive.original = "archive item".to_owned();
        archive.created_at += 2;
        let archive_id = lib.insert(&archive).unwrap();
        lib.set_review_status(archive_id, ReviewStatus::Archive)
            .unwrap();

        let later_page = lib.list_by_status(ReviewStatus::Later, 0).unwrap();
        assert_eq!(later_page.items.len(), 1);
        assert_eq!(later_page.items[0].id, later_id);

        let known_page = lib.list_by_status(ReviewStatus::Known, 0).unwrap();
        assert_eq!(known_page.items.len(), 1);
        assert_eq!(known_page.items[0].id, known_id);

        let archive_page = lib.list_by_status(ReviewStatus::Archive, 0).unwrap();
        assert_eq!(archive_page.items.len(), 1);
        assert_eq!(archive_page.items[0].id, archive_id);

        cleanup(&path);
    }

    #[test]
    fn count_by_status_reflects_transitions() {
        let path = temp_db("review-count");
        let lib = Library::open(&path).unwrap();

        assert_eq!(lib.count_by_status(ReviewStatus::Later).unwrap(), 0);

        let id = lib.insert(&a_translation()).unwrap();
        assert_eq!(lib.count_by_status(ReviewStatus::Later).unwrap(), 1);

        lib.set_review_status(id, ReviewStatus::Known).unwrap();
        assert_eq!(lib.count_by_status(ReviewStatus::Later).unwrap(), 0);
        assert_eq!(lib.count_by_status(ReviewStatus::Known).unwrap(), 1);

        cleanup(&path);
    }

    #[test]
    fn note_survives_status_transition() {
        let path = temp_db("review-note-survives-status");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        lib.update_note(id, "important context").unwrap();
        lib.set_review_status(id, ReviewStatus::Known).unwrap();

        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.note.as_deref(), Some("important context"));
        assert_eq!(item.review_status, ReviewStatus::Known);

        cleanup(&path);
    }

    #[test]
    fn status_survives_note_edit() {
        let path = temp_db("review-status-survives-note");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        lib.set_review_status(id, ReviewStatus::Archive).unwrap();
        lib.update_note(id, "a note").unwrap();

        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.review_status, ReviewStatus::Archive);
        assert_eq!(item.note.as_deref(), Some("a note"));

        cleanup(&path);
    }

    #[test]
    fn later_queue_ignores_wall_clock() {
        let path = temp_db("review-no-clock");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        // Later items are in the queue regardless of time.
        let later = lib.list_by_status(ReviewStatus::Later, 0).unwrap();
        assert_eq!(later.items.len(), 1);
        assert_eq!(later.items[0].id, id);

        // Even far in the future, the same items are in the queue.
        let later2 = lib.list_by_status(ReviewStatus::Later, 0).unwrap();
        assert_eq!(later2.items.len(), 1);

        cleanup(&path);
    }

    #[test]
    fn known_excluded_from_later_queue() {
        let path = temp_db("review-known-excluded");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();
        lib.set_review_status(id, ReviewStatus::Known).unwrap();

        let later = lib.list_by_status(ReviewStatus::Later, 0).unwrap();
        assert!(later.items.is_empty());

        cleanup(&path);
    }

    #[test]
    fn archive_excluded_from_later_queue() {
        let path = temp_db("review-archive-excluded");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();
        lib.set_review_status(id, ReviewStatus::Archive).unwrap();

        let later = lib.list_by_status(ReviewStatus::Later, 0).unwrap();
        assert!(later.items.is_empty());

        // Archive is retained and searchable.
        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.review_status, ReviewStatus::Archive);
        let search = lib.search("pointer", 0).unwrap();
        assert_eq!(search.items.len(), 1);

        cleanup(&path);
    }

    #[test]
    fn archive_is_not_delete() {
        let path = temp_db("review-archive-not-delete");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id, &["important".to_owned()]).unwrap();
        lib.update_note(id, "keep this").unwrap();
        lib.set_review_status(id, ReviewStatus::Archive).unwrap();

        // Row still exists.
        assert!(lib.get(id).unwrap().is_some());
        // Searchable.
        assert_eq!(lib.search("pointer", 0).unwrap().items.len(), 1);
        // Tags preserved.
        assert_eq!(lib.get_tags(id).unwrap(), vec!["important".to_owned()]);
        // Note preserved.
        assert_eq!(
            lib.get(id).unwrap().unwrap().note.as_deref(),
            Some("keep this")
        );
        // Counted in total.
        assert_eq!(lib.count().unwrap(), 1);

        cleanup(&path);
    }

    #[test]
    fn known_is_retained_and_searchable() {
        let path = temp_db("review-known-retained");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();
        lib.set_review_status(id, ReviewStatus::Known).unwrap();

        assert!(lib.get(id).unwrap().is_some());
        assert_eq!(lib.search("pointer", 0).unwrap().items.len(), 1);
        assert_eq!(lib.count().unwrap(), 1);

        cleanup(&path);
    }

    // -- History tests ------------------------------------------------------

    fn a_history_entry() -> HistoryEntry {
        HistoryEntry {
            id: 0,
            original: "hello world".to_owned(),
            answer: "안녕 세상".to_owned(),
            kind: "translation".to_owned(),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            model: Some("qwen2.5:3b".to_owned()),
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1_786_075_140,
        }
    }

    #[test]
    fn history_records_and_lists_entries() {
        let path = temp_db("history-basic");
        let lib = Library::open(&path).unwrap();

        let entry = a_history_entry();
        let id = lib.record_history(&entry).unwrap();
        assert!(id > 0);

        let page = lib.history_list(0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].original, "hello world");
        assert_eq!(page.entries[0].provenance, "accessibility");

        cleanup(&path);
    }

    #[test]
    fn history_count_reflects_insertions_and_deletions() {
        let path = temp_db("history-count");
        let lib = Library::open(&path).unwrap();

        assert_eq!(lib.history_count().unwrap(), 0);

        let mut entry = a_history_entry();
        entry.created_at = 1000;
        lib.record_history(&entry).unwrap();
        entry.created_at = 1001;
        entry.original = "second".to_owned();
        lib.record_history(&entry).unwrap();

        assert_eq!(lib.history_count().unwrap(), 2);

        let page = lib.history_list(0).unwrap();
        lib.history_delete(page.entries[0].id).unwrap();
        assert_eq!(lib.history_count().unwrap(), 1);

        cleanup(&path);
    }

    #[test]
    fn history_prune_removes_old_entries() {
        let path = temp_db("history-prune");
        let lib = Library::open(&path).unwrap();

        let mut entry = a_history_entry();
        entry.created_at = 1000;
        lib.record_history(&entry).unwrap();
        entry.created_at = 2000;
        entry.original = "newer".to_owned();
        lib.record_history(&entry).unwrap();

        // Prune entries older than 500 seconds from now=2100.
        let pruned = lib.history_prune(2100, 500).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(lib.history_count().unwrap(), 1);

        let page = lib.history_list(0).unwrap();
        assert_eq!(page.entries[0].original, "newer");

        cleanup(&path);
    }

    #[test]
    fn history_prune_zero_does_nothing() {
        let path = temp_db("history-prune-zero");
        let lib = Library::open(&path).unwrap();

        let entry = a_history_entry();
        lib.record_history(&entry).unwrap();
        assert_eq!(lib.history_prune(2100, 0).unwrap(), 0);
        assert_eq!(lib.history_count().unwrap(), 1);

        cleanup(&path);
    }

    #[test]
    fn history_promote_creates_library_item_and_removes_entry() {
        let path = temp_db("history-promote");
        let lib = Library::open(&path).unwrap();

        let entry = a_history_entry();
        let hid = lib.record_history(&entry).unwrap();
        assert_eq!(lib.history_count().unwrap(), 1);

        let library_id = lib.history_promote(hid).unwrap();
        assert!(library_id > 0);
        assert_eq!(lib.history_count().unwrap(), 0);

        // The promoted item is in the library.
        let item = lib.get(library_id).unwrap().unwrap();
        assert_eq!(item.original, "hello world");
        assert_eq!(item.answer, "안녕 세상");
        assert_eq!(item.review_status, ReviewStatus::Later);

        cleanup(&path);
    }

    #[test]
    fn history_entries_are_newest_first() {
        let path = temp_db("history-order");
        let lib = Library::open(&path).unwrap();

        let mut entry = a_history_entry();
        for i in 0..3 {
            entry.created_at = 1000 + i;
            entry.original = format!("entry {i}");
            lib.record_history(&entry).unwrap();
        }

        let page = lib.history_list(0).unwrap();
        assert_eq!(page.entries.len(), 3);
        assert_eq!(page.entries[0].original, "entry 2");
        assert_eq!(page.entries[2].original, "entry 0");

        cleanup(&path);
    }

    // M8 privacy invariant tests

    #[test]
    fn application_name_is_not_persisted_when_disabled() {
        // When history_store_app is false, the app field is None.
        // Verify it's stored as NULL in the database.
        let path = temp_db("history-no-app");
        let lib = Library::open(&path).unwrap();

        let mut entry = a_history_entry();
        entry.app = None; // Simulates history_store_app = false
        lib.record_history(&entry).unwrap();

        // Read the raw row to verify app is NULL
        let app_value: Option<String> = lib
            .conn
            .query_row("SELECT app FROM history WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(
            app_value.is_none(),
            "app field must be NULL when history_store_app is disabled"
        );

        cleanup(&path);
    }

    #[test]
    fn startup_prune_keeps_nonexpired_entries() {
        // Verify that prune only removes entries older than retention period.
        let path = temp_db("history-prune-keeps");
        let lib = Library::open(&path).unwrap();

        let mut entry = a_history_entry();

        // Add entries at different ages
        entry.created_at = 1000; // Old
        entry.original = "old".to_owned();
        lib.record_history(&entry).unwrap();

        entry.created_at = 1500; // Middle
        entry.original = "middle".to_owned();
        lib.record_history(&entry).unwrap();

        entry.created_at = 1900; // Recent
        entry.original = "recent".to_owned();
        lib.record_history(&entry).unwrap();

        // Prune entries older than 500 seconds from now=2000
        // Only the entry at t=1000 should be removed (age 1000 > 500)
        // Entries at t=1500 and t=1900 should remain (age 500 and 100 <= 500)
        let pruned = lib.history_prune(2000, 500).unwrap();
        assert_eq!(pruned, 1, "only one entry should be pruned");
        assert_eq!(lib.history_count().unwrap(), 2, "two entries should remain");

        let page = lib.history_list(0).unwrap();
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.entries[0].original, "recent");
        assert_eq!(page.entries[1].original, "middle");

        cleanup(&path);
    }

    #[test]
    fn history_table_schema_columns() {
        // Document the history table schema for privacy audit.
        let path = temp_db("history-schema");
        let lib = Library::open(&path).unwrap();

        // Get column info
        let mut stmt = lib.conn.prepare("PRAGMA table_info(history)").unwrap();
        let columns: Vec<(String, String, bool)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?, // name
                    row.get::<_, String>(2)?, // type
                    row.get::<_, bool>(3)?,   // notnull
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Verify expected columns exist
        let column_names: Vec<&str> = columns.iter().map(|(name, _, _)| name.as_str()).collect();
        assert!(column_names.contains(&"id"));
        assert!(column_names.contains(&"original"));
        assert!(column_names.contains(&"answer"));
        assert!(column_names.contains(&"app"));
        assert!(column_names.contains(&"created_at"));

        // Verify app is nullable (notnull = false)
        let app_column = columns.iter().find(|(name, _, _)| name == "app").unwrap();
        assert!(
            !app_column.2,
            "app column must be nullable for privacy minimization"
        );

        cleanup(&path);
    }

    #[test]
    fn history_cap_enforced_at_1000() {
        // ADR-0014: at most 1,000 entries by default.
        let path = temp_db("history-cap-1000");
        let lib = Library::open(&path).unwrap();

        let mut entry = HistoryEntry {
            id: 0,
            original: "test".to_owned(),
            answer: "answer".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };

        // Insert exactly 1000 entries
        for i in 0..1000 {
            entry.original = format!("entry-{i}");
            entry.created_at = 1000 + i;
            lib.record_history(&entry).unwrap();
        }

        assert_eq!(
            lib.history_count().unwrap(),
            1000,
            "exactly 1000 entries should remain"
        );

        cleanup(&path);
    }

    #[test]
    fn history_cap_prunes_oldest_when_exceeded() {
        // ADR-0014: oldest entries are removed when cap is exceeded.
        let path = temp_db("history-cap-prune");
        let lib = Library::open(&path).unwrap();

        let mut entry = HistoryEntry {
            id: 0,
            original: "test".to_owned(),
            answer: "answer".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };

        // Insert 1001 entries
        for i in 0..1001 {
            entry.original = format!("entry-{i}");
            entry.created_at = 1000 + i;
            lib.record_history(&entry).unwrap();
        }

        // Should be capped at 1000
        assert_eq!(
            lib.history_count().unwrap(),
            1000,
            "should be capped at 1000 entries"
        );

        // The oldest entry (entry-0) should be gone
        let page = lib.history_list(0).unwrap();
        let originals: Vec<&str> = page.entries.iter().map(|e| e.original.as_str()).collect();
        assert!(
            !originals.contains(&"entry-0"),
            "oldest entry should be pruned"
        );
        assert!(
            originals.contains(&"entry-1000"),
            "newest entry should remain"
        );

        cleanup(&path);
    }

    #[test]
    fn history_cap_deterministic_ordering_with_same_timestamp() {
        // When multiple entries have the same timestamp, ordering is deterministic by id.
        let path = temp_db("history-cap-deterministic");
        let lib = Library::open(&path).unwrap();

        let mut entry = HistoryEntry {
            id: 0,
            original: "test".to_owned(),
            answer: "answer".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000, // All same timestamp
        };

        // Insert 1005 entries all with the same timestamp
        for i in 0..1005 {
            entry.original = format!("entry-{i}");
            lib.record_history(&entry).unwrap();
        }

        // Should be capped at 1000
        assert_eq!(lib.history_count().unwrap(), 1000);

        // The first 5 entries (lowest ids) should be gone
        let page = lib.history_list(0).unwrap();
        let originals: Vec<&str> = page.entries.iter().map(|e| e.original.as_str()).collect();
        for i in 0..5 {
            assert!(
                !originals.contains(&format!("entry-{i}").as_str()),
                "entry-{i} should be pruned (lowest id)"
            );
        }
        for i in 1000..1005 {
            assert!(
                originals.contains(&format!("entry-{i}").as_str()),
                "entry-{i} should remain (highest id)"
            );
        }

        cleanup(&path);
    }
}
