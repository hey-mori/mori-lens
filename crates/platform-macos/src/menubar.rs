//! macOS menu bar integration.
//!
//! Provides an `NSStatusItem` in the menu bar with Pause/Resume and Quit.
//! The product loop polls for pause toggle and quit requests.

use std::ffi::CString;

extern "C" {
    fn lens_menubar_create() -> bool;
    fn lens_menubar_destroy();
    fn lens_menubar_set_title(title: *const std::os::raw::c_char);
    fn lens_menubar_poll_pause_toggle() -> bool;
    fn lens_menubar_poll_quit() -> bool;
}

/// The macOS menu bar item.
pub struct MenuBar {
    _private: (),
}

impl MenuBar {
    /// Create the menu bar item.
    pub fn create() -> Option<Self> {
        // SAFETY: lens_menubar_create creates an NSStatusItem.
        let ok = unsafe { lens_menubar_create() };
        if ok {
            Some(Self { _private: () })
        } else {
            None
        }
    }

    /// Set the menu bar title.
    pub fn set_title(&self, title: &str) {
        let c = CString::new(title).unwrap_or_default();
        // SAFETY: lens_menubar_set_title takes a C string.
        unsafe { lens_menubar_set_title(c.as_ptr()) };
    }

    /// Poll for a pause toggle from the user. Returns true if the user
    /// clicked Pause/Resume since the last poll. Edge-triggered.
    pub fn poll_pause_toggle(&self) -> bool {
        // SAFETY: lens_menubar_poll_pause_toggle is a pure query.
        unsafe { lens_menubar_poll_pause_toggle() }
    }

    /// Poll for a quit request. Returns true if the user clicked Quit.
    pub fn poll_quit(&self) -> bool {
        // SAFETY: lens_menubar_poll_quit is a pure query.
        unsafe { lens_menubar_poll_quit() }
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
