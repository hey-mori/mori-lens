//! macOS global hotkeys via CGEventTap.
//!
//! Intercepts Cmd+Opt+key combinations globally and maps them to product
//! commands. Requires Accessibility permission (already requested for AX).
//!
//! Commands are queued in a lock-free ring buffer on the native side and
//! polled by the Rust product loop.

use std::os::raw::c_int;

/// Product command from a global hotkey or menu action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ProductCommand {
    None = 0,
    PauseToggle = 1,
    Deep = 2,
    Copy = 3,
    Save = 4,
    ManualOcr = 5,
    Quit = 6,
    OpenSettings = 7,
    OpenLibrary = 8,
}

impl From<c_int> for ProductCommand {
    fn from(val: c_int) -> Self {
        match val {
            1 => Self::PauseToggle,
            2 => Self::Deep,
            3 => Self::Copy,
            4 => Self::Save,
            5 => Self::ManualOcr,
            6 => Self::Quit,
            7 => Self::OpenSettings,
            8 => Self::OpenLibrary,
            _ => Self::None,
        }
    }
}

extern "C" {
    fn lens_hotkey_install() -> bool;
    fn lens_hotkey_remove();
    fn lens_hotkey_poll() -> c_int;
}

/// Install the global event tap for hotkeys.
pub fn install() -> bool {
    // SAFETY: lens_hotkey_install creates a CGEventTap. Requires AX permission.
    unsafe { lens_hotkey_install() }
}

/// Remove the global event tap.
pub fn remove() {
    // SAFETY: lens_hotkey_remove tears down the CGEventTap.
    unsafe { lens_hotkey_remove() };
}

/// Poll for the next pending command. Returns `ProductCommand::None` if empty.
pub fn poll() -> ProductCommand {
    // SAFETY: lens_hotkey_poll reads from the lock-free ring buffer.
    let raw = unsafe { lens_hotkey_poll() };
    ProductCommand::from(raw)
}
