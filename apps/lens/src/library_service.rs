//! Library service: the shared handle to the SQLite library database.
//!
//! The database lives beside `saved.md` in the data directory. The service
//! wraps it in a `Mutex` so Tauri commands can read and write safely.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lens_storage::Library;

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
pub fn open(data_dir: &Path) -> LibraryState {
    let db_path = data_dir.join("library.db");
    let library = Library::open(&db_path).unwrap_or_else(|e| {
        eprintln!("library   cannot open {db_path:?}: {e}");
        // Fall back to a temporary database so the UI still works.
        Library::open(Path::new(":memory:")).expect("in-memory library")
    });
    LibraryState {
        library,
        path: db_path,
    }
}
