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
