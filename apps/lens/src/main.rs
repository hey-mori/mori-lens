//! Mori Lens. Rest the pointer on a sentence; read it in your language.
//!
//! Cross-platform entry point. Platform-neutral modules (Tauri shell,
//! library, settings, detail, keeping, Mori transfer) are declared here
//! and shared by both Windows and macOS platform consumers.

// No console window on Windows. A tray application that opens a black terminal
// you must leave open is not a tray application.
#![cfg_attr(windows, windows_subsystem = "windows")]
// Some items are architecturally defined for cross-platform parity
// but not yet wired into every product path. Kept for future use.
#![allow(dead_code)]

// -- Platform-neutral shared modules --
mod detail_view;
mod keeping;
mod library_service;
mod mori_transfer;
mod settings_service;
mod tauri_app;

// -- Windows: the full product loop, in win.rs. --
#[cfg(windows)]
#[path = "win.rs"]
mod win;

// -- macOS: native product loop with Tauri shell. --
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;

// Re-export the platform main so the binary has a crate-level entry point.
#[cfg(windows)]
fn main() -> std::process::ExitCode {
    win::main()
}

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    macos::main()
}
