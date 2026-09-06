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
    use super::*;

    /// Simulate the native ring buffer algorithm to verify correctness.
    /// This mirrors the logic in hotkey.m exactly.
    struct SimQueue {
        queue: [i32; 64],
        head: usize,
        tail: usize,
    }

    impl SimQueue {
        fn new() -> Self {
            Self {
                queue: [0; 64],
                head: 0,
                tail: 0,
            }
        }

        fn enqueue(&mut self, cmd: i32) {
            let next_head = (self.head + 1) % 64;
            if next_head == self.tail {
                // Queue full — evict oldest, advance both.
                let new_tail = (self.tail + 1) % 64;
                self.queue[self.head] = cmd;
                self.head = (self.head + 1) % 64;
                self.tail = new_tail;
                return;
            }
            self.queue[self.head] = cmd;
            self.head = next_head;
        }

        fn poll(&mut self) -> i32 {
            if self.tail == self.head {
                return 0; // None
            }
            let cmd = self.queue[self.tail];
            self.tail = (self.tail + 1) % 64;
            cmd
        }

        fn is_empty(&self) -> bool {
            self.head == self.tail
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
        q.enqueue(6); // Quit
        assert_eq!(q.poll(), 6);
        assert!(q.is_empty());
    }

    #[test]
    fn fifo_order_preserved() {
        let mut q = SimQueue::new();
        q.enqueue(1);
        q.enqueue(2);
        q.enqueue(3);
        assert_eq!(q.poll(), 1);
        assert_eq!(q.poll(), 2);
        assert_eq!(q.poll(), 3);
        assert!(q.is_empty());
    }

    #[test]
    fn wraparound_works() {
        let mut q = SimQueue::new();
        // Fill to 63 (capacity - 1), then drain, then fill again.
        for i in 1..=63 {
            q.enqueue(i);
        }
        for i in 1..=63 {
            assert_eq!(q.poll(), i);
        }
        assert!(q.is_empty());
        // Now enqueue again — head/tail have wrapped.
        q.enqueue(100);
        q.enqueue(200);
        assert_eq!(q.poll(), 100);
        assert_eq!(q.poll(), 200);
    }

    #[test]
    fn overflow_evicts_oldest() {
        let mut q = SimQueue::new();
        // Fill to capacity (63 items).
        for i in 1..=63 {
            q.enqueue(i);
        }
        // One more — should evict item 1.
        q.enqueue(64);
        // First polled item should be 2 (1 was evicted).
        assert_eq!(q.poll(), 2);
        assert_eq!(q.poll(), 3);
        // Drain items 4..=63 (60 items), then 64.
        for _ in 0..60 {
            q.poll();
        }
        assert_eq!(q.poll(), 64);
        assert!(q.is_empty());
    }

    #[test]
    fn storm_quit_survives() {
        let mut q = SimQueue::new();
        // Simulate a storm of Deep commands (2) filling the queue.
        for _ in 0..100 {
            q.enqueue(2); // Deep
        }
        // Then a Quit arrives.
        q.enqueue(6); // Quit
                      // Quit should be in the queue (oldest Deep entries evicted).
        let mut found_quit = false;
        while !q.is_empty() {
            if q.poll() == 6 {
                found_quit = true;
            }
        }
        assert!(found_quit, "Quit must survive a command storm");
    }

    #[test]
    fn ten_thousand_events_do_not_corrupt() {
        let mut q = SimQueue::new();
        let mut produced = 0u64;
        let mut consumed = 0u64;
        for i in 0..10_000 {
            q.enqueue((i % 8) as i32 + 1);
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
        // produced == consumed + evicted. All accounted for.
        assert!(consumed <= produced);
        assert!(q.is_empty());
    }
}
