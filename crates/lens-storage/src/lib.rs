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
    /// Which inference provider produced the answer: `"ollama"`, `"lmstudio"`, or `None` for legacy unknown provenance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The reader's own note.
    pub note: Option<String>,
    /// Tags for this item, populated on demand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Review state: "known", "later", or "archive".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_state: Option<String>,
}

/// A page of items returned by [`Library::list`] or [`Library::search`].
#[derive(Debug, Clone)]
pub struct Page {
    /// The items in this page, newest first.
    pub items: Vec<Item>,
    /// Whether more items exist beyond this page.
    pub has_more: bool,
    /// Total number of items matching the query/filter (for pagination UI).
    pub total: i64,
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
    /// Which inference provider produced the answer: `"ollama"`, `"lmstudio"`, or `None` for legacy unknown provenance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
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
    /// Filter by review state: "known", "later", or "archive".
    pub review_state: Option<String>,
    /// Filter by exact tag name (case-sensitive). Uses EXISTS subquery
    /// against the normalized tags/library_tags tables.
    pub tag: Option<String>,
}

/// A page size for listing and searching.
const PAGE_SIZE: i64 = 50;

/// The default maximum number of history entries (ADR-0014).
///
/// When the count exceeds this, the oldest entries are removed. Ordering is
/// deterministic: `created_at ASC, id ASC`.
const DEFAULT_HISTORY_MAX_ENTRIES: i64 = 1000;

/// Review state for an item the reader knows.
pub const REVIEW_KNOWN: &str = "known";
/// Review state for an item the reader wants to revisit.
pub const REVIEW_LATER: &str = "later";
/// Review state for an item the reader has archived.
pub const REVIEW_ARCHIVE: &str = "archive";

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
    /// A review state string was not one of the known values.
    InvalidReviewState(String),
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
            Self::InvalidReviewState(got) => write!(
                f,
                "review state must be known, later, or archive, not {got}",
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
        // Restrict permissions on Unix after all writes (including migrations)
        // are done. WAL and SHM files may be created by the migrate() call,
        // so permissions must be set AFTER migrate, not before.
        // Skip for in-memory databases (path is ":memory:").
        // A hardening failure is an error: the caller (library_service) falls
        // back to in-memory backing rather than claiming persistence is healthy.
        #[cfg(unix)]
        {
            let is_memory = path == Path::new(":memory:");
            if !is_memory {
                harden_unix(path)?;
                let wal_path = path.with_extension("db-wal");
                if wal_path.exists() {
                    harden_unix(&wal_path)?;
                }
                let shm_path = path.with_extension("db-shm");
                if shm_path.exists() {
                    harden_unix(&shm_path)?;
                }
            }
        }
        Ok(lib)
    }

    /// Insert one item. Returns the new item's id.
    ///
    /// The `id` field on the item is ignored; the database assigns one.
    /// Review state starts as `"later"`.
    pub fn insert(&self, item: &Item) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'later')",
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
                item.provider,
                item.note,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert one item with an explicit review state (used by JSONL import).
    fn insert_with_review_state(&self, item: &Item, review_state: &str) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                item.provider,
                item.note,
                review_state,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List items, newest first.
    pub fn list(&self, offset: i64) -> Result<Page, Error> {
        let limit = PAGE_SIZE + 1;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state FROM library ORDER BY created_at DESC, id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let items: Vec<Item> = stmt
            .query_map(params![limit, offset], row_to_item)?
            .collect::<Result<_, _>>()?;
        let has_more = items.len() as i64 > PAGE_SIZE;
        let mut items = items;
        items.truncate(PAGE_SIZE as usize);
        let total = self.count()?;
        Ok(Page {
            items,
            has_more,
            total,
        })
    }

    /// Get one item by id.
    pub fn get(&self, id: i64) -> Result<Option<Item>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state FROM library WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_item)?;
        match rows.next() {
            Some(Ok(item)) => Ok(Some(item)),
            Some(Err(e)) => Err(Error::Sqlite(e)),
            None => Ok(None),
        }
    }

    /// Check whether an item with the same original text, kind, provider, model, and
    /// target language already exists.
    ///
    /// ADR-0013 requires that the library can store more than one answer for
    /// the same sentence when they come from different models or target
    /// different languages. Duplicate identity is therefore
    /// `original + kind + provider + model + target_language`, not `original + kind` alone.
    /// Provider distinguishes answers from different inference engines (e.g., Ollama vs LM Studio).
    pub fn has_duplicate(
        &self,
        original: &str,
        kind: &str,
        provider: Option<&str>,
        model: Option<&str>,
        target_language: Option<&str>,
    ) -> Result<bool, Error> {
        let exists: bool = self
            .conn
            .prepare(
                "SELECT 1 FROM library WHERE original = ?1 AND kind = ?2 AND provider IS ?3 AND model IS ?4 AND target_language IS ?5",
            )?
            .exists(params![original, kind, provider, model, target_language])?;
        Ok(exists)
    }

    /// Find the id of an existing item with the same original text, kind, provider,
    /// model, and target language. Returns `Ok(None)` if no duplicate exists.
    ///
    /// Duplicate identity follows ADR-0013: `original + kind + provider + model +
    /// target_language`. When multiple rows match (should not happen but
    /// the schema allows it), returns the newest (highest id).
    /// Provider distinguishes answers from different inference engines.
    pub fn find_duplicate_id(
        &self,
        original: &str,
        kind: &str,
        provider: Option<&str>,
        model: Option<&str>,
        target_language: Option<&str>,
    ) -> Result<Option<i64>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM library WHERE original = ?1 AND kind = ?2 AND provider IS ?3 AND model IS ?4 AND target_language IS ?5 ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![original, kind, provider, model, target_language])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
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
    /// The operation is atomic: if any step fails, all changes are rolled back.
    pub fn set_tags(&self, id: i64, tags: &[String]) -> Result<bool, Error> {
        // Check if the item exists.
        let exists: bool = self
            .conn
            .prepare("SELECT 1 FROM library WHERE id = ?1")?
            .exists(params![id])?;
        if !exists {
            return Ok(false);
        }

        // Wrap DELETE + INSERTs in a transaction for atomicity.
        let tx = self.conn.unchecked_transaction()?;

        // Remove existing tags for this item.
        tx.execute(
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
            tx.execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                params![tag],
            )?;
            let tag_id: i64 =
                tx.query_row("SELECT id FROM tags WHERE name = ?1", params![tag], |row| {
                    row.get(0)
                })?;

            // Link the tag to the item.
            tx.execute(
                "INSERT OR IGNORE INTO library_tags (library_id, tag_id) VALUES (?1, ?2)",
                params![id, tag_id],
            )?;
        }

        tx.commit()?;
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
        let escaped = trimmed
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        let limit = PAGE_SIZE + 1;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state FROM library WHERE original LIKE ?1 ESCAPE '\\' OR answer LIKE ?1 ESCAPE '\\' OR note LIKE ?1 ESCAPE '\\' ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
        )?;
        let items: Vec<Item> = stmt
            .query_map(params![pattern, limit, offset], row_to_item)?
            .collect::<Result<_, _>>()?;
        let has_more = items.len() as i64 > PAGE_SIZE;
        let mut items = items;
        items.truncate(PAGE_SIZE as usize);
        let total = self.conn.query_row(
            "SELECT COUNT(*) FROM library WHERE original LIKE ?1 ESCAPE '\\' OR answer LIKE ?1 ESCAPE '\\' OR note LIKE ?1 ESCAPE '\\'",
            params![pattern],
            |row| row.get(0),
        )?;
        Ok(Page {
            items,
            has_more,
            total,
        })
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
        if let Some(ref rs) = filter.review_state {
            param_values.push(rs.clone());
            conditions.push(format!("review_state = ?{}", param_values.len()));
        }
        if let Some(ref tag) = filter.tag {
            let trimmed = tag.trim();
            if !trimmed.is_empty() {
                param_values.push(trimmed.to_owned());
                conditions.push(format!(
                    "EXISTS (SELECT 1 FROM library_tags lt JOIN tags t ON lt.tag_id = t.id WHERE lt.library_id = library.id AND t.name = ?{})",
                    param_values.len()
                ));
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let limit = PAGE_SIZE + 1;
        let sql = format!(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state FROM library{where_clause} ORDER BY created_at DESC, id DESC LIMIT ?{} OFFSET ?{}",
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
        let count_sql = format!("SELECT COUNT(*) FROM library{where_clause}");
        let mut count_stmt = self.conn.prepare(&count_sql)?;
        let count_params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|p| p as &dyn rusqlite::types::ToSql)
            .collect();
        let total: i64 = count_stmt.query_row(count_params.as_slice(), |row| row.get(0))?;
        Ok(Page {
            items,
            has_more,
            total,
        })
    }

    /// Search and filter combined. Applies both text search (original/answer)
    /// and structured filters (kind, app, languages, review_state) with AND semantics.
    ///
    /// Returns items matching BOTH the search query AND all specified filters.
    /// If query is empty, behaves like list_filtered. If filter is empty, behaves like search.
    pub fn search_and_filter(
        &self,
        query: &str,
        filter: &Filter,
        offset: i64,
    ) -> Result<Page, Error> {
        let trimmed = query.trim();

        // Build filter conditions first
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
        if let Some(ref rs) = filter.review_state {
            param_values.push(rs.clone());
            conditions.push(format!("review_state = ?{}", param_values.len()));
        }
        if let Some(ref tag) = filter.tag {
            let trimmed_tag = tag.trim();
            if !trimmed_tag.is_empty() {
                param_values.push(trimmed_tag.to_owned());
                conditions.push(format!(
                    "EXISTS (SELECT 1 FROM library_tags lt JOIN tags t ON lt.tag_id = t.id WHERE lt.library_id = library.id AND t.name = ?{})",
                    param_values.len()
                ));
            }
        }

        // Add search condition if query is non-empty
        let has_search = !trimmed.is_empty();
        if has_search {
            let escaped = trimmed
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let pattern = format!("%{escaped}%");
            param_values.push(pattern);
            conditions.push(format!(
                "(original LIKE ?{} ESCAPE '\\' OR answer LIKE ?{} ESCAPE '\\' OR note LIKE ?{} ESCAPE '\\')",
                param_values.len(),
                param_values.len(),
                param_values.len()
            ));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let limit = PAGE_SIZE + 1;
        let sql = format!(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state FROM library{where_clause} ORDER BY created_at DESC, id DESC LIMIT ?{} OFFSET ?{}",
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

        // Count query uses the same WHERE clause for predicate consistency
        let count_sql = format!("SELECT COUNT(*) FROM library{where_clause}");
        let mut count_stmt = self.conn.prepare(&count_sql)?;
        let count_params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|p| p as &dyn rusqlite::types::ToSql)
            .collect();
        let total: i64 = count_stmt.query_row(count_params.as_slice(), |row| row.get(0))?;

        Ok(Page {
            items,
            has_more,
            total,
        })
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

    /// Get distinct tag names that are actually used (attached to at least one
    /// library item), sorted alphabetically.
    pub fn distinct_tags(&self) -> Result<Vec<String>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT t.name FROM tags t INNER JOIN library_tags lt ON lt.tag_id = t.id ORDER BY t.name",
        )?;
        let tags = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(tags)
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
            .optional()?;
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
    /// The first line is a version header (`{"version": 3}`). Each
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

        let mut out = String::from("{\"version\":3}\n");
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
    /// The first line must be a version header (`{"version": 1}`,
    /// `{"version": 2}`, or `{"version": 3}`). Each subsequent line is one JSON object. Items
    /// with the same original text, kind, provider, and model as an existing row are skipped
    /// (duplicate detection).
    ///
    /// Version 1 items are inserted with review state `"later"`. Version 2
    /// items preserve their `review_state` field; unknown values default
    /// to `"later"`. Version 3 items include the `provider` field for
    /// inference engine provenance.
    ///
    /// Returns the number of items imported (not counting skipped
    /// duplicates).
    pub fn import_jsonl(&self, jsonl: &str) -> Result<i64, Error> {
        let mut imported = 0i64;
        let mut format_version: i64 = 0;
        for (i, line) in jsonl.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // First line is the version header.
            if i == 0 {
                let header: serde_json::Value = serde_json::from_str(trimmed)
                    .map_err(|e| Error::Io(std::io::Error::other(format!("line 1: {e}"))))?;
                format_version = header.get("version").and_then(|v| v.as_i64()).unwrap_or(0);
                if format_version != 1 && format_version != 2 && format_version != 3 {
                    return Err(Error::Io(std::io::Error::other(format!(
                        "unsupported JSONL version: {format_version}"
                    ))));
                }
                continue;
            }

            let item: Item = serde_json::from_str(trimmed).map_err(|e| {
                let ctx = if imported > 0 {
                    format!("line {} (after {} imported): {e}", i + 1, imported)
                } else {
                    format!("line {}: {e}", i + 1)
                };
                Error::Io(std::io::Error::other(ctx))
            })?;

            // Skip duplicates.
            if self
                .has_duplicate(
                    &item.original,
                    &item.kind,
                    item.provider.as_deref(),
                    item.model.as_deref(),
                    item.target_language.as_deref(),
                )
                .unwrap_or(false)
            {
                continue;
            }

            // Determine the review state for the imported item.
            let review_state = if format_version >= 2 {
                match item.review_state.as_deref() {
                    Some(REVIEW_KNOWN) | Some(REVIEW_LATER) | Some(REVIEW_ARCHIVE) => item
                        .review_state
                        .clone()
                        .unwrap_or_else(|| REVIEW_LATER.to_owned()),
                    _ => REVIEW_LATER.to_owned(),
                }
            } else {
                REVIEW_LATER.to_owned()
            };

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
                provider: item.provider,
                note: item.note,
                tags: None,
                review_state: None,
            };
            let id = self.insert_with_review_state(&insert_item, &review_state)?;

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

    // -- review list --------------------------------------------------------

    /// Set the review state of a library item.
    pub fn set_review_state(&self, id: i64, state: &str) -> Result<bool, Error> {
        if state != REVIEW_KNOWN && state != REVIEW_LATER && state != REVIEW_ARCHIVE {
            return Err(Error::InvalidReviewState(state.to_owned()));
        }
        let changed = self.conn.execute(
            "UPDATE library SET review_state = ?1 WHERE id = ?2",
            params![state, id],
        )?;
        Ok(changed > 0)
    }

    /// List items marked "later", newest first.
    pub fn list_later(&self, offset: i64) -> Result<Page, Error> {
        let limit = PAGE_SIZE + 1;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state FROM library WHERE review_state = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
        )?;
        let items: Vec<Item> = stmt
            .query_map(params![REVIEW_LATER, limit, offset], row_to_item)?
            .collect::<Result<_, _>>()?;
        let has_more = items.len() as i64 > PAGE_SIZE;
        let mut items = items;
        items.truncate(PAGE_SIZE as usize);
        let total = self.count_later()?;
        Ok(Page {
            items,
            has_more,
            total,
        })
    }

    /// Count items marked "later".
    pub fn count_later(&self) -> Result<i64, Error> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM library WHERE review_state = ?1",
                params![REVIEW_LATER],
                |row| row.get(0),
            )
            .map_err(Error::Sqlite)
    }

    /// Return ALL items marked "later", newest first, without pagination.
    ///
    /// Used by the review session to take a snapshot of the current later set
    /// at session start. The session tracks which items have been seen and
    /// advances through them without relying on offset pagination, which would
    /// be incorrect on a mutating dataset.
    pub fn list_later_session(&self) -> Result<Vec<Item>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state FROM library WHERE review_state = ?1 ORDER BY created_at DESC, id DESC",
        )?;
        let items: Vec<Item> = stmt
            .query_map(params![REVIEW_LATER], row_to_item)?
            .collect::<Result<_, _>>()?;
        Ok(items)
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
        const CURRENT_SCHEMA: i64 = 8;
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

        // Review contract v1 (#141): replace spaced-repetition scheduling
        // with manual Known/Later/Archive categorisation.
        //
        // - Remap review_state values: 'new' and 'learning' -> 'later',
        //   'mastered' -> 'known'.
        // - Drop the partial index on next_review_at (scheduling removed).
        // - Add `archived_at` column (NULL unless state is 'archive').
        if !applied.contains(&7) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "DROP INDEX IF EXISTS library_review_due;

                 UPDATE library SET review_state = 'later'
                     WHERE review_state IN ('new', 'learning');
                 UPDATE library SET review_state = 'known'
                     WHERE review_state = 'mastered';

                 ALTER TABLE library ADD COLUMN archived_at INTEGER;

                 INSERT INTO schema_migrations (version) VALUES (7);",
            )?;
            tx.commit()?;
        }

        // Provider provenance: which inference provider produced the answer.
        // P6 Phase 2: distinguish provider (Ollama/LM Studio) from provider_scope (trust location).
        // Legacy rows: NULL = unknown provenance, never guessed from model or endpoint.
        if !applied.contains(&8) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "ALTER TABLE history ADD COLUMN provider TEXT;
                 ALTER TABLE library ADD COLUMN provider TEXT;

                 INSERT INTO schema_migrations (version) VALUES (8);",
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
            "INSERT INTO history (original, answer, kind, source_language, target_language, model, provider, app, provenance, provider_scope, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                entry.original,
                entry.answer,
                entry.kind,
                entry.source_language,
                entry.target_language,
                entry.model,
                entry.provider,
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
            "SELECT id, original, answer, kind, source_language, target_language, model, provider, app, provenance, provider_scope, created_at FROM history ORDER BY created_at DESC, id DESC LIMIT ?1 OFFSET ?2",
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
            "SELECT id, original, answer, kind, source_language, target_language, model, provider, app, provenance, provider_scope, created_at FROM history WHERE id = ?1",
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
            "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, provider, note, review_state) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 'later')",
            params![
                entry.original,
                entry.answer,
                entry.app,
                entry.created_at,
                entry.kind,
                entry.source_language,
                entry.target_language,
                entry.model,
                entry.provider,
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

/// Tighten a file's Unix permissions to owner-only (0o600) if group/other
/// bits are set. Returns an error if the chmod itself fails, so callers
/// can distinguish "hardening succeeded or was unnecessary" from "could
/// not establish the intended privacy mode".
///
/// The file must already exist; callers check existence before calling
/// for auxiliary files (WAL, SHM).
#[cfg(unix)]
fn harden_unix(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(Error::Io)?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(Error::Io)?;
    }
    Ok(())
}

/// Map a result row to an [`Item`].
fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<Item> {
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
        provider: row.get(10)?,
        note: row.get(11)?,
        review_state: row.get::<_, Option<String>>(12)?,
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
        provider: row.get(7)?,
        app: row.get(8)?,
        provenance: row.get(9)?,
        provider_scope: row.get(10)?,
        created_at: row.get(11)?,
    })
}

/// Extension trait for optional query results.
trait OptionalExt {
    fn optional(self) -> Result<Option<String>, rusqlite::Error>;
}

impl OptionalExt for Result<String, rusqlite::Error> {
    fn optional(self) -> Result<Option<String>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
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
mod bench;

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
            provider: None,
            note: None,
            tags: None,
            review_state: None,
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
            provider: None,
            note: None,
            tags: None,
            review_state: None,
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
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8]);
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

    #[test]
    fn set_tags_replaces_tags_successfully() {
        let path = temp_db("tags-replace");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        // Set initial tags.
        lib.set_tags(id, &["alpha".to_owned(), "beta".to_owned()])
            .unwrap();
        assert_eq!(lib.get_tags(id).unwrap(), vec!["alpha", "beta"]);

        // Replace with new tags.
        lib.set_tags(id, &["gamma".to_owned()]).unwrap();
        assert_eq!(lib.get_tags(id).unwrap(), vec!["gamma"]);

        // Setting empty tags should clear all tags.
        lib.set_tags(id, &[]).unwrap();
        assert_eq!(lib.get_tags(id).unwrap(), Vec::<String>::new());

        // Verify tags can be re-set after clearing.
        lib.set_tags(id, &["delta".to_owned()]).unwrap();
        assert_eq!(lib.get_tags(id).unwrap(), vec!["delta"]);

        cleanup(&path);
    }

    #[test]
    fn set_tags_rolls_back_on_failure() {
        let path = temp_db("tags-rollback");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        // Set initial tags.
        lib.set_tags(id, &["alpha".to_owned(), "beta".to_owned()])
            .unwrap();
        assert_eq!(lib.get_tags(id).unwrap(), vec!["alpha", "beta"]);

        // Create a trigger that aborts when inserting a tag named "FAIL".
        lib.conn
            .execute_batch(
                "CREATE TRIGGER abort_on_fail_tag
                 BEFORE INSERT ON tags
                 WHEN NEW.name = 'FAIL'
                 BEGIN
                     SELECT RAISE(ABORT, 'forced failure for test');
                 END;",
            )
            .unwrap();

        // Attempt to set tags including the forbidden name.
        let result = lib.set_tags(id, &["gamma".to_owned(), "FAIL".to_owned()]);
        assert!(result.is_err(), "set_tags should fail when trigger aborts");

        // Verify the original tags are preserved (transaction rolled back).
        assert_eq!(
            lib.get_tags(id).unwrap(),
            vec!["alpha", "beta"],
            "original tags must survive after failed set_tags",
        );

        // Verify "gamma" was not partially inserted.
        let all_tags: Vec<String> = lib
            .conn
            .prepare("SELECT name FROM tags ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            !all_tags.contains(&"gamma".to_owned()),
            "gamma should not be in tags table after rollback",
        );

        cleanup(&path);
    }

    // -- Page.total regression tests ----------------------------------------

    #[test]
    fn page_total_reflects_unfiltered_count() {
        let path = temp_db("page-total-unfiltered");
        let lib = Library::open(&path).unwrap();

        lib.insert(&a_translation()).unwrap();
        lib.insert(&an_analysis()).unwrap();
        lib.insert(&a_translation()).unwrap();

        let page = lib.list(0).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 3);

        cleanup(&path);
    }

    #[test]
    fn page_total_reflects_search_result_count() {
        let path = temp_db("page-total-search");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.original = "hello world".to_owned();
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.original = "hello rust".to_owned();
        lib.insert(&item2).unwrap();

        let mut item3 = a_translation();
        item3.original = "goodbye".to_owned();
        lib.insert(&item3).unwrap();

        let page = lib.search("hello", 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);

        cleanup(&path);
    }

    #[test]
    fn page_total_reflects_filtered_count() {
        let path = temp_db("page-total-filtered");
        let lib = Library::open(&path).unwrap();

        lib.insert(&a_translation()).unwrap();
        lib.insert(&an_analysis()).unwrap();
        lib.insert(&a_translation()).unwrap();

        let filter = Filter {
            kind: Some("translation".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);

        let filter = Filter {
            kind: Some("analysis".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);

        cleanup(&path);
    }

    #[test]
    fn page_total_with_combined_filters() {
        let path = temp_db("page-total-combined");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.app = Some("Chrome".to_owned());
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.app = Some("VSCode".to_owned());
        lib.insert(&item2).unwrap();

        let mut item3 = an_analysis();
        item3.app = Some("Chrome".to_owned());
        lib.insert(&item3).unwrap();

        let filter = Filter {
            kind: Some("translation".to_owned()),
            app: Some("Chrome".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].kind, "translation");
        assert_eq!(page.items[0].app.as_deref(), Some("Chrome"));

        cleanup(&path);
    }

    #[test]
    fn page_total_with_pagination() {
        let path = temp_db("page-total-pagination");
        let lib = Library::open(&path).unwrap();

        // Insert 125 items (more than 2 pages of PAGE_SIZE 50)
        for i in 0..125 {
            let mut item = a_translation();
            item.original = format!("item {}", i);
            lib.insert(&item).unwrap();
        }

        let page1 = lib.list(0).unwrap();
        assert_eq!(page1.total, 125);
        assert_eq!(page1.items.len(), 50);
        assert!(page1.has_more);

        let page2 = lib.list(50).unwrap();
        assert_eq!(page2.total, 125);
        assert_eq!(page2.items.len(), 50);
        assert!(page2.has_more);

        let page3 = lib.list(100).unwrap();
        assert_eq!(page3.total, 125);
        assert_eq!(page3.items.len(), 25);
        assert!(!page3.has_more);

        cleanup(&path);
    }

    #[test]
    fn page_total_zero_for_empty_results() {
        let path = temp_db("page-total-zero");
        let lib = Library::open(&path).unwrap();

        lib.insert(&a_translation()).unwrap();

        let page = lib.search("nonexistent", 0).unwrap();
        assert_eq!(page.total, 0);
        assert_eq!(page.items.len(), 0);

        let filter = Filter {
            kind: Some("nonexistent".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.total, 0);
        assert_eq!(page.items.len(), 0);

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

    /// SQL LIKE wildcards (`%`, `_`) in the query are escaped so they
    /// match literally rather than acting as pattern characters.
    #[test]
    fn search_escapes_like_wildcards() {
        let path = temp_db("search-like");
        let lib = Library::open(&path).unwrap();

        let mut item = a_translation();
        item.original = "100% complete".to_owned();
        lib.insert(&item).unwrap();

        let mut other = a_translation();
        other.original = "100 things".to_owned();
        lib.insert(&other).unwrap();

        // Searching for "100%" must match only the literal "100%", not "100 things".
        let page = lib.search("100%", 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].original, "100% complete");

        // Searching for "_" must not match every single character.
        let page = lib.search("_", 0).unwrap();
        assert!(
            page.items.is_empty(),
            "underscore must not act as a wildcard"
        );

        cleanup(&path);
    }

    #[test]
    fn search_finds_items_by_note_text() {
        let path = temp_db("search-note");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.note = Some("important grammar rule".to_owned());
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.original = "unrelated text".to_owned();
        item2.note = None;
        lib.insert(&item2).unwrap();

        // Searching for "grammar" should find the item by note
        let page = lib.search("grammar", 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].note.as_deref(),
            Some("important grammar rule")
        );

        cleanup(&path);
    }

    #[test]
    fn search_finds_items_across_original_answer_and_note() {
        let path = temp_db("search-all-fields");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.original = "hello world".to_owned();
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.answer = "hello answer".to_owned();
        lib.insert(&item2).unwrap();

        let mut item3 = a_translation();
        item3.note = Some("hello note".to_owned());
        lib.insert(&item3).unwrap();

        let mut item4 = a_translation();
        item4.original = "goodbye".to_owned();
        lib.insert(&item4).unwrap();

        // Searching for "hello" should find all three items
        let page = lib.search("hello", 0).unwrap();
        assert_eq!(page.items.len(), 3);

        cleanup(&path);
    }

    // -- search + filter composition ----------------------------------------

    #[test]
    fn search_and_filter_combines_with_and_semantics() {
        let path = temp_db("search-filter-composition");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.original = "hello world".to_owned();
        item1.app = Some("Chrome".to_owned());
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.original = "hello rust".to_owned();
        item2.app = Some("VSCode".to_owned());
        lib.insert(&item2).unwrap();

        let mut item3 = an_analysis();
        item3.original = "hello analysis".to_owned();
        item3.app = Some("Chrome".to_owned());
        lib.insert(&item3).unwrap();

        let filter = Filter {
            kind: Some("translation".to_owned()),
            app: Some("Chrome".to_owned()),
            ..Default::default()
        };

        // Query "hello" + filter (translation, Chrome) should match only item1
        let page = lib.search_and_filter("hello", &filter, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].original, "hello world");
        assert_eq!(page.items[0].app.as_deref(), Some("Chrome"));
        assert_eq!(page.items[0].kind, "translation");

        cleanup(&path);
    }

    #[test]
    fn search_and_filter_empty_query_acts_as_filter_only() {
        let path = temp_db("search-filter-empty-query");
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

        let page = lib.search_and_filter("", &filter, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].app.as_deref(), Some("Chrome"));

        cleanup(&path);
    }

    #[test]
    fn search_and_filter_empty_filter_acts_as_search_only() {
        let path = temp_db("search-filter-empty-filter");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.original = "hello world".to_owned();
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.original = "goodbye".to_owned();
        lib.insert(&item2).unwrap();

        let filter = Filter::default();

        let page = lib.search_and_filter("hello", &filter, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].original, "hello world");

        cleanup(&path);
    }

    #[test]
    fn search_and_filter_with_multiple_filters() {
        let path = temp_db("search-filter-multiple");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.original = "hello".to_owned();
        item1.app = Some("Chrome".to_owned());
        item1.source_language = Some("en".to_owned());
        item1.target_language = Some("ko".to_owned());
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.original = "hello".to_owned();
        item2.app = Some("Chrome".to_owned());
        item2.source_language = Some("en".to_owned());
        item2.target_language = Some("ja".to_owned());
        lib.insert(&item2).unwrap();

        let filter = Filter {
            app: Some("Chrome".to_owned()),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            ..Default::default()
        };

        let page = lib.search_and_filter("hello", &filter, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].target_language.as_deref(), Some("ko"));

        cleanup(&path);
    }

    #[test]
    fn search_and_filter_no_results() {
        let path = temp_db("search-filter-no-results");
        let lib = Library::open(&path).unwrap();

        let mut item = a_translation();
        item.original = "hello".to_owned();
        item.app = Some("Chrome".to_owned());
        lib.insert(&item).unwrap();

        let filter = Filter {
            app: Some("VSCode".to_owned()),
            ..Default::default()
        };

        let page = lib.search_and_filter("hello", &filter, 0).unwrap();
        assert_eq!(page.total, 0);
        assert_eq!(page.items.len(), 0);

        cleanup(&path);
    }

    #[test]
    fn search_and_filter_escapes_like_wildcards() {
        let path = temp_db("search-filter-like-escape");
        let lib = Library::open(&path).unwrap();

        let mut item1 = a_translation();
        item1.original = "100% complete".to_owned();
        item1.app = Some("Chrome".to_owned());
        lib.insert(&item1).unwrap();

        let mut item2 = a_translation();
        item2.original = "100 things".to_owned();
        item2.app = Some("Chrome".to_owned());
        lib.insert(&item2).unwrap();

        let filter = Filter {
            app: Some("Chrome".to_owned()),
            ..Default::default()
        };

        // Searching for "100%" should only match the literal "100%"
        let page = lib.search_and_filter("100%", &filter, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].original, "100% complete");

        cleanup(&path);
    }

    #[test]
    fn search_and_filter_pagination_consistent_total() {
        let path = temp_db("search-filter-pagination");
        let lib = Library::open(&path).unwrap();

        // Insert 125 items with "hello" and Chrome
        for i in 0..125 {
            let mut item = a_translation();
            item.original = format!("hello {}", i);
            item.app = Some("Chrome".to_owned());
            lib.insert(&item).unwrap();
        }

        let filter = Filter {
            app: Some("Chrome".to_owned()),
            ..Default::default()
        };

        let page1 = lib.search_and_filter("hello", &filter, 0).unwrap();
        assert_eq!(page1.total, 125);
        assert_eq!(page1.items.len(), 50);
        assert!(page1.has_more);

        let page2 = lib.search_and_filter("hello", &filter, 50).unwrap();
        assert_eq!(page2.total, 125);
        assert_eq!(page2.items.len(), 50);
        assert!(page2.has_more);

        let page3 = lib.search_and_filter("hello", &filter, 100).unwrap();
        assert_eq!(page3.total, 125);
        assert_eq!(page3.items.len(), 25);
        assert!(!page3.has_more);

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
            provider: None,
            note: None,
            tags: None,
            review_state: None,
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
            provider: None,
            note: Some("my note".to_owned()),
            tags: None,
            review_state: None,
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
            provider: None,
            note: None,
            tags: None,
            review_state: None,
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
        assert_eq!(header["version"], 3);

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
            provider: None,
            note: Some("a note".to_owned()),
            tags: None,
            review_state: None,
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
    fn has_duplicate_finds_matching_original_kind_model_and_language() {
        let path = temp_db("has-duplicate");
        let lib = Library::open(&path).unwrap();

        let item = a_translation();
        lib.insert(&item).unwrap();

        // Same original + kind + provider(None) + model(None) + target_language(Some("ko")) is a duplicate.
        assert!(lib
            .has_duplicate(&item.original, "translation", None, None, Some("ko"))
            .unwrap());
        // Different kind is not a duplicate.
        assert!(!lib
            .has_duplicate(&item.original, "analysis", None, None, Some("ko"))
            .unwrap());
        // Different original is not a duplicate.
        assert!(!lib
            .has_duplicate("different text", "translation", None, None, Some("ko"))
            .unwrap());
        // Different model is not a duplicate (ADR-0013).
        assert!(!lib
            .has_duplicate(
                &item.original,
                "translation",
                None,
                Some("qwen2.5:7b"),
                Some("ko")
            )
            .unwrap());
        // Different target language is not a duplicate (ADR-0013).
        assert!(!lib
            .has_duplicate(&item.original, "translation", None, None, Some("ja"))
            .unwrap());
        cleanup(&path);
    }

    /// find_duplicate_id returns the existing row id for a duplicate,
    /// and None when there is no match.
    #[test]
    fn find_duplicate_id_returns_existing_row_id() {
        let path = temp_db("find-dup-id");
        let lib = Library::open(&path).unwrap();

        let item = a_translation();
        let id = lib.insert(&item).unwrap();
        assert!(id > 0, "inserted row must have a positive id");

        // Same key: returns the existing id.
        let found = lib
            .find_duplicate_id(&item.original, "translation", None, None, Some("ko"))
            .unwrap();
        assert_eq!(found, Some(id), "duplicate must return the existing id");

        // Different kind: no match.
        let found = lib
            .find_duplicate_id(&item.original, "analysis", None, None, Some("ko"))
            .unwrap();
        assert_eq!(found, None, "different kind is not a duplicate");

        // Different model: no match (ADR-0013).
        let found = lib
            .find_duplicate_id(
                &item.original,
                "translation",
                None,
                Some("qwen2.5:7b"),
                Some("ko"),
            )
            .unwrap();
        assert_eq!(found, None, "different model is not a duplicate");

        cleanup(&path);
    }

    /// Provider-aware duplicate detection: same sentence with different providers
    /// are NOT duplicates (P6 Phase 2).
    #[test]
    fn provider_makes_items_distinct() {
        let path = temp_db("provider-distinct");
        let lib = Library::open(&path).unwrap();

        // Insert with provider = None (legacy)
        let item1 = Item {
            id: 0,
            original: "test sentence".to_owned(),
            answer: "answer 1".to_owned(),
            caveat: None,
            app: None,
            created_at: 1000,
            kind: "translation".to_owned(),
            source_language: None,
            target_language: Some("ko".to_owned()),
            model: None,
            provider: None,
            note: None,
            tags: None,
            review_state: None,
        };
        let id1 = lib.insert(&item1).unwrap();

        // Same sentence with provider = Some("ollama") is NOT a duplicate
        let item2 = Item {
            id: 0,
            original: "test sentence".to_owned(),
            answer: "answer 2".to_owned(),
            caveat: None,
            app: None,
            created_at: 2000,
            kind: "translation".to_owned(),
            source_language: None,
            target_language: Some("ko".to_owned()),
            model: None,
            provider: Some("ollama".to_owned()),
            note: None,
            tags: None,
            review_state: None,
        };
        let id2 = lib.insert(&item2).unwrap();
        assert_ne!(id1, id2, "different providers create distinct items");

        // Same sentence with provider = Some("lmstudio") is also NOT a duplicate
        let item3 = Item {
            id: 0,
            original: "test sentence".to_owned(),
            answer: "answer 3".to_owned(),
            caveat: None,
            app: None,
            created_at: 3000,
            kind: "translation".to_owned(),
            source_language: None,
            target_language: Some("ko".to_owned()),
            model: None,
            provider: Some("lmstudio".to_owned()),
            note: None,
            tags: None,
            review_state: None,
        };
        let id3 = lib.insert(&item3).unwrap();
        assert_ne!(id1, id3, "different providers create distinct items");
        assert_ne!(id2, id3, "different providers create distinct items");

        // Same provider is a duplicate
        assert!(lib
            .has_duplicate(
                "test sentence",
                "translation",
                Some("ollama"),
                None,
                Some("ko")
            )
            .unwrap());

        cleanup(&path);
    }

    /// Migration v7 → v8: REAL migration with actual legacy data.
    /// Creates a v7 database (no provider columns), inserts data, then opens
    /// with current code to trigger migration 8. Verifies:
    /// - provider column is added to both history and library tables
    /// - legacy data survives with provider = NULL
    /// - schema version becomes 8
    /// - reopen is idempotent (migration 8 not re-applied)
    #[test]
    fn migration_v7_to_v8_real_with_legacy_data() {
        use rusqlite::params;

        let path = temp_db("migration-v7-to-v8-real");

        // Manually create a v7 schema database (migrations 1-7 only)
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER NOT NULL UNIQUE);
                 CREATE TABLE library (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    original TEXT NOT NULL,
                    answer TEXT NOT NULL,
                    caveat TEXT,
                    app TEXT,
                    created_at INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    source_language TEXT,
                    target_language TEXT,
                    model TEXT,
                    note TEXT,
                    tags TEXT,
                    review_state TEXT,
                    imported_at INTEGER DEFAULT 0,
                    UNIQUE(original, kind, model, target_language)
                 );
                 CREATE TABLE history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    original TEXT NOT NULL,
                    answer TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    source_language TEXT,
                    target_language TEXT,
                    model TEXT,
                    app TEXT,
                    provenance TEXT NOT NULL,
                    provider_scope TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS import_tracking (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    imported BOOLEAN NOT NULL DEFAULT 0
                 );
                 INSERT INTO schema_migrations VALUES (1), (2), (3), (4), (5), (6), (7);",
            )
            .unwrap();

            // Insert realistic legacy data into library
            conn.execute(
                "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, tags, review_state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    "포인터가 멈춘 뒤에야 읽습니다.",
                    "Read only after the pointer rests.",
                    None::<String>,
                    Some("Code.exe"),
                    1700000000i64,
                    "translation",
                    Some("ko"),
                    Some("en"),
                    Some("qwen2.5:7b"),
                    None::<String>,
                    None::<String>,
                    None::<String>,
                ],
            )
            .unwrap();

            conn.execute(
                "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, tags, review_state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    "멱등성",
                    "원형 · 멱등성",
                    Some("AI 설명 · 사실 확인 필요"),
                    None::<String>,
                    1700001000i64,
                    "analysis",
                    Some("ko"),
                    Some("en"),
                    Some("qwen2.5:14b"),
                    Some("사용자 노트"),
                    None::<String>,
                    None::<String>,
                ],
            )
            .unwrap();

            // Insert realistic legacy data into history
            conn.execute(
                "INSERT INTO history (original, answer, kind, source_language, target_language, model, app, provenance, provider_scope, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    "번역 테스트",
                    "translation test",
                    "translation",
                    Some("ko"),
                    Some("en"),
                    Some("qwen2.5:7b"),
                    Some("chrome.exe"),
                    "accessibility",
                    "local",
                    1700002000i64,
                ],
            )
            .unwrap();
        }

        // Open with current code to trigger migration 8
        let lib = Library::open(&path).unwrap();

        // Verify schema version is now 8
        let version: i64 = lib
            .conn()
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 8, "schema must be upgraded to version 8");

        // Verify library table has provider column
        let has_provider_col: bool = lib
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('library') WHERE name = 'provider'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_provider_col, "library table must have provider column");

        // Verify history table has provider column
        let has_provider_col: bool = lib
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('history') WHERE name = 'provider'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has_provider_col, "history table must have provider column");

        // Verify legacy library data survived with provider = NULL
        let item1 = lib.get(1).unwrap().unwrap();
        assert_eq!(item1.original, "포인터가 멈춘 뒤에야 읽습니다.");
        assert_eq!(
            item1.provider, None,
            "legacy library row must have NULL provider"
        );
        assert_eq!(item1.model, Some("qwen2.5:7b".to_owned()));

        let item2 = lib.get(2).unwrap().unwrap();
        assert_eq!(item2.original, "멱등성");
        assert_eq!(
            item2.provider, None,
            "legacy library row must have NULL provider"
        );
        assert_eq!(item2.caveat, Some("AI 설명 · 사실 확인 필요".to_owned()));

        // Verify legacy history data survived with provider = NULL
        let history_page = lib.history_list(0).unwrap();
        assert_eq!(history_page.entries.len(), 1);
        assert_eq!(history_page.entries[0].original, "번역 테스트");
        assert_eq!(
            history_page.entries[0].provider, None,
            "legacy history row must have NULL provider"
        );
        assert_eq!(history_page.entries[0].model, Some("qwen2.5:7b".to_owned()));

        // Verify migration is idempotent: close and reopen
        drop(lib);
        let lib2 = Library::open(&path).unwrap();
        let version2: i64 = lib2
            .conn()
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version2, 8, "schema version must remain 8 after reopen");

        // Verify data is still there
        let item1_again = lib2.get(1).unwrap().unwrap();
        assert_eq!(item1_again.original, "포인터가 멈춘 뒤에야 읽습니다.");
        assert_eq!(item1_again.provider, None);

        cleanup(&path);
    }

    /// Migration v7 → v8: provider column is added, legacy rows have provider = NULL.
    #[test]
    fn migration_v8_adds_provider_column() {
        let path = temp_db("migration-v8");
        let lib = Library::open(&path).unwrap();

        // Verify schema version is 8
        let version: i64 = lib
            .conn()
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 8, "schema must be at version 8");

        // Insert an item and verify provider can be read back
        let item = Item {
            id: 0,
            original: "test".to_owned(),
            answer: "answer".to_owned(),
            caveat: None,
            app: None,
            created_at: 1000,
            kind: "translation".to_owned(),
            source_language: None,
            target_language: Some("ko".to_owned()),
            model: None,
            provider: Some("ollama".to_owned()),
            note: None,
            tags: None,
            review_state: None,
        };
        let id = lib.insert(&item).unwrap();

        let retrieved = lib.get(id).unwrap().unwrap();
        assert_eq!(retrieved.provider, Some("ollama".to_owned()));

        // Insert an item with provider = None (legacy simulation)
        let legacy_item = Item {
            id: 0,
            original: "legacy".to_owned(),
            answer: "old answer".to_owned(),
            caveat: None,
            app: None,
            created_at: 500,
            kind: "translation".to_owned(),
            source_language: None,
            target_language: Some("ko".to_owned()),
            model: None,
            provider: None,
            note: None,
            tags: None,
            review_state: None,
        };
        let legacy_id = lib.insert(&legacy_item).unwrap();

        let legacy_retrieved = lib.get(legacy_id).unwrap().unwrap();
        assert_eq!(
            legacy_retrieved.provider, None,
            "legacy items have NULL provider"
        );

        cleanup(&path);
    }

    /// History preserves provider through promotion to library.
    #[test]
    fn history_promote_preserves_provider() {
        let path = temp_db("history-promote-provider");
        let lib = Library::open(&path).unwrap();

        let entry = HistoryEntry {
            id: 0,
            original: "promoted text".to_owned(),
            answer: "promoted answer".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: Some("ko".to_owned()),
            model: Some("qwen2.5:7b".to_owned()),
            provider: Some("ollama".to_owned()),
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        let history_id = lib.record_history(&entry).unwrap();

        let library_id = lib.history_promote(history_id).unwrap();

        let promoted = lib.get(library_id).unwrap().unwrap();
        assert_eq!(promoted.original, "promoted text");
        assert_eq!(
            promoted.provider,
            Some("ollama".to_owned()),
            "provider must survive promotion"
        );
        assert_eq!(promoted.model, Some("qwen2.5:7b".to_owned()));

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
            provider: None,
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
            provider: None,
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

        // Verify schema version is 8
        let version: i64 = lib
            .conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 8);

        // Verify library table has review_state column (from migration 4)
        // and archived_at column (from migration 7)
        let columns: Vec<String> = lib
            .conn
            .prepare("PRAGMA table_info(library)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.contains(&"review_state".to_owned()));
        assert!(columns.contains(&"archived_at".to_owned()));

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
            provider: None,
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

        // Verify schema version is still 8
        let version: i64 = lib2
            .conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 8);

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

    /// JSONL v2 round-trip preserves review state.
    #[test]
    fn jsonl_round_trip_preserves_review_state() {
        let path_a = temp_db("jsonl-review-export");
        let lib_a = Library::open(&path_a).unwrap();

        // Create items with different review states.
        let item1 = a_translation();
        let id1 = lib_a.insert(&item1).unwrap();
        lib_a.set_review_state(id1, REVIEW_KNOWN).unwrap();

        let mut item2 = a_translation();
        item2.original = "second item".to_owned();
        let id2 = lib_a.insert(&item2).unwrap();
        lib_a.set_review_state(id2, REVIEW_ARCHIVE).unwrap();

        // Export and import (v2 format).
        let jsonl = lib_a.export_jsonl().unwrap();
        let path_b = temp_db("jsonl-review-import");
        let lib_b = Library::open(&path_b).unwrap();
        lib_b.import_jsonl(&jsonl).unwrap();

        // Verify review states are preserved.
        let items = lib_b.list(0).unwrap().items;
        assert_eq!(items.len(), 2);
        let known = items.iter().find(|i| i.original == item1.original).unwrap();
        assert_eq!(known.review_state.as_deref(), Some("known"));
        let archived = items.iter().find(|i| i.original == "second item").unwrap();
        assert_eq!(archived.review_state.as_deref(), Some("archive"));

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

    /// Error message truthfully reports how many items were imported before failure.
    #[test]
    fn jsonl_error_message_reports_partial_import_count() {
        let path = temp_db("jsonl-partial-import-msg");
        let lib = Library::open(&path).unwrap();

        // Two valid items, then a malformed line, then another valid item.
        let jsonl = r#"{"version":1}
{"id":0,"original":"first","answer":"first","kind":"translation","created_at":1000}
{"id":0,"original":"second","answer":"second","kind":"translation","created_at":2000}
{malformed json}
{"id":0,"original":"fourth","answer":"fourth","kind":"translation","created_at":3000}
"#;

        let result = lib.import_jsonl(jsonl);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();

        // Error must mention the line number where the parse failed.
        assert!(
            err_msg.contains("line 4"),
            "error should name the failing line, got: {err_msg}"
        );
        // Error must truthfully report how many items were already imported.
        assert!(
            err_msg.contains("after 2 imported"),
            "error should report partial import count, got: {err_msg}"
        );
        // The two valid items before the malformed line are committed.
        assert_eq!(lib.count().unwrap(), 2);

        cleanup(&path);
    }

    /// Error message when no items were imported yet does not claim partial import.
    #[test]
    fn jsonl_error_message_no_partial_when_zero_imported() {
        let path = temp_db("jsonl-error-zero-imported");
        let lib = Library::open(&path).unwrap();

        let jsonl = r#"{"version":1}
{malformed json on first data line}
"#;

        let result = lib.import_jsonl(jsonl);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();

        assert!(
            err_msg.contains("line 2"),
            "error should name the failing line, got: {err_msg}"
        );
        assert!(
            !err_msg.contains("after"),
            "error should not mention partial import when none imported, got: {err_msg}"
        );
        assert_eq!(lib.count().unwrap(), 0);

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

    /// JSONL round-trip preserves all fields (assertion matrix).
    /// This test verifies the complete field contract per directive section 33.
    #[test]
    fn jsonl_round_trip_preserves_all_fields() {
        let path_a = temp_db("jsonl-all-fields-export");
        let lib_a = Library::open(&path_a).unwrap();

        // Create item with all fields populated
        let item = Item {
            id: 0,
            original: "测试文本".to_owned(),
            answer: "Test text".to_owned(),
            caveat: Some("Note: formal register".to_owned()),
            app: Some("chrome.exe".to_owned()),
            created_at: 1234567890,
            kind: "translation".to_owned(),
            source_language: Some("zh".to_owned()),
            target_language: Some("en".to_owned()),
            model: Some("qwen2.5:7b".to_owned()),
            provider: None,
            note: Some("User's personal note".to_owned()),
            tags: None,
            review_state: None,
        };
        let id = lib_a.insert(&item).unwrap();
        // Set review_state and tags after insertion (insert() hardcodes review_state to "later")
        lib_a.set_review_state(id, REVIEW_KNOWN).unwrap();
        lib_a
            .set_tags(id, &["work".to_owned(), "important".to_owned()])
            .unwrap();

        // Export
        let jsonl = lib_a.export_jsonl().unwrap();
        drop(lib_a);

        // Import to fresh DB
        let path_b = temp_db("jsonl-all-fields-import");
        let lib_b = Library::open(&path_b).unwrap();
        let imported = lib_b.import_jsonl(&jsonl).unwrap();
        assert_eq!(imported, 1);

        // Verify all fields preserved
        let items = lib_b.list(0).unwrap().items;
        assert_eq!(items.len(), 1);
        let imported_item = &items[0];

        assert_eq!(imported_item.original, "测试文本");
        assert_eq!(imported_item.answer, "Test text");
        assert_eq!(
            imported_item.caveat.as_deref(),
            Some("Note: formal register")
        );
        assert_eq!(imported_item.app.as_deref(), Some("chrome.exe"));
        assert_eq!(imported_item.kind, "translation");
        assert_eq!(imported_item.source_language.as_deref(), Some("zh"));
        assert_eq!(imported_item.target_language.as_deref(), Some("en"));
        assert_eq!(imported_item.model.as_deref(), Some("qwen2.5:7b"));
        assert_eq!(imported_item.note.as_deref(), Some("User's personal note"));
        assert_eq!(imported_item.review_state.as_deref(), Some("known"));

        let tags = lib_b.get_tags(imported_item.id).unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"work".to_owned()));
        assert!(tags.contains(&"important".to_owned()));

        // Note: created_at is not preserved (items get new timestamp on import)
        // This is intentional per ADR-0013: imported items are treated as new

        cleanup(&path_a);
        cleanup(&path_b);
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
            provider: None,
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
        assert_eq!(item.review_state.as_deref(), Some("later")); // Fresh review state

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
            provider: None,
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
            provider: None,
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

    /// Review state is initialized to 'later' when promoting history to library.
    #[test]
    fn history_promote_initializes_review_state() {
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
            provider: None,
            app: None,
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };
        let history_id = lib.record_history(&entry).unwrap();
        let library_id = lib.history_promote(history_id).unwrap();

        let item = lib.get(library_id).unwrap().unwrap();
        assert_eq!(item.review_state.as_deref(), Some("later"));

        cleanup(&path);
    }

    // -- review list --------------------------------------------------------

    #[test]
    fn new_items_start_with_review_state_later() {
        let path = temp_db("review-new");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.review_state.as_deref(), Some("later"));

        cleanup(&path);
    }

    #[test]
    fn set_review_state_changes_the_state() {
        let path = temp_db("review-set-state");
        let lib = Library::open(&path).unwrap();
        let id = lib.insert(&a_translation()).unwrap();

        assert!(lib.set_review_state(id, REVIEW_KNOWN).unwrap());
        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.review_state.as_deref(), Some("known"));

        assert!(lib.set_review_state(id, REVIEW_ARCHIVE).unwrap());
        let item = lib.get(id).unwrap().unwrap();
        assert_eq!(item.review_state.as_deref(), Some("archive"));

        // Setting review state on a missing item returns false.
        assert!(!lib.set_review_state(999, REVIEW_KNOWN).unwrap());

        cleanup(&path);
    }

    #[test]
    fn list_later_returns_only_later_items() {
        let path = temp_db("review-later");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        let mut item2 = a_translation();
        item2.original = "second".to_owned();
        let id2 = lib.insert(&item2).unwrap();
        lib.set_review_state(id2, REVIEW_KNOWN).unwrap();

        let page = lib.list_later(0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, id1);

        cleanup(&path);
    }

    #[test]
    fn count_later_returns_the_correct_count() {
        let path = temp_db("review-count-later");
        let lib = Library::open(&path).unwrap();

        assert_eq!(lib.count_later().unwrap(), 0);

        let id1 = lib.insert(&a_translation()).unwrap();
        assert_eq!(lib.count_later().unwrap(), 1);

        let mut item2 = a_translation();
        item2.original = "second".to_owned();
        let id2 = lib.insert(&item2).unwrap();
        assert_eq!(lib.count_later().unwrap(), 2);

        lib.set_review_state(id1, REVIEW_KNOWN).unwrap();
        assert_eq!(lib.count_later().unwrap(), 1);

        lib.set_review_state(id2, REVIEW_ARCHIVE).unwrap();
        assert_eq!(lib.count_later().unwrap(), 0);

        // Unused variable suppression.
        let _ = id1;

        cleanup(&path);
    }

    #[test]
    fn filter_by_review_state() {
        let path = temp_db("review-filter");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        let mut item2 = a_translation();
        item2.original = "known item".to_owned();
        let id2 = lib.insert(&item2).unwrap();
        lib.set_review_state(id2, REVIEW_KNOWN).unwrap();

        let filter = Filter {
            review_state: Some(REVIEW_LATER.to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, id1);

        let filter = Filter {
            review_state: Some(REVIEW_KNOWN.to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, id2);

        cleanup(&path);
    }

    // -- Tag filter tests ---------------------------------------------------

    #[test]
    fn tag_filter_finds_tagged_items() {
        let path = temp_db("tag-filter-basic");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned()]).unwrap();

        let mut item2 = a_translation();
        item2.original = "second".to_owned();
        let id2 = lib.insert(&item2).unwrap();

        let filter = Filter {
            tag: Some("work".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, id1);

        let _ = id2;
        cleanup(&path);
    }

    #[test]
    fn tag_filter_excludes_untagged_items() {
        let path = temp_db("tag-filter-exclude");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned()]).unwrap();

        let mut item2 = a_translation();
        item2.original = "untagged".to_owned();
        let _id2 = lib.insert(&item2).unwrap();

        let filter = Filter {
            tag: Some("work".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, id1);
        assert_eq!(page.total, 1);

        cleanup(&path);
    }

    #[test]
    fn tag_filter_finds_multiple_items_with_same_tag() {
        let path = temp_db("tag-filter-multi");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned()]).unwrap();

        let mut item2 = a_translation();
        item2.original = "second work item".to_owned();
        let id2 = lib.insert(&item2).unwrap();
        lib.set_tags(id2, &["work".to_owned()]).unwrap();

        let filter = Filter {
            tag: Some("work".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, 2);

        cleanup(&path);
    }

    #[test]
    fn tag_filter_supports_korean_tags() {
        let path = temp_db("tag-filter-korean");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["작업".to_owned()]).unwrap();

        let filter = Filter {
            tag: Some("작업".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, id1);

        cleanup(&path);
    }

    #[test]
    fn tag_filter_trims_whitespace() {
        let path = temp_db("tag-filter-trim");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned()]).unwrap();

        let filter = Filter {
            tag: Some("  work  ".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);

        cleanup(&path);
    }

    #[test]
    fn tag_filter_empty_is_no_restriction() {
        let path = temp_db("tag-filter-empty");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned()]).unwrap();

        let mut item2 = a_translation();
        item2.original = "untagged".to_owned();
        let _id2 = lib.insert(&item2).unwrap();

        let filter = Filter {
            tag: Some("".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 2, "empty tag filter returns all items");

        let filter = Filter {
            tag: Some("   ".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(
            page.items.len(),
            2,
            "whitespace-only tag filter returns all items"
        );

        cleanup(&path);
    }

    #[test]
    fn query_and_tag_filter_compose_with_and() {
        let path = temp_db("tag-filter-query-composition");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned()]).unwrap();

        let mut item2 = a_translation();
        item2.original = "work related text".to_owned();
        let _id2 = lib.insert(&item2).unwrap();

        let mut item3 = a_translation();
        item3.original = "work meeting notes".to_owned();
        let id3 = lib.insert(&item3).unwrap();
        lib.set_tags(id3, &["work".to_owned()]).unwrap();

        let filter = Filter {
            tag: Some("work".to_owned()),
            ..Default::default()
        };
        let page = lib.search_and_filter("work", &filter, 0).unwrap();
        assert_eq!(
            page.items.len(),
            1,
            "only item with both 'work' in text AND 'work' tag"
        );
        assert_eq!(page.items[0].id, id3);
        assert_eq!(page.total, 1);

        cleanup(&path);
    }

    #[test]
    fn tag_and_app_filter_compose() {
        let path = temp_db("tag-filter-app-composition");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned()]).unwrap();

        let mut item2 = a_translation();
        item2.original = "second".to_owned();
        item2.app = Some("Firefox".to_owned());
        let id2 = lib.insert(&item2).unwrap();
        lib.set_tags(id2, &["work".to_owned()]).unwrap();

        let filter = Filter {
            tag: Some("work".to_owned()),
            app: Some("Firefox".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, id2);
        assert_eq!(page.total, 1);

        cleanup(&path);
    }

    #[test]
    fn tag_filter_page_total_consistent() {
        let path = temp_db("tag-filter-total");
        let lib = Library::open(&path).unwrap();

        for i in 0..5 {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            let id = lib.insert(&item).unwrap();
            if i < 3 {
                lib.set_tags(id, &["important".to_owned()]).unwrap();
            }
        }

        let filter = Filter {
            tag: Some("important".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.total, 3, "total matches items count for tag filter");

        cleanup(&path);
    }

    #[test]
    fn distinct_tags_returns_only_used_tags() {
        let path = temp_db("tag-distinct");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned(), "urgent".to_owned()])
            .unwrap();

        let mut item2 = a_translation();
        item2.original = "second".to_owned();
        let id2 = lib.insert(&item2).unwrap();
        lib.set_tags(id2, &["personal".to_owned()]).unwrap();

        let mut item3 = a_translation();
        item3.original = "third".to_owned();
        let _id3 = lib.insert(&item3).unwrap();

        let tags = lib.distinct_tags().unwrap();
        assert_eq!(tags.len(), 3);
        assert!(tags.contains(&"work".to_owned()));
        assert!(tags.contains(&"urgent".to_owned()));
        assert!(tags.contains(&"personal".to_owned()));

        let _ = id1;
        let _ = id2;
        cleanup(&path);
    }

    #[test]
    fn tag_filter_no_duplicate_rows() {
        let path = temp_db("tag-filter-no-dupes");
        let lib = Library::open(&path).unwrap();

        let id1 = lib.insert(&a_translation()).unwrap();
        lib.set_tags(id1, &["work".to_owned(), "urgent".to_owned()])
            .unwrap();

        let filter = Filter {
            tag: Some("work".to_owned()),
            ..Default::default()
        };
        let page = lib.list_filtered(&filter, 0).unwrap();
        assert_eq!(
            page.items.len(),
            1,
            "EXISTS subquery prevents duplicate rows"
        );
        assert_eq!(page.total, 1);

        cleanup(&path);
    }

    // -- History tests ------------------------------------------------------

    // -- Review session regression tests ------------------------------------

    #[test]
    fn list_later_session_returns_all_items() {
        let path = temp_db("review-session-all");
        let lib = Library::open(&path).unwrap();

        for i in 0..5 {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            lib.insert(&item).unwrap();
        }

        let session = lib.list_later_session().unwrap();
        assert_eq!(
            session.len(),
            5,
            "all 5 later items returned without pagination"
        );

        cleanup(&path);
    }

    #[test]
    fn list_later_session_exceeds_page_size() {
        let path = temp_db("review-session-page-size");
        let lib = Library::open(&path).unwrap();

        let total = PAGE_SIZE + 7;
        for i in 0..total {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            lib.insert(&item).unwrap();
        }

        let session = lib.list_later_session().unwrap();
        assert_eq!(
            session.len(),
            total as usize,
            "session returns all items beyond PAGE_SIZE"
        );

        cleanup(&path);
    }

    #[test]
    fn review_session_all_known_clears_later() {
        let path = temp_db("review-session-all-known");
        let lib = Library::open(&path).unwrap();

        let mut ids = Vec::new();
        for i in 0..3 {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            ids.push(lib.insert(&item).unwrap());
        }

        let session = lib.list_later_session().unwrap();
        assert_eq!(session.len(), 3);

        for id in &ids {
            lib.set_review_state(*id, REVIEW_KNOWN).unwrap();
        }

        assert_eq!(lib.count_later().unwrap(), 0);
        let session_after = lib.list_later_session().unwrap();
        assert!(session_after.is_empty());

        cleanup(&path);
    }

    #[test]
    fn review_session_all_archive_clears_later() {
        let path = temp_db("review-session-all-archive");
        let lib = Library::open(&path).unwrap();

        let mut ids = Vec::new();
        for i in 0..3 {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            ids.push(lib.insert(&item).unwrap());
        }

        for id in &ids {
            lib.set_review_state(*id, REVIEW_ARCHIVE).unwrap();
        }

        assert_eq!(lib.count_later().unwrap(), 0);

        cleanup(&path);
    }

    #[test]
    fn review_session_keep_later_preserves_db_state() {
        let path = temp_db("review-session-keep-later");
        let lib = Library::open(&path).unwrap();

        for i in 0..3 {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            lib.insert(&item).unwrap();
        }

        let session = lib.list_later_session().unwrap();
        assert_eq!(session.len(), 3);

        for item in &session {
            let state = item.review_state.as_deref().unwrap();
            assert_eq!(state, REVIEW_LATER, "items start as later");
        }

        assert_eq!(
            lib.count_later().unwrap(),
            3,
            "count unchanged after reading session"
        );

        cleanup(&path);
    }

    #[test]
    fn review_session_mixed_actions_exact_final_count() {
        let path = temp_db("review-session-mixed");
        let lib = Library::open(&path).unwrap();

        let mut ids = Vec::new();
        for i in 0..5 {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            ids.push(lib.insert(&item).unwrap());
        }

        lib.set_review_state(ids[0], REVIEW_KNOWN).unwrap();
        lib.set_review_state(ids[1], REVIEW_KNOWN).unwrap();
        lib.set_review_state(ids[2], REVIEW_ARCHIVE).unwrap();

        assert_eq!(
            lib.count_later().unwrap(),
            2,
            "3 removed (2 known + 1 archive), 2 remain later"
        );

        let session = lib.list_later_session().unwrap();
        assert_eq!(session.len(), 2);
        let remaining_ids: Vec<i64> = session.iter().map(|i| i.id).collect();
        assert!(remaining_ids.contains(&ids[3]));
        assert!(remaining_ids.contains(&ids[4]));

        cleanup(&path);
    }

    #[test]
    fn review_session_empty_starting_state() {
        let path = temp_db("review-session-empty");
        let lib = Library::open(&path).unwrap();

        let session = lib.list_later_session().unwrap();
        assert!(session.is_empty());
        assert_eq!(lib.count_later().unwrap(), 0);

        cleanup(&path);
    }

    #[test]
    fn review_session_snapshot_consistent_during_mutation() {
        let path = temp_db("review-session-snapshot");
        let lib = Library::open(&path).unwrap();

        for i in 0..10 {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            lib.insert(&item).unwrap();
        }

        let session = lib.list_later_session().unwrap();
        assert_eq!(session.len(), 10);

        let first_ids: Vec<i64> = session.iter().map(|i| i.id).collect();

        for id in &first_ids[..5] {
            lib.set_review_state(*id, REVIEW_KNOWN).unwrap();
        }

        assert_eq!(lib.count_later().unwrap(), 5);

        let snapshot_ids: std::collections::HashSet<i64> = first_ids.into_iter().collect();
        let db_remaining = lib.list_later_session().unwrap();
        for item in &db_remaining {
            assert!(
                !snapshot_ids.contains(&item.id)
                    || item.review_state.as_deref() == Some(REVIEW_LATER),
                "session snapshot items that were mutated are no longer later"
            );
        }
        assert_eq!(db_remaining.len(), 5);

        cleanup(&path);
    }

    #[test]
    fn review_session_large_set_no_skipped_items() {
        let path = temp_db("review-session-large");
        let lib = Library::open(&path).unwrap();

        let total = PAGE_SIZE * 2 + 13;
        let mut ids = Vec::new();
        for i in 0..total {
            let mut item = a_translation();
            item.original = format!("item {i}");
            item.created_at = 1_786_075_140 + i;
            ids.push(lib.insert(&item).unwrap());
        }

        let session = lib.list_later_session().unwrap();
        assert_eq!(session.len(), total as usize, "all items returned");

        let session_ids: std::collections::HashSet<i64> = session.iter().map(|i| i.id).collect();
        for id in &ids {
            assert!(session_ids.contains(id), "no item skipped in session");
        }

        let mut seen = std::collections::HashSet::new();
        for item in &session {
            assert!(seen.insert(item.id), "no duplicate in session");
        }

        cleanup(&path);
    }

    fn a_history_entry() -> HistoryEntry {
        HistoryEntry {
            id: 0,
            original: "hello world".to_owned(),
            answer: "안녕 세상".to_owned(),
            kind: "translation".to_owned(),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            model: Some("qwen2.5:3b".to_owned()),
            provider: None,
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
        assert_eq!(item.review_state.as_deref(), Some("later"));

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
            provider: None,
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
            provider: None,
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
            provider: None,
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

    #[test]
    fn an_invalid_review_state_is_refused() {
        let path = temp_db("invalid_review_state");
        let lib = Library::open(&path).unwrap();
        let item = a_translation();
        let id = lib.insert(&item).unwrap();

        let result = lib.set_review_state(id, "bogus");
        assert!(result.is_err(), "an invalid state must not be written");
        match result.unwrap_err() {
            Error::InvalidReviewState(got) => assert_eq!(got, "bogus"),
            other => panic!("expected InvalidReviewState, got {other:?}"),
        }

        // The three valid states still work.
        assert!(lib.set_review_state(id, REVIEW_KNOWN).unwrap());
        assert!(lib.set_review_state(id, REVIEW_LATER).unwrap());
        assert!(lib.set_review_state(id, REVIEW_ARCHIVE).unwrap());

        cleanup(&path);
    }

    // -- M8 gate: JSON semantic round-trip ----------------------------------

    /// Exporting as JSON and deserializing back preserves all semantic fields.
    ///
    /// This is the M8 gate condition "export round-trips" for the JSON format.
    /// The JSON export is described as "the lossless export format" — this test
    /// proves it by round-tripping through serde deserialization.
    #[test]
    fn json_export_semantic_round_trip() {
        let path = temp_db("json-round-trip");
        let lib = Library::open(&path).unwrap();

        let original_item = Item {
            id: 0,
            original: "The pointer must rest before anything is read.".to_owned(),
            answer: "포인터가 멈춘 뒤에야 무엇이든 읽습니다.".to_owned(),
            caveat: Some("AI 설명 · 사실 확인 필요".to_owned()),
            app: Some("Code".to_owned()),
            created_at: 1_786_075_140,
            kind: "translation".to_owned(),
            source_language: Some("en".to_owned()),
            target_language: Some("ko".to_owned()),
            model: Some("qwen2.5:7b".to_owned()),
            provider: None,
            note: Some("important".to_owned()),
            tags: None,
            review_state: Some(REVIEW_KNOWN.to_owned()),
        };
        let id = lib.insert(&original_item).unwrap();
        lib.set_tags(id, &["grammar".to_owned(), "essential".to_owned()])
            .unwrap();
        lib.set_review_state(id, REVIEW_KNOWN).unwrap();

        let json = lib.export_json().unwrap();
        let deserialized: Vec<Item> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.len(), 1);

        let rt = &deserialized[0];
        assert_eq!(rt.original, original_item.original);
        assert_eq!(rt.answer, original_item.answer);
        assert_eq!(rt.caveat, original_item.caveat);
        assert_eq!(rt.app, original_item.app);
        assert_eq!(rt.created_at, original_item.created_at);
        assert_eq!(rt.kind, original_item.kind);
        assert_eq!(rt.source_language, original_item.source_language);
        assert_eq!(rt.target_language, original_item.target_language);
        assert_eq!(rt.model, original_item.model);
        assert_eq!(rt.note, original_item.note);
        assert_eq!(rt.review_state, original_item.review_state);
        let mut rt_tags = rt.tags.clone().unwrap_or_default();
        rt_tags.sort();
        assert_eq!(rt_tags, vec!["essential".to_owned(), "grammar".to_owned()]);

        cleanup(&path);
    }

    // -- M8 gate: single delete survives reopen -----------------------------

    /// Deleting a single item persists across database reopen.
    ///
    /// This is the M8 gate condition "deletion is real" for single-item
    /// delete. The existing `secure_wipe_survives_reopen` test covers bulk
    /// wipe; this test covers the per-item contract: delete one row, close
    /// the database, reopen it, and verify the row is gone while other rows
    /// and their associated state survive.
    #[test]
    fn single_delete_survives_reopen() {
        let path = temp_db("delete-reopen");
        let lib = Library::open(&path).unwrap();

        let mut item_a = a_translation();
        item_a.original = "first sentence".to_owned();
        let id_a = lib.insert(&item_a).unwrap();
        lib.set_tags(id_a, &["keep".to_owned()]).unwrap();
        lib.update_note(id_a, "note on A").unwrap();

        let mut item_b = a_translation();
        item_b.original = "second sentence".to_owned();
        let id_b = lib.insert(&item_b).unwrap();
        lib.set_tags(id_b, &["also-keep".to_owned()]).unwrap();

        assert_eq!(lib.count().unwrap(), 2);

        lib.delete(id_a).unwrap();
        assert_eq!(lib.count().unwrap(), 1);

        drop(lib);

        let lib2 = Library::open(&path).unwrap();
        assert_eq!(lib2.count().unwrap(), 1, "one item after reopen");
        assert!(
            lib2.get(id_a).unwrap().is_none(),
            "deleted item A is absent after reopen"
        );
        let b = lib2.get(id_b).unwrap().expect("item B survives");
        assert_eq!(b.original, "second sentence");

        let b_tags = lib2.get_tags(id_b).unwrap();
        assert_eq!(b_tags, vec!["also-keep".to_owned()]);

        let page = lib2.search("first sentence", 0).unwrap();
        assert!(page.items.is_empty(), "search cannot find the deleted item");

        cleanup(&path);
    }

    #[test]
    fn duplicate_originals_with_different_answers_are_both_imported() {
        // Migration is preservation-first. If a historical saved.md contains
        // the same original with different answers (e.g., from different model
        // generations), both entries must be imported, not deduplicated.
        let path = temp_db("duplicate_originals_preserved");
        let lib = Library::open(&path).unwrap();

        let content = concat!(
            "---\n",
            "\n",
            "## 2026-08-07 03:59\n",
            "\n",
            "> same sentence\n",
            "\n",
            "> first answer\n",
            "\n",
            "---\n",
            "\n",
            "## 2026-08-07 04:00\n",
            "\n",
            "> same sentence\n",
            "\n",
            "> second answer\n",
        );

        let entries = parse_saved_md(content);
        assert_eq!(entries.len(), 2, "parser finds both entries");

        // Import both entries without deduplication
        for entry in &entries {
            let kind = if entry.caveat.is_some() {
                "analysis"
            } else {
                "translation"
            };
            let item = Item {
                id: 0,
                original: entry.original.clone(),
                answer: entry.answer.clone(),
                caveat: entry.caveat.clone(),
                app: entry.app.clone(),
                created_at: entry.timestamp,
                kind: kind.to_owned(),
                source_language: None,
                target_language: None,
                model: None,
                provider: None,
                note: None,
                tags: None,
                review_state: None,
            };
            lib.insert(&item).unwrap();
        }

        assert_eq!(lib.count().unwrap(), 2, "both entries imported");
        cleanup(&path);
    }

    // ─── Unix permission hardening ───────────────────────────────────────────

    /// An existing database with broad permissions is tightened to 0o600.
    #[test]
    #[cfg(unix)]
    fn existing_broad_database_is_tightened() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let path = temp_db("harden-broad");
        // Create a DB file with 0o644 (group/other readable).
        {
            let f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .mode(0o644)
                .open(&path)
                .expect("create broad file");
            drop(f);
        }
        let mode_before = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode_before, 0o644, "test setup: file starts broad");

        let lib = Library::open(&path).expect("open succeeds with hardening");
        drop(lib);

        let mode_after = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode_after, 0o600, "hardening tightened to owner-only");
        cleanup(&path);
    }

    /// An existing WAL file with broad permissions is tightened.
    #[test]
    #[cfg(unix)]
    fn existing_broad_wal_is_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_db("harden-wal");
        // Keep a real WAL open. SQLite may remove an unused or invalid WAL
        // when the final connection closes, so a fabricated empty file does
        // not test the permissions of an active database.
        let first = Library::open(&path).expect("open");
        first
            .conn
            .execute_batch(
                "CREATE TABLE wal_probe (value INTEGER); INSERT INTO wal_probe VALUES (1);",
            )
            .expect("write through WAL");
        let wal_path = path.with_extension("db-wal");
        assert!(wal_path.exists(), "test setup: live WAL exists");
        std::fs::set_permissions(&wal_path, std::fs::Permissions::from_mode(0o644))
            .expect("make existing WAL broad");
        let mode_before = std::fs::metadata(&wal_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode_before, 0o644, "test setup: WAL starts broad");

        let second = Library::open(&path).expect("reopen succeeds");
        let mode_after = std::fs::metadata(&wal_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode_after, 0o600, "WAL tightened to owner-only");
        drop(second);
        drop(first);
        cleanup(&path);
    }

    /// A fresh database is created with private permissions from the start.
    #[test]
    #[cfg(unix)]
    fn fresh_database_has_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_db("harden-fresh");
        assert!(!path.exists(), "test setup: no file yet");

        let lib = Library::open(&path).expect("open fresh");
        drop(lib);

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        // SQLite creates files with whatever umask allows. After hardening,
        // group/other bits must be zero.
        assert_eq!(mode & 0o077, 0, "fresh DB must have no group/other bits");
        cleanup(&path);
    }
}
