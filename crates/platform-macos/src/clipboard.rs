//! macOS clipboard via NSPasteboard.

use std::ffi::CString;

extern "C" {
    fn lens_clipboard_put_text(text: *const std::os::raw::c_char) -> bool;
}

/// Put text on the system clipboard.
pub fn put_text(text: &str) -> bool {
    let c = match CString::new(text) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // SAFETY: lens_clipboard_put_text takes a valid C string and copies it
    // into the NSPasteboard.
    unsafe { lens_clipboard_put_text(c.as_ptr()) }
}
