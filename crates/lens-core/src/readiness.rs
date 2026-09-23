//! Readiness domain model.
//!
//! This module represents the setup/readiness lifecycle, which is separate from
//! operational health. Health answers "did inference work when tested?" Readiness
//! answers "can the user get a first card?"
//!
//! The evaluator is pure: no network, no filesystem, no platform calls. It takes
//! observed inputs and returns a readiness assessment. The platform adapter provides
//! the inputs; this module classifies them.
//!
//! # Vocabulary mapping
//!
//! The readiness domain uses different terms than the provider runtime:
//! - `ProviderRuntime::model_state()` returns `ModelState::Available` or `ModelState::Loaded`
//! - This readiness domain derives `ModelLifecycle` from those observations
//! - `ModelState::Available` maps to `ModelLifecycle::Present` (downloaded but not necessarily loaded)
//! - `ModelState::Loaded` maps to `ModelLifecycle::Ready` (in memory, ready for inference)
//! - Additional lifecycle states (Missing, Downloading, Loading, Failed) come from setup operations

use serde::{Deserialize, Serialize};

/// Whether the provider runtime is present on the system.
///
/// This is about installation, not about whether it is currently running.
/// Evidence may come from PATH detection, registry, or service enumeration.
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
/// This is validated by a provider-specific management probe:
/// - Ollama: GET /api/tags
/// - LM Studio: GET /api/v1/models
///
/// A random process listening on the port is not sufficient; the probe must
/// return provider-identity evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointState {
    /// Have not checked yet or check is in progress.
    Unknown,
    /// Provider probe failed (connection refused, timeout, DNS failure, or non-provider response).
    Unreachable,
    /// Provider probe succeeded with provider-identity evidence.
    Reachable,
}

impl Default for EndpointState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Lifecycle state of a model relative to a provider.
///
/// Derived from `ProviderRuntime::model_state()` observations plus setup operations.
/// This tracks the model's presence and readiness for inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelLifecycle {
    /// Have not checked yet or check failed in a way that does not distinguish.
    Unknown,
    /// Model is not present locally (needs download).
    Missing,
    /// Model is currently being downloaded.
    Downloading,
    /// Model is present locally but not currently loaded.
    /// For Ollama: installed but not in /api/ps
    /// For LM Studio: downloaded but not in loaded_instances
    ///
    /// Whether this is sufficient for inference depends on provider semantics:
    /// - Ollama supports JIT loading, so Present may be sufficient for first card
    /// - LM Studio may require explicit load before inference
    ///
    /// The readiness evaluator receives a capability flag to distinguish these cases.
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
    /// Runtime presence and endpoint state are not yet known.
    Unknown,
    /// Runtime is not installed. User needs to install the provider.
    RuntimeNotInstalled,
    /// Endpoint probe failed. User needs to start the provider or check endpoint configuration.
    EndpointUnreachable,
    /// Endpoint is reachable but no compatible model is present. User needs to download a model.
    NoCompatibleModel,
    /// Model is downloaded but not loaded. For providers requiring explicit load, user needs to load it.
    ModelPresent,
    /// Model is being downloaded.
    ModelDownloading,
    /// Model is being loaded (LM Studio).
    ModelLoading,
    /// Model download or load failed.
    ModelFailed,
    /// Model is present and ready for inference. Lane is ready for first card.
    Ready,
}

impl Default for LaneReadiness {
    fn default() -> Self {
        Self::Unknown
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
    /// Whether a Present model is sufficient for inference without explicit load.
    /// - Ollama: true (supports JIT loading)
    /// - LM Studio: false (may require explicit load)
    ///
    /// This flag allows the readiness evaluator to treat Present as Ready for
    /// providers that support JIT, while requiring explicit Ready state for others.
    pub can_infer_from_present_model: bool,
}

/// Pure readiness evaluator.
///
/// Takes the observed dimensions and returns a LaneReadiness assessment.
/// This function is pure: no network, no filesystem, no platform calls.
///
/// # Precedence rules
///
/// 1. If endpoint is Reachable, continue model evaluation (runtime presence Unknown does not block).
///    Direct validated reachability is stronger evidence than installation detection.
/// 2. If endpoint is Unreachable:
///    - If runtime presence is NotInstalled, report RuntimeNotInstalled
///    - Otherwise, report EndpointUnreachable
/// 3. If endpoint is Unknown:
///    - If runtime presence is NotInstalled, report RuntimeNotInstalled
///    - Otherwise, report Unknown (not EndpointUnreachable, which would be evidence inflation)
/// 4. If endpoint is Reachable and model lifecycle is Present:
///    - If can_infer_from_present_model is true, report Ready (JIT-capable provider)
///    - Otherwise, report ModelPresent (explicit load required)
pub fn evaluate_lane_readiness(state: &SetupReadinessState) -> LaneReadiness {
    // Step 1: Check endpoint reachability with evidence precedence.
    match state.endpoint_state {
        EndpointState::Reachable => {
            // Endpoint is reachable; continue to model evaluation.
            // Runtime presence Unknown does not block here because direct
            // validated reachability is stronger evidence.
        }
        EndpointState::Unreachable => {
            // Endpoint probe failed.
            // Check if we have strong installation evidence.
            if state.runtime_presence == RuntimePresence::NotInstalled {
                return LaneReadiness::RuntimeNotInstalled;
            }
            return LaneReadiness::EndpointUnreachable;
        }
        EndpointState::Unknown => {
            // Endpoint not yet checked or check in progress.
            // Check if we have strong installation evidence.
            if state.runtime_presence == RuntimePresence::NotInstalled {
                return LaneReadiness::RuntimeNotInstalled;
            }
            // Do not report EndpointUnreachable (evidence inflation).
            return LaneReadiness::Unknown;
        }
    }

    // Step 2: Check model lifecycle (endpoint is Reachable).
    match state.model_lifecycle {
        ModelLifecycle::Unknown => LaneReadiness::Unknown,
        ModelLifecycle::Missing => LaneReadiness::NoCompatibleModel,
        ModelLifecycle::Downloading => LaneReadiness::ModelDownloading,
        ModelLifecycle::Present => {
            // Whether Present is sufficient depends on provider capability.
            if state.can_infer_from_present_model {
                LaneReadiness::Ready // JIT-capable provider (Ollama)
            } else {
                LaneReadiness::ModelPresent // Explicit load required (LM Studio)
            }
        }
        ModelLifecycle::Loading => LaneReadiness::ModelLoading,
        ModelLifecycle::Ready => LaneReadiness::Ready,
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

/// Map provider runtime ModelState to readiness ModelLifecycle.
///
/// This is a pure mapping function with no side effects.
pub fn model_state_to_lifecycle(model_state: crate::translate::ModelState) -> ModelLifecycle {
    match model_state {
        crate::translate::ModelState::Available { .. } => ModelLifecycle::Present,
        crate::translate::ModelState::Loaded { .. } => ModelLifecycle::Ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_unknown_runtime_unknown_produces_unknown() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Unknown,
            endpoint_state: EndpointState::Unknown,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Unknown);
    }

    #[test]
    fn endpoint_unknown_runtime_not_installed_produces_not_installed() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::NotInstalled,
            endpoint_state: EndpointState::Unknown,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::RuntimeNotInstalled
        );
    }

    #[test]
    fn endpoint_unreachable_runtime_unknown_produces_unreachable() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Unknown,
            endpoint_state: EndpointState::Unreachable,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::EndpointUnreachable
        );
        // Note: does NOT claim NotInstalled because runtime evidence is Unknown
    }

    #[test]
    fn endpoint_unreachable_runtime_not_installed_produces_not_installed() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::NotInstalled,
            endpoint_state: EndpointState::Unreachable,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::RuntimeNotInstalled
        );
    }

    #[test]
    fn endpoint_reachable_runtime_unknown_model_ready_produces_ready() {
        // Direct validated reachability overrides installation uncertainty.
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Unknown,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: false,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Ready);
    }

    #[test]
    fn endpoint_reachable_model_missing_produces_no_compatible_model() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
            can_infer_from_present_model: false,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::NoCompatibleModel
        );
    }

    #[test]
    fn endpoint_reachable_model_downloading_produces_model_downloading() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Downloading,
            can_infer_from_present_model: false,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::ModelDownloading
        );
    }

    #[test]
    fn endpoint_reachable_model_present_with_jit_produces_ready() {
        // Ollama: Present is sufficient because JIT loading is supported.
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Present,
            can_infer_from_present_model: true, // Ollama
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Ready);
    }

    #[test]
    fn endpoint_reachable_model_present_without_jit_produces_model_present() {
        // LM Studio: Present requires explicit load.
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Present,
            can_infer_from_present_model: false, // LM Studio
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::ModelPresent);
    }

    #[test]
    fn endpoint_reachable_model_loading_produces_model_loading() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Loading,
            can_infer_from_present_model: false,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::ModelLoading);
    }

    #[test]
    fn endpoint_reachable_model_ready_produces_ready() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: false,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Ready);
    }

    #[test]
    fn endpoint_reachable_model_failed_produces_model_failed() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Failed,
            can_infer_from_present_model: false,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::ModelFailed);
    }

    #[test]
    fn quick_ready_deep_not_ready_is_valid_state() {
        // Quick ready + Deep not ready is a usable first-card state.
        let quick_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
        };
        let deep_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
            can_infer_from_present_model: true,
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
            can_infer_from_present_model: true,
        };
        let deep_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
        };
        let snapshot = evaluate_readiness(&quick_state, &deep_state);
        assert_eq!(snapshot.quick, LaneReadiness::Ready);
        assert_eq!(snapshot.deep, LaneReadiness::Ready);
    }

    #[test]
    fn contradictory_evidence_not_installed_but_reachable() {
        // Direct validated reachability wins over installation detection failure.
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::NotInstalled,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Ready);
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
    fn readiness_serialization_contains_no_canaries() {
        // Privacy check using the same canary pattern as other core modules.
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

        let snapshot = ReadinessSnapshot {
            quick: LaneReadiness::Ready,
            deep: LaneReadiness::Ready,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        for canary in &canaries {
            assert!(
                !json.contains(canary),
                "readiness snapshot contains canary: {canary}"
            );
        }
    }

    #[test]
    fn default_snapshot_is_all_unknown() {
        let snapshot = ReadinessSnapshot::default();
        assert_eq!(snapshot.quick, LaneReadiness::Unknown);
        assert_eq!(snapshot.deep, LaneReadiness::Unknown);
    }

    #[test]
    fn representative_truth_table_cases() {
        // Representative sampling of the truth table (not exhaustive).
        let cases = vec![
            (
                RuntimePresence::Unknown,
                EndpointState::Unknown,
                ModelLifecycle::Unknown,
                false,
                LaneReadiness::Unknown,
            ),
            (
                RuntimePresence::NotInstalled,
                EndpointState::Unreachable,
                ModelLifecycle::Missing,
                false,
                LaneReadiness::RuntimeNotInstalled,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Unreachable,
                ModelLifecycle::Present,
                false,
                LaneReadiness::EndpointUnreachable,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Reachable,
                ModelLifecycle::Missing,
                false,
                LaneReadiness::NoCompatibleModel,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Reachable,
                ModelLifecycle::Ready,
                false,
                LaneReadiness::Ready,
            ),
            (
                RuntimePresence::Unknown,
                EndpointState::Reachable,
                ModelLifecycle::Ready,
                false,
                LaneReadiness::Ready,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Reachable,
                ModelLifecycle::Present,
                true, // JIT-capable
                LaneReadiness::Ready,
            ),
            (
                RuntimePresence::Installed,
                EndpointState::Reachable,
                ModelLifecycle::Present,
                false, // JIT not capable
                LaneReadiness::ModelPresent,
            ),
        ];

        for (runtime, endpoint, model, can_infer, expected) in cases {
            let state = SetupReadinessState {
                runtime_presence: runtime,
                endpoint_state: endpoint,
                model_lifecycle: model,
                can_infer_from_present_model: can_infer,
            };
            assert_eq!(
                evaluate_lane_readiness(&state),
                expected,
                "Failed for runtime={runtime:?}, endpoint={endpoint:?}, model={model:?}, can_infer={can_infer}",
            );
        }
    }

    #[test]
    fn unknown_does_not_become_unreachable_or_not_installed() {
        // Ensure serialization/UI mapping cannot turn Unknown into stronger claims.
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Unknown,
            endpoint_state: EndpointState::Unknown,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
        };
        let readiness = evaluate_lane_readiness(&state);
        assert_eq!(readiness, LaneReadiness::Unknown);
        assert_ne!(readiness, LaneReadiness::EndpointUnreachable);
        assert_ne!(readiness, LaneReadiness::RuntimeNotInstalled);
        assert_ne!(readiness, LaneReadiness::NoCompatibleModel);
    }

    #[test]
    fn model_state_available_maps_to_present() {
        let model_state = crate::translate::ModelState::Available {
            name: "llama3.2".to_string(),
            size: 4_000_000_000,
        };
        assert_eq!(
            model_state_to_lifecycle(model_state),
            ModelLifecycle::Present
        );
    }

    #[test]
    fn model_state_loaded_maps_to_ready() {
        let model_state = crate::translate::ModelState::Loaded {
            name: "llama3.2".to_string(),
        };
        assert_eq!(model_state_to_lifecycle(model_state), ModelLifecycle::Ready);
    }
}
