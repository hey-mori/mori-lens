//! Tauri main window integration.
//!
//! Phase 1 of #165: a Tauri window managed by the product binary, not a
//! separate process. The window is hidden by default and shown when the reader
//! clicks "Settings" or "Show Diagnostics" in the tray menu.

// Phase 1 adds the infrastructure; phase 2 wires it up.
#![allow(dead_code)]

use tauri::Manager;

/// Initialize the Tauri application and create the main window.
///
/// The window is hidden by default. Call `show_main_window` to display it.
/// Returns the Tauri app handle, which can be used to show/hide the window
/// from other parts of the application.
pub fn init() -> Result<tauri::AppHandle, tauri::Error> {
    let app = tauri::Builder::default()
        .setup(|app| {
            // The window is created but hidden. It will be shown when the reader
            // requests it from the tray menu.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            Ok(())
        })
        .build(tauri::generate_context!())?;

    let handle = app.handle().clone();
    Ok(handle)
}

/// Show the main window.
///
/// Called when the reader clicks "Settings" or "Show Diagnostics" in the tray
/// menu. The window is brought to the front and focused.
pub fn show_main_window(handle: &tauri::AppHandle) {
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Hide the main window.
///
/// Called when the reader closes the window or when the application is exiting.
pub fn hide_main_window(handle: &tauri::AppHandle) {
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.hide();
    }
}
