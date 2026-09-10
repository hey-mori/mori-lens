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

/// The service's state: the database handle and its path.
pub struct LibraryState {
    /// The database handle.
    library: Library,
    /// The database path, kept for diagnostics.
    path: PathBuf,
}

impl LibraryState {
    /// Access the underlying library.
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// The database path.
    pub fn path(&self) -> &Path {
        &self.path
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
    let library = match Library::open(&db_path) {
        Ok(lib) => lib,
        Err(lens_storage::Error::FutureSchema { have, understand }) => {
            eprintln!(
                "library   {db_path:?} was written by a newer Lens (schema {have}, this build understands {understand}); using in-memory database until you upgrade"
            );
            Library::open(Path::new(":memory:")).expect("in-memory library")
        }
        Err(e) => {
            eprintln!("library   cannot open {db_path:?}: {e}");
            // Fall back to a temporary database so the UI still works.
            Library::open(Path::new(":memory:")).expect("in-memory library")
        }
    };
    import_saved_md(&library, data_dir);
    LibraryState {
        library,
        path: db_path,
    }
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
