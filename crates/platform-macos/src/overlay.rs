//! macOS native overlay window.
//!
//! A transparent NSPanel that shows a translation card. The panel:
//! - Does not steal focus (`nonactivatingPanel`, `becomesKeyOnlyIfNeeded`)
//! - Passes all clicks through (`ignoresMouseEvents`)
//! - Sits above normal windows (`NSStatusWindowLevel`)
//! - Has per-pixel alpha transparency
//!
//! The actual window management is in a small Objective-C bridge
//! (`native/overlay.m`), compiled by `cc` in `build.rs`. This module provides
//! the safe Rust wrapper.

use std::ffi::CString;

// FFI declarations for the Objective-C bridge.
extern "C" {
    fn lens_overlay_create() -> bool;
    fn lens_overlay_show(
        source_text: *const std::os::raw::c_char,
        translation_text: *const std::os::raw::c_char,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> bool;
    fn lens_overlay_hide();
    fn lens_overlay_destroy();
    fn lens_overlay_is_visible() -> bool;
}

/// The macOS overlay window.
///
/// Wraps the Objective-C NSPanel bridge. Creating an `Overlay` initializes
/// the native panel; dropping it tears it down.
pub struct Overlay {
    _private: (),
}

impl Overlay {
    /// Create the overlay. The panel is created hidden.
    pub fn start() -> Option<Self> {
        // SAFETY: lens_overlay_create is a C function that creates an NSPanel.
        // It is safe to call and returns false on failure.
        let ok = unsafe { lens_overlay_create() };
        if ok {
            Some(Self { _private: () })
        } else {
            None
        }
    }

    /// Show the overlay with text at the given position.
    ///
    /// Coordinates are in **logical screen points** (top-left origin),
    /// matching the macOS coordinate system used by AppKit.
    pub fn show_text(
        &self,
        source: &str,
        translation: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> bool {
        let src = CString::new(source).unwrap_or_default();
        let trans = CString::new(translation).unwrap_or_default();
        // SAFETY: lens_overlay_show takes C strings and doubles. The panel
        // was created by start(). The strings are valid for the duration of
        // the call.
        unsafe { lens_overlay_show(src.as_ptr(), trans.as_ptr(), x, y, width, height) }
    }

    /// Hide the overlay.
    pub fn hide(&self) {
        // SAFETY: lens_overlay_hide is safe to call after start().
        unsafe { lens_overlay_hide() };
    }

    /// Whether the overlay is currently visible.
    pub fn is_visible(&self) -> bool {
        // SAFETY: lens_overlay_is_visible is a pure query.
        unsafe { lens_overlay_is_visible() }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // SAFETY: lens_overlay_destroy releases the NSPanel. Called once on
        // drop; the panel is not used after this.
        unsafe { lens_overlay_destroy() };
    }
}

impl std::fmt::Debug for Overlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Overlay")
            .field("visible", &self.is_visible())
            .finish()
    }
}
