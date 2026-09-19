//! macOS global hotkeys via CGEventTap.
//!
//! Intercepts configured key combinations globally and maps them to product
//! commands. Requires Accessibility permission (already requested for AX).
//!
//! Commands are queued in a lock-free ring buffer on the native side and
//! polled by the Rust product loop.
//!
//! # Configuration
//!
//! Shortcut bindings are configurable: call [`configure`] with a
//! [`ShortcutConfig`] from `lens_core::shortcut` to install the mapping.
//! The native layer translates semantic shortcuts into macOS modifier flags
//! and keycodes, then stores them in an atomic binding table that the
//! event tap callback reads.

use std::os::raw::c_int;

use lens_core::shortcut::{Key, Modifiers, Shortcut, ShortcutAction, ShortcutConfig};

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

impl ProductCommand {
    /// Whether this command should execute even when the product is paused.
    ///
    /// Only Quit and PauseToggle work while paused — matching Windows behavior
    /// where every other command is gated with `!pause.is_paused()`.
    pub const fn works_when_paused(self) -> bool {
        matches!(self, Self::Quit | Self::PauseToggle)
    }
}

/// Native hotkey binding: (modifiers, keycode) → command.
/// Matches the C struct `LensHotkeyBinding` in hotkey.h.
#[repr(C)]
struct NativeBinding {
    modifiers: u32,
    keycode: u16,
    command: c_int,
}

// macOS CGEventFlags for modifiers.
const K_CG_EVENT_FLAG_MASK_SHIFT: u32 = 0x00020000;
const K_CG_EVENT_FLAG_MASK_CONTROL: u32 = 0x00040000;
const K_CG_EVENT_FLAG_MASK_ALTERNATE: u32 = 0x00080000;
const K_CG_EVENT_FLAG_MASK_COMMAND: u32 = 0x00100000;

// macOS virtual keycodes (from Carbon/HIToolbox).
const VK_A: u16 = 0x00;
const VK_S: u16 = 0x01;
const VK_D: u16 = 0x02;
const VK_F: u16 = 0x03;
const VK_H: u16 = 0x04;
const VK_G: u16 = 0x05;
const VK_Z: u16 = 0x06;
const VK_X: u16 = 0x07;
const VK_C: u16 = 0x08;
const VK_V: u16 = 0x09;
const VK_B: u16 = 0x0B;
const VK_Q: u16 = 0x0C;
const VK_W: u16 = 0x0D;
const VK_E: u16 = 0x0E;
const VK_R: u16 = 0x0F;
const VK_Y: u16 = 0x10;
const VK_T: u16 = 0x11;
const VK_1: u16 = 0x12;
const VK_2: u16 = 0x13;
const VK_3: u16 = 0x14;
const VK_4: u16 = 0x15;
const VK_6: u16 = 0x16;
const VK_5: u16 = 0x17;
const VK_9: u16 = 0x19;
const VK_7: u16 = 0x1A;
const VK_8: u16 = 0x1C;
const VK_0: u16 = 0x1D;
const VK_O: u16 = 0x1F;
const VK_U: u16 = 0x20;
const VK_I: u16 = 0x22;
const VK_P: u16 = 0x23;
const VK_L: u16 = 0x25;
const VK_J: u16 = 0x26;
const VK_K: u16 = 0x28;
const VK_SPACE: u16 = 0x31;
const VK_RETURN: u16 = 0x24;
const VK_ESCAPE: u16 = 0x35;
const VK_TAB: u16 = 0x30;
const VK_F1: u16 = 0x7A;
const VK_UP: u16 = 0x7E;
const VK_DOWN: u16 = 0x7D;
const VK_LEFT: u16 = 0x7B;
const VK_RIGHT: u16 = 0x7C;

/// Map a semantic Key to a macOS virtual keycode.
/// Returns None if the key is not supported on macOS.
fn macos_keycode(key: Key) -> Option<u16> {
    match key {
        Key::Letter(c) => match c.to_ascii_uppercase() {
            'A' => Some(VK_A),
            'B' => Some(VK_B),
            'C' => Some(VK_C),
            'D' => Some(VK_D),
            'E' => Some(VK_E),
            'F' => Some(VK_F),
            'G' => Some(VK_G),
            'H' => Some(VK_H),
            'I' => Some(VK_I),
            'J' => Some(VK_J),
            'K' => Some(VK_K),
            'L' => Some(VK_L),
            'O' => Some(VK_O),
            'P' => Some(VK_P),
            'Q' => Some(VK_Q),
            'R' => Some(VK_R),
            'S' => Some(VK_S),
            'T' => Some(VK_T),
            'U' => Some(VK_U),
            'V' => Some(VK_V),
            'W' => Some(VK_W),
            'X' => Some(VK_X),
            'Y' => Some(VK_Y),
            'Z' => Some(VK_Z),
            _ => None,
        },
        Key::Digit(c) => match c {
            '0' => Some(VK_0),
            '1' => Some(VK_1),
            '2' => Some(VK_2),
            '3' => Some(VK_3),
            '4' => Some(VK_4),
            '5' => Some(VK_5),
            '6' => Some(VK_6),
            '7' => Some(VK_7),
            '8' => Some(VK_8),
            '9' => Some(VK_9),
            _ => None,
        },
        Key::Function(n) => {
            // F1-F12 have non-contiguous keycodes on macOS.
            match n {
                1 => Some(VK_F1),
                // F2-F12 not commonly used for shortcuts; extend if needed.
                _ => None,
            }
        }
        Key::Space => Some(VK_SPACE),
        Key::Enter => Some(VK_RETURN),
        Key::Escape => Some(VK_ESCAPE),
        Key::Tab => Some(VK_TAB),
        Key::Arrow(arrow) => match arrow {
            lens_core::shortcut::ArrowKey::Up => Some(VK_UP),
            lens_core::shortcut::ArrowKey::Down => Some(VK_DOWN),
            lens_core::shortcut::ArrowKey::Left => Some(VK_LEFT),
            lens_core::shortcut::ArrowKey::Right => Some(VK_RIGHT),
        },
    }
}

/// Map semantic Modifiers to macOS CGEventFlags.
fn macos_modifiers(mods: Modifiers) -> u32 {
    let mut flags = 0u32;
    if mods.contains(Modifiers::CONTROL) {
        flags |= K_CG_EVENT_FLAG_MASK_CONTROL;
    }
    if mods.contains(Modifiers::ALT) {
        flags |= K_CG_EVENT_FLAG_MASK_ALTERNATE;
    }
    if mods.contains(Modifiers::SHIFT) {
        flags |= K_CG_EVENT_FLAG_MASK_SHIFT;
    }
    if mods.contains(Modifiers::META) {
        flags |= K_CG_EVENT_FLAG_MASK_COMMAND;
    }
    flags
}

/// Map a ShortcutAction to the native LensCommand value.
/// Returns None if the action is not supported on macOS (e.g., Pin).
fn native_command(action: ShortcutAction) -> Option<c_int> {
    match action {
        ShortcutAction::Pause => Some(1),     // LENS_CMD_PAUSE_TOGGLE
        ShortcutAction::Deep => Some(2),      // LENS_CMD_DEEP
        ShortcutAction::Copy => Some(3),      // LENS_CMD_COPY
        ShortcutAction::Save => Some(4),      // LENS_CMD_SAVE
        ShortcutAction::ManualOcr => Some(5), // LENS_CMD_MANUAL_OCR
        ShortcutAction::Pin => None,          // Not supported on macOS
    }
}

extern "C" {
    fn lens_hotkey_install() -> bool;
    fn lens_hotkey_remove();
    fn lens_hotkey_configure(bindings: *const NativeBinding, count: c_int);
    fn lens_hotkey_poll() -> c_int;
    fn lens_hotkey_is_enabled() -> bool;
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

/// Configure the hotkey bindings from a ShortcutConfig.
///
/// Translates semantic shortcuts into native modifier flags and keycodes,
/// then installs the mapping table in the native layer. Actions that are
/// unbound or not supported on macOS (e.g., Pin) are skipped.
///
/// This function is safe to call while the event tap is active. The native
/// layer swaps the binding table atomically.
pub fn configure(config: &ShortcutConfig) {
    let mut native_bindings = Vec::new();

    for (action, shortcut) in &config.bindings {
        let cmd = match native_command(*action) {
            Some(c) => c,
            None => continue, // Action not supported on macOS
        };

        let shortcut = match shortcut {
            Some(s) => s,
            None => continue, // Action is unbound
        };

        let keycode = match macos_keycode(shortcut.key) {
            Some(k) => k,
            None => continue, // Key not supported on macOS
        };

        native_bindings.push(NativeBinding {
            modifiers: macos_modifiers(shortcut.modifiers),
            keycode,
            command: cmd,
        });
    }

    // SAFETY: lens_hotkey_configure reads from the bindings array and copies
    // it into native storage. The array is valid for the duration of the call.
    unsafe {
        lens_hotkey_configure(native_bindings.as_ptr(), native_bindings.len() as c_int);
    }
}

/// Poll for the next pending command. Returns `ProductCommand::None` if empty.
pub fn poll() -> ProductCommand {
    // SAFETY: lens_hotkey_poll reads from the lock-free ring buffer.
    let raw = unsafe { lens_hotkey_poll() };
    ProductCommand::from(raw)
}

/// Whether the event tap is currently enabled.
///
/// Returns `false` if the tap is not installed or has been disabled by macOS
/// (e.g. due to callback timeout). The product loop can use this for
/// diagnostics.
pub fn is_enabled() -> bool {
    // SAFETY: lens_hotkey_is_enabled queries the CGEventTap state.
    unsafe { lens_hotkey_is_enabled() }
}

#[cfg(test)]
mod tests {
    use super::ProductCommand;

    /// Simulate the native hotkey queue algorithm to verify correctness.
    /// This mirrors the logic in hotkey.m exactly:
    /// - Ring buffer with drop-newest overflow (true SPSC).
    /// - Quit uses a bool flag (idempotent).
    /// - Pause uses a saturating unsigned counter (max 1024).
    const PAUSE_PENDING_MAX: u32 = 1024;

    struct SimQueue {
        queue: [i32; 64],
        head: usize,
        tail: usize,
        quit_flag: bool,
        pause_count: u32,
    }

    impl SimQueue {
        fn new() -> Self {
            Self {
                queue: [0; 64],
                head: 0,
                tail: 0,
                quit_flag: false,
                pause_count: 0,
            }
        }

        fn enqueue(&mut self, cmd: i32) {
            // Critical commands use dedicated atomics — never dropped.
            if cmd == 6 {
                // LENS_CMD_QUIT
                self.quit_flag = true;
                return;
            }
            if cmd == 3 {
                // LENS_CMD_PAUSE_TOGGLE — saturating increment.
                if self.pause_count < PAUSE_PENDING_MAX {
                    self.pause_count += 1;
                }
                return;
            }

            let next_head = (self.head + 1) % 64;
            if next_head == self.tail {
                // Queue full — drop the NEWEST command (true SPSC).
                return;
            }
            self.queue[self.head] = cmd;
            self.head = next_head;
        }

        fn poll(&mut self) -> i32 {
            // Critical commands have priority.
            if self.quit_flag {
                self.quit_flag = false;
                return 6;
            }
            // Pause counter: each poll returns one toggle and decrements.
            if self.pause_count > 0 {
                self.pause_count -= 1;
                return 3;
            }
            if self.tail == self.head {
                return 0; // None
            }
            let cmd = self.queue[self.tail];
            self.tail = (self.tail + 1) % 64;
            cmd
        }

        fn is_empty(&self) -> bool {
            self.head == self.tail && !self.quit_flag && self.pause_count == 0
        }
    }

    #[test]
    fn empty_queue_returns_none() {
        let mut q = SimQueue::new();
        assert_eq!(q.poll(), 0);
    }

    #[test]
    fn enqueue_and_poll_single() {
        let mut q = SimQueue::new();
        q.enqueue(1); // Deep (non-critical)
        assert_eq!(q.poll(), 1);
        assert!(q.is_empty());
    }

    #[test]
    fn fifo_order_preserved() {
        let mut q = SimQueue::new();
        q.enqueue(1);
        q.enqueue(2);
        q.enqueue(4);
        assert_eq!(q.poll(), 1);
        assert_eq!(q.poll(), 2);
        assert_eq!(q.poll(), 4);
        assert!(q.is_empty());
    }

    #[test]
    fn wraparound_works() {
        let mut q = SimQueue::new();
        // Fill to 63 (capacity - 1), avoiding critical values 3 and 6.
        let cmds: Vec<i32> = (1..=100).filter(|&c| c != 3 && c != 6).take(63).collect();
        for &c in &cmds {
            q.enqueue(c);
        }
        for &c in &cmds {
            assert_eq!(q.poll(), c);
        }
        assert!(q.is_empty());
        // Now enqueue again — head/tail have wrapped.
        q.enqueue(100);
        q.enqueue(200);
        assert_eq!(q.poll(), 100);
        assert_eq!(q.poll(), 200);
    }

    #[test]
    fn overflow_drops_newest() {
        let mut q = SimQueue::new();
        // Fill to capacity (63 items), avoiding critical values 3 and 6.
        let cmds: Vec<i32> = (1..=100).filter(|&c| c != 3 && c != 6).take(63).collect();
        for &c in &cmds {
            q.enqueue(c);
        }
        // One more non-critical — should be DROPPED (not evict oldest).
        q.enqueue(64);
        // First polled item should still be cmds[0] (nothing evicted).
        assert_eq!(q.poll(), cmds[0]);
        assert_eq!(q.poll(), cmds[1]);
        // Drain remaining.
        for _ in 0..61 {
            q.poll();
        }
        assert!(q.is_empty());
        // 64 was dropped, not queued.
    }

    #[test]
    fn critical_quit_survives_storm() {
        let mut q = SimQueue::new();
        // Fill the queue with Deep commands.
        for _ in 0..100 {
            q.enqueue(2); // Deep
        }
        // Quit arrives — uses the flag, bypasses the ring buffer.
        q.enqueue(6); // Quit
                      // Quit has priority in poll.
        assert_eq!(q.poll(), 6, "Quit must be returned immediately via flag");
    }

    #[test]
    fn critical_pause_survives_storm() {
        let mut q = SimQueue::new();
        for _ in 0..100 {
            q.enqueue(2);
        }
        q.enqueue(3); // Pause
        assert_eq!(
            q.poll(),
            3,
            "Pause must be returned immediately via counter"
        );
    }

    #[test]
    fn pause_multiplicity_preserved() {
        let mut q = SimQueue::new();
        // Two rapid pause presses before any poll.
        q.enqueue(3);
        q.enqueue(3);
        // Both must produce separate toggle events.
        assert_eq!(q.poll(), 3, "first pause toggle");
        assert_eq!(q.poll(), 3, "second pause toggle");
        assert_eq!(q.poll(), 0, "no more pauses");
    }

    #[test]
    fn pause_three_presses() {
        let mut q = SimQueue::new();
        q.enqueue(3);
        q.enqueue(3);
        q.enqueue(3);
        assert_eq!(q.poll(), 3);
        assert_eq!(q.poll(), 3);
        assert_eq!(q.poll(), 3);
        assert_eq!(q.poll(), 0);
    }

    #[test]
    fn critical_flag_cleared_after_poll() {
        let mut q = SimQueue::new();
        q.enqueue(6); // Quit
        assert_eq!(q.poll(), 6);
        // Flag is cleared — second poll returns None (or next queued item).
        assert_eq!(q.poll(), 0);
    }

    #[test]
    fn ten_thousand_events_do_not_corrupt() {
        let mut q = SimQueue::new();
        let mut produced = 0u64;
        let mut consumed = 0u64;
        for i in 0..10_000 {
            q.enqueue((i % 8) + 1);
            produced += 1;
            // Poll roughly half the time.
            if i % 2 == 0 {
                let cmd = q.poll();
                if cmd != 0 {
                    consumed += 1;
                }
            }
        }
        // Drain remaining.
        loop {
            let cmd = q.poll();
            if cmd == 0 {
                break;
            }
            consumed += 1;
        }
        // produced >= consumed (some may have been dropped on overflow).
        assert!(consumed <= produced);
        assert!(q.is_empty());
    }

    #[test]
    fn pause_saturates_at_max() {
        let mut q = SimQueue::new();
        // Enqueue far more than PAUSE_PENDING_MAX.
        for _ in 0..2000 {
            q.enqueue(3);
        }
        // Count how many toggles we can poll.
        let mut count = 0;
        loop {
            let cmd = q.poll();
            if cmd == 0 {
                break;
            }
            assert_eq!(cmd, 3);
            count += 1;
        }
        assert_eq!(
            count, PAUSE_PENDING_MAX as i32,
            "pause must saturate at max"
        );
    }

    #[test]
    fn pause_priority_over_queued_commands() {
        let mut q = SimQueue::new();
        // Enqueue some normal commands first.
        q.enqueue(2); // Deep
        q.enqueue(4); // Copy
                      // Then a Pause.
        q.enqueue(3);
        // Pause must be returned before any queued command.
        assert_eq!(q.poll(), 3, "Pause has priority over queued commands");
        assert_eq!(q.poll(), 2, "then queued commands in FIFO order");
        assert_eq!(q.poll(), 4);
    }

    #[test]
    fn quit_priority_over_everything() {
        let mut q = SimQueue::new();
        q.enqueue(2); // Deep
        q.enqueue(3); // Pause
        q.enqueue(4); // Copy
        q.enqueue(6); // Quit
                      // Quit must be first.
        assert_eq!(q.poll(), 6, "Quit has highest priority");
        // Then Pause (also critical).
        assert_eq!(q.poll(), 3, "Pause before queued commands");
        // Then queued commands in FIFO.
        assert_eq!(q.poll(), 2);
        assert_eq!(q.poll(), 4);
    }

    #[test]
    fn quit_under_full_queue() {
        let mut q = SimQueue::new();
        // Fill the ring buffer with non-critical commands.
        let cmds: Vec<i32> = (1..=100).filter(|&c| c != 3 && c != 6).collect();
        for &c in &cmds {
            q.enqueue(c);
        }
        // Quit must still be delivered.
        q.enqueue(6);
        assert_eq!(q.poll(), 6, "Quit survives full queue");
    }

    #[test]
    fn pause_deep_pause_ordering() {
        let mut q = SimQueue::new();
        q.enqueue(3); // Pause
        q.enqueue(2); // Deep
        q.enqueue(3); // Pause
                      // Both Pauses come before Deep (critical priority).
        assert_eq!(q.poll(), 3);
        assert_eq!(q.poll(), 3);
        assert_eq!(q.poll(), 2);
        assert!(q.is_empty());
    }

    /// Pause gate policy: only Quit and PauseToggle work while paused.
    /// This matches Windows behavior where every command is gated with
    /// `!pause.is_paused()` except quit/unpause.
    #[test]
    fn pause_gate_allows_only_quit_and_pause_toggle() {
        assert!(ProductCommand::Quit.works_when_paused());
        assert!(ProductCommand::PauseToggle.works_when_paused());

        assert!(!ProductCommand::Deep.works_when_paused());
        assert!(!ProductCommand::Copy.works_when_paused());
        assert!(!ProductCommand::Save.works_when_paused());
        assert!(!ProductCommand::ManualOcr.works_when_paused());
        assert!(!ProductCommand::OpenSettings.works_when_paused());
        assert!(!ProductCommand::OpenLibrary.works_when_paused());
        assert!(!ProductCommand::None.works_when_paused());
    }

    #[test]
    fn macos_keycode_maps_letters() {
        use super::{macos_keycode, Key};
        assert_eq!(macos_keycode(Key::Letter('L')), Some(0x25));
        assert_eq!(macos_keycode(Key::Letter('O')), Some(0x1F));
        assert_eq!(macos_keycode(Key::Letter('D')), Some(0x02));
        assert_eq!(macos_keycode(Key::Letter('C')), Some(0x08));
        assert_eq!(macos_keycode(Key::Letter('S')), Some(0x01));
        assert_eq!(macos_keycode(Key::Letter('A')), Some(0x00));
    }

    #[test]
    fn macos_keycode_maps_special_keys() {
        use super::{macos_keycode, Key};
        assert_eq!(macos_keycode(Key::Space), Some(0x31));
        assert_eq!(macos_keycode(Key::Enter), Some(0x24));
        assert_eq!(macos_keycode(Key::Escape), Some(0x35));
        assert_eq!(macos_keycode(Key::Tab), Some(0x30));
    }

    #[test]
    fn macos_modifiers_maps_correctly() {
        use super::{macos_modifiers, Modifiers};
        // Cmd+Opt (default macOS modifier)
        let mods = Modifiers::META.union(Modifiers::ALT);
        let flags = macos_modifiers(mods);
        assert_eq!(flags, 0x00100000 | 0x00080000); // Command | Alternate
    }

    #[test]
    fn native_command_excludes_pin() {
        use super::{native_command, ShortcutAction};
        assert!(native_command(ShortcutAction::Pause).is_some());
        assert!(native_command(ShortcutAction::ManualOcr).is_some());
        assert!(native_command(ShortcutAction::Deep).is_some());
        assert!(native_command(ShortcutAction::Copy).is_some());
        assert!(native_command(ShortcutAction::Save).is_some());
        // Pin is not supported on macOS
        assert!(native_command(ShortcutAction::Pin).is_none());
    }
}
