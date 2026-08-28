//! Tauri product shell.
//!
//! The hover engine runs on a worker thread; this module owns the Tauri
//! application on the main thread. A separate process (`lens-window`) used
//! to hold the settings window; the product now runs it directly so the
//! lifecycle is single-owner (#164, #165).

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::Manager;

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

/// Build the Tauri application and return its handle, shared diagnostics, and the `App` itself.
///
/// The `App` must be kept alive and `.run()` must be called on it. Dropping
/// it without running destroys the event loop; keeping only the `AppHandle`
/// without the `App` leaves no event loop running.
pub fn build() -> Result<(AppHandle, SharedDiagnostics, tauri::App), tauri::Error> {
    let diagnostics: SharedDiagnostics = Arc::new(Mutex::new(DiagnosticsSnapshot::default()));

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_diagnostics])
        .manage(diagnostics.clone())
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
