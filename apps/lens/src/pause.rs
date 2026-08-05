//! A global key that stops Lens reading the screen.
//!
//! `docs/privacy-model.md` §5 lists this as something a user can check, and it
//! is the one control that matters most: a tool with this reach needs a way to
//! stop it that does not involve finding its window.
//!
//! **Pause means the pointer is not sampled**, not that results are hidden.
//! That distinction is the whole promise. A paused Lens makes no
//! accessibility call, so there is nothing to hide, and a reader can verify
//! that with any process monitor rather than taking it on trust.
//!
//! # Why a registered hotkey rather than a polled key combination
//!
//! Polling `GetAsyncKeyState` for a chord is what the pointer loop already
//! does for the modifier, and it would have been consistent. It is also wrong
//! here: a polled chord is missed when the loop is busy, fires repeatedly while
//! the keys are held, and requires edge detection that gets it wrong at
//! exactly the moment someone is trying to stop the tool.
//!
//! `RegisterHotKey` posts one message per press, to this thread's queue, and
//! the loop drains it. No edge detection, nothing missed.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
};
use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY};

/// Our hotkey's id. Arbitrary; it only has to be unique within this thread.
const HOTKEY_ID: i32 = 0x4C31;

/// `L`, for Lens.
const VK_L: u32 = 0x4C;

/// Registers Ctrl+Alt+L and reports presses.
#[derive(Debug)]
pub struct PauseKey {
    registered: bool,
    paused: bool,
}

impl PauseKey {
    /// Register the hotkey. Failure is not fatal: another application may
    /// already own this chord, and a Lens without a pause key is worse than a
    /// Lens that refuses to start, so the caller is told and carries on.
    pub fn register() -> Self {
        // SAFETY: registers a documented thread-local hotkey; `None` binds it
        // to this thread's message queue rather than to a window.
        let registered = unsafe {
            RegisterHotKey(
                None,
                HOTKEY_ID,
                // NOREPEAT so holding the chord does not toggle repeatedly.
                HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0),
                VK_L,
            )
        }
        .is_ok();

        Self {
            registered,
            paused: false,
        }
    }

    pub fn is_registered(&self) -> bool {
        self.registered
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Flip the state without a hotkey.
    ///
    /// The tray icon's entrance. It sets the same flag rather than keeping its
    /// own, because a product whose icon and hotkey disagree about whether it
    /// is paused is worse than one with neither.
    pub fn toggle(&mut self) {
        self.paused = !self.paused;
    }

    /// Drain pending messages and apply any hotkey presses.
    ///
    /// Returns whether the paused state changed, so the caller can report it
    /// and retire the card without printing a line on every loop iteration.
    pub fn poll(&mut self) -> bool {
        if !self.registered {
            return false;
        }
        let mut changed = false;
        let mut message = MSG::default();
        // SAFETY: reads and removes messages from this thread's own queue.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
                self.paused = !self.paused;
                changed = true;
            }
        }
        changed
    }
}

impl Drop for PauseKey {
    fn drop(&mut self) {
        if self.registered {
            // SAFETY: unregisters the hotkey this instance registered.
            unsafe {
                let _ = UnregisterHotKey(None, HOTKEY_ID);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

    /// Post the message Windows would post, and check the toggle.
    ///
    /// This covers the draining and the state change, which is the part with
    /// logic in it. It deliberately does **not** cover Windows delivering the
    /// message for a real key press: that would mean injecting keystrokes into
    /// the user's session, and a test that types Ctrl+Alt+L into whatever has
    /// focus is not a test worth having.
    fn press(key: &mut PauseKey) {
        // SAFETY: posts to this thread's own queue.
        unsafe {
            let _ = PostThreadMessageW(
                GetCurrentThreadId(),
                WM_HOTKEY,
                WPARAM(HOTKEY_ID as usize),
                LPARAM(0),
            );
        }
        key.poll();
    }

    #[test]
    fn the_key_toggles_rather_than_only_pausing() {
        let mut key = PauseKey::register();
        if !key.is_registered() {
            println!("SKIPPED: Ctrl+Alt+L is held by another application");
            return;
        }
        assert!(!key.is_paused(), "starts running");
        press(&mut key);
        assert!(key.is_paused(), "first press pauses");
        press(&mut key);
        assert!(!key.is_paused(), "second press resumes");
    }

    #[test]
    fn an_unrelated_message_does_not_pause_anything() {
        // The queue carries more than hotkeys. Toggling on anything that
        // arrives would pause Lens at random, which reads as the tool being
        // broken rather than as a control working.
        let mut key = PauseKey::register();
        if !key.is_registered() {
            return;
        }
        // SAFETY: posts to this thread's own queue.
        unsafe {
            let _ = PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY + 1, WPARAM(0), LPARAM(0));
        }
        assert!(!key.poll(), "an unrelated message changed the state");
        assert!(!key.is_paused());
    }
}
