//! The two global keys, and why they are one struct.
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

/// Hotkey ids. Arbitrary; they only have to be unique within this thread.
const PAUSE_ID: i32 = 0x4C31;
const OCR_ID: i32 = 0x4C32;
const EXPLAIN_ID: i32 = 0x4C33;
const COPY_ID: i32 = 0x4C34;

/// `L`, for Lens.
const VK_L: u32 = 0x4C;
/// `O`, for OCR.
const VK_O: u32 = 0x4F;
/// `D`, for 설명. `E` is taken by Explorer on many machines and `A` by
/// select-all in whatever has focus, which matters because a global chord that
/// fails to register is a key that silently does nothing.
const VK_D: u32 = 0x44;
/// `C`, for copy, and the letter every other program uses for it.
const VK_C: u32 = 0x43;

/// What a press asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// Pause, or resume. Applied before this is returned.
    PauseChanged,
    /// Read what is under the pointer with OCR, because the accessibility
    /// path has nothing to give there.
    ReadWithOcr,
    /// Explain the sentence the card is already showing.
    ///
    /// The deep path of
    /// [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md), and the
    /// user action that whole path is gated on. A key rather than a button on
    /// the card, because the overlay passes clicks through
    /// ([ADR-0004](../../../docs/decisions/ADR-0004-passive-overlay.md)) and a
    /// card that could be clicked would be a card that takes focus.
    Explain,
    /// Put what the card is showing on the clipboard.
    ///
    /// The card cannot be selected, because the overlay passes clicks through,
    /// so without this there is no way at all to keep an answer.
    Copy,
}

/// Registers both global keys and reports presses.
///
/// **One struct because there is one message queue.** `poll` drains this
/// thread's messages, and a second handler doing the same would take the
/// first one's messages and drop them: whichever ran first would work and the
/// other would appear registered and never fire. That is a bug with no symptom
/// except a key that does nothing.
#[derive(Debug)]
pub struct PauseKey {
    registered: bool,
    ocr_registered: bool,
    explain_registered: bool,
    copy_registered: bool,
    paused: bool,
}

impl PauseKey {
    /// Register the hotkey. Failure is not fatal: another application may
    /// already own this chord, and a Lens without a pause key is worse than a
    /// Lens that refuses to start, so the caller is told and carries on.
    pub fn register() -> Self {
        // SAFETY: registers a documented thread-local hotkey; `None` binds it
        // to this thread's message queue rather than to a window.
        // SAFETY: registers documented thread-local hotkeys; `None` binds them
        // to this thread's message queue rather than to a window.
        let (registered, ocr_registered, explain_registered, copy_registered) = unsafe {
            // NOREPEAT so holding a chord does not fire repeatedly.
            let modifiers = HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0);
            (
                RegisterHotKey(None, PAUSE_ID, modifiers, VK_L).is_ok(),
                RegisterHotKey(None, OCR_ID, modifiers, VK_O).is_ok(),
                RegisterHotKey(None, EXPLAIN_ID, modifiers, VK_D).is_ok(),
                RegisterHotKey(None, COPY_ID, modifiers, VK_C).is_ok(),
            )
        };

        Self {
            registered,
            ocr_registered,
            explain_registered,
            copy_registered,
            paused: false,
        }
    }

    /// Whether Ctrl+Alt+O was taken by something else.
    pub fn ocr_is_registered(&self) -> bool {
        self.ocr_registered
    }

    /// Whether Ctrl+Alt+D was taken by something else.
    pub fn explain_is_registered(&self) -> bool {
        self.explain_registered
    }

    /// Whether Ctrl+Alt+C was taken by something else.
    pub fn copy_is_registered(&self) -> bool {
        self.copy_registered
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
    pub fn poll(&mut self) -> Vec<Pressed> {
        let mut pressed = Vec::new();
        if !self.registered
            && !self.ocr_registered
            && !self.explain_registered
            && !self.copy_registered
        {
            return pressed;
        }
        let mut message = MSG::default();
        // SAFETY: reads and removes messages from this thread's own queue.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message != WM_HOTKEY {
                continue;
            }
            match message.wParam.0 as i32 {
                PAUSE_ID => {
                    self.paused = !self.paused;
                    pressed.push(Pressed::PauseChanged);
                }
                OCR_ID => pressed.push(Pressed::ReadWithOcr),
                EXPLAIN_ID => pressed.push(Pressed::Explain),
                COPY_ID => pressed.push(Pressed::Copy),
                _ => {}
            }
        }
        pressed
    }
}

impl Drop for PauseKey {
    fn drop(&mut self) {
        // Each one guarded by its own flag. They were all behind `registered`,
        // so a machine where another application held Ctrl+Alt+L left the
        // other two registered on the way out: the flag for one key was
        // deciding the fate of three.
        for (held, id) in [
            (self.registered, PAUSE_ID),
            (self.ocr_registered, OCR_ID),
            (self.explain_registered, EXPLAIN_ID),
            (self.copy_registered, COPY_ID),
        ] {
            if held {
                // SAFETY: unregisters a hotkey this instance registered.
                unsafe {
                    let _ = UnregisterHotKey(None, id);
                }
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
    fn press_id(key: &mut PauseKey, id: i32) -> Vec<Pressed> {
        // SAFETY: posts to this thread's own queue.
        unsafe {
            let _ = PostThreadMessageW(
                GetCurrentThreadId(),
                WM_HOTKEY,
                WPARAM(id as usize),
                LPARAM(0),
            );
        }
        key.poll()
    }

    fn press(key: &mut PauseKey) {
        press_id(key, PAUSE_ID);
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
        assert!(
            key.poll().is_empty(),
            "an unrelated message was reported as a press",
        );
        assert!(!key.is_paused());
    }

    /// Both keys share one drain, and that is the point of them sharing a
    /// struct: a second handler calling `PeekMessage` would take these
    /// messages and drop them, so one key would work and the other would look
    /// registered and never fire.
    #[test]
    fn the_two_keys_do_not_take_each_others_messages() {
        let mut key = PauseKey::register();
        if !key.is_registered() || !key.ocr_is_registered() {
            println!("SKIPPED: one of the chords is held by another application");
            return;
        }

        assert_eq!(press_id(&mut key, OCR_ID), vec![Pressed::ReadWithOcr]);
        assert!(!key.is_paused(), "the OCR key must not pause anything");

        assert_eq!(press_id(&mut key, PAUSE_ID), vec![Pressed::PauseChanged]);
        assert!(key.is_paused());
    }

    /// One drain, two presses, both reported and in order.
    #[test]
    fn two_presses_in_one_drain_are_both_reported() {
        let mut key = PauseKey::register();
        if !key.is_registered() || !key.ocr_is_registered() {
            return;
        }
        // SAFETY: posts to this thread's own queue.
        unsafe {
            let thread = GetCurrentThreadId();
            let _ = PostThreadMessageW(thread, WM_HOTKEY, WPARAM(OCR_ID as usize), LPARAM(0));
            let _ = PostThreadMessageW(thread, WM_HOTKEY, WPARAM(PAUSE_ID as usize), LPARAM(0));
        }
        assert_eq!(
            key.poll(),
            vec![Pressed::ReadWithOcr, Pressed::PauseChanged],
        );
    }
}
