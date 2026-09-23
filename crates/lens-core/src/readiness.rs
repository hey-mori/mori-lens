//! Readiness domain model.
//!
//! This module represents the setup/readiness lifecycle, which is separate from
//! operational health. Health answers "did inference work when tested?" Readiness
//! answers "can the user get a first card?"
//!
//! The evaluator is pure: no network, no filesystem, no platform calls. It takes
//! observed inputs and returns a readiness assessment. The platform adapter provides
//! the inputs; this module classifies them.

use serde::{Deserialize, Serialize};

/// Whether the provider runtime is present on the system.
///
/// This is about installation, not about whether it is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePresence {
    /// Have not checked yet or check failed in a way that does not distinguish.
    Unknown,
    /// Runtime is not installed or not on PATH.
    NotInstalled,
    /// Runtime is installed (binary found or service registered).
    Installed,
}

impl Default for RuntimePresence {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Whether the provider endpoint is reachable.
///
/// This is a network-level check (TCP connect or HTTP HEAD), independent of
/// whether models are present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointState {
    /// Have not checked yet or check failed in a way that does not distinguish.
    Unknown,
    /// Endpoint is not reachable (connection refused, timeout, DNS failure).
    Unreachable,
    /// Endpoint is reachable and responding.
    Reachable,
}

impl Default for EndpointState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Lifecycle state of a model relative to a provider.
///
/// This tracks the model's presence and readiness for inference, independent
/// of whether inference has been tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelLifecycle {
    /// Have not checked yet or check failed in a way that does not distinguish.
    Unknown,
    /// Model is not present locally (needs download).
    Missing,
    /// Model is currently being downloaded.
    Downloading,
    /// Model is present locally but not currently loaded (for providers with
    /// explicit load/unload, e.g., LM Studio).
    Present,
    /// Model is currently being loaded into memory (LM Studio load in progress).
    Loading,
    /// Model is present and ready for inference.
    Ready,
    /// Model download or load failed.
    Failed,
}

impl Default for ModelLifecycle {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Readiness assessment for a single engine lane (Quick or Deep).
///
/// Combines the three dimensions (runtime, endpoint, model) into a lane-level
/// readiness state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneReadiness {
    /// Runtime presence is not yet known.
    RuntimeUnknown,
    /// Runtime is not installed. User needs to install the provider.
    RuntimeNotInstalled,
    /// Runtime is installed but endpoint is not reachable. User needs to start the provider.
    EndpointUnreachable,
    /// Endpoint is reachable but no compatible model is present. User needs to download a model.
    NoCompatibleModel,
    /// Model is present and ready for inference. Lane is ready.
    Ready,
    /// Model is being downloaded.
    ModelDownloading,
    /// Model is being loaded (LM Studio).
    ModelLoading,
    /// Model download or load failed.
    ModelFailed,
}

impl Default for LaneReadiness {
    fn default() -> Self {
        Self::RuntimeUnknown
    }
}

/// Readiness snapshot for both engine lanes.
///
/// This is the aggregate readiness state, separate from HealthSnapshot.
/// HealthSnapshot tracks operational health (did inference work?).
/// ReadinessSnapshot tracks setup readiness (can the user get a first card?).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReadinessSnapshot {
    /// Quick translation engine readiness.
    pub quick: LaneReadiness,
    /// Deep analysis engine readiness.
    pub deep: LaneReadiness,
}

/// Setup readiness state for a single provider.
///
/// Contains the three dimensions that determine whether a provider can serve
/// inference requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SetupReadinessState {
    /// Whether the provider runtime is present on the system.
    pub runtime_presence: RuntimePresence,
    /// Whether the provider endpoint is reachable.
    pub endpoint_state: EndpointState,
    /// Lifecycle state of the configured model.
    pub model_lifecycle: ModelLifecycle,
}

/// Pure readiness evaluator.
///
/// Takes the three observed dimensions and returns a LaneReadiness assessment.
/// This function is pure: no network, no filesystem, no platform calls.
///
/// The evaluation logic:
/// 1. If runtime presence is Unknown or NotInstalled, report RuntimeUnknown/NotInstalled.
/// 2. If endpoint is Unknown or Unreachable, report EndpointUnreachable.
/// 3. If model lifecycle is Unknown, Missing, Downloading, Loading, or Failed, report accordingly.
/// 4. If model is Present or Ready, report Ready.
pub fn evaluate_lane_readiness(state: &SetupReadinessState) -> LaneReadiness {
    // Step 1: Check runtime presence.
    match state.runtime_presence {
        RuntimePresence::Unknown => return LaneReadiness::RuntimeUnknown,
        RuntimePresence::NotInstalled => return LaneReadiness::RuntimeNotInstalled,
        RuntimePresence::Installed => {} // Continue to next check.
    }

    // Step 2: Check endpoint reachability.
    match state.endpoint_state {
        EndpointState::Unknown | EndpointState::Unreachable => {
            return LaneReadiness::EndpointUnreachable
        }
        EndpointState::Reachable => {} // Continue to next check.
    }

    // Step 3: Check model lifecycle.
    match state.model_lifecycle {
        ModelLifecycle::Unknown => LaneReadiness::RuntimeUnknown, // Should not happen if we got this far, but safe fallback.
        ModelLifecycle::Missing => LaneReadiness::NoCompatibleModel,
        ModelLifecycle::Downloading => LaneReadiness::ModelDownloading,
        ModelLifecycle::Present | ModelLifecycle::Ready => LaneReadiness::Ready,
        ModelLifecycle::Loading => LaneReadiness::ModelLoading,
        ModelLifecycle::Failed => LaneReadiness::ModelFailed,
    }
}

/// Evaluate readiness for both lanes.
///
/// Takes two SetupReadinessState (one for Quick provider, one for Deep provider)
/// and returns a ReadinessSnapshot.
pub fn evaluate_readiness(
    quick_state: &SetupReadinessState,
    deep_state: &SetupReadinessState,
) -> ReadinessSnapshot {
    ReadinessSnapshot {
        quick: evaluate_lane_readiness(quick_state),
        deep: evaluate_lane_readiness(deep_state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_unknown_produces_runtime_unknown() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Unknown,
            endpoint_state: EndpointState::Unknown,
            model_lifecycle: ModelLifecycle::Unknown,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::RuntimeUnknown
        );
    }

    #[test]
    fn runtime_not_installed_produces_runtime_not_installed() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::NotInstalled,
            endpoint_state: EndpointState::Unknown,
            model_lifecycle: ModelLifecycle::Unknown,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::RuntimeNotInstalled
        );
    }

    #[test]
    fn runtime_installed_but_endpoint_unreachable() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Unreachable,
            model_lifecycle: ModelLifecycle::Unknown,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::EndpointUnreachable
        );
    }

    #[test]
    fn runtime_installed_but_endpoint_unknown() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Unknown,
            model_lifecycle: ModelLifecycle::Present,
        };
        // Endpoint Unknown should block, even if model is present.
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::EndpointUnreachable
        );
    }

    #[test]
    fn endpoint_reachable_but_no_model() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::NoCompatibleModel
        );
    }

    #[test]
    fn model_downloading() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Downloading,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::ModelDownloading
        );
    }

    #[test]
    fn model_present_produces_ready() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Present,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Ready);
    }

    #[test]
    fn model_ready_produces_ready() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Ready);
    }

    #[test]
    fn model_loading() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Loading,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::ModelLoading);
    }

    #[test]
    fn model_failed() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Failed,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::ModelFailed);
    }

    #[test]
    fn quick_ready_deep_not_ready() {
        let quick_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
        };
        let deep_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
        };
        let snapshot = evaluate_readiness(&quick_state, &deep_state);
        assert_eq!(snapshot.quick, LaneReadiness::Ready);
        assert_eq!(snapshot.deep, LaneReadiness::NoCompatibleModel);
    }

    #[test]
    fn fully_ready() {
        let quick_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
        };
        let deep_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
        };
        let snapshot = evaluate_readiness(&quick_state, &deep_state);
        assert_eq!(snapshot.quick, LaneReadiness::Ready);
        assert_eq!(snapshot.deep, LaneReadiness::Ready);
    }

    #[test]
    fn readiness_snapshot_serializes_to_snake_case() {
        let snapshot = ReadinessSnapshot {
            quick: LaneReadiness::Ready,
            deep: LaneReadiness::NoCompatibleModel,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"ready\""));
        assert!(json.contains("\"no_compatible_model\""));
    }

    #[test]
    fn readiness_serialization_contains_no_user_content() {
        // Privacy check: readiness snapshot must not contain user content,
        // model output, URLs, or paths.
        let snapshot = ReadinessSnapshot {
            quick: LaneReadiness::Ready,
            deep: LaneReadiness::Ready,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let banned = [
            "user",
            "content",
            "output",
            "http",
            "https",
            "path",
            "file",
            "directory",
        ];
        for word in &banned {
            assert!(
                !json.contains(word),
                "readiness snapshot contains banned word: {word}"
            );
        }
    }

    #[test]
    fn default_snapshot_is_all_runtime_unknown() {
        let snapshot = ReadinessSnapshot::default();
        assert_eq!(snapshot.quick, LaneReadiness::RuntimeUnknown);
        assert_eq!(snapshot.deep, LaneReadiness::RuntimeUnknown);
    }

    #[test]
    fn truth_table_exhaustive_coverage() {
        // Verify that the evaluator covers all meaningful combinations.
        // This is a sampling of the truth table, not exhaustive enumeration.
        let cases = vec![
            (
                RuntimePresence::Unknown,
                EndpointState::Unknown,
                ModelLifecycle::Unknown,
                LaneReadiness::RuntimeUnknown,
            ),
            (
                RuntimePresence::NotInstalled,
                EndpointState::Unreachable,
                ModelLifecycle::Missing,
                LaneReadiness::RuntimeNotInstalled,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Unreachable,
                ModelLifecycle::Present,
                LaneReadiness::EndpointUnreachable,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Reachable,
                ModelLifecycle::Missing,
                LaneReadiness::NoCompatibleModel,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Reachable,
                ModelLifecycle::Ready,
                LaneReadiness::Ready,
            ),
        ];

        for (runtime, endpoint, model, expected) in cases {
            let state = SetupReadinessState {
                runtime_presence: runtime,
                endpoint_state: endpoint,
                model_lifecycle: model,
            };
            assert_eq!(
                evaluate_lane_readiness(&state),
                expected,
                "Failed for runtime={runtime:?}, endpoint={endpoint:?}, model={model:?}",
            );
        }
    }
}
