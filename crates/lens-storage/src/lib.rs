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
use serde::Serialize;
use std::path::Path;

/// One item in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}

/// A page of items returned by [`Library::list`] or [`Library::search`].
#[derive(Debug, Clone)]
pub struct Page {
    /// The items in this page, newest first.
    pub items: Vec<Item>,
    /// Whether more items exist beyond this page.
    pub has_more: bool,
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
}

/// A page size for listing and searching.
const PAGE_SIZE: i64 = 50;

/// Errors from library operations.
#[derive(Debug)]
pub enum Error {
    /// A SQLite error.
    Sqlite(rusqlite::Error),
    /// The database file could not be created or accessed.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "library database: {e}"),
            Self::Io(e) => write!(f, "library file: {e}"),
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
    pub fn insert(&self, item: &Item) -> Result<i64, Error> {
        self.conn.execute(
            "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List items, newest first.
    pub fn list(&self, offset: i64) -> Result<Page, Error> {
        let limit = PAGE_SIZE + 1;
        let mut stmt = self.conn.prepare(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note FROM library ORDER BY created_at DESC, id DESC LIMIT ?1 OFFSET ?2",
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
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note FROM library WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_item)?;
        match rows.next() {
            Some(Ok(item)) => Ok(Some(item)),
            Some(Err(e)) => Err(Error::Sqlite(e)),
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
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note FROM library WHERE original LIKE ?1 OR answer LIKE ?1 ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
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

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let limit = PAGE_SIZE + 1;
        let sql = format!(
            "SELECT id, original, answer, caveat, app, created_at, kind, source_language, target_language, model, note FROM library{where_clause} ORDER BY created_at DESC, id DESC LIMIT ?{} OFFSET ?{}",
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

    /// The database connection, for callers that need to run their own
    /// queries (e.g. schema introspection in tests).
    pub fn conn(&self) -> &Connection {
        &self.conn
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

        if !applied.contains(&1) {
            self.conn.execute_batch(
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
        }

        // The `meta` table for import tracking. Added as a separate
        // migration so an existing library from migration 1 gains it.
        if !applied.contains(&2) {
            self.conn.execute_batch(
                "CREATE TABLE meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                INSERT INTO schema_migrations (version) VALUES (2);",
            )?;
        }

        // Tags: a many-to-many relationship between items and tags.
        if !applied.contains(&3) {
            self.conn.execute_batch(
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
        }

        Ok(())
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
        note: row.get(10)?,
        tags: None,
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
        assert_eq!(versions, vec![1, 2, 3]);
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
}
