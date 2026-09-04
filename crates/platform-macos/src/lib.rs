//! Mori Lens · macOS text acquisition.
//!
//! One implementation of [`lens_core::acquire::TextAcquirer`], over the macOS
//! Accessibility API. Nothing above this crate knows it exists
//! (`docs/decisions/ADR-0003-rust-core-platform-adapters.md`).
//!
//! # This crate cannot be fully unit tested
//!
//! The Accessibility API needs a live desktop with a real window and the
//! process must be trusted. `cargo test` compiles this and exercises the pure
//! helpers (coordinate conversion, UTF conversion, permission state), but a
//! green test run is not evidence that a change here works against real
//! applications. The verification is running the product against named
//! applications and recording results.

#![cfg(target_os = "macos")]

mod acquirer;
pub mod coords;
pub mod ffi;
pub mod overlay;
pub mod permission;
pub mod utf;
pub mod vision_ocr;

pub use acquirer::MacTextAcquirer;
pub use overlay::Overlay;
pub use permission::AxPermission;
pub use vision_ocr::{
    is_available as vision_is_available, recognize as vision_recognize, VisionOcrResult,
};
