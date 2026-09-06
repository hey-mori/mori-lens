//! macOS menu bar integration.
//!
//! Provides an `NSStatusItem` with Pause, Deep Analysis, Copy, Save,
//! Manual OCR, and Quit actions. The product loop polls for each action.

use std::ffi::CString;

extern "C" {
    fn lens_menubar_create() -> bool;
    fn lens_menubar_destroy();
    fn lens_menubar_set_title(title: *const std::os::raw::c_char);
    fn lens_menubar_poll_pause_toggle() -> bool;
    fn lens_menubar_poll_quit() -> bool;
    fn lens_menubar_poll_deep() -> bool;
    fn lens_menubar_poll_copy() -> bool;
    fn lens_menubar_poll_save() -> bool;
    fn lens_menubar_poll_ocr() -> bool;
    fn lens_menubar_poll_open_settings() -> bool;
    fn lens_menubar_poll_open_library() -> bool;
}

/// The macOS menu bar item.
pub struct MenuBar {
    _private: (),
}

impl MenuBar {
    pub fn create() -> Option<Self> {
        // SAFETY: lens_menubar_create creates an NSStatusItem on the main thread.
        let ok = unsafe { lens_menubar_create() };
        if ok {
            Some(Self { _private: () })
        } else {
            None
        }
    }

    pub fn set_title(&self, title: &str) {
        let c = CString::new(title).unwrap_or_default();
        // SAFETY: lens_menubar_set_title takes a valid C string.
        unsafe { lens_menubar_set_title(c.as_ptr()) };
    }

    /// Edge-triggered: returns true once per Pause/Resume click.
    pub fn poll_pause_toggle(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_pause_toggle() }
    }

    pub fn poll_quit(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_quit() }
    }

    /// Edge-triggered: returns true once per Deep Analysis click.
    pub fn poll_deep(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_deep() }
    }

    pub fn poll_copy(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_copy() }
    }

    pub fn poll_save(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_save() }
    }

    pub fn poll_ocr(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_ocr() }
    }

    /// Edge-triggered: returns true once per Open Settings click.
    pub fn poll_open_settings(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_open_settings() }
    }

    /// Edge-triggered: returns true once per Open Library click.
    pub fn poll_open_library(&self) -> bool {
        // SAFETY: pure atomic read.
        unsafe { lens_menubar_poll_open_library() }
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        // SAFETY: lens_menubar_destroy removes the NSStatusItem.
        unsafe { lens_menubar_destroy() };
    }
}

impl std::fmt::Debug for MenuBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBar").finish()
    }
}
