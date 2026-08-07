//! Appending what a reader kept to a file they own.
//!
//! The format is [`lens_core::keep`]; this is the part that touches a disk,
//! which is why it is here and not there.
//!
//! # The only file Lens writes that came off a screen
//!
//! `settings.conf` is what somebody typed. The translation cache is memory and
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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use lens_core::keep::{block, Entry, PREAMBLE};

/// Where saved items go, beside the settings file.
///
/// Beside it because a reader who has found one has found the other, and the
/// settings file's path is printed at startup and reachable from the tray.
pub fn path_beside(settings: &Path) -> PathBuf {
    settings
        .parent()
        .map(|folder| folder.join("saved.md"))
        .unwrap_or_else(|| PathBuf::from("saved.md"))
}

/// Why an entry was not kept, in words a card can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotKept {
    /// The card has no answer on it, so there is nothing to keep.
    NothingToKeep,
    /// The file could not be opened or written.
    NotWritten,
}

impl NotKept {
    pub fn user_message(self) -> &'static str {
        match self {
            Self::NothingToKeep => "저장할 내용이 아직 없습니다",
            Self::NotWritten => "저장 파일에 쓰지 못했습니다",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            Self::NothingToKeep => "nothing-to-keep",
            Self::NotWritten => "not-written",
        }
    }
}

/// Append one entry, creating the file and its heading if it is not there.
///
/// Returns the path, so the caller can say where it went. A reader who presses
/// a key and is told only "saved" has been told the least useful half.
pub fn append(file: &Path, entry: &Entry, at: SystemTime) -> Result<(), NotKept> {
    if let Some(folder) = file.parent() {
        // The settings folder already exists by the time anything is saved,
        // but a reader who pointed `--settings` somewhere new has not created
        // one yet.
        let _ = std::fs::create_dir_all(folder);
    }
    let fresh = !file.exists();
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .map_err(|_| NotKept::NotWritten)?;

    // The heading is written once, with the first entry rather than at
    // startup: a reader who never saves anything should not find a file
    // explaining a feature they did not use.
    if fresh {
        handle
            .write_all(PREAMBLE.as_bytes())
            .map_err(|_| NotKept::NotWritten)?;
    }
    handle
        .write_all(block(entry, at).as_bytes())
        .map_err(|_| NotKept::NotWritten)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("mori-lens-keep-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("saved.md")
    }

    fn an_entry(original: &str) -> Entry {
        Entry {
            original: original.to_owned(),
            answer: "포인터가 멈춘 뒤에야 읽습니다.".to_owned(),
            caveat: None,
            app: Some("Code".to_owned()),
        }
    }

    #[test]
    fn the_heading_is_written_once_and_entries_accumulate() {
        let file = scratch("accumulate");
        let at = UNIX_EPOCH + Duration::from_secs(1_786_075_140);

        append(&file, &an_entry("first"), at).expect("kept");
        append(&file, &an_entry("second"), at).expect("kept");
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
        append(&file, &an_entry("first"), at).expect("kept");

        let mine = "\n<!-- my own note, do not eat -->\n";
        let mut handle = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .expect("open");
        handle.write_all(mine.as_bytes()).expect("write");
        drop(handle);

        append(&file, &an_entry("second"), at).expect("kept");
        let written = std::fs::read_to_string(&file).expect("a file");
        assert!(written.contains("my own note, do not eat"), "{written}");
        let _ = std::fs::remove_dir_all(file.parent().expect("a folder"));
    }

    #[test]
    fn the_file_lands_beside_the_settings() {
        let settings =
            PathBuf::from("C:\\Users\\someone\\AppData\\Roaming\\Mori Lens\\settings.conf");
        assert_eq!(
            path_beside(&settings),
            PathBuf::from("C:\\Users\\someone\\AppData\\Roaming\\Mori Lens\\saved.md"),
        );
    }

    #[test]
    fn every_refusal_has_a_sentence_and_a_reason() {
        for refusal in [NotKept::NothingToKeep, NotKept::NotWritten] {
            assert!(!refusal.user_message().is_empty());
            assert_ne!(refusal.user_message(), refusal.reason());
        }
    }
}
