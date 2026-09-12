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

/// Shared session-recent state, written by the polling loop and read by Tauri commands.
pub type SharedRecent = Arc<Mutex<lens_core::recent::SessionRecent>>;

/// A recent card for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct RecentCardView {
    pub original: String,
    pub answer: String,
    pub shown_at: i64,
    pub app: Option<String>,
}

/// A snapshot of the session's diagnostics, shared with the webview.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DiagnosticsSnapshot {
    pub version: String,
    pub build: String,
    pub configuration: Vec<(String, String)>,
    pub cards: u32,
    pub settled_hovers: u32,
    pub refusals: Vec<(String, u32)>,
    pub ocr_mode: String,
    /// Whether reading is currently paused. Shared with the Control Center so
    /// it can show the current state and provide a pause/resume toggle.
    pub paused: bool,
}

/// Shared diagnostics state, updated by the polling loop and read by Tauri commands.
pub type SharedDiagnostics = Arc<Mutex<DiagnosticsSnapshot>>;

/// Shared pause toggle request.
///
/// The polling loop checks this on each iteration and applies any pending toggle.
/// This allows the Control Center (via Tauri commands) to request a pause/resume
/// without directly owning the pause state, which remains on the polling loop thread.
pub type SharedPauseRequest = Arc<std::sync::atomic::AtomicBool>;
pub type SharedQuitRequest = Arc<std::sync::atomic::AtomicBool>;

/// Mori availability status for the webview.
///
/// `status` is one of: `"not-installed"`, `"no-capability-marker"`,
/// `"version-mismatch"`, `"supported"`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MoriStatusView {
    pub status: String,
    pub negotiated_version: Option<u32>,
}

impl From<crate::mori_transfer::MoriStatus> for MoriStatusView {
    fn from(status: crate::mori_transfer::MoriStatus) -> Self {
        match status {
            crate::mori_transfer::MoriStatus::NotInstalled => Self {
                status: "not-installed".to_string(),
                negotiated_version: None,
            },
            crate::mori_transfer::MoriStatus::NoCapabilityMarker => Self {
                status: "no-capability-marker".to_string(),
                negotiated_version: None,
            },
            crate::mori_transfer::MoriStatus::VersionMismatch { .. } => Self {
                status: "version-mismatch".to_string(),
                negotiated_version: None,
            },
            crate::mori_transfer::MoriStatus::Supported { negotiated_version } => Self {
                status: "supported".to_string(),
                negotiated_version: Some(negotiated_version),
            },
        }
    }
}

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

    /// Show the main window and navigate to a specific section.
    ///
    /// Emits a Tauri event that the web UI listens for to switch sections.
    /// Section names: "status", "library", "history", "general", etc.
    pub fn show_window_section(&self, section: &str) {
        if let Some(window) = self.0.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.emit("navigate-section", section);
        }
    }

    /// Show the pinned detail window.
    #[allow(dead_code)]
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

/// Runtime status for the Control Center Status page.
///
/// Contains operational product truth that the Status page needs to display
/// but that is not part of the diagnostic snapshot.
#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatus {
    pub history_enabled: bool,
    pub history_days: i64,
    pub library_persistent: bool,
    pub autostart_enabled: bool,
}

/// Tauri command: return current runtime status for the Control Center.
#[tauri::command]
fn get_runtime_status(
    settings: tauri::State<'_, SharedSettings>,
    library: tauri::State<'_, SharedLibrary>,
) -> RuntimeStatus {
    let settings_view = settings.lock().unwrap().view();
    let lib_guard = library.lock().unwrap();
    RuntimeStatus {
        history_enabled: settings_view.history,
        history_days: settings_view.history_days,
        library_persistent: lib_guard.library().is_some(),
        autostart_enabled: lens_platform_windows::autostart::is_enabled(),
    }
}

/// Tauri command: request a pause/resume toggle.
///
/// Sets a flag that the polling loop checks and applies. The actual pause state
/// remains owned by the polling loop thread; this is just a request mechanism.
#[tauri::command]
fn request_pause_toggle(pause_request: tauri::State<'_, SharedPauseRequest>) {
    pause_request.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Tauri command: request the application to quit.
///
/// Sets a flag that the polling loop checks. The loop will exit cleanly,
/// which triggers the normal shutdown sequence.
#[tauri::command]
fn request_quit(quit_request: tauri::State<'_, SharedQuitRequest>) {
    quit_request.store(true, std::sync::atomic::Ordering::Relaxed);
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
        .map_err(|e| format!("{e}"))
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
        .map_err(|e| format!("{e}"))
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
    review_state: Option<String>,
    offset: i64,
) -> Result<LibraryPageView, String> {
    use lens_storage::Filter;
    let lib = state.lock().unwrap();
    let filter = Filter {
        kind,
        app,
        source_language,
        target_language,
        review_state,
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
    lib.library()
        .import_jsonl(&jsonl)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn recent_list(recent: tauri::State<'_, SharedRecent>) -> Vec<RecentCardView> {
    let mut guard = recent.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    guard
        .list(now)
        .into_iter()
        .map(|c| RecentCardView {
            original: c.original,
            answer: c.answer,
            shown_at: c.shown_at,
            app: c.app,
        })
        .collect()
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
fn library_later_list(
    state: tauri::State<'_, SharedLibrary>,
    offset: Option<i64>,
) -> Result<LibraryPageView, String> {
    let lib = state.lock().unwrap();
    let page = lib
        .library()
        .list_later(offset.unwrap_or(0))
        .map_err(|e| format!("{e}"))?;
    let total = lib.library().count_later().unwrap_or(0);
    Ok(LibraryPageView {
        items: page.items.iter().map(LibraryItemView::from).collect(),
        has_more: page.has_more,
        total,
    })
}

#[tauri::command]
fn library_later_count(state: tauri::State<'_, SharedLibrary>) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library().count_later().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_set_review_state(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
    state_: String,
) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .set_review_state(id, &state_)
        .map_err(|e| format!("{e}"))
}

// -- History commands (ADR-0014 Tier 3) -------------------------------------

/// A history entry for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct HistoryEntryView {
    pub id: i64,
    pub original: String,
    pub answer: String,
    pub kind: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub model: Option<String>,
    pub app: Option<String>,
    pub provenance: String,
    pub provider_scope: String,
    pub created_at: i64,
}

/// A page of history entries for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct HistoryPageView {
    pub entries: Vec<HistoryEntryView>,
    pub has_more: bool,
    pub total: i64,
}

impl From<&lens_storage::HistoryEntry> for HistoryEntryView {
    fn from(e: &lens_storage::HistoryEntry) -> Self {
        Self {
            id: e.id,
            original: e.original.clone(),
            answer: e.answer.clone(),
            kind: e.kind.clone(),
            source_language: e.source_language.clone(),
            target_language: e.target_language.clone(),
            model: e.model.clone(),
            app: e.app.clone(),
            provenance: e.provenance.clone(),
            provider_scope: e.provider_scope.clone(),
            created_at: e.created_at,
        }
    }
}

#[tauri::command]
fn history_list(
    state: tauri::State<'_, SharedLibrary>,
    offset: Option<i64>,
) -> Result<HistoryPageView, String> {
    let lib = state.lock().unwrap();
    let page = lib
        .library()
        .history_list(offset.unwrap_or(0))
        .map_err(|e| format!("{e}"))?;
    Ok(HistoryPageView {
        entries: page.entries.iter().map(HistoryEntryView::from).collect(),
        has_more: page.has_more,
        total: page.total,
    })
}

#[tauri::command]
fn history_count(state: tauri::State<'_, SharedLibrary>) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library().history_count().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn history_delete(state: tauri::State<'_, SharedLibrary>, id: i64) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library().history_delete(id).map_err(|e| format!("{e}"))
}

#[tauri::command]
fn history_promote(state: tauri::State<'_, SharedLibrary>, id: i64) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .history_promote(id)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn history_prune(
    state: tauri::State<'_, SharedLibrary>,
    retention_secs: i64,
) -> Result<usize, String> {
    let lib = state.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    lib.library()
        .history_prune(now, retention_secs)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn mori_status() -> MoriStatusView {
    MoriStatusView::from(crate::mori_transfer::check_mori_status())
}

#[tauri::command]
fn mori_send_item(state: tauri::State<'_, SharedLibrary>, item_id: i64) -> Result<String, String> {
    let lib = state.lock().unwrap();
    let item = lib
        .library()
        .get(item_id)
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| format!("item {item_id} not found"))?;
    crate::mori_transfer::send_to_mori(&item).map_err(|e| format!("{e}"))
}

/// Diagnostics export for bug reports.
///
/// Explicit user action only: the reader clicks "Export diagnostics" in
/// the settings window. The result is a JSON string they can paste into
/// a bug report. **No captured text is included.** Everything here is
/// metadata: versions, counts, configuration labels, and error classes.
#[tauri::command]
fn export_diagnostics(
    diagnostics: tauri::State<'_, SharedDiagnostics>,
    settings: tauri::State<'_, SharedSettings>,
    library: tauri::State<'_, SharedLibrary>,
) -> String {
    let diag = diagnostics.lock().unwrap();
    let conf = settings.lock().unwrap();
    let lib = library.lock().unwrap();

    let schema_version = lib.library().schema_version().unwrap_or(-1);
    let library_count = lib.library().count().unwrap_or(0);
    let history_count = lib.library().history_count().unwrap_or(0);

    let report = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "build": diag.build,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "schema_version": schema_version,
        "library_count": library_count,
        "history_count": history_count,
        "configuration": diag.configuration,
        "cards": diag.cards,
        "settled_hovers": diag.settled_hovers,
        "refusals": diag.refusals,
        "ocr_mode": diag.ocr_mode,
        "language": conf.current().language,
    });

    serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_owned())
}

/// Build the Tauri application and return its handle, shared diagnostics, pause request, quit request, and the `App` itself.
///
/// The `App` must be kept alive and `.run()` must be called on it. Dropping
/// it without running destroys the event loop; keeping only the `AppHandle`
/// without the `App` leaves no event loop running.
pub fn build(
    shared_settings: SharedSettings,
    shared_library: SharedLibrary,
    shared_detail: SharedDetail,
    shared_recent: SharedRecent,
    explain_flag: ExplainFlag,
    save_path: SharedSavePath,
) -> Result<(AppHandle, SharedDiagnostics, SharedPauseRequest, SharedQuitRequest, tauri::App), tauri::Error> {
    let diagnostics: SharedDiagnostics = Arc::new(Mutex::new(DiagnosticsSnapshot {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        ..Default::default()
    }));
    let pause_request: SharedPauseRequest = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let quit_request: SharedQuitRequest = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_diagnostics,
            get_runtime_status,
            request_pause_toggle,
            request_quit,
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
            library_export,
            library_export_json,
            library_export_jsonl,
            library_import_jsonl,
            library_secure_wipe,
            recent_list,
            library_later_list,
            library_later_count,
            library_set_review_state,
            history_list,
            history_count,
            history_delete,
            history_promote,
            history_prune,
            mori_status,
            mori_send_item,
            export_diagnostics,
            crate::detail_view::detail_read,
            crate::detail_view::detail_copy,
            crate::detail_view::detail_save,
            crate::detail_view::detail_save_note,
            crate::detail_view::detail_request_explain,
            crate::detail_view::detail_open_in_library,
        ])
        .manage(diagnostics.clone())
        .manage(pause_request.clone())
        .manage(quit_request.clone())
        .manage(shared_settings)
        .manage(shared_library)
        .manage(shared_detail)
        .manage(shared_recent)
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
    Ok((handle, diagnostics, pause_request, quit_request, app))
}

/// Run the Tauri event loop. Consumes the `App`.
///
/// This is the main thread's final act. It returns when the event loop
/// exits, which happens when `AppHandle::exit` is called.
///
/// Closing the settings window hides it rather than exiting: the polling
/// loop on its own thread decides when the app exits, and a user closing
/// a settings window is not a user quitting the product.
pub fn run(app: tauri::App) {
    app.run(|app_handle, event| {
        if let tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } = event
        {
            // A tray application does not exit when its window is closed.
            // The polling loop calls `AppHandle::exit` when the user quits
            // from the tray, and that is the only path to shutdown.
            if let Some(window) = app_handle.get_webview_window(&label) {
                let _ = window.hide();
            }
            api.prevent_close();
        }
    });
}
