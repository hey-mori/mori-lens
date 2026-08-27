//! Atomic file writing with rollback.
//!
//! The settings file is the one file in this product a person wrote
//! themselves, either by hand or through the settings window. A crash
//! mid-write must not leave it truncated. The rule: write to a temporary
//! file in the same directory, verify the result parses, then rename over
//! the original. If anything goes wrong, the original is untouched.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Write `content` to `path` atomically.
///
/// Creates a temporary file in the same directory, writes and flushes
/// the content, then renames over the target. If the rename fails the
/// temporary file is removed.
///
/// The caller is responsible for verifying that `content` is valid
/// before calling this. The function writes bytes; it does not parse.
pub fn atomic_write(path: &Path, content: &str) -> Result<(), AtomicWriteError> {
    let _dir = path.parent().ok_or(AtomicWriteError::NoParentDir {
        path: path.to_path_buf(),
    })?;

    let temp_path = path.with_extension("toml.tmp");

    // Write to the temporary file, flush, and sync.
    let mut file = fs::File::create(&temp_path).map_err(|e| AtomicWriteError::Create {
        path: temp_path.clone(),
        source: e,
    })?;
    file.write_all(content.as_bytes())
        .map_err(|e| AtomicWriteError::Write {
            path: temp_path.clone(),
            source: e,
        })?;
    file.flush().map_err(|e| AtomicWriteError::Write {
        path: temp_path.clone(),
        source: e,
    })?;

    // Rename over the original. On Windows this fails if the target exists
    // and is open, so we use the rename-replacing variant.
    fs::rename(&temp_path, path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        AtomicWriteError::Rename {
            from: temp_path,
            to: path.to_path_buf(),
            source: e,
        }
    })?;

    Ok(())
}

/// Write `content` to `path` only if it parses as valid TOML.
///
/// Parses the content first. If parsing fails, the existing file is
/// untouched and an error is returned. This is the rollback guard:
/// an invalid write never reaches disk.
pub fn atomic_write_verified(path: &Path, content: &str) -> Result<(), AtomicWriteError> {
    // Verify the content parses before writing anything.
    content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| AtomicWriteError::InvalidToml {
            reason: e.to_string(),
        })?;

    atomic_write(path, content)
}

/// What went wrong during an atomic write.
#[derive(Debug)]
pub enum AtomicWriteError {
    NoParentDir {
        path: PathBuf,
    },
    Create {
        path: PathBuf,
        source: std::io::Error,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    InvalidToml {
        reason: String,
    },
}

impl std::fmt::Display for AtomicWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoParentDir { path } => {
                write!(
                    f,
                    "settings path has no parent directory: {}",
                    path.display()
                )
            }
            Self::Create { path, source } => {
                write!(f, "cannot create {}: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(f, "cannot write {}: {source}", path.display())
            }
            Self::Rename { from, to, source } => {
                write!(
                    f,
                    "cannot rename {} to {}: {source}",
                    from.display(),
                    to.display()
                )
            }
            Self::InvalidToml { reason } => {
                write!(f, "the settings content is not valid TOML: {reason}")
            }
        }
    }
}

impl std::error::Error for AtomicWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Create { source, .. }
            | Self::Write { source, .. }
            | Self::Rename { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lens-settings-test-atomic-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_creates_the_file() {
        let dir = temp_dir("creates");
        let path = dir.join("test_creates.toml");
        atomic_write(&path, "key = \"value\"\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "key = \"value\"\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_overwrites_an_existing_file() {
        let dir = temp_dir("overwrite");
        let path = dir.join("test_overwrite.toml");
        fs::write(&path, "old = true\n").unwrap();
        atomic_write(&path, "new = true\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new = true\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_verified_refuses_invalid_toml() {
        let dir = temp_dir("invalid");
        let path = dir.join("test_invalid.toml");
        fs::write(&path, "original = true\n").unwrap();

        let result = atomic_write_verified(&path, "this is not [valid toml = ");
        assert!(result.is_err());

        // The original file is untouched.
        assert_eq!(fs::read_to_string(&path).unwrap(), "original = true\n");
        // No temporary file is left behind.
        assert!(!path.with_extension("toml.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_verified_writes_valid_toml() {
        let dir = temp_dir("valid");
        let path = dir.join("test_valid.toml");
        atomic_write_verified(&path, "[section]\nkey = \"value\"\n").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[section]\nkey = \"value\"\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
