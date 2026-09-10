//! Pinned interactive detail: a Tauri webview that stays until the reader
//! closes it, with copy, save, note, and open-in-library buttons.
//!
//! The native [`lens_platform_windows::PanelWindow`] shows a read-only panel.
//! Adding buttons and a textarea to a GDI window would be several hundred
//! lines of Win32, and the product already has a Tauri webview for the
//! settings and library. A second webview window is the pragmatic choice,
//! amending [ADR-0012](../../../docs/decisions/ADR-0012-three-product-surfaces.md)'s
//! implementation detail (the decision rule, interactive surface separate
//! from the passive card, is unchanged).
//!
//! # Data flow
//!
//! The polling loop in `main.rs` populates [`SharedDetail`] from the current
//! card when the reader presses Ctrl+Alt+P, then shows the Tauri window.
//! The webview reads the snapshot on load via `detail_read` and calls back
//! into Tauri commands for each button.
//!
//! The card and the detail window are independent: the card goes when the
//! pointer leaves, but the detail stays because the reader pinned it.

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use serde::Serialize;
use tauri::Emitter;

use crate::keeping;
use crate::library_service::SharedLibrary;

/// A snapshot of what the detail window shows.
///
/// Populated by the polling loop from a `Card` and read by the webview
/// through `detail_read`. Carries enough to save to the library without
/// reaching back to the card.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DetailView {
    /// The sentence the reader pointed at.
    pub original: String,
    /// The translation, when there is one.
    pub translation: Option<String>,
    /// The explanation rows (label, value).
    pub rows: Vec<(String, String)>,
    /// Engine, model and latency.
    pub footer: String,
    /// A caveat, carried from an analysis card.
    pub caveat: Option<String>,
    /// The application the text came from.
    pub app: Option<String>,
    /// The library item id, set after a successful save.
    pub item_id: Option<i64>,
    /// The reader's note, read back from the library after save.
    pub note: Option<String>,
    /// The source language of the original text.
    pub source_language: Option<String>,
    /// The target language the text was translated into.
    pub target_language: Option<String>,
    /// The model that produced the answer.
    pub model: Option<String>,
}

/// Shared detail state, written by the polling loop and read by Tauri commands.
pub type SharedDetail = Arc<Mutex<Option<DetailView>>>;

/// A flag the detail window sets to ask the polling loop for an explanation.
///
/// Atomic rather than behind the Mutex, because the polling loop checks it
/// on every tick and a lock contention on a keypress would be the wrong
/// failure mode.
pub type ExplainFlag = Arc<AtomicBool>;

/// The saved.md path, shared with Tauri commands that need to write to it.
///
/// Immutable after startup, so an `Arc<PathBuf>` rather than a `Mutex`.
pub type SharedSavePath = Arc<PathBuf>;

/// Build a [`DetailView`] from a [`lens_core::card::Card`].
///
/// Returns `None` when the card has nothing worth pinning (a problem or a
/// half-arrived answer), matching [`lens_core::detail::Detail::from_card`].
#[allow(dead_code)]
pub fn view_from_card(card: &lens_core::card::Card) -> Option<DetailView> {
    use lens_core::card::CardBody;

    if card.original.trim().is_empty() {
        return None;
    }
    let (translation, rows, caveat) = match &card.body {
        CardBody::Translation(text) => (Some(text.clone()), Vec::new(), None),
        CardBody::Analysis {
            translation, rows, ..
        } => (translation.clone(), rows.clone(), None),
        CardBody::Problem(_) | CardBody::Waiting(_) => return None,
    };
    Some(DetailView {
        original: card.original.clone(),
        translation,
        rows,
        footer: card.footer.clone(),
        caveat,
        app: None,
        item_id: None,
        note: None,
        source_language: None,
        target_language: None,
        model: None,
    })
}

/// Tauri command: read the current detail snapshot.
#[tauri::command]
pub(crate) fn detail_read(state: tauri::State<'_, SharedDetail>) -> Option<DetailView> {
    state.lock().unwrap().clone()
}

/// Tauri command: copy the answer to the clipboard.
///
/// The answer is the translation when there is one, otherwise the first
/// explanation row's value. Returns whether the copy succeeded.
#[tauri::command]
pub(crate) fn detail_copy(state: tauri::State<'_, SharedDetail>) -> bool {
    let guard = state.lock().unwrap();
    let Some(view) = guard.as_ref() else {
        return false;
    };
    let text = view
        .translation
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            view.rows
                .first()
                .map(|(_, value)| value.as_str())
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or(&view.original);
    #[cfg(windows)]
    {
        lens_platform_windows::clipboard::put_text(text)
    }
    #[cfg(target_os = "macos")]
    {
        lens_platform_macos::clipboard::put_text(text)
    }
}

/// Tauri command: save the detail to the library and saved.md.
///
/// Calls [`keeping::append`] and stores the returned item id in the shared
/// state so subsequent note saves and the open-in-library button can find
/// the row. Returns the item id, or `None` when the database skipped it
/// (duplicate or error).
#[tauri::command]
pub(crate) fn detail_save(
    state: tauri::State<'_, SharedDetail>,
    save_path: tauri::State<'_, SharedSavePath>,
) -> Result<Option<i64>, String> {
    let guard = state.lock().unwrap();
    let Some(view) = guard.as_ref() else {
        return Err("nothing to save".to_owned());
    };
    let entry = lens_core::keep::Entry {
        original: view.original.clone(),
        answer: view.translation.clone().unwrap_or_default(),
        caveat: view.caveat.clone(),
        app: view.app.clone(),
        source_language: view.source_language.clone(),
        target_language: view.target_language.clone(),
        model: view.model.clone(),
    };
    drop(guard);

    let result = keeping::append(save_path.as_ref(), &entry, std::time::SystemTime::now());
    match result {
        Ok(item_id) => {
            // Store the item id so the note and open-in-library can find it.
            let mut guard = state.lock().unwrap();
            if let Some(view) = guard.as_mut() {
                view.item_id = item_id;
            }
            Ok(item_id)
        }
        Err(not_kept) => Err(not_kept.user_message().to_owned()),
    }
}

/// Tauri command: save a note on a library item.
///
/// The item must already be saved (the save button runs first and returns
/// an id). Returns whether the update landed.
#[tauri::command]
pub(crate) fn detail_save_note(
    library: tauri::State<'_, SharedLibrary>,
    id: i64,
    note: String,
) -> Result<bool, String> {
    let lib = library.lock().unwrap();
    lib.library()
        .update_note(id, &note)
        .map_err(|e| format!("{e}"))
}

/// Tauri command: ask the polling loop to explain the current sentence.
///
/// Sets the [`ExplainFlag`]; the polling loop picks it up on the next tick
/// and dispatches a `Job::Explain`. The detail window does not auto-update:
/// the reader pins again to see the result, which is the same lifecycle as
/// the native panel.
#[tauri::command]
pub(crate) fn detail_request_explain(flag: tauri::State<'_, ExplainFlag>) -> bool {
    flag.store(true, Ordering::Relaxed);
    true
}

/// Tauri command: open the item in the library tab.
///
/// Emits a Tauri event that the settings/library webview listens for.
/// If the item has not been saved yet, saves it first so the library has
/// something to navigate to.
#[tauri::command]
pub(crate) fn detail_open_in_library(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedDetail>,
) -> Result<Option<i64>, String> {
    let guard = state.lock().unwrap();
    let item_id = guard.as_ref().and_then(|v| v.item_id);
    drop(guard);
    // The event payload is the item id (or null if not yet saved).
    app.emit("open-library-item", item_id)
        .map_err(|e| format!("{e}"))?;
    Ok(item_id)
}

/// Build the shared detail state and explain flag.
pub fn new_shared() -> (SharedDetail, ExplainFlag) {
    (Arc::new(Mutex::new(None)), Arc::new(AtomicBool::new(false)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lens_core::card::{Card, CardBody};

    fn translation_card(original: &str, text: &str) -> Card {
        Card {
            original: original.to_owned(),
            body: CardBody::Translation(text.to_owned()),
            footer: "Ollama · 1.6s".to_owned(),
            metadata: None,
        }
    }

    fn problem_card(msg: &str) -> Card {
        Card {
            original: "some text".to_owned(),
            body: CardBody::Problem(msg.to_owned()),
            footer: "Ollama".to_owned(),
            metadata: None,
        }
    }

    fn waiting_card(msg: &str) -> Card {
        Card {
            original: "some text".to_owned(),
            body: CardBody::Waiting(msg.to_owned()),
            footer: "".to_owned(),
            metadata: None,
        }
    }

    fn analysis_card(
        original: &str,
        translation: Option<&str>,
        rows: Vec<(String, String)>,
    ) -> Card {
        Card {
            original: original.to_owned(),
            body: CardBody::Analysis {
                translation: translation.map(|s| s.to_owned()),
                word_at: None,
                counterpart_at: None,
                links: vec![],
                rows,
            },
            footer: "Ollama · 4.4s".to_owned(),
            metadata: None,
        }
    }

    #[test]
    fn view_from_translation_card_carries_original_and_translation() {
        let card = translation_card("Hello", "안녕");
        let view = view_from_card(&card).expect("should produce a view");
        assert_eq!(view.original, "Hello");
        assert_eq!(view.translation.as_deref(), Some("안녕"));
        assert_eq!(view.footer, "Ollama · 1.6s");
        assert!(view.rows.is_empty());
        assert!(view.item_id.is_none());
    }

    #[test]
    fn view_from_analysis_card_carries_rows() {
        let rows = vec![
            ("Lemma".to_owned(), "run".to_owned()),
            ("POS".to_owned(), "verb".to_owned()),
        ];
        let card = analysis_card("running", Some("달리고"), rows.clone());
        let view = view_from_card(&card).expect("should produce a view");
        assert_eq!(view.original, "running");
        assert_eq!(view.translation.as_deref(), Some("달리고"));
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.rows[0], ("Lemma".to_owned(), "run".to_owned()));
    }

    #[test]
    fn view_from_problem_card_is_none() {
        let card = problem_card("Ollama is not running");
        assert!(view_from_card(&card).is_none());
    }

    #[test]
    fn view_from_waiting_card_is_none() {
        let card = waiting_card("asking Ollama...");
        assert!(view_from_card(&card).is_none());
    }

    #[test]
    fn view_from_empty_original_is_none() {
        let card = Card {
            original: "   ".to_owned(),
            body: CardBody::Translation("should not appear".to_owned()),
            footer: "".to_owned(),
            metadata: None,
        };
        assert!(view_from_card(&card).is_none());
    }

    #[test]
    fn view_from_analysis_without_translation_has_none_translation() {
        let rows = vec![("Sense".to_owned(), "the feeling".to_owned())];
        let card = analysis_card("feeling", None, rows);
        let view = view_from_card(&card).expect("should produce a view");
        assert!(view.translation.is_none());
        assert_eq!(view.rows.len(), 1);
    }

    #[test]
    fn new_shared_starts_empty_and_false() {
        let (detail, flag) = new_shared();
        assert!(detail.lock().unwrap().is_none());
        assert!(!flag.load(Ordering::Relaxed));
    }
}
