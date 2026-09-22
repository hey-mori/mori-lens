// Regression test for P19B managed-state correctness fix.
//
// Verifies that the three semantic states (pause, quit, health check running)
// have distinct TypeIds, preventing Tauri state registry collisions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Newtype for pause request with focused API.
#[derive(Clone, Debug)]
pub struct PauseRequest(Arc<AtomicBool>);

impl PauseRequest {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

impl Default for PauseRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Newtype for quit request with focused API.
#[derive(Clone, Debug)]
pub struct QuitRequest(Arc<AtomicBool>);

impl QuitRequest {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

impl Default for QuitRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Newtype for health check running guard with RAII semantics.
#[derive(Clone, Debug)]
pub struct HealthCheckRunning(Arc<AtomicBool>);

impl HealthCheckRunning {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn try_acquire(&self) -> Option<HealthCheckGuard> {
        if self.0.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            Some(HealthCheckGuard(self.0.clone()))
        } else {
            None
        }
    }
}

impl Default for HealthCheckRunning {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for health check execution.
#[derive(Debug)]
pub struct HealthCheckGuard(Arc<AtomicBool>);

impl Drop for HealthCheckGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[test]
    fn test_distinct_type_ids() {
        // Regression test: these must have distinct TypeIds to avoid Tauri state collisions.
        assert_ne!(
            TypeId::of::<PauseRequest>(),
            TypeId::of::<QuitRequest>(),
            "PauseRequest and QuitRequest must have distinct TypeIds"
        );

        assert_ne!(
            TypeId::of::<PauseRequest>(),
            TypeId::of::<HealthCheckRunning>(),
            "PauseRequest and HealthCheckRunning must have distinct TypeIds"
        );

        assert_ne!(
            TypeId::of::<QuitRequest>(),
            TypeId::of::<HealthCheckRunning>(),
            "QuitRequest and HealthCheckRunning must have distinct TypeIds"
        );
    }

    #[test]
    fn test_pause_request_behavior() {
        let pause = PauseRequest::new();

        // Initially false
        assert!(!pause.take(), "pause should start false");

        // Request sets to true
        pause.request();
        assert!(pause.take(), "take after request should return true");

        // Take resets to false
        assert!(!pause.take(), "second take should return false");

        // Multiple requests
        pause.request();
        pause.request();
        assert!(pause.take(), "take should return true");
        assert!(!pause.take(), "take should reset to false");
    }

    #[test]
    fn test_quit_request_behavior() {
        let quit = QuitRequest::new();

        // Initially false
        assert!(!quit.take(), "quit should start false");

        // Request sets to true
        quit.request();
        assert!(quit.take(), "take after request should return true");

        // Take resets to false
        assert!(!quit.take(), "second take should return false");
    }

    #[test]
    fn test_health_check_guard_behavior() {
        let health = HealthCheckRunning::new();

        // First acquire succeeds
        let guard1 = health.try_acquire();
        assert!(guard1.is_some(), "first acquire should succeed");

        // Second acquire fails while guard is held
        let guard2 = health.try_acquire();
        assert!(guard2.is_none(), "second acquire should fail while guard held");

        // Drop first guard
        drop(guard1);

        // Third acquire succeeds after drop
        let guard3 = health.try_acquire();
        assert!(guard3.is_some(), "acquire after drop should succeed");

        drop(guard3);
    }

    #[test]
    fn test_health_check_guard_drop_releases() {
        let health = HealthCheckRunning::new();

        {
            let _guard = health.try_acquire().expect("acquire should succeed");
            // Guard is dropped at end of scope
        }

        // Should be able to acquire again after scope exit
        let guard = health.try_acquire();
        assert!(guard.is_some(), "acquire after scope exit should succeed");
    }
}
