//! Single-instance guard for macOS.
//!
//! Prevents two Lens processes from running simultaneously. Uses a PID file
//! with `flock()` for the lock. If another instance holds the lock, this
//! returns `None`.
//!
//! The lock file lives at `~/Library/Application Support/Mori Lens/.lock`.

use std::fs;
use std::os::raw::c_int;
use std::path::PathBuf;

extern "C" {
    fn open(path: *const std::os::raw::c_char, flags: c_int, mode: u32) -> c_int;
    fn flock(fd: c_int, operation: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn getpid() -> i32;
}

const O_RDWR: c_int = 2;
const O_CREAT: c_int = 0x0200;
const LOCK_EX: c_int = 2;
const LOCK_NB: c_int = 4;

/// Guard that holds the single-instance lock. When dropped, the lock is
/// released and the file descriptor is closed.
pub struct SingleInstance {
    fd: c_int,
    path: PathBuf,
}

impl SingleInstance {
    /// Try to claim the single-instance lock.
    ///
    /// Returns `Some(guard)` if this is the only instance, `None` if another
    /// Lens process is already running.
    pub fn claim() -> Option<Self> {
        let path = lock_file_path();

        // Ensure the parent directory exists.
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let path_cstr = match std::ffi::CString::new(path.to_str().unwrap_or("")) {
            Ok(c) => c,
            Err(_) => return None,
        };

        // SAFETY: open() with O_CREAT creates the file if it doesn't exist.
        // The path is a valid C string.
        let fd = unsafe { open(path_cstr.as_ptr(), O_RDWR | O_CREAT, 0o600) };
        if fd < 0 {
            return None;
        }

        // Try to acquire an exclusive non-blocking lock.
        // SAFETY: fd is a valid file descriptor from open().
        let result = unsafe { flock(fd, LOCK_EX | LOCK_NB) };
        if result != 0 {
            // Another instance holds the lock.
            // SAFETY: fd is valid.
            unsafe { close(fd) };
            return None;
        }

        // Write our PID to the lock file for diagnostics.
        let pid = unsafe { getpid() };
        let _ = fs::write(&path, format!("{pid}\n"));

        Some(Self { fd, path })
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: fd is valid (was returned by open and not yet closed).
        unsafe {
            flock(self.fd, 8); // LOCK_UN
            close(self.fd);
        }
        // Remove the lock file.
        let _ = fs::remove_file(&self.path);
    }
}

impl std::fmt::Debug for SingleInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleInstance")
            .field("path", &self.path)
            .finish()
    }
}

fn lock_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Mori Lens")
        .join(".lock")
}
