//! Library service: the shared handle to the SQLite library database.
//!
//! The database lives beside `saved.md` in the data directory. The service
//! wraps it in a `Mutex` so Tauri commands can read and write safely.
//!
//! On first activation, any existing `saved.md` is imported into the database
//! ([ADR-0013](../../docs/decisions/ADR-0013-the-local-library.md)). The
//! import is best-effort and tracked via the `meta` table so it runs once.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lens_storage::{parse_saved_md, Item, Library};

/// Shared library state, accessed by Tauri commands.
pub type SharedLibrary = Arc<Mutex<LibraryState>>;

/// How the library database is backed.
///
/// The persistent case is the normal path: SQLite on disk. The in-memory
/// case is a degraded fallback when the disk database could not be opened
/// (corrupt, locked, permission denied, or future schema from a newer
/// build). UI and diagnostics should distinguish these so operators do
/// not assume persistence when writes are going nowhere durable.
pub enum LibraryBacking {
    /// The library lives on disk at the given path. Writes survive restarts.
    Persistent(PathBuf),
    /// The library is an ephemeral in-memory database. The intended path
    /// is recorded so diagnostics can tell the operator where data _would_
    /// have gone, and the reason explains why the fallback was chosen.
    InMemoryFallback {
        intended_path: PathBuf,
        reason: String,
    },
}

/// The service's state: the database handle and its backing.
pub struct LibraryState {
    /// The database handle.
    library: Library,
    /// Where the database lives (disk or in-memory fallback).
    backing: LibraryBacking,
}

impl LibraryState {
    /// Access the underlying library.
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// The backing mode. Use this to distinguish persistent storage from
    /// a degraded in-memory fallback.
    pub fn backing(&self) -> &LibraryBacking {
        &self.backing
    }

    /// The database path. For persistent backing this is where the file
    /// lives. For in-memory fallback this is where it _would_ have been
    /// opened (useful for diagnostics and log messages).
    pub fn path(&self) -> &Path {
        match &self.backing {
            LibraryBacking::Persistent(p) => p,
            LibraryBacking::InMemoryFallback { intended_path, .. } => intended_path,
        }
    }
}

/// Open (or create) the library database beside the data directory.
///
/// If a `saved.md` exists in the data directory and has not been imported
/// yet, its entries are imported into the database before returning.
///
/// # Failure handling
///
/// If the database cannot be opened (corrupt, locked, permission denied),
/// falls back to an in-memory database so the UI still works. If the
/// database was written by a newer Lens build (future schema), the
/// fallback is also in-memory but with a clearer diagnostic: the user
/// needs to upgrade Lens, not repair the database.
pub fn open(data_dir: &Path) -> LibraryState {
    let db_path = data_dir.join("library.db");
    let (library, backing) = match Library::open(&db_path) {
        Ok(lib) => (lib, LibraryBacking::Persistent(db_path.clone())),
        Err(lens_storage::Error::FutureSchema { have, understand }) => {
            let reason = format!(
                "written by newer Lens (schema {have}, this build understands {understand})"
            );
            eprintln!(
                "library   {db_path:?} was written by a newer Lens (schema {have}, this build understands {understand}); using in-memory database until you upgrade"
            );
            (
                Library::open(Path::new(":memory:")).expect("in-memory library"),
                LibraryBacking::InMemoryFallback {
                    intended_path: db_path.clone(),
                    reason,
                },
            )
        }
        Err(e) => {
            let reason = format!("cannot open: {e}");
            eprintln!("library   cannot open {db_path:?}: {e}");
            // Fall back to a temporary database so the UI still works.
            (
                Library::open(Path::new(":memory:")).expect("in-memory library"),
                LibraryBacking::InMemoryFallback {
                    intended_path: db_path.clone(),
                    reason,
                },
            )
        }
    };
    import_saved_md(&library, data_dir);
    LibraryState { library, backing }
}

/// Import an existing `saved.md` into the library database, if not done yet.
///
/// Best-effort: parse errors and insert failures are logged but do not
/// prevent the application from starting. The import is tracked so it
/// runs at most once per database.
fn import_saved_md(library: &Library, data_dir: &Path) {
    // Already imported, or the database cannot track imports.
    let already = match library.is_imported() {
        Ok(true) => return,
        Ok(false) => false,
        Err(e) => {
            eprintln!("library   import check failed: {e}");
            return;
        }
    };
    let _ = already;

    let md_path = data_dir.join("saved.md");
    let content = match std::fs::read_to_string(&md_path) {
        Ok(c) => c,
        Err(_) => {
            // No saved.md to import. Mark as imported so we do not check again.
            let _ = library.mark_imported();
            return;
        }
    };

    let entries = parse_saved_md(&content);
    if entries.is_empty() {
        let _ = library.mark_imported();
        return;
    }

    let mut imported = 0u32;
    for entry in &entries {
        let kind = if entry.caveat.is_some() {
            "analysis"
        } else {
            "translation"
        };
        // Migration is preservation-first, not merge. Historical entries may
        // have the same original with different answers (e.g., from different
        // model generations or prompt versions). Import all of them; the live
        // dedup policy applies to new saves, not to historical migration.
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
        match library.insert(&item) {
            Ok(_) => imported += 1,
            Err(e) => eprintln!("library   import insert failed: {e}"),
        }
    }

    if let Err(e) = library.mark_imported() {
        eprintln!("library   import mark failed: {e}");
    }
    if imported > 0 {
        println!("library   imported {imported} entries from saved.md");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lens-library-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Opening against a fresh temp directory produces a persistent backing.
    #[test]
    fn open_with_valid_data_dir_is_persistent() {
        let dir = temp_dir("persistent");
        let state = open(&dir);
        assert!(
            matches!(state.backing(), LibraryBacking::Persistent(p) if *p == dir.join("library.db"))
        );
        assert_eq!(state.path(), dir.join("library.db"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opening against a path where the database file cannot be created
    /// (because a directory already exists at that name) produces an
    /// in-memory fallback with the intended path recorded.
    #[test]
    fn open_with_blocked_db_path_falls_back_to_in_memory() {
        let dir = temp_dir("fallback");
        // Create a directory where the DB file should go, so SQLite cannot
        // open it as a file.
        let db_path = dir.join("library.db");
        std::fs::create_dir(&db_path).expect("create blocking dir");

        let state = open(&dir);
        match state.backing() {
            LibraryBacking::InMemoryFallback {
                intended_path,
                reason,
            } => {
                assert_eq!(*intended_path, db_path);
                assert!(!reason.is_empty());
            }
            LibraryBacking::Persistent(_) => {
                panic!("expected in-memory fallback when DB path is a directory");
            }
        }
        assert_eq!(state.path(), db_path);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// path() returns the intended disk path regardless of backing.
    #[test]
    fn path_always_returns_disk_path() {
        let dir = temp_dir("path");
        let state = open(&dir);
        assert_eq!(state.path(), dir.join("library.db"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
