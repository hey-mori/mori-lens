//! Appending what a reader kept to a file they own, and to the library database.
//!
//! The format is [`lens_core::keep`]; this is the part that touches a disk,
//! which is why it is here and not there.
//!
//! # The only file Lens writes that came off a screen
//!
//! `settings.toml` is what somebody typed. The translation cache is memory and
//! dies with the process, on purpose: `docs/privacy-model.md` says a durable
//! cache would be a record of what was read, so it is a separate opt-in and
//! does not exist.
//!
//! This file **is** a record of what was read. What makes it allowed is that a
//! person made every entry, one keypress at a time, looking at the card. That
//! is a property of the trigger, so the rule this module has to keep is that
//! nothing here is reachable from a hover.
//!
//! # Appended, never rewritten
//!
//! Opened in append mode and closed again. Lens never reads it back, never
//! parses it, and never rewrites it, which is what makes it safe for a reader
//! to keep their own notes in it and what makes deleting a block a real
//! deletion rather than a flag.
//!
//! # The library database
//!
//! Every entry that goes to `saved.md` also goes to `library.db`
//! ([ADR-0013](../../docs/decisions/ADR-0013-the-local-library.md)). The
//! database is the structured store; the file is the human-readable export.
//! If the database write fails the entry is still written to the file: the
//! file is the safety net that was there before the database existed.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use lens_core::card::Card;
use lens_core::keep::{block, Entry, PREAMBLE};
use lens_storage::{Item, Library};

/// Serialize all save operations so concurrent saves cannot interleave
/// file writes or race on the preamble/duplicate checks.
static SAVE_LOCK: Mutex<()> = Mutex::new(());

/// Where saved items go, beside the settings file.
///
/// Beside it because a reader who has found one has found the other, and the
/// settings file's path is printed at startup and reachable from the tray.
#[allow(dead_code)]
pub fn path_beside(settings: &Path) -> PathBuf {
    settings
        .parent()
        .map(|folder| folder.join("saved.md"))
        .unwrap_or_else(|| PathBuf::from("saved.md"))
}

/// Outcome of writing to the structured store (library.db).
///
/// The markdown file (saved.md) is the durable safety net and is always
/// written before the structured store. This type distinguishes what
/// happened on the structured-store side so callers can tell a legitimate
/// duplicate from a query or insert failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuredSave {
    /// The item was inserted. Carries the new row id.
    Inserted(i64),
    /// The item is a duplicate (same original, kind, model, target_language
    /// per ADR-0013). The database already has this item.
    Duplicate,
    /// The structured store write failed. The markdown file already has
    /// the entry. Carries the error detail for diagnostics.
    Failed(String),
}

impl StructuredSave {
    /// The inserted row id, if any.
    #[allow(dead_code)]
    pub fn item_id(&self) -> Option<i64> {
        match self {
            Self::Inserted(id) => Some(*id),
            Self::Duplicate | Self::Failed(_) => None,
        }
    }
}

/// Why an entry was not kept, in words a card can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotKept {
    /// The card has no answer on it, so there is nothing to keep.
    NothingToKeep,
    /// The file could not be opened or written. Carries the I/O error
    /// detail for logging; `user_message` returns a stable Korean string.
    NotWritten(String),
}

impl NotKept {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::NothingToKeep => "저장할 내용이 아직 없습니다",
            Self::NotWritten(_) => "저장 파일에 쓰지 못했습니다",
        }
    }

    #[allow(dead_code)]
    pub fn reason(&self) -> &'static str {
        match self {
            Self::NothingToKeep => "nothing-to-keep",
            Self::NotWritten(_) => "not-written",
        }
    }

    /// I/O error detail for logging. Empty for non-I/O variants.
    #[allow(dead_code)] // Retained for cross-platform parity (used by macOS).
    pub fn detail(&self) -> &str {
        match self {
            Self::NotWritten(ref msg) => msg,
            _ => "",
        }
    }
}

/// Append one entry to `saved.md` and insert it into `library.db`.
///
/// The file write is the primary store. The database write is best effort:
/// if it fails the entry is still saved to the file. Both files live beside
/// the settings file.
///
/// Creates the file and its heading on first use, and opens (or creates) the
/// database and runs any pending migrations.
///
/// Returns a [`StructuredSave`] that distinguishes a real insert from a
/// duplicate or a database failure. The file write always succeeds
/// independently.
///
/// # Concurrency
///
/// All persistence is serialized through a process-wide `SAVE_LOCK`. Every
/// caller — `save_card()`, direct `append()` from Windows hotkey, Tauri
/// detail save, self-test — passes through this single lock. No production
/// path can bypass it.
pub fn append(
    file: &Path,
    entry: &Entry,
    at: SystemTime,
    library: &Library,
) -> Result<StructuredSave, NotKept> {
    let _guard = SAVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(folder) = file.parent() {
        // The settings folder already exists by the time anything is saved,
        // but a reader who pointed `--settings` somewhere new has not created
        // one yet.
        let _ = std::fs::create_dir_all(folder);
    }
    let mut handle = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(file)
                .map_err(|e| NotKept::NotWritten(e.to_string()))?
        }
        #[cfg(not(unix))]
        {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file)
                .map_err(|e| NotKept::NotWritten(e.to_string()))?
        }
    };

    // The heading is written once, with the first entry rather than at
    // startup: a reader who never saves anything should not find a file
    // explaining a feature they did not use. Check the actual file size
    // after opening to avoid a TOCTOU race with concurrent saves.
    let is_empty = file.metadata().map(|m| m.len() == 0).unwrap_or(false);
    if is_empty {
        handle
            .write_all(PREAMBLE.as_bytes())
            .map_err(|e| NotKept::NotWritten(e.to_string()))?;
    }

    // Tighten permissions on existing files that were created before the
    // mode(0o600) guard. A file with group/other read bits is tightened
    // to owner-only. Errors are ignored because the file is still usable
    // even if chmod fails (e.g. on a filesystem without Unix permissions).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = file.metadata() {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                let _ = std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    handle
        .write_all(block(entry, at).as_bytes())
        .map_err(|e| NotKept::NotWritten(e.to_string()))?;

    // The structured store. If the database write fails, the file has
    // already succeeded, and the entry is not lost.
    Ok(write_to_library(library, entry, at))
}

/// Cross-platform save policy: eligibility check and metadata population.
///
/// All Save entry points (Windows hotkey, macOS hotkey, detail UI) should use
/// this function when they have a `Card` to save. It enforces:
///
/// - **Eligibility**: only Translation and Analysis cards are savable.
///   Problem and Waiting cards return `Err(NotKept::NothingToKeep)`.
/// - **Metadata**: target_language and model are populated on the Entry.
/// - **Storage contract**: both saved.md and library.db are written via
///   `append()`, per ADR-0013/ADR-0015.
///
/// Serialization is handled by `append()` itself — no separate lock needed here.
///
/// The model name comes from the card's metadata, which was set at card
/// construction time from the actual translation result. Callers must not
/// pass a model from current configuration: a settings change between the
/// reading and the save would overwrite the provenance with a model that
/// did not produce the answer.
#[allow(dead_code)] // Used by macOS; Windows calls `append` directly.
pub fn save_card(
    card: &Card,
    app: Option<&str>,
    save_path: &Path,
    target_language: Option<String>,
    library: &Library,
) -> Result<StructuredSave, NotKept> {
    let mut entry = card.kept_as(app).ok_or(NotKept::NothingToKeep)?;
    entry.target_language = target_language;

    append(save_path, &entry, SystemTime::now(), library)
}

/// Insert the entry into the shared library database.
///
/// Returns a [`StructuredSave`] that distinguishes a real insert from a
/// legitimate duplicate and from a query or insert failure. The caller
/// (`append`) has already written the file, so a database error here does
/// not prevent the save from completing.
///
/// A duplicate (same original text, kind, model, and target language per
/// ADR-0013) is silently skipped: the file still gets the entry because
/// it is append-only, but the database does not store the same sentence
/// twice with the same provenance.
///
/// A `has_duplicate` query failure is reported as [`StructuredSave::Failed`]
/// rather than silently treated as "not duplicate" — a SELECT error is not
/// evidence that the item is new.
fn write_to_library(lib: &Library, entry: &Entry, at: SystemTime) -> StructuredSave {
    let seconds = at
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs() as i64;
    let kind = if entry.caveat.is_some() {
        "analysis"
    } else {
        "translation"
    };

    // Skip duplicates: a sentence kept twice with the same model and target
    // language produces one database row. ADR-0013 requires that a different
    // model or a different target language for the same sentence is a new row.
    let duplicate_check = lib.has_duplicate(
        &entry.original,
        kind,
        entry.model.as_deref(),
        entry.target_language.as_deref(),
    );
    match duplicate_check {
        Ok(true) => return StructuredSave::Duplicate,
        Err(e) => {
            // A SELECT failure is not evidence that the item is new. Return
            // Failed rather than proceeding to insert and creating a duplicate.
            return StructuredSave::Failed(format!("has_duplicate: {e}"));
        }
        Ok(false) => {}
    }

    let item = Item {
        id: 0,
        original: entry.original.clone(),
        answer: entry.answer.clone(),
        caveat: entry.caveat.clone(),
        app: entry.app.clone(),
        created_at: seconds,
        kind: kind.to_owned(),
        source_language: entry.source_language.clone(),
        target_language: entry.target_language.clone(),
        model: entry.model.clone(),
        note: None,
        tags: None,
        review_state: None,
    };
    match lib.insert(&item) {
        Ok(id) => StructuredSave::Inserted(id),
        Err(e) => StructuredSave::Failed(format!("insert: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("mori-lens-keep-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("create scratch directory");
        folder.join("saved.md")
    }

    /// Create a Library backed by the library.db next to the scratch file.
    fn scratch_library(md_path: &Path) -> Library {
        let db_path = md_path.parent().unwrap().join("library.db");
        Library::open(&db_path).expect("in-memory or disk library")
    }

    fn an_entry(original: &str) -> Entry {
        Entry {
            original: original.to_owned(),
            answer: "포인터가 멈춘 뒤에야 읽습니다.".to_owned(),
            caveat: None,
            app: Some("Code".to_owned()),
            source_language: None,
            target_language: None,
            model: None,
        }
    }

    #[test]
    fn the_heading_is_written_once_and_entries_accumulate() {
        let file = scratch("accumulate");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);
        let lib = scratch_library(&file);

        append(&file, &an_entry("first"), at, &lib).expect("kept");
        append(&file, &an_entry("second"), at, &lib).expect("kept");
        let written = std::fs::read_to_string(&file).expect("a file");

        assert_eq!(written.matches("# Saved from Mori Lens").count(), 1);
        assert!(written.contains("> first"), "{written}");
        assert!(written.contains("> second"), "{written}");
        // In the order they were kept, because a reader reads their notes
        // forward.
        assert!(written.find("> first") < written.find("> second"));
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    /// Nothing already in the file is touched.
    ///
    /// The reader is invited to write their own notes in it, and an append
    /// that rewrote the file would eat them. This is also what makes deleting
    /// a block a real deletion.
    #[test]
    fn a_readers_own_notes_survive_an_append() {
        let file = scratch("notes");
        let at = UNIX_EPOCH;
        let lib = scratch_library(&file);
        append(&file, &an_entry("first"), at, &lib).expect("kept");

        let mine = "\n<!-- my own note, do not eat -->\n";
        let mut handle = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .expect("open");
        handle.write_all(mine.as_bytes()).expect("write");
        drop(handle);

        append(&file, &an_entry("second"), at, &lib).expect("kept");
        let written = std::fs::read_to_string(&file).expect("a file");
        assert!(written.contains("my own note, do not eat"), "{written}");
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    #[test]
    #[cfg(windows)]
    fn the_file_lands_beside_the_settings() {
        let settings =
            PathBuf::from("C:\\Users\\someone\\AppData\\Roaming\\Mori Lens\\settings.toml");
        assert_eq!(
            path_beside(&settings),
            PathBuf::from("C:\\Users\\someone\\AppData\\Roaming\\Mori Lens\\saved.md"),
        );
    }

    #[test]
    #[cfg(windows)]
    fn the_database_lands_beside_the_settings() {
        let settings =
            PathBuf::from("C:\\Users\\someone\\AppData\\Roaming\\Mori Lens\\settings.toml");
        let db = settings.parent().map(|p| p.join("library.db")).unwrap();
        assert_eq!(
            db,
            PathBuf::from("C:\\Users\\someone\\AppData\\Roaming\\Mori Lens\\library.db"),
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn the_file_lands_beside_the_settings_unix() {
        let settings = PathBuf::from("/home/someone/.config/Mori Lens/settings.toml");
        assert_eq!(
            path_beside(&settings),
            PathBuf::from("/home/someone/.config/Mori Lens/saved.md"),
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn the_database_lands_beside_the_settings_unix() {
        let settings = PathBuf::from("/home/someone/.config/Mori Lens/settings.toml");
        let db = settings.parent().map(|p| p.join("library.db")).unwrap();
        assert_eq!(
            db,
            PathBuf::from("/home/someone/.config/Mori Lens/library.db"),
        );
    }

    #[test]
    fn every_refusal_has_a_sentence_and_a_reason() {
        let refusals = [NotKept::NothingToKeep, NotKept::NotWritten("test".into())];
        for refusal in &refusals {
            assert!(!refusal.user_message().is_empty());
            assert_ne!(refusal.user_message(), refusal.reason());
        }
    }

    /// A save also creates a library.db with the item in it.
    #[test]
    fn a_save_also_writes_to_the_library_database() {
        let file = scratch("library-write");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);
        let lib = scratch_library(&file);
        append(&file, &an_entry("hello"), at, &lib).expect("kept");

        let db_path = file.parent().unwrap().join("library.db");
        assert!(db_path.exists(), "library.db was created");

        let verify = Library::open(&db_path).unwrap();
        assert_eq!(verify.count().unwrap(), 1);
        let item = verify.get(1).unwrap().unwrap();
        assert_eq!(item.original, "hello");
        assert_eq!(item.kind, "translation");
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    /// An analysis entry is stored with kind "analysis".
    #[test]
    fn an_analysis_entry_is_kind_analysis_in_the_database() {
        let file = scratch("library-analysis");
        let at = UNIX_EPOCH;
        let lib = scratch_library(&file);
        let entry = Entry {
            original: "멱등성".to_owned(),
            answer: "원형 · 멱등성".to_owned(),
            caveat: Some("AI 설명 · 사실 확인 필요".to_owned()),
            app: None,
            source_language: None,
            target_language: None,
            model: None,
        };
        append(&file, &entry, at, &lib).expect("kept");

        let db_path = file.parent().unwrap().join("library.db");
        let verify = Library::open(&db_path).unwrap();
        let item = verify.get(1).unwrap().unwrap();
        assert_eq!(item.kind, "analysis");
        assert!(item.caveat.is_some());
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    /// A save returns the library item id so the caller can attach a note.
    #[test]
    fn append_returns_the_library_item_id() {
        let file = scratch("append-returns-id");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);
        let lib = scratch_library(&file);

        let save = append(&file, &an_entry("hello"), at, &lib).expect("kept");
        assert!(matches!(save, StructuredSave::Inserted(id) if id > 0));

        // A duplicate returns Duplicate (file succeeds, database skips).
        let dup = append(&file, &an_entry("hello"), at, &lib).expect("kept");
        assert_eq!(
            dup,
            StructuredSave::Duplicate,
            "duplicate returns Duplicate"
        );
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    /// StructuredSave::item_id() returns Some only for Inserted.
    #[test]
    fn structured_save_item_id_variants() {
        assert_eq!(StructuredSave::Inserted(42).item_id(), Some(42));
        assert_eq!(StructuredSave::Duplicate.item_id(), None);
        assert_eq!(StructuredSave::Failed("err".into()).item_id(), None);
    }

    /// A different model for the same sentence is a new row, not a duplicate.
    #[test]
    fn different_model_is_not_a_duplicate() {
        let file = scratch("diff-model-not-dup");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);
        let lib = scratch_library(&file);

        let mut entry_a = an_entry("same sentence");
        entry_a.model = Some("model-a".to_owned());
        let mut entry_b = an_entry("same sentence");
        entry_b.model = Some("model-b".to_owned());

        let save_a = append(&file, &entry_a, at, &lib).expect("first keep");
        assert!(matches!(save_a, StructuredSave::Inserted(_)));

        let save_b = append(&file, &entry_b, at, &lib).expect("second keep");
        assert!(matches!(save_b, StructuredSave::Inserted(_)));

        // Different models produce two rows even for the same original text.
        let db_path = file.parent().unwrap().join("library.db");
        let verify = Library::open(&db_path).unwrap();
        assert_eq!(
            verify.count().unwrap(),
            2,
            "different model is not duplicate"
        );
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    /// A duplicate sentence is written to the file but not stored twice.
    #[test]
    fn a_duplicate_sentence_is_not_stored_twice_in_the_database() {
        let file = scratch("library-duplicate");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);
        let entry = an_entry("the same sentence");
        let lib = scratch_library(&file);

        append(&file, &entry, at, &lib).expect("first keep");
        append(&file, &entry, at, &lib).expect("second keep");

        // The file has both entries (append-only).
        let written = std::fs::read_to_string(&file).expect("a file");
        assert_eq!(
            written.matches("> the same sentence").count(),
            2,
            "file is append-only"
        );

        // The database has only one (deduplicated).
        let db_path = file.parent().unwrap().join("library.db");
        let verify = Library::open(&db_path).unwrap();
        assert_eq!(verify.count().unwrap(), 1, "database deduplicates");
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    /// `append()` writes to both saved.md and the shared library database.
    /// The caller creates the library before calling append.
    ///
    /// Caller audit (the M8 "nothing stored without user action" contract):
    ///
    /// - win.rs:1305 — Windows Ctrl+Alt+S hotkey (Pressed::Keep)
    /// - detail_view.rs:175 — Tauri detail window Save button
    /// - win.rs:2925 — self-test diagnostic (temp file, not user data)
    ///
    /// No other code path (hover, Quick, Deep, SessionRecent) calls append().
    /// macOS execute_save() also uses save_card(), writing to both files.
    #[test]
    fn append_writes_both_file_and_library() {
        let dir = std::env::temp_dir().join("mori-lens-keep-no-persist");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved.md");
        let db = dir.join("library.db");

        assert!(!file.exists(), "saved.md must not exist before save");

        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);
        let lib = scratch_library(&file);
        append(&file, &an_entry("hello"), at, &lib).expect("kept");

        assert!(file.exists(), "saved.md exists after save");
        assert!(db.exists(), "library.db exists after save");

        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("> hello"));

        assert_eq!(lib.count().unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Language and model metadata survive the round-trip into the database.
    #[test]
    fn language_and_model_metadata_are_stored_in_the_database() {
        let file = scratch("library-metadata");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);
        let lib = scratch_library(&file);
        let entry = Entry {
            original: "The pointer must rest.".to_owned(),
            answer: "포인터가 멈춘 뒤에야 읽습니다.".to_owned(),
            caveat: None,
            app: Some("Code".to_owned()),
            source_language: Some("English".to_owned()),
            target_language: Some("Korean".to_owned()),
            model: Some("qwen2.5:7b".to_owned()),
        };
        append(&file, &entry, at, &lib).expect("kept");

        let db_path = file.parent().unwrap().join("library.db");
        let verify = Library::open(&db_path).unwrap();
        let item = verify.get(1).unwrap().unwrap();
        assert_eq!(item.source_language.as_deref(), Some("English"));
        assert_eq!(item.target_language.as_deref(), Some("Korean"));
        assert_eq!(item.model.as_deref(), Some("qwen2.5:7b"));
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    // -- save_card: cross-platform save policy ------------------------------

    fn a_translation_card() -> Card {
        use lens_core::translate::{EngineId, QuickTranslation};
        Card::translated(
            "Hello world",
            &QuickTranslation {
                text: "안녕 세계".to_owned(),
                engine: EngineId::LocalOllama,
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 42,
                cached: false,
                part_was_removed: false,
                name_changed: false,
            },
        )
    }

    /// save_card refuses Waiting cards — they are transient, not savable.
    #[test]
    fn save_card_refuses_waiting() {
        let dir = std::env::temp_dir().join("mori-lens-save-refuse-waiting");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved.md");
        let lib = Library::open(Path::new(":memory:")).unwrap();

        let card = Card::waiting("번역 중");
        let result = save_card(&card, None, &file, Some("ko".into()), &lib);
        assert!(
            matches!(result, Err(NotKept::NothingToKeep)),
            "Waiting card must be refused"
        );
        assert!(!file.exists(), "saved.md must not be created for Waiting");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// save_card accepts Translation cards and writes both saved.md and library.db.
    #[test]
    fn save_card_accepts_translation() {
        let dir = std::env::temp_dir().join("mori-lens-save-accept-translation");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved.md");
        let db_path = dir.join("library.db");
        let lib = Library::open(&db_path).unwrap();

        let card = a_translation_card();
        let result = save_card(&card, Some("TestApp"), &file, Some("ko".into()), &lib);
        assert!(result.is_ok(), "Translation card must be savable");
        assert!(file.exists(), "saved.md must exist after save");

        let verify = Library::open(&db_path).unwrap();
        assert_eq!(verify.count().unwrap(), 1);
        let item = verify.get(1).unwrap().unwrap();
        assert_eq!(item.original, "Hello world");
        assert_eq!(item.target_language.as_deref(), Some("ko"));
        assert_eq!(item.model.as_deref(), Some("qwen2.5:7b"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// save_card stores the model from the card's metadata, not from any
    /// external source. This is the deterministic proof that a settings
    /// change between card creation and save cannot alter the provenance:
    /// the model was baked into the card when the translation result
    /// arrived, and save_card copies it from there.
    #[test]
    fn save_card_model_comes_from_card_metadata() {
        let dir = std::env::temp_dir().join("mori-lens-save-model-metadata");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved.md");
        let db_path = dir.join("library.db");
        let lib = Library::open(&db_path).unwrap();

        // The card carries "qwen2.5:7b" in its metadata from construction.
        let card = a_translation_card();
        assert_eq!(
            card.metadata().and_then(|m| m.model.as_deref()),
            Some("qwen2.5:7b"),
            "test setup: card metadata must carry the model"
        );

        let result = save_card(&card, Some("TestApp"), &file, Some("ko".into()), &lib);
        assert!(result.is_ok());

        // The stored model matches the card metadata, not any caller value.
        let verify = Library::open(&db_path).unwrap();
        let item = verify.get(1).unwrap().unwrap();
        assert_eq!(
            item.model.as_deref(),
            Some("qwen2.5:7b"),
            "model must come from card metadata, not caller"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Concurrent saves via `save_card()` produce exactly one preamble.
    #[test]
    fn concurrent_save_card_does_not_duplicate_preamble() {
        use std::sync::Arc;
        use std::thread;

        let dir = std::env::temp_dir().join("mori-lens-save-concurrent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = Arc::new(dir.join("saved.md"));
        let db_path = dir.join("library.db");

        // Pre-initialise the database so concurrent Library::open calls
        // do not race on migrations.
        Library::open(&db_path).unwrap();

        let threads: Vec<_> = (0..4)
            .map(|_| {
                let file = file.clone();
                let db_path = db_path.clone();
                thread::spawn(move || {
                    let card = a_translation_card();
                    let lib = Library::open(&db_path).unwrap();
                    let _ = save_card(
                        &card,
                        Some("ConcurrentTest"),
                        &file,
                        Some("ko".into()),
                        &lib,
                    );
                })
            })
            .collect();

        for t in threads {
            t.join().unwrap();
        }

        let content = std::fs::read_to_string(file.as_path()).unwrap();
        let preamble_count = content.matches("# Saved from Mori Lens").count();
        assert_eq!(
            preamble_count, 1,
            "exactly one preamble expected, got {preamble_count}"
        );

        let block_count = content.matches("> Hello world").count();
        assert_eq!(
            block_count, 4,
            "all 4 saves must be present, got {block_count}"
        );

        let lib = Library::open(&db_path).unwrap();
        assert!(lib.count().unwrap() >= 1, "library must have rows");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Concurrent direct `append()` calls produce exactly one preamble.
    /// This tests the real serialization boundary — the lock lives in
    /// `append()`, so even callers that bypass `save_card()` are serialized.
    #[test]
    fn concurrent_direct_append_does_not_duplicate_preamble() {
        use std::sync::Arc;
        use std::thread;

        let dir = std::env::temp_dir().join("mori-lens-append-concurrent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = Arc::new(dir.join("saved.md"));
        let db_path = dir.join("library.db");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);

        // Pre-initialise the database so concurrent Library::open calls
        // do not race on migrations.
        Library::open(&db_path).unwrap();

        let threads: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                let db_path = db_path.clone();
                thread::spawn(move || {
                    let entry = an_entry(&format!("concurrent item {i}"));
                    let lib = Library::open(&db_path).unwrap();
                    let _ = append(&file, &entry, at, &lib);
                })
            })
            .collect();

        for t in threads {
            t.join().unwrap();
        }

        let content = std::fs::read_to_string(file.as_path()).unwrap();
        let preamble_count = content.matches("# Saved from Mori Lens").count();
        assert_eq!(
            preamble_count, 1,
            "exactly one preamble expected, got {preamble_count}"
        );

        for i in 0..8 {
            assert!(
                content.contains(&format!("concurrent item {i}")),
                "item {i} missing from saved.md"
            );
        }

        let lib = Library::open(&db_path).unwrap();
        assert_eq!(
            lib.count().unwrap(),
            8,
            "all 8 items must be in the database"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
