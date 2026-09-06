//! macOS lifecycle event observation.
//!
//! Observes NSWorkspace notifications for sleep/wake and display parameter
//! changes. The product loop polls for these events and refreshes state.

extern "C" {
    fn lens_lifecycle_start() -> bool;
    fn lens_lifecycle_stop();
    fn lens_lifecycle_poll_wake() -> bool;
    fn lens_lifecycle_poll_display_change() -> bool;
}

/// Lifecycle event observer. Start once, poll each tick, drop to stop.
pub struct Lifecycle {
    _private: (),
}

impl Lifecycle {
    /// Start observing system lifecycle events.
    pub fn start() -> Option<Self> {
        // SAFETY: lens_lifecycle_start registers NSWorkspace observers.
        let ok = unsafe { lens_lifecycle_start() };
        if ok {
            Some(Self { _private: () })
        } else {
            None
        }
    }

    /// Returns true if the system woke from sleep since the last poll.
    pub fn poll_wake(&self) -> bool {
        // SAFETY: atomic read.
        unsafe { lens_lifecycle_poll_wake() }
    }

    /// Returns true if the display configuration changed since the last poll.
    pub fn poll_display_change(&self) -> bool {
        // SAFETY: atomic read.
        unsafe { lens_lifecycle_poll_display_change() }
    }
}

impl Drop for Lifecycle {
    fn drop(&mut self) {
        // SAFETY: lens_lifecycle_stop removes observers.
        unsafe { lens_lifecycle_stop() };
    }
}

impl std::fmt::Debug for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lifecycle").finish()
    }
}
