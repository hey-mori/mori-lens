//! One Lens at a time.
//!
//! # What two of them do
//!
//! Measured by starting two, because the answer was worse than the guess:
//!
//! ```text
//!   pause     UNAVAILABLE: Ctrl+Alt+L is taken by another application
//!   ocr       manual · UNAVAILABLE: Ctrl+Alt+O is taken by another application
//!   tray      click to pause, right-click for the menu
//! ```
//!
//! The second instance runs to the end. Two tray icons, two overlays racing
//! for the same hover, two model leases held against a runtime that gives one
//! at a time, and **both hotkeys reported as taken by another application**,
//! which is a sentence about Lens that names something else.
//!
//! Clicking the tray to pause then pauses one of them, and the other carries
//! on reading. A privacy control that stops half the product is not one.
//!
//! # Why it is easy to do
//!
//! Lens opens no window on launch. Somebody who double-clicks it and sees
//! nothing happen double-clicks it again, which is the correct instinct for
//! every other program they own.
//!
//! # The name
//!
//! Per-session rather than global. Two people signed into one machine should
//! each get their own Lens, and `Local\` is exactly that boundary. A `Global\`
//! name would have the first person's Lens lock out the second's.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// Held for the life of the process. Dropping it releases the claim.
#[derive(Debug)]
pub struct OnlyInstance {
    handle: HANDLE,
}

impl Drop for OnlyInstance {
    fn drop(&mut self) {
        // SAFETY: a handle this type created and owns, closed once.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Claim the right to be the only Lens in this session.
///
/// `None` when another one already holds it. The claim is released by the
/// kernel when the holder exits, including when it crashes, so a Lens that
/// died badly does not lock the next one out.
pub fn claim() -> Option<OnlyInstance> {
    // A name nothing else would pick. Not derived from anything: a name built
    // from a path or a version would let two builds run at once, which is the
    // situation this exists to prevent rather than an exception to it.
    let name = windows::core::w!("Local\\swarea.mori-lens.single-instance");
    // SAFETY: a documented call with a null-terminated static name and no
    // security attributes, which gives the default descriptor.
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.ok()?;

    // **The error is read before the handle is used.** `CreateMutexW` succeeds
    // and returns a handle to the *existing* mutex when there is one, so the
    // return value alone says nothing about who owns it.
    // SAFETY: reading the calling thread's last error immediately after the
    // call that set it.
    let already = unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS;
    if already {
        // SAFETY: the handle this call just produced, closed once and not
        // used again.
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }
    Some(OnlyInstance { handle })
}
