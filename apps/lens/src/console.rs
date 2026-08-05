//! Where the startup banner goes when there is nowhere for it to go.
//!
//! Lens is built as a **Windows subsystem** binary, so double-clicking it does
//! not open a black console window that then has to stay open for the product
//! to keep running. That console was the single most unfinished thing about
//! this application: a tray tool whose window you must not close is not a tray
//! tool.
//!
//! The cost is that a Windows subsystem process starts with no standard output
//! at all, so everything the banner says would vanish, including the line that
//! tells a user where their settings file is and the line that says Ollama is
//! missing.
//!
//! So the process takes the console of whoever launched it, if there was one.
//! Run from a terminal, it prints. Double-clicked, it does not, and nothing
//! else changes.

#![cfg(windows)]

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
    STD_OUTPUT_HANDLE,
};

/// Take the launching terminal's console, if this process has no output yet.
///
/// Call before anything is printed.
pub fn attach_to_launcher() {
    // SAFETY: every call is a documented console or file API, and the handles
    // are handed to `SetStdHandle`, which takes ownership of them for the life
    // of the process.
    unsafe {
        // A redirect is already a perfectly good place for output, and
        // replacing it would send `lens > out.txt` to a console instead of the
        // file the user asked for. Only an absent handle is worth fixing.
        if has_output() {
            return;
        }
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            // Double-clicked, or launched by the shell. There is no console to
            // take and that is not a failure: the tray icon and the card carry
            // everything a user needs, which is why they carry it.
            return;
        }

        // Attaching gives the process a console but does not point its
        // standard handles at it. Without these two, `println!` writes to a
        // handle that is still null and the banner is silently lost, which
        // looks exactly like the product not starting.
        if let Some(out) = console_handle() {
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, out);
        }
        if let Some(err) = console_handle() {
            let _ = SetStdHandle(STD_ERROR_HANDLE, err);
        }
    }
}

/// # Safety
/// Reads a documented process-wide handle.
unsafe fn has_output() -> bool {
    // SAFETY: as above.
    unsafe {
        GetStdHandle(STD_OUTPUT_HANDLE)
            .map(|handle| !handle.is_invalid() && !handle.0.is_null())
            .unwrap_or(false)
    }
}

/// A fresh handle to this process's console screen buffer.
///
/// # Safety
/// Opens a documented special file; the handle belongs to the caller.
unsafe fn console_handle() -> Option<HANDLE> {
    // SAFETY: `CONOUT$` is the documented name for the attached console's
    // screen buffer and exists exactly when a console is attached.
    unsafe {
        CreateFileW(
            PCWSTR(windows::core::w!("CONOUT$").as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()
    }
}
