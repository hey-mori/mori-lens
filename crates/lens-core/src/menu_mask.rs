//! Keep the reading modifier from opening the application's menu.
//!
//! On Windows, pressing and releasing Alt on its own is a command: the window
//! under it moves keyboard focus to its menu bar, and newer applications draw
//! key tips over every control. Lens asks the reader to hold exactly that key
//! while they read. Without this, every card ends with the application they
//! were reading switching into menu mode the moment they let go, and the next
//! thing they type goes to a menu instead of the document.
//!
//! The system only treats a release as a menu command when no other key went
//! down while Alt was held. So once Lens has acted on a press, the adapter
//! sends one key that no keyboard produces and no application binds, and the
//! release that follows is an ordinary one. This is the technique AutoHotkey
//! calls its menu mask key.
//!
//! # Only when Lens acted
//!
//! A reader who taps Alt to reach a menu must still reach it. The mask is sent
//! only after a hover settled while the modifier was down, which a quick tap
//! never produces, and at most once per press.

/// Tracks one press of the reading modifier.
#[derive(Debug, Default)]
pub struct MenuMask {
    masked: bool,
}

impl MenuMask {
    pub fn new() -> Self {
        Self::default()
    }

    /// Report the modifier's state on every tick. A release ends the press, so
    /// the next press may be masked again.
    pub fn modifier(&mut self, held: bool) {
        if !held {
            self.masked = false;
        }
    }

    /// Lens acted on the pointer. Returns `true` when the caller should send
    /// the mask key now: the modifier is still down and this press has not
    /// been masked yet.
    pub fn lens_acted(&mut self, modifier_held: bool) -> bool {
        if modifier_held && !self.masked {
            self.masked = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case the module exists for: hold, read, release.
    #[test]
    fn a_press_lens_acted_on_is_masked_once() {
        let mut mask = MenuMask::new();
        mask.modifier(true);
        assert!(mask.lens_acted(true));
        // A second card in the same press needs nothing more: one other key
        // is enough to turn the release into an ordinary one.
        assert!(!mask.lens_acted(true));
    }

    /// A tap to reach the menu is left alone, because nothing settled.
    #[test]
    fn a_press_lens_did_not_act_on_is_not_masked() {
        let mut mask = MenuMask::new();
        mask.modifier(true);
        mask.modifier(false);
        assert!(!mask.masked);
    }

    /// An answer that arrives after the release is too late to mask and must
    /// not send a key into whatever the reader is doing now.
    #[test]
    fn nothing_is_sent_once_the_modifier_is_up() {
        let mut mask = MenuMask::new();
        mask.modifier(false);
        assert!(!mask.lens_acted(false));
    }

    /// Each press is its own.
    #[test]
    fn the_next_press_is_masked_again() {
        let mut mask = MenuMask::new();
        mask.modifier(true);
        assert!(mask.lens_acted(true));
        mask.modifier(false);
        mask.modifier(true);
        assert!(mask.lens_acted(true));
    }
}
