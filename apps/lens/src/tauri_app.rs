//! Tauri product shell.
//!
//! The hover engine runs on a worker thread; this module owns the Tauri
//! application on the main thread, including the settings webview (#164, #165).

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::Manager;

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

/// Build the Tauri application and return its handle, shared diagnostics, and the `App` itself.
///
/// The `App` must be kept alive and `.run()` must be called on it. Dropping
/// it without running destroys the event loop; keeping only the `AppHandle`
/// without the `App` leaves no event loop running.
pub fn build(
    shared_settings: SharedSettings,
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
        ])
        .manage(diagnostics.clone())
        .manage(shared_settings)
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
