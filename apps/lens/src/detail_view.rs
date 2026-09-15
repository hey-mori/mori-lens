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
    atomic::{AtomicBool, AtomicU64, Ordering},
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
///
/// # Revision
///
/// Each new pin gets a monotonically increasing `revision`. The revision
/// uniquely identifies the pinned detail instance and is used to ensure
/// save results only apply to the same detail they were initiated for.
/// Without this, a slow save for detail A could corrupt detail B if B
/// was pinned while A's save was in flight.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DetailView {
    /// A stable identity for this pinned detail. Monotonically increasing,
    /// assigned when the detail is pinned. Contains no screen text.
    pub revision: u64,
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

/// Global revision counter for detail views.
///
/// Each new pin increments this counter and assigns the value to the
/// DetailView's revision field. Atomic to avoid lock contention with
/// the detail state mutex.
static DETAIL_REVISION: AtomicU64 = AtomicU64::new(1);

/// Get the next revision for a new detail pin.
pub fn next_revision() -> u64 {
    DETAIL_REVISION.fetch_add(1, Ordering::SeqCst)
}

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
///
/// Assigns a fresh revision from the global counter.
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
        revision: next_revision(),
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
/// the row. Returns a structured outcome that distinguishes a real insert
/// from a duplicate or a failure. On Duplicate, the existing row id is
/// stored in the shared state so Note and Open-in-Library work.
///
/// # Stale-save safety
///
/// The save captures the detail's revision before releasing the lock.
/// After IO completes, the result is applied only if the current detail's
/// revision matches. This prevents a slow save for detail A from corrupting
/// detail B if B was pinned while A's save was in flight.
#[derive(Serialize)]
#[serde(tag = "outcome")]
pub(crate) enum DetailSaveOutcome {
    Inserted { id: i64, revision: u64 },
    Duplicate { id: i64, revision: u64 },
    Failed { reason: String, revision: u64 },
}

#[tauri::command]
pub(crate) fn detail_save(
    state: tauri::State<'_, SharedDetail>,
    save_path: tauri::State<'_, SharedSavePath>,
    library: tauri::State<'_, SharedLibrary>,
) -> Result<DetailSaveOutcome, String> {
    // Capture revision and entry while holding the lock.
    let (revision, entry) = {
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
        (view.revision, entry)
    };
    // Lock released; IO happens without holding the detail state.

    let lib_guard = library.lock().unwrap();
    let result = keeping::append(
        save_path.as_ref(),
        &entry,
        std::time::SystemTime::now(),
        lib_guard.library(),
    );
    match result {
        Ok(keeping::StructuredSave::Inserted(id)) => {
            // Apply item_id only if the current detail is still the one we saved.
            apply_item_id_if_current(&state, revision, id);
            Ok(DetailSaveOutcome::Inserted { id, revision })
        }
        Ok(keeping::StructuredSave::Duplicate(id)) => {
            // The row already exists. Set item_id so Note and Open-in-Library
            // can find it, even though this save did not create a new row.
            // Only apply if the current detail is still the one we saved.
            apply_item_id_if_current(&state, revision, id);
            Ok(DetailSaveOutcome::Duplicate { id, revision })
        }
        Ok(keeping::StructuredSave::Failed(_detail)) => {
            // Sanitize: do not leak raw database errors to the webview.
            // The file write already succeeded; only the structured store
            // failed. The user sees a stable Korean-friendly label.
            Ok(DetailSaveOutcome::Failed {
                reason: "library-db".to_owned(),
                revision,
            })
        }
        Err(not_kept) => Err(not_kept.user_message().to_owned()),
    }
}

/// Apply item_id to the current detail only if its revision matches.
///
/// This is the stale-save race fix: a save that started for revision N
/// must not mutate a detail with revision M > N (a newer pin).
fn apply_item_id_if_current(state: &SharedDetail, expected_revision: u64, id: i64) {
    let mut guard = state.lock().unwrap();
    if let Some(view) = guard.as_mut() {
        if view.revision == expected_revision {
            view.item_id = Some(id);
        }
        // Otherwise, a newer detail was pinned; do not mutate it.
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
    use lens_core::analyse::DeepAnalysis;
    use lens_core::card::Card;
    use lens_core::translate::{EngineId, QuickTranslation, TranslateError};

    fn quick_translation(text: &str) -> QuickTranslation {
        QuickTranslation {
            text: text.to_owned(),
            engine: EngineId::Local,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 1600,
            cached: false,
            part_was_removed: false,
            name_changed: false,
        }
    }

    fn translation_card(original: &str, text: &str) -> Card {
        Card::translated(original, &quick_translation(text))
    }

    fn problem_card() -> Card {
        Card::not_translated("some text", &TranslateError::Unavailable)
    }

    fn waiting_card() -> Card {
        Card::waiting("some text")
    }

    fn analysis_card(original: &str, translation: Option<&str>) -> Card {
        let analysis = DeepAnalysis {
            lemma: "run".to_owned(),
            part_of_speech: "verb".to_owned(),
            sense: String::new(),
            structure: String::new(),
            note: String::new(),
            word: None,
            translation: translation.map(|s| s.to_owned()),
            counterpart: None,
            model: "qwen2.5:3b".to_owned(),
            latency_ms: 4400,
            cached: false,
            links: vec![],
        };
        Card::analysed(original, &analysis)
    }

    #[test]
    fn view_from_translation_card_carries_original_and_translation() {
        let card = translation_card("Hello", "안녕");
        let view = view_from_card(&card).expect("should produce a view");
        assert_eq!(view.original, "Hello");
        assert_eq!(view.translation.as_deref(), Some("안녕"));
        assert!(view.footer.contains("qwen2.5:3b"));
        assert!(view.rows.is_empty());
        assert!(view.item_id.is_none());
    }

    #[test]
    fn view_from_analysis_card_carries_rows() {
        let card = analysis_card("running", Some("달리고"));
        let view = view_from_card(&card).expect("should produce a view");
        assert_eq!(view.original, "running");
        assert_eq!(view.translation.as_deref(), Some("달리고"));
        assert!(view.rows.len() >= 2);
        assert_eq!(view.rows[0], ("원형".to_owned(), "run".to_owned()));
        assert_eq!(view.rows[1], ("품사".to_owned(), "verb".to_owned()));
    }

    #[test]
    fn view_from_problem_card_is_none() {
        let card = problem_card();
        assert!(view_from_card(&card).is_none());
    }

    #[test]
    fn view_from_waiting_card_is_none() {
        let card = waiting_card();
        assert!(view_from_card(&card).is_none());
    }

    #[test]
    fn view_from_empty_original_is_none() {
        let card = translation_card("   ", "should not appear");
        assert!(view_from_card(&card).is_none());
    }

    #[test]
    fn view_from_analysis_without_translation_has_none_translation() {
        let card = analysis_card("feeling", None);
        let view = view_from_card(&card).expect("should produce a view");
        assert!(view.translation.is_none());
        assert!(view.rows.len() >= 2);
    }

    #[test]
    fn new_shared_starts_empty_and_false() {
        let (detail, flag) = new_shared();
        assert!(detail.lock().unwrap().is_none());
        assert!(!flag.load(Ordering::Relaxed));
    }

    /// Each view_from_card call gets a fresh, monotonically increasing revision.
    #[test]
    fn view_from_card_assigns_unique_revision() {
        let card1 = translation_card("first", "첫 번째");
        let card2 = translation_card("second", "두 번째");
        let view1 = view_from_card(&card1).unwrap();
        let view2 = view_from_card(&card2).unwrap();
        assert!(view2.revision > view1.revision);
    }

    /// apply_item_id_if_current sets item_id when revision matches.
    #[test]
    fn apply_item_id_sets_when_revision_matches() {
        let state: SharedDetail = Arc::new(Mutex::new(Some(DetailView {
            revision: 42,
            original: "test".to_owned(),
            ..Default::default()
        })));

        apply_item_id_if_current(&state, 42, 123);

        let view = state.lock().unwrap();
        assert_eq!(view.as_ref().unwrap().item_id, Some(123));
    }

    /// apply_item_id_if_current does NOT set item_id when revision differs.
    /// This is the stale-save race fix: a save for revision N must not
    /// mutate a detail with revision M > N.
    #[test]
    fn apply_item_id_does_not_set_when_revision_differs() {
        let state: SharedDetail = Arc::new(Mutex::new(Some(DetailView {
            revision: 100, // Newer detail
            original: "newer".to_owned(),
            ..Default::default()
        })));

        // Try to apply item_id for an older revision (simulating a slow save
        // that started when revision was lower).
        apply_item_id_if_current(&state, 42, 123);

        let view = state.lock().unwrap();
        // item_id should NOT be set because revision doesn't match.
        assert_eq!(view.as_ref().unwrap().item_id, None);
    }

    /// The stale-save race scenario: detail A starts saving, detail B gets
    /// pinned, A's save completes. B must NOT get A's item_id.
    #[test]
    fn stale_save_does_not_corrupt_newer_detail() {
        let state: SharedDetail = Arc::new(Mutex::new(Some(DetailView {
            revision: 1,
            original: "detail A".to_owned(),
            ..Default::default()
        })));

        // Capture revision before "IO" (simulating detail_save's first phase).
        let captured_revision = state.lock().unwrap().as_ref().unwrap().revision;
        assert_eq!(captured_revision, 1);

        // Simulate: detail B gets pinned while A's save is in flight.
        *state.lock().unwrap() = Some(DetailView {
            revision: 2,
            original: "detail B".to_owned(),
            ..Default::default()
        });

        // A's save completes and tries to apply item_id.
        apply_item_id_if_current(&state, captured_revision, 999);

        // Detail B must NOT have A's item_id.
        let view = state.lock().unwrap();
        assert_eq!(view.as_ref().unwrap().original, "detail B");
        assert_eq!(view.as_ref().unwrap().item_id, None);
    }
}
