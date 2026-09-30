//! Mori Lens. Rest the pointer on a sentence; read it in your language.
//!
//! Cross-platform entry point. Platform-neutral modules (Tauri shell,
//! library, settings, detail, keeping, Mori transfer) are declared here
//! and shared by both Windows and macOS platform consumers.

// No console window on Windows. A tray application that opens a black terminal
// you must leave open is not a tray application.
#![cfg_attr(windows, windows_subsystem = "windows")]

// -- Platform-neutral shared modules --
mod ai_runtime;
mod detail_view;
mod dispatch_carrier;
mod download_worker;
mod keeping;
mod library_service;
mod mori_transfer;
mod settings_service;
mod shared_orchestration;
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

// Keep the shared Tauri command wiring checkable on development hosts that
// cannot run either product adapter. This is not a Linux product entry point.
#[cfg(not(any(windows, target_os = "macos")))]
fn main() -> std::process::ExitCode {
    eprintln!("Mori Lens desktop supports Windows and macOS only");
    std::process::ExitCode::FAILURE
}
