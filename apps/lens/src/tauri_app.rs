//! Tauri product shell.
//!
//! The hover engine runs on a worker thread; this module owns the Tauri
//! application on the main thread, including the settings webview (#164, #165).

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::Manager;

use crate::detail_view::{ExplainFlag, SharedDetail, SharedSavePath};
use crate::library_service::SharedLibrary;
use crate::settings_service::{SettingsView, SharedSettings};

/// A snapshot of the session's diagnostics, shared with the webview.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DiagnosticsSnapshot {
    pub build: String,
    pub configuration: Vec<(String, String)>,
    pub cards: u32,
    pub settled_hovers: u32,
    pub refusals: Vec<(String, u32)>,
    pub ocr_mode: String,
}

/// Shared diagnostics state, updated by the polling loop and read by Tauri commands.
pub type SharedDiagnostics = Arc<Mutex<DiagnosticsSnapshot>>;

/// A model entry for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct ModelInfoView {
    pub name: String,
    pub size: u64,
}

/// The result of a test translation for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct TestResultView {
    pub translated: String,
    pub latency_ms: u64,
    pub model: String,
}

/// A library item for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct LibraryItemView {
    pub id: i64,
    pub original: String,
    pub answer: String,
    pub caveat: Option<String>,
    pub app: Option<String>,
    pub created_at: i64,
    pub kind: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub model: Option<String>,
    pub note: Option<String>,
    pub review_state: Option<String>,
    pub next_review_at: Option<i64>,
    pub review_count: Option<i64>,
}

/// A page of library items for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct LibraryPageView {
    pub items: Vec<LibraryItemView>,
    pub has_more: bool,
    pub total: i64,
}

impl From<&lens_storage::Item> for LibraryItemView {
    fn from(item: &lens_storage::Item) -> Self {
        Self {
            id: item.id,
            original: item.original.clone(),
            answer: item.answer.clone(),
            caveat: item.caveat.clone(),
            app: item.app.clone(),
            created_at: item.created_at,
            kind: item.kind.clone(),
            source_language: item.source_language.clone(),
            target_language: item.target_language.clone(),
            model: item.model.clone(),
            note: item.note.clone(),
            review_state: item.review_state.clone(),
            next_review_at: item.next_review_at,
            review_count: item.review_count,
        }
    }
}

/// Handle to the running Tauri application.
///
/// Cloned into the polling worker so it can show the window or exit the
/// application when the tray asks.
#[derive(Clone)]
pub struct AppHandle(tauri::AppHandle);

impl AppHandle {
    /// Show the main settings/diagnostics window.
    pub fn show_window(&self) {
        if let Some(window) = self.0.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    /// Show the pinned detail window.
    pub fn show_detail_window(&self) {
        if let Some(window) = self.0.get_webview_window("detail") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    /// Exit the application.
    pub fn exit(&self) {
        self.0.exit(0);
    }
}

/// Tauri command: return the current diagnostics snapshot.
#[tauri::command]
fn get_diagnostics(state: tauri::State<'_, SharedDiagnostics>) -> DiagnosticsSnapshot {
    state.lock().unwrap().clone()
}

/// Tauri command: return the current settings view.
#[tauri::command]
fn get_settings(state: tauri::State<'_, SharedSettings>) -> SettingsView {
    state.lock().unwrap().view()
}

/// Tauri command: save settings from the webview.
#[tauri::command]
fn save_settings(
    state: tauri::State<'_, SharedSettings>,
    settings: SettingsView,
) -> Result<(), String> {
    state.lock().unwrap().apply(settings)
}

/// Tauri command: list models on an Ollama endpoint.
///
/// Runs the blocking HTTP call on a background thread so the Tauri
/// runtime is not stalled for the duration.
#[tauri::command]
async fn list_models(endpoint: String) -> Result<Vec<ModelInfoView>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = lens_translate::list_models(&endpoint);
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| "thread error".to_string())?
        .map(|models| {
            models
                .into_iter()
                .map(|m| ModelInfoView {
                    name: m.name,
                    size: m.size,
                })
                .collect()
        })
        .map_err(|e| format!("{e:?}"))
}

/// Tauri command: check whether an Ollama endpoint is reachable.
#[tauri::command]
async fn check_ollama(endpoint: String) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(lens_translate::check_reachable(&endpoint));
    });
    rx.recv().unwrap_or(false)
}

/// Tauri command: translate a fixed sentence to test a model.
#[tauri::command]
async fn test_model(endpoint: String, model: String) -> Result<TestResultView, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use lens_core::translate::{QuickRequest, Translator};
        let translator = lens_translate::OllamaTranslator::new(&endpoint, &model);
        let request = QuickRequest::new("The output files came back empty.", "ko");
        let result = translator.quick(&request);
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| "thread error".to_string())?
        .map(|t| TestResultView {
            translated: t.text,
            latency_ms: t.latency_ms as u64,
            model: t.model,
        })
        .map_err(|e| format!("{e:?}"))
}

/// Tauri command: list library items, newest first.
#[tauri::command]
fn library_list(
    state: tauri::State<'_, SharedLibrary>,
    offset: i64,
) -> Result<LibraryPageView, String> {
    let lib = state.lock().unwrap();
    let page = lib.library().list(offset).map_err(|e| format!("{e}"))?;
    let total = lib.library().count().unwrap_or(0);
    Ok(LibraryPageView {
        items: page.items.iter().map(LibraryItemView::from).collect(),
        has_more: page.has_more,
        total,
    })
}

/// Tauri command: list library items with filters.
#[tauri::command]
fn library_filter(
    state: tauri::State<'_, SharedLibrary>,
    kind: Option<String>,
    app: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
    offset: i64,
) -> Result<LibraryPageView, String> {
    use lens_storage::Filter;
    let lib = state.lock().unwrap();
    let filter = Filter {
        kind,
        app,
        source_language,
        target_language,
    };
    let page = lib
        .library()
        .list_filtered(&filter, offset)
        .map_err(|e| format!("{e}"))?;
    let total = lib.library().count().unwrap_or(0);
    Ok(LibraryPageView {
        items: page.items.iter().map(LibraryItemView::from).collect(),
        has_more: page.has_more,
        total,
    })
}

/// Tauri command: search the library.
#[tauri::command]
fn library_search(
    state: tauri::State<'_, SharedLibrary>,
    query: String,
    offset: i64,
) -> Result<LibraryPageView, String> {
    let lib = state.lock().unwrap();
    let page = lib
        .library()
        .search(&query, offset)
        .map_err(|e| format!("{e}"))?;
    let total = lib.library().count().unwrap_or(0);
    Ok(LibraryPageView {
        items: page.items.iter().map(LibraryItemView::from).collect(),
        has_more: page.has_more,
        total,
    })
}

/// Tauri command: get a single library item.
#[tauri::command]
fn library_get(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
) -> Result<Option<LibraryItemView>, String> {
    let lib = state.lock().unwrap();
    let item = lib.library().get(id).map_err(|e| format!("{e}"))?;
    Ok(item.as_ref().map(LibraryItemView::from))
}

/// Tauri command: delete a library item.
#[tauri::command]
fn library_delete(state: tauri::State<'_, SharedLibrary>, id: i64) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library().delete(id).map_err(|e| format!("{e}"))
}

/// Tauri command: update the note on a library item.
#[tauri::command]
fn library_update_note(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
    note: String,
) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .update_note(id, &note)
        .map_err(|e| format!("{e}"))
}

/// Tauri command: set tags for a library item.
#[tauri::command]
fn library_set_tags(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
    tags: Vec<String>,
) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .set_tags(id, &tags)
        .map_err(|e| format!("{e}"))
}

/// Tauri command: get tags for a library item.
#[tauri::command]
fn library_get_tags(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library().get_tags(id).map_err(|e| format!("{e}"))
}

/// Tauri command: get distinct app names from the library.
#[tauri::command]
fn library_distinct_apps(state: tauri::State<'_, SharedLibrary>) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library().distinct_apps().map_err(|e| format!("{e}"))
}

/// Tauri command: get distinct source languages from the library.
#[tauri::command]
fn library_distinct_source_languages(
    state: tauri::State<'_, SharedLibrary>,
) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .distinct_source_languages()
        .map_err(|e| format!("{e}"))
}

/// Tauri command: get distinct target languages from the library.
#[tauri::command]
fn library_distinct_target_languages(
    state: tauri::State<'_, SharedLibrary>,
) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .distinct_target_languages()
        .map_err(|e| format!("{e}"))
}

/// Tauri command: get the library count.
#[tauri::command]
fn library_count(state: tauri::State<'_, SharedLibrary>) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library().count().map_err(|e| format!("{e}"))
}

/// Tauri command: export the library as Markdown.
#[tauri::command]
fn library_export(state: tauri::State<'_, SharedLibrary>) -> Result<String, String> {
    let lib = state.lock().unwrap();
    lib.library().export_markdown().map_err(|e| format!("{e}"))
}

/// Tauri command: export the library as JSON (lossless format).
#[tauri::command]
fn library_export_json(state: tauri::State<'_, SharedLibrary>) -> Result<String, String> {
    let lib = state.lock().unwrap();
    lib.library().export_json().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_export_jsonl(state: tauri::State<'_, SharedLibrary>) -> Result<String, String> {
    let lib = state.lock().unwrap();
    lib.library().export_jsonl().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_import_jsonl(
    state: tauri::State<'_, SharedLibrary>,
    jsonl: String,
) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library().import_jsonl(&jsonl).map_err(|e| format!("{e}"))
}

/// Tauri command: securely wipe all data from the library database.
///
/// Checkpoints the WAL, enables `secure_delete`, deletes all rows, and
/// runs `VACUUM` to rebuild the file. See
/// [ADR-0013](../../../docs/decisions/ADR-0013-the-local-library.md) for
/// what this reaches and what it does not.
#[tauri::command]
fn library_secure_wipe(state: tauri::State<'_, SharedLibrary>) -> Result<(), String> {
    let lib = state.lock().unwrap();
    lib.library().secure_wipe().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_review_list(
    state: tauri::State<'_, SharedLibrary>,
    offset: Option<i64>,
) -> Result<LibraryPageView, String> {
    let lib = state.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let page = lib
        .library()
        .list_due(now, offset.unwrap_or(0))
        .map_err(|e| format!("{e}"))?;
    let total = lib.library().count_due(now).unwrap_or(0);
    Ok(LibraryPageView {
        items: page.items.iter().map(LibraryItemView::from).collect(),
        has_more: page.has_more,
        total,
    })
}

#[tauri::command]
fn library_review_count(state: tauri::State<'_, SharedLibrary>) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    lib.library().count_due(now).map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_record_review(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
    success: bool,
) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    lib.library()
        .record_review(id, success, now)
        .map_err(|e| format!("{e}"))
}

/// Build the Tauri application and return its handle, shared diagnostics, and the `App` itself.
///
/// The `App` must be kept alive and `.run()` must be called on it. Dropping
/// it without running destroys the event loop; keeping only the `AppHandle`
/// without the `App` leaves no event loop running.
pub fn build(
    shared_settings: SharedSettings,
    shared_library: SharedLibrary,
    shared_detail: SharedDetail,
    explain_flag: ExplainFlag,
    save_path: SharedSavePath,
) -> Result<(AppHandle, SharedDiagnostics, tauri::App), tauri::Error> {
    let diagnostics: SharedDiagnostics = Arc::new(Mutex::new(DiagnosticsSnapshot::default()));

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_diagnostics,
            get_settings,
            save_settings,
            list_models,
            check_ollama,
            test_model,
            library_list,
            library_filter,
            library_search,
            library_get,
            library_delete,
            library_update_note,
            library_set_tags,
            library_get_tags,
            library_distinct_apps,
            library_distinct_source_languages,
            library_distinct_target_languages,
            library_count,
            library_export,
            library_export_json,
            library_export_jsonl,
            library_import_jsonl,
            library_secure_wipe,
            library_review_list,
            library_review_count,
            library_record_review,
            crate::detail_view::detail_read,
            crate::detail_view::detail_copy,
            crate::detail_view::detail_save,
            crate::detail_view::detail_save_note,
            crate::detail_view::detail_request_explain,
            crate::detail_view::detail_open_in_library,
        ])
        .manage(diagnostics.clone())
        .manage(shared_settings)
        .manage(shared_library)
        .manage(shared_detail)
        .manage(explain_flag)
        .manage(save_path)
        .setup(|app| {
            // Hide the window at startup. The reader opens it from the tray;
            // a product that opens a settings window on every launch is not
            // a tray application.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            Ok(())
        })
        .build(tauri::generate_context!())?;

    let handle = AppHandle(app.handle().clone());
    Ok((handle, diagnostics, app))
}

/// Run the Tauri event loop. Consumes the `App`.
///
/// This is the main thread's final act. It returns when the event loop
/// exits, which happens when `AppHandle::exit` is called.
pub fn run(app: tauri::App) {
    app.run(|_, _| {});
}
