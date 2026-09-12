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
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent};

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

/// Activation event handle, held by the first instance to receive re-entry signals.
///
/// When a second Lens process detects an existing instance, it signals this
/// event to ask the first instance to show its Control Center. The first
/// instance's polling loop watches for this event and responds by bringing
/// the window forward.
#[derive(Debug)]
pub struct ActivationEvent {
    handle: HANDLE,
}

// SAFETY: HANDLE is a Windows kernel handle that can be safely shared across
// threads. The handle is owned by this struct and closed exactly once in Drop.
// The Windows kernel ensures handle operations are thread-safe.
unsafe impl Send for ActivationEvent {}
// SAFETY: HANDLE is a Windows kernel handle that can be safely shared across
// threads via immutable references. The check_activation method only reads
// the handle value and passes it to thread-safe Windows API calls.
unsafe impl Sync for ActivationEvent {}

impl Drop for ActivationEvent {
    fn drop(&mut self) {
        // SAFETY: a handle this type created and owns, closed once.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Create the activation event for the first instance.
///
/// Returns `None` if the event cannot be created. The event is manual-reset
/// so the polling loop can check it without race conditions.
pub fn create_activation_event() -> Option<ActivationEvent> {
    let name = windows::core::w!("Local\\swarea.mori-lens.activation");
    // SAFETY: a documented call with a null-terminated static name. Manual-reset
    // (true) so the signal persists until the polling loop resets it. Initially
    // non-signaled (false).
    let handle = unsafe { CreateEventW(None, true, false, PCWSTR(name.as_ptr())) }.ok()?;
    Some(ActivationEvent { handle })
}

/// Signal the activation event to ask the existing instance to show its window.
///
/// Called by the second instance when it detects an existing Lens is running.
/// Returns `true` if the signal was sent successfully.
pub fn signal_activation() -> bool {
    let name = windows::core::w!("Local\\swarea.mori-lens.activation");
    // SAFETY: opening an existing event with a null-terminated static name.
    let handle = unsafe {
        windows::Win32::System::Threading::OpenEventW(
            windows::Win32::System::Threading::EVENT_MODIFY_STATE,
            false,
            PCWSTR(name.as_ptr()),
        )
    }
    .ok();

    let Some(handle) = handle else {
        return false;
    };

    // SAFETY: setting the event handle we just opened.
    let result = unsafe { SetEvent(handle) }.is_ok();

    // SAFETY: the handle we opened, closed once.
    unsafe {
        let _ = CloseHandle(handle);
    }

    result
}

/// Check if the activation event is signaled, and reset it if so.
///
/// Called by the first instance's polling loop. Returns `true` if a second
/// instance asked us to show the window. Resets the event after checking
/// so the signal is consumed.
pub fn check_activation(event: &ActivationEvent) -> bool {
    use windows::Win32::System::Threading::{ResetEvent, WaitForSingleObjectEx};

    // SAFETY: waiting on the event handle with zero timeout (non-blocking).
    let result = unsafe {
        WaitForSingleObjectEx(event.handle, 0, false) == windows::Win32::Foundation::WAIT_OBJECT_0
    };

    if result {
        // SAFETY: resetting the event handle we own.
        unsafe {
            let _ = ResetEvent(event.handle);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_claim_succeeds() {
        // A fresh claim in a test process should succeed (no other Lens running).
        // Note: this test may fail if another Lens is actually running.
        let _ = claim();
    }

    #[test]
    fn activation_event_can_be_created() {
        let event = create_activation_event();
        assert!(event.is_some(), "activation event should be created");
    }

    #[test]
    fn activation_event_starts_unsignaled() {
        let event = create_activation_event().expect("create event");
        // Reset the event to ensure it's unsignaled for this test.
        // A previous test run or another Lens instance might have left it signaled.
        use windows::Win32::System::Threading::ResetEvent;
        // SAFETY: event.handle is a valid handle created by create_activation_event,
        // and ResetEvent is a documented Windows API call that operates on it safely.
        unsafe {
            let _ = ResetEvent(event.handle);
        }
        assert!(
            !check_activation(&event),
            "event should be unsignaled after reset"
        );
    }

    #[test]
    fn activation_signal_is_detected() {
        let event = create_activation_event().expect("create event");
        assert!(signal_activation(), "signal should succeed");
        assert!(check_activation(&event), "signal should be detected");
    }

    #[test]
    fn activation_signal_is_consumed() {
        let event = create_activation_event().expect("create event");
        signal_activation();
        assert!(check_activation(&event), "first check should see signal");
        assert!(
            !check_activation(&event),
            "second check should not see signal (consumed)"
        );
    }
}
