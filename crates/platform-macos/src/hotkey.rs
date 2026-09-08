//! macOS global hotkeys via CGEventTap.
//!
//! Intercepts Cmd+Opt+key combinations globally and maps them to product
//! commands. Requires Accessibility permission (already requested for AX).
//!
//! Commands are queued in a lock-free ring buffer on the native side and
//! polled by the Rust product loop.

use std::os::raw::c_int;

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

extern "C" {
    fn lens_hotkey_install() -> bool;
    fn lens_hotkey_remove();
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
}
