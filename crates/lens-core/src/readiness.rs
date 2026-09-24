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

/// Why a provider endpoint is not usable.
///
/// The frontend uses this to decide whether to show "Start Ollama",
/// "Enter API key", or "Check endpoint". Each variant represents a
/// distinct product fact; collapsing them hides actionable information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAccessState {
    /// Endpoint is responding normally.
    Available,
    /// Have not checked yet or check is in progress.
    Unknown,
    /// Connection refused, DNS failure, or timeout. Provider is not reachable
    /// at the transport layer.
    Unreachable { reason: String },
    /// Provider requires authentication (HTTP 401/403). The provider answered
    /// but refused the request due to missing or invalid credentials.
    AuthenticationRequired,
    /// Provider returned an error (HTTP 5xx, malformed response, etc.).
    /// The provider is reachable but not functioning correctly.
    ProviderError { reason: String },
}

impl Default for ProviderAccessState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Result of looking up the model inventory on a provider.
///
/// Distinguishes between "no models installed" (an empty successful lookup)
/// and "could not retrieve the list" (network error, auth error, etc.).
/// Collapsing both into `Vec<String>` would make an unreachable provider
/// look identical to one with no models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInventoryView {
    /// Successfully retrieved the list. Empty vec means no models are installed.
    Available { models: Vec<String> },
    /// Lookup failed. Distinguishes from empty inventory.
    LookupFailed { reason: String },
    /// Have not attempted lookup yet (endpoint not reachable, etc.).
    NotAttempted,
}

impl Default for ModelInventoryView {
    fn default() -> Self {
        Self::NotAttempted
    }
}

/// Semantic next action for a lane.
///
/// Tells the frontend which button to show. The backend computes this from
/// the readiness state; the frontend does not duplicate the logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    /// Lane is ready; no action needed.
    None,
    /// Refresh the readiness state (retry after failure).
    Refresh,
    /// Download the configured model.
    Download,
    /// Load the model into memory (LM Studio explicit load).
    Load,
    /// Configure which model to use.
    ConfigureModel,
    /// Resolve authentication issue.
    ResolveAuth,
}

impl Default for NextAction {
    fn default() -> Self {
        Self::None
    }
}

/// Readiness assessment for a single engine lane (Quick or Deep).
///
/// Combines the three dimensions (runtime, endpoint, model) into a lane-level
/// readiness state. Each variant represents a distinct product truth; the
/// frontend uses it to decide what to show the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneReadiness {
    /// Runtime presence and endpoint state are not yet known.
    Unknown,
    /// Runtime is not installed. User needs to install the provider.
    RuntimeNotInstalled,
    /// Endpoint probe failed at the transport layer (connection refused,
    /// timeout, DNS failure). User needs to start the provider or check
    /// endpoint configuration.
    EndpointUnreachable,
    /// Provider is reachable but requires authentication (HTTP 401/403).
    /// The provider answered but refused the request.
    AuthenticationRequired,
    /// Provider is reachable but returned an error (HTTP 5xx, malformed
    /// response). The provider is not functioning correctly.
    ProviderError,
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
/// Contains the dimensions that determine whether a provider can serve
/// inference requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    /// Whether the provider supports downloading models.
    /// - Ollama: true
    /// - LM Studio: true
    ///
    /// When false, Download action becomes ConfigureModel (user must manually set up).
    pub can_download_model: bool,
    /// Whether the provider supports explicit model loading.
    /// - Ollama: false (JIT loading)
    /// - LM Studio: true
    ///
    /// When false, Load action is not offered.
    pub can_load_model: bool,
    /// Why the provider endpoint is or is not usable.
    /// Extends endpoint_state with diagnostic detail (auth vs error vs unknown).
    pub provider_access: ProviderAccessState,
    /// The model inventory on the provider, or why it could not be retrieved.
    pub model_inventory: ModelInventoryView,
    /// The explicitly configured model name, if any.
    /// None means auto-selection mode.
    pub configured_model: Option<String>,
    /// The concrete download target, if one exists and is valid.
    /// None means no valid download target is available.
    pub download_target: Option<String>,
}

/// Pure readiness evaluator.
///
/// Takes the observed dimensions and returns a LaneReadiness assessment.
/// This function is pure: no network, no filesystem, no platform calls.
///
/// # Precedence rules
///
/// 1. If provider_access is AuthenticationRequired, report AuthenticationRequired
///    (the provider answered but refused due to auth; this is distinct from
///    transport-level unreachable).
/// 2. If provider_access is ProviderError, report ProviderError (the provider
///    answered but is malfunctioning; distinct from unreachable).
/// 3. If endpoint is Reachable, continue model evaluation (runtime presence
///    Unknown does not block). Direct validated reachability is stronger
///    evidence than installation detection.
/// 4. If endpoint is Unreachable:
///    - If runtime presence is NotInstalled, report RuntimeNotInstalled
///    - Otherwise, report EndpointUnreachable
/// 5. If endpoint is Unknown:
///    - If runtime presence is NotInstalled, report RuntimeNotInstalled
///    - Otherwise, report Unknown (not EndpointUnreachable, which would be
///      evidence inflation)
/// 6. If endpoint is Reachable and model lifecycle is Present:
///    - If can_infer_from_present_model is true, report Ready (JIT-capable)
///    - Otherwise, report ModelPresent (explicit load required)
pub fn evaluate_lane_readiness(state: &SetupReadinessState) -> LaneReadiness {
    // Step 0: Provider access state takes precedence for auth and provider errors.
    // These are distinct from transport-level unreachable: the provider answered.
    match &state.provider_access {
        ProviderAccessState::AuthenticationRequired => {
            return LaneReadiness::AuthenticationRequired;
        }
        ProviderAccessState::ProviderError { .. } => {
            return LaneReadiness::ProviderError;
        }
        ProviderAccessState::Unreachable { .. } => {
            // Transport-level failure; fall through to endpoint_state logic below.
        }
        ProviderAccessState::Available | ProviderAccessState::Unknown => {
            // No access-level blocker; continue to endpoint/model evaluation.
        }
    }

    // Step 1: Check endpoint reachability with evidence precedence.
    match state.endpoint_state {
        EndpointState::Reachable => {}
        EndpointState::Unreachable => {
            if state.runtime_presence == RuntimePresence::NotInstalled {
                return LaneReadiness::RuntimeNotInstalled;
            }
            return LaneReadiness::EndpointUnreachable;
        }
        EndpointState::Unknown => {
            if state.runtime_presence == RuntimePresence::NotInstalled {
                return LaneReadiness::RuntimeNotInstalled;
            }
            return LaneReadiness::Unknown;
        }
    }

    // Step 2: Check model lifecycle (endpoint is Reachable).
    match state.model_lifecycle {
        ModelLifecycle::Unknown => LaneReadiness::Unknown,
        ModelLifecycle::Missing => LaneReadiness::NoCompatibleModel,
        ModelLifecycle::Downloading => LaneReadiness::ModelDownloading,
        ModelLifecycle::Present => {
            if state.can_infer_from_present_model {
                LaneReadiness::Ready
            } else {
                LaneReadiness::ModelPresent
            }
        }
        ModelLifecycle::Loading => LaneReadiness::ModelLoading,
        ModelLifecycle::Ready => LaneReadiness::Ready,
        ModelLifecycle::Failed => LaneReadiness::ModelFailed,
    }
}

/// Compute the semantic next action for a lane.
///
/// Pure function: no side effects. Takes the readiness state and returns
/// what the frontend should prompt the user to do. The lane readiness alone
/// is not enough: two lanes with `EndpointUnreachable` need different actions
/// (one needs "Start Ollama", another needs "Enter API key").
pub fn compute_next_action(state: &SetupReadinessState) -> NextAction {
    // Auth errors take precedence over everything else.
    if state.provider_access == ProviderAccessState::AuthenticationRequired {
        return NextAction::ResolveAuth;
    }

    // Provider errors suggest refresh (retry).
    if matches!(
        state.provider_access,
        ProviderAccessState::ProviderError { .. }
    ) {
        return NextAction::Refresh;
    }

    // If the endpoint is not reachable, suggest refresh (retry) unless
    // the runtime is not installed.
    match state.endpoint_state {
        EndpointState::Unreachable => {
            if state.runtime_presence == RuntimePresence::NotInstalled {
                // Cannot fix by refreshing; but this is an install action,
                // which is outside the scope of NextAction for now.
                return NextAction::Refresh;
            }
            return NextAction::Refresh;
        }
        EndpointState::Unknown => return NextAction::Refresh,
        EndpointState::Reachable => {}
    }

    // Endpoint is reachable. What about the model?
    match state.model_lifecycle {
        ModelLifecycle::Ready | ModelLifecycle::Downloading | ModelLifecycle::Loading => {
            NextAction::None
        }
        ModelLifecycle::Present => {
            if state.can_infer_from_present_model {
                NextAction::None
            } else if state.can_load_model {
                NextAction::Load
            } else {
                // Provider doesn't support explicit load; this shouldn't happen
                // for Present state, but handle it defensively.
                NextAction::None
            }
        }
        ModelLifecycle::Missing => {
            // If a model is explicitly configured, download that model.
            if state.configured_model.is_some() {
                if state.can_download_model {
                    NextAction::Download
                } else {
                    // Provider doesn't support downloading; user must configure manually.
                    NextAction::ConfigureModel
                }
            } else if state.download_target.is_some() {
                // Auto mode with a valid download target (e.g., Ollama DEFAULT_MODEL).
                if state.can_download_model {
                    NextAction::Download
                } else {
                    // Provider doesn't support downloading; user must configure manually.
                    NextAction::ConfigureModel
                }
            } else {
                // No configured model and no valid download target.
                // User needs to configure a model or provider has no default.
                NextAction::ConfigureModel
            }
        }
        ModelLifecycle::Failed => NextAction::Refresh,
        ModelLifecycle::Unknown => NextAction::Refresh,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Unknown,
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Unknown,
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Unreachable {
                reason: "connection refused".into(),
            },
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
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
            configured_model: None,
            download_target: None,
            runtime_presence: RuntimePresence::NotInstalled,
            endpoint_state: EndpointState::Unreachable,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Unreachable {
                reason: "connection refused".into(),
            },
            model_inventory: ModelInventoryView::NotAttempted,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::RuntimeNotInstalled
        );
    }

    #[test]
    fn endpoint_reachable_runtime_unknown_model_ready_produces_ready() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Unknown,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["llama3.2".into()],
            },
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available { models: vec![] },
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::ModelDownloading
        );
    }

    #[test]
    fn endpoint_reachable_model_present_with_jit_produces_ready() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Present,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["qwen2.5:7b".into()],
            },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::Ready);
    }

    #[test]
    fn endpoint_reachable_model_present_without_jit_produces_model_present() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Present,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["llama-3.1-8b".into()],
            },
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["qwen2.5-7b".into()],
            },
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["qwen2.5:7b".into()],
            },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(evaluate_lane_readiness(&state), LaneReadiness::ModelFailed);
    }

    #[test]
    fn quick_ready_deep_not_ready_is_valid_state() {
        let quick_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["qwen2.5:7b".into()],
            },
            configured_model: None,
            download_target: None,
        };
        let deep_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available { models: vec![] },
            configured_model: None,
            download_target: None,
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
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["qwen2.5:7b".into()],
            },
            configured_model: None,
            download_target: None,
        };
        let deep_state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["qwen2.5:7b".into()],
            },
            configured_model: None,
            download_target: None,
        };
        let snapshot = evaluate_readiness(&quick_state, &deep_state);
        assert_eq!(snapshot.quick, LaneReadiness::Ready);
        assert_eq!(snapshot.deep, LaneReadiness::Ready);
    }

    #[test]
    fn contradictory_evidence_not_installed_but_reachable() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::NotInstalled,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["qwen2.5:7b".into()],
            },
            configured_model: None,
            download_target: None,
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
            let provider_access = match endpoint {
                EndpointState::Reachable => ProviderAccessState::Available,
                EndpointState::Unreachable => ProviderAccessState::Unreachable {
                    reason: "connection refused".into(),
                },
                EndpointState::Unknown => ProviderAccessState::Unknown,
            };
            let state = SetupReadinessState {
                runtime_presence: runtime,
                endpoint_state: endpoint,
                model_lifecycle: model,
                can_infer_from_present_model: can_infer,
                can_download_model: true,
                can_load_model: true,
                provider_access,
                model_inventory: ModelInventoryView::NotAttempted,
                configured_model: None,
                download_target: None,
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
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Unknown,
            endpoint_state: EndpointState::Unknown,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Unknown,
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
        };
        let readiness = evaluate_lane_readiness(&state);
        assert_eq!(readiness, LaneReadiness::Unknown);
        assert_ne!(readiness, LaneReadiness::EndpointUnreachable);
        assert_ne!(readiness, LaneReadiness::RuntimeNotInstalled);
        assert_ne!(readiness, LaneReadiness::NoCompatibleModel);
    }

    #[test]
    fn auth_required_produces_authentication_required_lane() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::AuthenticationRequired,
            model_inventory: ModelInventoryView::Available { models: vec![] },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::AuthenticationRequired
        );
    }

    #[test]
    fn provider_error_produces_provider_error_lane() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::ProviderError {
                reason: "500 internal server error".into(),
            },
            model_inventory: ModelInventoryView::Available { models: vec![] },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::ProviderError
        );
    }

    #[test]
    fn unreachable_access_produces_endpoint_unreachable_lane() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Unreachable,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Unreachable {
                reason: "connection refused".into(),
            },
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
        };
        assert_eq!(
            evaluate_lane_readiness(&state),
            LaneReadiness::EndpointUnreachable
        );
    }

    #[test]
    fn next_action_resolve_auth_takes_precedence() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: true,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::AuthenticationRequired,
            model_inventory: ModelInventoryView::Available { models: vec![] },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(compute_next_action(&state), NextAction::ResolveAuth);
    }

    #[test]
    fn next_action_none_when_ready() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Ready,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["m".into()],
            },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(compute_next_action(&state), NextAction::None);
    }

    #[test]
    fn next_action_load_when_present_non_jit() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Present,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["m".into()],
            },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(compute_next_action(&state), NextAction::Load);
    }

    #[test]
    fn next_action_download_when_model_missing_with_target() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["other".into()],
            },
            configured_model: None,
            download_target: Some("llama3.2:latest".into()),
        };
        assert_eq!(compute_next_action(&state), NextAction::Download);
    }

    #[test]
    fn next_action_download_when_explicit_model_missing() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available {
                models: vec!["other".into()],
            },
            configured_model: Some("my-model".into()),
            download_target: None,
        };
        assert_eq!(compute_next_action(&state), NextAction::Download);
    }

    #[test]
    fn next_action_configure_model_when_inventory_empty() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Reachable,
            model_lifecycle: ModelLifecycle::Missing,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Available,
            model_inventory: ModelInventoryView::Available { models: vec![] },
            configured_model: None,
            download_target: None,
        };
        assert_eq!(compute_next_action(&state), NextAction::ConfigureModel);
    }

    #[test]
    fn next_action_refresh_when_endpoint_unreachable() {
        let state = SetupReadinessState {
            runtime_presence: RuntimePresence::Installed,
            endpoint_state: EndpointState::Unreachable,
            model_lifecycle: ModelLifecycle::Unknown,
            can_infer_from_present_model: false,
            can_download_model: true,
            can_load_model: true,
            provider_access: ProviderAccessState::Unreachable {
                reason: "connection refused".into(),
            },
            model_inventory: ModelInventoryView::NotAttempted,
            configured_model: None,
            download_target: None,
        };
        assert_eq!(compute_next_action(&state), NextAction::Refresh);
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
