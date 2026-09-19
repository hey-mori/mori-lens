//! Privacy-safe health snapshot.
//!
//! Process-local cached health state for Quick and Deep engines.
//! Updated by explicit health checks, read by Status page.
//! No persistence, no disk spill. No user content or model output.

use serde::Serialize;
use std::sync::{Arc, Mutex};

use crate::event_ring::DiagnosticFailureClass;

/// Health state for a single engine (Quick or Deep).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    /// Never checked or invalidated by config change.
    Unknown,
    /// Health check is currently running.
    Running,
    /// Engine is operational.
    Ready,
    /// Engine responded but output was incomplete or slow.
    Degraded,
    /// Engine could not be reached or failed.
    Unavailable,
}

/// Health information for a single engine lane.
#[derive(Debug, Clone, Serialize)]
pub struct HealthLane {
    /// Current health state.
    pub state: HealthState,
    /// Typed failure class (present only for Unavailable/Degraded).
    pub failure: Option<DiagnosticFailureClass>,
    /// Latency of the last successful check in milliseconds.
    pub latency_ms: Option<u64>,
    /// Milliseconds since process start when this was last updated.
    pub updated_ms: Option<u64>,
}

impl Default for HealthLane {
    fn default() -> Self {
        Self {
            state: HealthState::Unknown,
            failure: None,
            latency_ms: None,
            updated_ms: None,
        }
    }
}

/// Cached health state for both engines.
///
/// Privacy-safe: contains only enums, numbers, and relative timestamps.
/// No user content, no model output, no raw errors, no URLs.
#[derive(Debug, Clone, Serialize)]
pub struct HealthSnapshot {
    /// Quick translation engine health.
    pub quick: HealthLane,
    /// Deep analysis engine health.
    pub deep: HealthLane,
    /// Whether the snapshot is stale (invalidated by config change).
    pub stale: bool,
    /// Milliseconds since process start when snapshot was created.
    pub created_ms: u64,
}

impl Default for HealthSnapshot {
    fn default() -> Self {
        Self {
            quick: HealthLane::default(),
            deep: HealthLane::default(),
            stale: false,
            created_ms: 0,
        }
    }
}

/// Shared health snapshot, protected by mutex.
pub type SharedHealthSnapshot = Arc<Mutex<HealthSnapshot>>;

impl HealthSnapshot {
    /// Create a new empty snapshot with current timestamp.
    pub fn new(created_ms: u64) -> Self {
        Self {
            created_ms,
            ..Default::default()
        }
    }

    /// Mark the snapshot as stale (invalidated by config change).
    pub fn mark_stale(&mut self) {
        self.stale = true;
        self.quick.state = HealthState::Unknown;
        self.deep.state = HealthState::Unknown;
    }

    /// Update Quick lane health.
    pub fn update_quick(
        &mut self,
        state: HealthState,
        failure: Option<DiagnosticFailureClass>,
        latency_ms: Option<u64>,
        updated_ms: u64,
    ) {
        self.quick = HealthLane {
            state,
            failure,
            latency_ms,
            updated_ms: Some(updated_ms),
        };
        self.stale = false;
    }

    /// Update Deep lane health.
    pub fn update_deep(
        &mut self,
        state: HealthState,
        failure: Option<DiagnosticFailureClass>,
        latency_ms: Option<u64>,
        updated_ms: u64,
    ) {
        self.deep = HealthLane {
            state,
            failure,
            latency_ms,
            updated_ms: Some(updated_ms),
        };
        self.stale = false;
    }

    /// Mark both lanes as Running.
    pub fn mark_running(&mut self, updated_ms: u64) {
        self.quick.state = HealthState::Running;
        self.quick.updated_ms = Some(updated_ms);
        self.deep.state = HealthState::Running;
        self.deep.updated_ms = Some(updated_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_snapshot_is_unknown_and_not_stale() {
        let snapshot = HealthSnapshot::new(1000);
        assert_eq!(snapshot.quick.state, HealthState::Unknown);
        assert_eq!(snapshot.deep.state, HealthState::Unknown);
        assert!(!snapshot.stale);
        assert_eq!(snapshot.created_ms, 1000);
    }

    #[test]
    fn mark_stale_sets_unknown_and_stale_flag() {
        let mut snapshot = HealthSnapshot::new(1000);
        snapshot.update_quick(HealthState::Ready, None, Some(100), 2000);
        snapshot.update_deep(HealthState::Ready, None, Some(200), 2000);

        snapshot.mark_stale();

        assert!(snapshot.stale);
        assert_eq!(snapshot.quick.state, HealthState::Unknown);
        assert_eq!(snapshot.deep.state, HealthState::Unknown);
    }

    #[test]
    fn update_quick_clears_stale_flag() {
        let mut snapshot = HealthSnapshot::new(1000);
        snapshot.mark_stale();
        assert!(snapshot.stale);

        snapshot.update_quick(HealthState::Ready, None, Some(100), 2000);
        assert!(!snapshot.stale);
        assert_eq!(snapshot.quick.state, HealthState::Ready);
    }

    #[test]
    fn update_deep_clears_stale_flag() {
        let mut snapshot = HealthSnapshot::new(1000);
        snapshot.mark_stale();
        assert!(snapshot.stale);

        snapshot.update_deep(HealthState::Ready, None, Some(200), 2000);
        assert!(!snapshot.stale);
        assert_eq!(snapshot.deep.state, HealthState::Ready);
    }

    #[test]
    fn serialized_snapshot_contains_no_canaries() {
        let canaries = [
            "SUPER_SECRET_ORIGINAL_CANARY",
            "SUPER_SECRET_TRANSLATION_CANARY",
            "SUPER_SECRET_ANALYSIS_CANARY",
            "SUPER_SECRET_NOTE_CANARY",
            "SUPER_SECRET_HISTORY_CANARY",
            "SUPER_SECRET_RECENT_CANARY",
            "SUPER_SECRET_WINDOW_TITLE_CANARY",
            "SUPER_SECRET_APP_NAME_CANARY",
            "SUPER_SECRET_PATH_CANARY",
            "SUPER_SECRET_TOKEN_CANARY",
            "SUPER_SECRET_ERROR_BODY_CANARY",
        ];

        let mut snapshot = HealthSnapshot::new(1000);
        snapshot.update_quick(
            HealthState::Unavailable,
            Some(DiagnosticFailureClass::Timeout),
            None,
            2000,
        );
        snapshot.update_deep(HealthState::Ready, None, Some(200), 2000);

        let json = serde_json::to_string(&snapshot).unwrap();
        for canary in &canaries {
            assert!(
                !json.contains(canary),
                "health snapshot contains canary: {canary}"
            );
        }
    }
}
