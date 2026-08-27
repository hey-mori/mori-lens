//! Mori Lens main window.
//!
//! A Tauri v2 webview that holds the settings, diagnostics, and model
//! management screens. Runs as a separate process from the hover
//! engine, launched on demand when the reader clicks "Settings" in
//! the tray menu.
//!
//! # Communication with the engine
//!
//! The engine writes a JSON snapshot to `%APPDATA%\Mori Lens\diagnostics.json`
//! when the reader opens this window. The window reads it at startup
//! and displays it. Settings changes will write back through the same
//! path in a later phase.

#![cfg(windows)]
#![windows_subsystem = "windows"]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

/// The diagnostics data written by the engine and read by this window.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiagnosticsSnapshot {
    /// The build stamp (commit SHA).
    build: String,
    /// Configuration key-value pairs.
    configuration: Vec<(String, String)>,
    /// Session counters.
    cards: u32,
    settled_hovers: u32,
    /// Refusal counts: (reason, count).
    refusals: Vec<(String, u32)>,
    /// OCR mode: "off", "manual", or "automatic".
    ocr_mode: String,
}

/// The directory where Lens stores its data.
fn data_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(appdata).join("Mori Lens")
}

/// Read the diagnostics snapshot written by the engine.
#[tauri::command]
fn get_diagnostics() -> Option<DiagnosticsSnapshot> {
    let path = data_dir().join("diagnostics.json");
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_diagnostics])
        .setup(|app| {
            // Show the window on startup.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Mori Lens");
}
