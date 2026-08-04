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
mod com;
pub mod overlay;
mod uia;
mod window;

#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod protected_harness;

pub use appearance::system_preferences;
pub use overlay::Overlay;
pub use uia::UiaTextAcquirer;
pub use window::{
    declare_dpi_awareness, process_name_at, root_window_at, top_level_windows, WindowInfo,
};
