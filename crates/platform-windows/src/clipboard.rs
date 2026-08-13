//! Putting one string on the clipboard, and nothing else.
//!
//! The only writing path in this product besides the settings file. It exists
//! because a reader who has just been handed a translation has no way to keep
//! it: the card is drawn on a window that passes clicks through
//! ([ADR-0004](../../../docs/decisions/ADR-0004-native-overlay.md)), so there
//! is nothing to select and nothing to drag.
//!
//! # It only ever runs on a keypress
//!
//! Same rule as the OCR key and the explain key. Nothing here is reachable
//! from a hover, and the clipboard is somebody's own workspace: a tool that
//! wrote to it because the pointer settled would be replacing whatever they
//! had copied a moment ago, silently, several times a minute.
//!
//! # What Windows requires, and what goes wrong when it is not done
//!
//! `OpenClipboard` takes an exclusive lock the whole session shares, so a
//! failure to close it leaves every other application unable to copy anything
//! until this process exits. That is the one way this file can damage a machine
//! rather than fail on it, and it is why the close is on every path out
//! including the failures.
//!
//! The memory handed to `SetClipboardData` is **owned by the clipboard**
//! afterwards. Freeing it is a double free; not freeing it when the call fails
//! is a leak. Both branches are here for that reason.

use windows::Win32::Foundation::{GlobalFree, HANDLE};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Put `text` on the clipboard, replacing whatever was there.
///
/// `false` when Windows would not give up the clipboard, which happens when
/// another application is holding it and is an ordinary state rather than an
/// error: the caller says so on a card and the reader presses the key again.
pub fn put_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    // UTF-16 with a terminator, which is what `CF_UNICODETEXT` means.
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = std::mem::size_of_val(wide.as_slice());

    // SAFETY: a documented call with a null owner, which binds the clipboard
    // to this thread. Every path below closes it.
    if unsafe { OpenClipboard(None) }.is_err() {
        return false;
    }

    let put = || -> bool {
        // SAFETY: allocates moveable global memory of a size computed from the
        // buffer above.
        let Ok(handle) = (unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }) else {
            return false;
        };
        // SAFETY: locks memory this call just allocated, to get a pointer to
        // its `bytes` writable bytes.
        let destination = unsafe { GlobalLock(handle) };
        if destination.is_null() {
            // SAFETY: frees memory nothing else has a handle to. The clipboard
            // has not been given it, so ownership is still here.
            unsafe {
                let _ = GlobalFree(Some(handle));
            }
            return false;
        }
        // SAFETY: both buffers are `bytes` long by construction, and the
        // destination was just locked.
        unsafe {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), destination as *mut u16, wide.len());
            let _ = GlobalUnlock(handle);
        }

        // SAFETY: empties the clipboard this thread owns, then hands it the
        // memory. After a successful `SetClipboardData` the block belongs to
        // the clipboard and must not be freed here.
        unsafe {
            let _ = EmptyClipboard();
            if SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0))).is_err() {
                // It refused, so ownership never moved and this is still ours.
                let _ = GlobalFree(Some(handle));
                return false;
            }
        }
        true
    };

    let copied = put();
    // SAFETY: releases the lock this function took, on every path. Leaving it
    // held would stop every other application on the machine from copying
    // anything until this process exits.
    unsafe {
        let _ = CloseClipboard();
    }
    copied
}

/// Read the clipboard back, for the live check only.
///
/// Not part of the product. Lens has no reason to read what somebody copied,
/// and a function that could would be one refactor away from a tool that does.
#[cfg(test)]
pub fn read_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::GetClipboardData;

    // SAFETY: documented call, closed on both paths below.
    if unsafe { OpenClipboard(None) }.is_err() {
        return None;
    }
    // SAFETY: asks for the format this module writes; a null handle means the
    // clipboard holds something else.
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0 as u32) };
    let text = match handle {
        Ok(handle) if !handle.is_invalid() => {
            let block = HGLOBAL(handle.0);
            // SAFETY: locks a block the clipboard owns, for reading only.
            let source = unsafe { GlobalLock(block) } as *const u16;
            if source.is_null() {
                None
            } else {
                // SAFETY: the clipboard guarantees a null-terminated UTF-16
                // string for this format.
                let read = unsafe {
                    let mut length = 0usize;
                    while *source.add(length) != 0 {
                        length += 1;
                    }
                    String::from_utf16_lossy(std::slice::from_raw_parts(source, length))
                };
                // SAFETY: releases the lock taken above.
                unsafe {
                    let _ = GlobalUnlock(block);
                }
                Some(read)
            }
        }
        _ => None,
    };
    // SAFETY: releases the lock this function took.
    unsafe {
        let _ = CloseClipboard();
    }
    text
}
