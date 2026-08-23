//! Mori Lens · Windows text acquisition.
//!
//! One implementation of [`lens_core::acquire::TextAcquirer`], over UI
//! Automation. Nothing above this crate knows it exists
//! (`docs/decisions/ADR-0003-rust-core-platform-adapters.md`).
//!
//! # This crate cannot be unit tested
//!
//! UI Automation needs a live desktop with a real window. `cargo test`
//! compiles this and exercises nothing, so a green test run is not evidence
//! that a change here works. The verification is a probe run against named
//! applications, recorded in `docs/compatibility/windows-app-matrix.md`.
//!
//! Saying so here rather than only in the workflow document, because this is
//! the file someone reads before changing it.

#![cfg(windows)]

mod appearance;
pub mod autostart;
pub mod capture;
pub mod clipboard;
mod com;
mod glass;
mod inventory;
pub mod ocr;
pub mod ocr_acquire;
pub mod overlay;
pub mod single;
pub mod tray;
mod uia;
pub mod welcome;
mod window;

#[cfg(test)]
mod fullscreen_harness;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod protected_harness;

pub use appearance::system_preferences;
pub use capture::Capture;
pub use inventory::{text_inventory, text_rectangles_of, text_words_of, WindowTextInventory};
pub use ocr_acquire::{CaptureCost, OcrTextAcquirer};
pub use overlay::Overlay;
pub use tray::{Tray, TrayEvent};
pub use uia::UiaTextAcquirer;
pub use window::{
    declare_dpi_awareness, foreground_window, monitor_scale_at, process_name_at, root_window_at,
    top_level_windows, ForegroundProblem, WindowInfo,
};

/// Send the accessibility wake message to a window, by its opaque id.
///
/// Separate from `UiaTextAcquirer::wake_window`, which also sleeps the settle:
/// this is the message alone, so the message and the settle can be measured as
/// the two different things they are (issue #4).
pub fn wake_accessibility_for(window_id: i64) {
    window::wake_accessibility(windows::Win32::Foundation::HWND(window_id as *mut _));
}
