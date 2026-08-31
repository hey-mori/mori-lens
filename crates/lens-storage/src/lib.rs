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
use std::path::Path;

/// One item in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Total number of items in the library.
    pub fn count(&self) -> Result<i64, Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM library", [], |row| row.get(0))
            .map_err(Error::Sqlite)
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
        }
    }

    // -- schema and migration -----------------------------------------------

    #[test]
    fn opening_a_fresh_database_creates_the_schema() {
        let path = temp_db("fresh");
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.count().unwrap(), 0);

        // Both migration versions are recorded.
        let versions: Vec<i64> = lib
            .conn()
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(versions, vec![1, 2]);
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
}
