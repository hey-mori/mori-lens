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
//!
//! # Configurable shortcuts (P8)
//!
//! Each action's shortcut is read from [`ShortcutConfig`]. An unbound action
//! (shortcut set to `None`) is not registered. The Pause action is
//! safety-critical: if its configured shortcut fails to register, the tray
//! tooltip tells the user.

use lens_core::shortcut::{Key, Modifiers, Shortcut, ShortcutAction, ShortcutConfig};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN, VK_DOWN, VK_ESCAPE, VK_F1, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_TAB,
    VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY};

/// Hotkey ids. Arbitrary; they only have to be unique within this thread.
const PAUSE_ID: i32 = 0x4C31;
const OCR_ID: i32 = 0x4C32;
const EXPLAIN_ID: i32 = 0x4C33;
const COPY_ID: i32 = 0x4C34;
const KEEP_ID: i32 = 0x4C35;
const PIN_ID: i32 = 0x4C36;

/// Map a [`ShortcutAction`] to its hotkey id.
fn action_id(action: ShortcutAction) -> i32 {
    match action {
        ShortcutAction::Pause => PAUSE_ID,
        ShortcutAction::ManualOcr => OCR_ID,
        ShortcutAction::Deep => EXPLAIN_ID,
        ShortcutAction::Copy => COPY_ID,
        ShortcutAction::Save => KEEP_ID,
        ShortcutAction::Pin => PIN_ID,
    }
}

/// Convert a semantic [`Modifiers`] set to Win32 hotkey modifiers.
fn win_modifiers(mods: Modifiers) -> HOT_KEY_MODIFIERS {
    let mut result = MOD_NOREPEAT.0;
    if mods.contains(Modifiers::CONTROL) {
        result |= MOD_CONTROL.0;
    }
    if mods.contains(Modifiers::ALT) {
        result |= MOD_ALT.0;
    }
    if mods.contains(Modifiers::SHIFT) {
        result |= MOD_SHIFT.0;
    }
    if mods.contains(Modifiers::META) {
        result |= MOD_WIN.0;
    }
    HOT_KEY_MODIFIERS(result)
}

/// Convert a semantic [`Key`] to a Win32 virtual key code.
///
/// Returns `None` for keys that cannot be registered as a hotkey (e.g.
/// unsupported platform keys).
fn win_vk(key: Key) -> Option<u32> {
    match key {
        Key::Letter(c) => Some(c.to_ascii_uppercase() as u32),
        Key::Digit(c) => Some(c as u32),
        Key::Function(n) => {
            let base = VK_F1.0 as u32;
            // F1..F12 are contiguous in the VK table
            Some(base + (n as u32 - 1))
        }
        Key::Space => Some(VK_SPACE.0 as u32),
        Key::Enter => Some(VK_RETURN.0 as u32),
        Key::Escape => Some(VK_ESCAPE.0 as u32),
        Key::Tab => Some(VK_TAB.0 as u32),
        Key::Arrow(lens_core::shortcut::ArrowKey::Up) => Some(VK_UP.0 as u32),
        Key::Arrow(lens_core::shortcut::ArrowKey::Down) => Some(VK_DOWN.0 as u32),
        Key::Arrow(lens_core::shortcut::ArrowKey::Left) => Some(VK_LEFT.0 as u32),
        Key::Arrow(lens_core::shortcut::ArrowKey::Right) => Some(VK_RIGHT.0 as u32),
    }
}

/// Try to register one shortcut for one action. Returns whether registration
/// succeeded.
unsafe fn register_one(action: ShortcutAction, shortcut: &Shortcut) -> bool {
    let Some(vk) = win_vk(shortcut.key) else {
        return false;
    };
    let id = action_id(action);
    let modifiers = win_modifiers(shortcut.modifiers);
    // SAFETY: registers a documented thread-local hotkey; `None` binds it
    // to this thread's message queue rather than to a window.
    unsafe { RegisterHotKey(None, id, modifiers, vk).is_ok() }
}

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
    /// ([ADR-0004](../../../docs/decisions/ADR-0004-native-overlay.md)) and a
    /// card that could be clicked would be a card that takes focus.
    Explain,
    /// Put what the card is showing on the clipboard.
    ///
    /// The card cannot be selected, because the overlay passes clicks through,
    /// so without this there is no way at all to keep an answer.
    Copy,
    /// Write what the card is showing into the saved file.
    ///
    /// The durable version of [`Pressed::Copy`]. A clipboard holds one thing
    /// until the next thing replaces it, and a reader who looked something up
    /// twice has learnt nothing the second time.
    Keep,
    /// Pin what the card is showing into a window that stays.
    ///
    /// A card goes when the pointer does
    /// ([ADR-0009](../../../docs/decisions/ADR-0009-pointer-distance-gate.md)),
    /// which is right for a tooltip and is why a reader who looks away and comes
    /// back has to ask for the explanation again. A session log on 2026-08-18
    /// shows one sentence producing eight cards for that reason.
    Pin,
}

/// Map a hotkey id to its [`Pressed`] variant.
fn id_to_pressed(id: i32) -> Option<Pressed> {
    match id {
        PAUSE_ID => Some(Pressed::PauseChanged),
        OCR_ID => Some(Pressed::ReadWithOcr),
        EXPLAIN_ID => Some(Pressed::Explain),
        COPY_ID => Some(Pressed::Copy),
        KEEP_ID => Some(Pressed::Keep),
        PIN_ID => Some(Pressed::Pin),
        _ => None,
    }
}

/// Registration state for one hotkey slot.
#[derive(Debug)]
struct Slot {
    action: ShortcutAction,
    registered: bool,
}

/// Registers global keys and reports presses.
///
/// **One struct because there is one message queue.** `poll` drains this
/// thread's messages, and a second handler doing the same would take the
/// first one's messages and drop them: whichever ran first would work and the
/// other would appear registered and never fire. That is a bug with no symptom
/// except a key that does nothing.
#[derive(Debug)]
pub struct PauseKey {
    slots: Vec<Slot>,
    paused: bool,
    shortcuts: ShortcutConfig,
}

impl PauseKey {
    /// Register with the platform defaults (Ctrl+Alt+letter for each action).
    ///
    /// Failure is not fatal: another application may already own a chord, and
    /// a Lens without a pause key is worse than a Lens that refuses to start,
    /// so the caller is told and carries on.
    #[allow(dead_code)]
    pub fn register() -> Self {
        Self::register_with(&lens_core::shortcut::default_windows_shortcuts())
    }

    /// Register using a configurable shortcut configuration.
    ///
    /// Actions whose binding is `None` are not registered. Actions whose
    /// binding is `Some(sc)` are registered with the configured modifiers and
    /// key. If registration fails (another application holds the chord), the
    /// slot is marked unregistered so the tray tooltip can report it.
    pub fn register_with(config: &ShortcutConfig) -> Self {
        let mut slots = Vec::new();
        for (action, shortcut) in &config.bindings {
            let registered = if let Some(sc) = shortcut {
                // SAFETY: registers a documented thread-local hotkey.
                unsafe { register_one(*action, sc) }
            } else {
                false
            };
            slots.push(Slot {
                action: *action,
                registered,
            });
        }
        Self {
            slots,
            paused: false,
            shortcuts: config.clone(),
        }
    }

    /// Access the configured shortcuts.
    pub fn shortcuts(&self) -> &ShortcutConfig {
        &self.shortcuts
    }

    fn slot_for(&self, action: ShortcutAction) -> Option<&Slot> {
        self.slots.iter().find(|s| s.action == action)
    }

    /// Whether the ManualOcr shortcut was taken by something else.
    pub fn ocr_is_registered(&self) -> bool {
        self.slot_for(ShortcutAction::ManualOcr)
            .is_some_and(|s| s.registered)
    }

    /// Whether the Deep/Explain shortcut was taken by something else.
    pub fn explain_is_registered(&self) -> bool {
        self.slot_for(ShortcutAction::Deep)
            .is_some_and(|s| s.registered)
    }

    /// Whether the Copy shortcut was taken by something else.
    pub fn copy_is_registered(&self) -> bool {
        self.slot_for(ShortcutAction::Copy)
            .is_some_and(|s| s.registered)
    }

    /// Whether the Save/Keep shortcut was taken by something else.
    pub fn keep_is_registered(&self) -> bool {
        self.slot_for(ShortcutAction::Save)
            .is_some_and(|s| s.registered)
    }

    /// Whether the Pin shortcut was taken by something else.
    pub fn pin_is_registered(&self) -> bool {
        self.slot_for(ShortcutAction::Pin)
            .is_some_and(|s| s.registered)
    }

    /// Whether the Pause shortcut was registered.
    pub fn is_registered(&self) -> bool {
        self.slot_for(ShortcutAction::Pause)
            .is_some_and(|s| s.registered)
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
        if !self.slots.iter().any(|s| s.registered) {
            return pressed;
        }
        let mut message = MSG::default();
        // SAFETY: reads and removes messages from this thread's own queue.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message != WM_HOTKEY {
                continue;
            }
            let id = message.wParam.0 as i32;
            if let Some(p) = id_to_pressed(id) {
                if let Pressed::PauseChanged = p {
                    self.paused = !self.paused;
                }
                pressed.push(p);
            }
        }
        pressed
    }
}

impl Drop for PauseKey {
    fn drop(&mut self) {
        // Each slot unregistered by its own flag. A machine where another
        // application held a chord leaves the other slots registered on the
        // way out.
        for slot in &self.slots {
            if slot.registered {
                let id = action_id(slot.action);
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

    #[test]
    fn register_with_unbound_action_is_not_registered() {
        let config = ShortcutConfig::new(vec![
            (
                ShortcutAction::Pause,
                Some(Shortcut::parse("Ctrl+Alt+L").unwrap()),
            ),
            (ShortcutAction::ManualOcr, None),
        ]);
        let key = PauseKey::register_with(&config);
        assert!(key.is_registered());
        assert!(!key.ocr_is_registered());
    }

    #[test]
    fn win_modifiers_converts_correctly() {
        let mods = Modifiers::CONTROL.union(Modifiers::ALT);
        let win = win_modifiers(mods);
        assert_eq!(win.0, MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0);
    }

    #[test]
    fn win_vk_maps_letters() {
        assert_eq!(win_vk(Key::Letter('A')), Some(0x41));
        assert_eq!(win_vk(Key::Letter('L')), Some(0x4C));
        assert_eq!(win_vk(Key::Letter('z')), Some(0x5A));
    }
}
