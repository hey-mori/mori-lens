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

/// Claim the single-instance mutex with the production name.
pub fn claim() -> Option<OnlyInstance> {
    claim_named(windows::core::w!("Local\\swarea.mori-lens.single-instance"))
}

/// Claim a named single-instance mutex.
///
/// `None` when another holder already owns it. The claim is released by the
/// kernel when the holder exits, including when it crashes, so a Lens that
/// died badly does not lock the next one out.
fn claim_named(name: PCWSTR) -> Option<OnlyInstance> {
    // SAFETY: a documented call with a null-terminated name and no security
    // attributes, which gives the default descriptor.
    let handle = unsafe { CreateMutexW(None, true, name) }.ok()?;

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
/// instance's event-watcher thread converts the native Win32 signal into an
/// AtomicBool request, and the polling loop owns the Tauri UI action.
///
/// Multiple rapid second launches may coalesce into one activation. That is
/// acceptable: the semantic is "bring the Control Center forward", not
/// "count every launch".
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

/// Create the activation event for the first instance, using the production name.
///
/// Returns `None` if the event cannot be created. The event is manual-reset
/// so the polling loop can check it without race conditions.
pub fn create_activation_event() -> Option<ActivationEvent> {
    create_activation_event_named(windows::core::w!("Local\\swarea.mori-lens.activation"))
}

/// Create a named activation event.
///
/// Tests use isolated names to avoid interfering with a running Lens or with
/// each other. The production caller uses the canonical name.
fn create_activation_event_named(name: PCWSTR) -> Option<ActivationEvent> {
    // SAFETY: a documented call with a null-terminated name. Manual-reset
    // (true) so the signal persists until the polling loop resets it. Initially
    // non-signaled (false).
    let handle = unsafe { CreateEventW(None, true, false, name) }.ok()?;
    Some(ActivationEvent { handle })
}

/// Signal the activation event using the production name.
///
/// Called by the second instance when it detects an existing Lens is running.
/// Returns `true` if the signal was sent successfully.
pub fn signal_activation() -> bool {
    signal_activation_named(windows::core::w!("Local\\swarea.mori-lens.activation"))
}

/// Signal a named activation event.
///
/// Tests use isolated names to avoid signaling a real Lens instance.
fn signal_activation_named(name: PCWSTR) -> bool {
    // SAFETY: opening an existing event with a null-terminated name.
    let handle = unsafe {
        windows::Win32::System::Threading::OpenEventW(
            windows::Win32::System::Threading::EVENT_MODIFY_STATE,
            false,
            name,
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
/// Called by the first instance's event-watcher thread. Returns `true` if a
/// second instance asked us to show the window. Resets the event after
/// checking so the signal is consumed.
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

/// Generate a unique event/mutex name for tests.
///
/// Combines the process ID with a monotonic counter to ensure tests running
/// in parallel do not interfere with each other or with a real Lens instance.
#[cfg(test)]
fn test_name(kind: &str) -> windows::core::HSTRING {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!(
        "Local\\test.mori-lens.{kind}.{pid}.{id}",
        kind = kind,
        pid = std::process::id(),
        id = id
    );
    windows::core::HSTRING::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_named_succeeds_for_fresh_name() {
        let name = test_name("mutex");
        let first = claim_named(PCWSTR(name.as_ptr()));
        assert!(first.is_some(), "fresh claim should succeed");
        // Second claim with the same name should fail (we hold it).
        let second = claim_named(PCWSTR(name.as_ptr()));
        assert!(
            second.is_none(),
            "second claim should fail while first holds it"
        );
        // Drop the first, then a third claim should succeed.
        drop(first);
        let third = claim_named(PCWSTR(name.as_ptr()));
        assert!(third.is_some(), "claim after release should succeed");
    }

    #[test]
    fn activation_event_can_be_created_with_isolated_name() {
        let name = test_name("event");
        let event = create_activation_event_named(PCWSTR(name.as_ptr()));
        assert!(event.is_some(), "activation event should be created");
    }

    #[test]
    fn activation_event_starts_unsignaled_with_isolated_name() {
        let name = test_name("event");
        let event = create_activation_event_named(PCWSTR(name.as_ptr())).expect("create event");
        assert!(
            !check_activation(&event),
            "freshly created event should be unsignaled"
        );
    }

    #[test]
    fn activation_signal_is_detected_with_isolated_names() {
        let name = test_name("event");
        let event = create_activation_event_named(PCWSTR(name.as_ptr())).expect("create event");
        assert!(
            signal_activation_named(PCWSTR(name.as_ptr())),
            "signal should succeed"
        );
        assert!(check_activation(&event), "signal should be detected");
    }

    #[test]
    fn activation_signal_is_consumed_with_isolated_names() {
        let name = test_name("event");
        let event = create_activation_event_named(PCWSTR(name.as_ptr())).expect("create event");
        signal_activation_named(PCWSTR(name.as_ptr()));
        assert!(check_activation(&event), "first check should see signal");
        assert!(
            !check_activation(&event),
            "second check should not see signal (consumed)"
        );
    }

    #[test]
    fn test_names_are_unique() {
        let a = test_name("check");
        let b = test_name("check");
        assert_ne!(a, b, "consecutive test names must differ");
    }
}
