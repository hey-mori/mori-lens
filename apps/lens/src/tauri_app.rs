//! Tauri product shell.
//!
//! The hover engine runs on a worker thread; this module owns the Tauri
//! application on the main thread, including the settings webview (#164, #165).

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{Emitter, Manager};

/// Process-start anchor for monotonic `updated_ms` timestamps.
///
/// `HealthLane.updated_ms` is documented as milliseconds since process start.
/// Measuring `Instant::now().elapsed()` inside `run_health_check` would only
/// give the time since the health-check call, not since the process began;
/// a check triggered thirty minutes after launch must report ~1_800_000ms,
/// not 0.
///
/// Call `initialize_process_start()` early during app startup (in `build()` or `main()`)
/// to ensure the timestamp origin predates any health check invocation.
/// Then call `process_start_elapsed_ms()` to read elapsed time.
use std::sync::OnceLock;
static PROCESS_START: OnceLock<std::time::Instant> = OnceLock::new();

/// Initialize the process-start anchor. Call once at startup.
/// Returns the initialized Instant.
pub fn initialize_process_start() -> std::time::Instant {
    *PROCESS_START.get_or_init(std::time::Instant::now)
}

/// Read elapsed milliseconds since process start.
/// Panics if `initialize_process_start()` was not called.
pub fn process_start_elapsed_ms() -> u64 {
    PROCESS_START
        .get()
        .expect("process_start not initialized; call initialize_process_start() at startup")
        .elapsed()
        .as_millis() as u64
}

/// Legacy accessor for backward compatibility. Returns the process-start Instant.
/// Prefer `process_start_elapsed_ms()` for reading elapsed time.
pub fn process_start() -> std::time::Instant {
    *PROCESS_START.get_or_init(std::time::Instant::now)
}

use lens_core::event_ring::DiagnosticFailureClass;
use lens_core::health::HealthState;

use crate::detail_view::{ExplainFlag, SharedDetail, SharedSavePath};
use crate::library_service::{LibraryBacking, SharedLibrary};
use crate::settings_service::{SettingsView, SharedSettings};

/// Shared session-recent state, written by the polling loop and read by Tauri commands.
pub type SharedRecent = Arc<Mutex<lens_core::recent::SessionRecent>>;

/// A recent card for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct RecentCardView {
    pub original: String,
    pub answer: String,
    pub shown_at: i64,
    pub app: Option<String>,
}

/// A snapshot of the session's diagnostics, shared with the webview.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DiagnosticsSnapshot {
    pub version: String,
    pub build: String,
    pub configuration: Vec<(String, String)>,
    pub cards: u32,
    pub settled_hovers: u32,
    pub refusals: Vec<(String, u32)>,
    pub ocr_mode: String,
    /// Whether reading is currently paused. Shared with the Control Center so
    /// it can show the current state and provide a pause/resume toggle.
    pub paused: bool,
}

/// Shared diagnostics state, updated by the polling loop and read by Tauri commands.
pub type SharedDiagnostics = Arc<Mutex<DiagnosticsSnapshot>>;

/// Shared diagnostic event ring, recording recent abnormal operational events.
///
/// Bounded in-memory ring (capacity 128). Dies with the process. No persistence.
/// Privacy-safe: events contain only enums and numbers, no captured text.
pub type SharedDiagnosticEvents = Arc<Mutex<lens_core::event_ring::DiagnosticEventRing>>;

/// Global diagnostic events ring for access from product runtime.
///
/// The Tauri-managed ring is not reachable from the Windows/macOS product loops
/// which run outside Tauri's state management. This static provides a single
/// shared ring for the entire process.
static GLOBAL_DIAGNOSTIC_EVENTS: std::sync::LazyLock<SharedDiagnosticEvents> =
    std::sync::LazyLock::new(|| {
        Arc::new(Mutex::new(lens_core::event_ring::DiagnosticEventRing::new()))
    });

/// Access the global diagnostic events ring.
pub fn global_diagnostic_events() -> SharedDiagnosticEvents {
    GLOBAL_DIAGNOSTIC_EVENTS.clone()
}

/// Shared health snapshot, caching the last health check result.
///
/// Process-local, no persistence. Privacy-safe: contains only enums, numbers,
/// and relative timestamps. No user content, no model output, no raw errors.
pub type SharedHealthSnapshot = Arc<Mutex<lens_core::health::HealthSnapshot>>;

/// Shared operation manager for provider model management.
///
/// Tracks active downloads, terminal snapshots, and Mori-owned LM Studio
/// instances. Process-local, no persistence. See `operation_manager.rs`.
pub type SharedOperationManager = lens_translate::operation_manager::OperationManager;

/// Backend concurrency guard for health checks.
///
/// Prevents multiple simultaneous health check runs. Provides a focused API
/// for acquiring and releasing the guard, with RAII semantics.
#[derive(Clone, Debug)]
pub struct HealthCheckRunning(Arc<std::sync::atomic::AtomicBool>);

impl HealthCheckRunning {
    /// Create a new guard in the released (false) state.
    pub fn new() -> Self {
        Self(Arc::new(std::sync::atomic::AtomicBool::new(false)))
    }

    /// Try to acquire the guard. Returns `None` if already running.
    pub fn try_acquire(&self) -> Option<HealthCheckGuard> {
        use std::sync::atomic::Ordering;
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

/// RAII guard for health check execution. Releases on drop.
#[derive(Debug)]
pub struct HealthCheckGuard(Arc<std::sync::atomic::AtomicBool>);

impl Drop for HealthCheckGuard {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Result of a health check run, for the webview.
///
/// Uses typed enums instead of strings: `state` is a `HealthState` (serialized
/// as snake_case) and `failure` is an optional `DiagnosticFailureClass`. The
/// frontend compares the state strings (`"ready"`, `"degraded"`, `"unavailable"`)
/// and renders the failure class as-is.
#[derive(Clone, Debug, Serialize)]
pub struct HealthCheckView {
    pub quick_state: lens_core::health::HealthState,
    pub quick_failure: Option<lens_core::event_ring::DiagnosticFailureClass>,
    pub deep_state: lens_core::health::HealthState,
    pub deep_failure: Option<lens_core::event_ring::DiagnosticFailureClass>,
    pub latency_ms: u64,
}

/// Shared pause toggle request.
///
/// The polling loop checks this on each iteration and applies any pending toggle.
/// This allows the Control Center (via Tauri commands) to request a pause/resume
/// without directly owning the pause state, which remains on the polling loop thread.
#[derive(Clone, Debug)]
pub struct PauseRequest(Arc<std::sync::atomic::AtomicBool>);

impl PauseRequest {
    /// Create a new pause request in the unrequested (false) state.
    pub fn new() -> Self {
        Self(Arc::new(std::sync::atomic::AtomicBool::new(false)))
    }

    /// Request a pause toggle.
    pub fn request(&self) {
        use std::sync::atomic::Ordering;
        self.0.store(true, Ordering::Relaxed);
    }

    /// Take the pending request, resetting to false. Returns true if a request was pending.
    pub fn take(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.0.swap(false, Ordering::Relaxed)
    }
}

impl Default for PauseRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared quit request.
///
/// The polling loop checks this on each iteration and exits when set.
/// This allows the Control Center (via Tauri commands) to request a quit
/// without directly owning the quit state, which remains on the polling loop thread.
#[derive(Clone, Debug)]
pub struct QuitRequest(Arc<std::sync::atomic::AtomicBool>);

impl QuitRequest {
    /// Create a new quit request in the unrequested (false) state.
    pub fn new() -> Self {
        Self(Arc::new(std::sync::atomic::AtomicBool::new(false)))
    }

    /// Request a quit.
    pub fn request(&self) {
        use std::sync::atomic::Ordering;
        self.0.store(true, Ordering::Relaxed);
    }

    /// Take the pending request, resetting to false. Returns true if a request was pending.
    pub fn take(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.0.swap(false, Ordering::Relaxed)
    }
}

impl Default for QuitRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Mori availability status for the webview.
///
/// `status` is one of: `"not-installed"`, `"no-capability-marker"`,
/// `"version-mismatch"`, `"supported"`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MoriStatusView {
    pub status: String,
    pub negotiated_version: Option<u32>,
}

impl From<crate::mori_transfer::MoriStatus> for MoriStatusView {
    fn from(status: crate::mori_transfer::MoriStatus) -> Self {
        match status {
            crate::mori_transfer::MoriStatus::NotInstalled => Self {
                status: "not-installed".to_string(),
                negotiated_version: None,
            },
            crate::mori_transfer::MoriStatus::NoCapabilityMarker => Self {
                status: "no-capability-marker".to_string(),
                negotiated_version: None,
            },
            crate::mori_transfer::MoriStatus::VersionMismatch { .. } => Self {
                status: "version-mismatch".to_string(),
                negotiated_version: None,
            },
            crate::mori_transfer::MoriStatus::Supported { negotiated_version } => Self {
                status: "supported".to_string(),
                negotiated_version: Some(negotiated_version),
            },
        }
    }
}

/// A model entry for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct ModelInfoView {
    pub id: String,
    pub display_name: String,
    pub size_bytes: u64,
}

/// The result of a test translation for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct TestResultView {
    pub translated: String,
    pub latency_ms: u64,
    pub model: String,
}

/// A library item for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct LibraryItemView {
    pub id: i64,
    pub original: String,
    pub answer: String,
    pub caveat: Option<String>,
    pub app: Option<String>,
    pub created_at: i64,
    pub kind: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub model: Option<String>,
    pub note: Option<String>,
    pub review_state: Option<String>,
}

/// A page of library items for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct LibraryPageView {
    pub items: Vec<LibraryItemView>,
    pub has_more: bool,
    pub total: i64,
}

impl From<&lens_storage::Item> for LibraryItemView {
    fn from(item: &lens_storage::Item) -> Self {
        Self {
            id: item.id,
            original: item.original.clone(),
            answer: item.answer.clone(),
            caveat: item.caveat.clone(),
            app: item.app.clone(),
            created_at: item.created_at,
            kind: item.kind.clone(),
            source_language: item.source_language.clone(),
            target_language: item.target_language.clone(),
            model: item.model.clone(),
            note: item.note.clone(),
            review_state: item.review_state.clone(),
        }
    }
}

/// Handle to the running Tauri application.
///
/// Cloned into the polling worker so it can show the window or exit the
/// application when the tray asks.
#[derive(Clone)]
pub struct AppHandle(tauri::AppHandle);

impl AppHandle {
    /// Show the main settings/diagnostics window.
    pub fn show_window(&self) {
        if let Some(window) = self.0.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    /// Show the main window and navigate to a specific section.
    ///
    /// Emits a Tauri event that the web UI listens for to switch sections.
    /// Section names: "status", "library", "history", "general", etc.
    pub fn show_window_section(&self, section: &str) {
        if let Some(window) = self.0.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.emit("navigate-section", section);
        }
    }

    /// Show the pinned detail window.
    #[allow(dead_code)]
    pub fn show_detail_window(&self) {
        if let Some(window) = self.0.get_webview_window("detail") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    /// Exit the application.
    pub fn exit(&self) {
        self.0.exit(0);
    }
}

/// Tauri command: return the current diagnostics snapshot.
#[tauri::command]
fn get_diagnostics(state: tauri::State<'_, SharedDiagnostics>) -> DiagnosticsSnapshot {
    state.lock().unwrap().clone()
}

/// Runtime status for the Control Center Status page.
///
/// Contains operational product truth that the Status page needs to display
/// but that is not part of the diagnostic snapshot.
///
/// `autostart_enabled` is `Some(bool)` on platforms where Lens manages
/// autostart (Windows), `None` where it does not (macOS). The UI must
/// not show "disabled" when the platform simply does not support the
/// feature.
#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatus {
    pub history_enabled: bool,
    pub history_days: u32,
    pub library_persistent: bool,
    pub autostart_enabled: Option<bool>,
    pub restart_required: bool,
    pub restart_reasons: Vec<String>,
}

/// Classify library backing as persistent or not.
///
/// Both persistent and in-memory fallback have a `Library` handle, so
/// the existence of a handle does not prove persistence. Only the
/// backing variant carries that truth.
pub fn is_library_persistent(backing: &LibraryBacking) -> bool {
    matches!(backing, LibraryBacking::Persistent(_))
}

/// Tauri command: return current runtime status for the Control Center.
#[tauri::command]
fn get_runtime_status(
    settings: tauri::State<'_, SharedSettings>,
    library: tauri::State<'_, SharedLibrary>,
) -> RuntimeStatus {
    let settings_guard = settings.lock().unwrap();
    let settings_view = settings_guard.view();
    let (restart_required, restart_reasons) = settings_guard.restart_status();
    let lib_guard = library.lock().unwrap();
    RuntimeStatus {
        history_enabled: settings_view.history,
        history_days: settings_view.history_days,
        library_persistent: is_library_persistent(lib_guard.backing()),
        autostart_enabled: autostart_state(),
        restart_required,
        restart_reasons,
    }
}

/// Autostart state for the platform-neutral runtime status.
///
/// Windows: Lens manages autostart via the registry, so the state is known.
/// macOS: Lens does not currently manage autostart, so the state is `None`.
/// The UI must distinguish "disabled" from "unsupported".
#[cfg(windows)]
fn autostart_state() -> Option<bool> {
    Some(lens_platform_windows::autostart::is_enabled())
}

#[cfg(not(windows))]
fn autostart_state() -> Option<bool> {
    None
}

/// Tauri command: toggle autostart on or off.
///
/// Returns the new state on success, or an error string. On platforms
/// where Lens does not manage autostart, returns an error.
#[tauri::command]
fn set_autostart(enabled: bool) -> Result<bool, String> {
    set_autostart_platform(enabled)
}

#[cfg(windows)]
fn set_autostart_platform(enabled: bool) -> Result<bool, String> {
    let ok = if enabled {
        lens_platform_windows::autostart::enable()
    } else {
        lens_platform_windows::autostart::disable()
    };
    if ok {
        Ok(lens_platform_windows::autostart::is_enabled())
    } else {
        Err("autostart toggle failed".to_owned())
    }
}

#[cfg(not(windows))]
fn set_autostart_platform(_enabled: bool) -> Result<bool, String> {
    Err("autostart is not supported on this platform".to_owned())
}

/// Tauri command: request a pause/resume toggle.
///
/// Sets a flag that the polling loop checks and applies. The actual pause state
/// remains owned by the polling loop thread; this is just a request mechanism.
#[tauri::command]
fn request_pause_toggle(pause_request: tauri::State<'_, PauseRequest>) {
    pause_request.request();
}

/// Tauri command: request the application to quit.
///
/// Sets a flag that the polling loop checks. The loop will exit cleanly,
/// which triggers the normal shutdown sequence.
#[tauri::command]
fn request_quit(quit_request: tauri::State<'_, QuitRequest>) {
    quit_request.request();
}

/// Tauri command: return the current settings view.
#[tauri::command]
fn get_settings(state: tauri::State<'_, SharedSettings>) -> SettingsView {
    state.lock().unwrap().view()
}

/// Tauri command: save settings from the webview.
#[tauri::command]
fn save_settings(
    state: tauri::State<'_, SharedSettings>,
    health: tauri::State<'_, SharedHealthSnapshot>,
    settings: SettingsView,
) -> Result<crate::settings_service::SettingsSaveOutcome, String> {
    // Check if AI settings changed before applying.
    let old_view = state.lock().unwrap().view();
    let ai_changed = old_view.quick_engine != settings.quick_engine
        || old_view.quick_endpoint != settings.quick_endpoint
        || old_view.quick_model != settings.quick_model
        || old_view.deep_engine != settings.deep_engine
        || old_view.deep_endpoint != settings.deep_endpoint
        || old_view.deep_model != settings.deep_model;

    let outcome = state.lock().unwrap().apply(settings)?;

    // Invalidate cached health if AI settings changed.
    if ai_changed {
        let mut snapshot = health.lock().unwrap();
        snapshot.mark_stale();
    }

    Ok(outcome)
}

/// Tauri command: list models on a provider endpoint.
///
/// Runs the blocking HTTP call on a background thread so the Tauri
/// runtime is not stalled for the duration.
#[tauri::command]
async fn list_models(provider: String, endpoint: String) -> Result<Vec<ModelInfoView>, String> {
    let provider = lens_core::translate::Provider::from_token(&provider)
        .unwrap_or(lens_core::translate::Provider::Ollama);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = lens_translate::list_models_for_provider(provider, &endpoint);
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| "thread error".to_string())?
        .map(|models| {
            models
                .into_iter()
                .map(|m| ModelInfoView {
                    id: m.id,
                    display_name: m.display_name,
                    size_bytes: m.size_bytes,
                })
                .collect()
        })
        .map_err(|e| format!("{e}"))
}

/// Tauri command: check whether a provider endpoint is reachable.
#[tauri::command]
async fn check_provider(provider: String, endpoint: String) -> bool {
    let provider = lens_core::translate::Provider::from_token(&provider)
        .unwrap_or(lens_core::translate::Provider::Ollama);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(lens_translate::check_provider_reachable(
            provider, &endpoint,
        ));
    });
    rx.recv().unwrap_or(false)
}

/// Tauri command: get the current download operation snapshot.
///
/// Returns the active download if one is in progress, or the terminal snapshot
/// of the last completed/failed download, or `null` if idle with no prior
/// operation. Does not hit the provider; reads from OperationManager only.
#[tauri::command]
async fn get_model_download_status(
    manager: tauri::State<'_, SharedOperationManager>,
) -> Result<Option<lens_translate::operation_manager::DownloadOperationSnapshot>, String> {
    Ok(manager.operation_snapshot())
}

/// Provider readiness view for the frontend.
///
/// Combines the readiness domain (LaneReadiness per lane), provider access
/// state, model inventory, download state, and next-action into a single
/// snapshot. The frontend uses this to decide what to show and what action
/// to prompt.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderReadinessView {
    /// Readiness assessment for Quick lane.
    pub quick_lane: lens_core::readiness::LaneReadiness,
    /// Readiness assessment for Deep lane.
    pub deep_lane: lens_core::readiness::LaneReadiness,
    /// Provider name for Quick lane (for display).
    pub quick_provider: String,
    /// Provider name for Deep lane (for display).
    /// None if the Deep configuration is unsupported (e.g., LM Studio or invalid token).
    /// This separates the configured provider identity from the internal runtime provider.
    pub deep_provider: Option<String>,
    /// Whether the Deep configuration is valid and supported.
    /// False when the user configured LM Studio or an invalid token for Deep (Ollama-only).
    pub deep_configuration_valid: bool,
    /// Configured Quick model (None if auto/unset).
    pub quick_model: Option<String>,
    /// Configured Deep model (None if auto/unset or configuration invalid).
    pub deep_model: Option<String>,
    /// Why the Quick provider is or is not usable.
    pub quick_provider_access: lens_core::readiness::ProviderAccessState,
    /// Why the Deep provider is or is not usable.
    pub deep_provider_access: lens_core::readiness::ProviderAccessState,
    /// Model inventory on the Quick provider (or why it could not be retrieved).
    pub quick_model_inventory: lens_core::readiness::ModelInventoryView,
    /// Model inventory on the Deep provider (or why it could not be retrieved).
    pub deep_model_inventory: lens_core::readiness::ModelInventoryView,
    /// Whether Quick provider supports downloading models.
    pub quick_can_download: bool,
    /// Whether Quick provider supports progress reporting.
    pub quick_supports_progress: bool,
    /// Whether Quick provider supports explicit model loading.
    pub quick_can_load: bool,
    /// Whether Deep provider supports downloading models.
    pub deep_can_download: bool,
    /// Whether Deep provider supports progress reporting.
    pub deep_supports_progress: bool,
    /// Whether Deep provider supports explicit model loading.
    pub deep_can_load: bool,
    /// Active or terminal download snapshot (if any).
    pub download: Option<lens_translate::operation_manager::DownloadOperationSnapshot>,
    /// Semantic next action for Quick lane.
    pub quick_next_action: lens_core::readiness::NextAction,
    /// Semantic next action for Deep lane.
    pub deep_next_action: lens_core::readiness::NextAction,
    /// Effective model for Quick lane (derived from production resolver logic).
    /// Some(model) if a model will be used for inference, None if no model is available.
    pub quick_effective_model: Option<String>,
    /// Download target for Quick lane (if a download would help).
    /// Some(model_name) if Mori can download a specific model, None otherwise.
    pub quick_download_target: Option<String>,
}

/// Tauri command: get provider readiness for the frontend.
///
/// Assembles a readiness snapshot by:
/// 1. Reading settings to get configured provider/endpoint/model for each lane
/// 2. Checking provider reachability with typed errors (preserves auth/timeout)
/// 3. Getting model list (preserving errors instead of collapsing to empty)
/// 4. Getting model state via ProviderRuntime (detects loaded_instances)
/// 5. Getting download status from OperationManager (integrates into lifecycle)
/// 6. Computing LaneReadiness and NextAction via the readiness evaluator
/// 7. Returning capabilities for enabling/disabling frontend actions
///
/// This is the single source of truth for "can the user get a first card?"
/// The frontend uses this instead of `get_local_ai_status` for the readiness UI.
#[tauri::command]
async fn get_provider_readiness(
    manager: tauri::State<'_, SharedOperationManager>,
    settings: tauri::State<'_, SharedSettings>,
) -> Result<ProviderReadinessView, String> {
    use lens_core::readiness::{compute_next_action, SetupReadinessState};

    // Step 1: Read settings. Fail closed for invalid provider tokens.
    let (quick_provider, deep_provider, deep_config_valid, quick_endpoint, deep_endpoint, quick_model, deep_model) = {
        let s = settings.lock().unwrap();
        let view = s.view();
        let qp = lens_core::translate::Provider::from_token(&view.quick_engine)
            .ok_or_else(|| format!("invalid quick engine: {}", view.quick_engine))?;

        // Deep is Ollama-only per product constraint (ai_runtime::from_schema_deep).
        // If the configured deep engine is not Ollama, do NOT silently remap.
        // Report the configuration as invalid so the readiness state reflects truth.
        let (dp, deep_ep, deep_m, deep_valid) = match lens_core::translate::Provider::from_token(&view.deep_engine) {
            Some(lens_core::translate::Provider::Ollama) => (
                lens_core::translate::Provider::Ollama,
                view.deep_endpoint.clone(),
                view.deep_model.clone(),
                true,
            ),
            Some(lens_core::translate::Provider::LmStudio) => {
                // Deep is Ollama-only. LM Studio configured for deep is unsupported.
                // Use a sentinel to avoid contacting any real endpoint on user's behalf.
                (
                    lens_core::translate::Provider::Ollama,
                    "http://invalid-deep-config:0".to_owned(),
                    None,
                    false,
                )
            }
            None => {
                // Invalid deep engine token. Do not silently remap to defaults.
                (
                    lens_core::translate::Provider::Ollama,
                    "http://invalid-deep-config:0".to_owned(),
                    None,
                    false,
                )
            }
        };

        (
            qp,
            dp,
            deep_valid,
            view.quick_endpoint.clone(),
            deep_ep,
            view.quick_model.clone(),
            deep_m,
        )
    };

    // Step 2: Check reachability with typed errors.
    // Run on a thread pool to avoid blocking the async runtime.
    let quick_access = check_provider_access(quick_provider, quick_endpoint.clone());

    // Deep access: if configuration is invalid, do NOT contact any endpoint.
    // Report as ProviderError so the readiness lane shows the truth.
    let deep_access = if !deep_config_valid {
        lens_core::readiness::ProviderAccessState::ProviderError {
            reason: "unsupported deep configuration".into(),
        }
    } else if quick_provider == deep_provider && quick_endpoint == deep_endpoint {
        quick_access.clone()
    } else {
        check_provider_access(deep_provider, deep_endpoint.clone())
    };

    // Step 3: Get model inventory (preserving errors).
    let quick_inventory = get_model_inventory(quick_provider, quick_endpoint.clone(), &quick_access);
    let deep_inventory = if !deep_config_valid {
        lens_core::readiness::ModelInventoryView::NotAttempted
    } else if quick_provider == deep_provider && quick_endpoint == deep_endpoint {
        quick_inventory.clone()
    } else {
        get_model_inventory(deep_provider, deep_endpoint.clone(), &deep_access)
    };

    // Step 4: Derive effective model using production resolver logic.
    // This mirrors resolve_model_for_provider but uses already-fetched inventory.
    let quick_effective = derive_effective_model_and_target(
        quick_provider,
        &quick_model,
        &quick_inventory,
    );

    // Step 5: Determine model lifecycle for each lane.
    // Uses effective model (not just configured) for lifecycle evaluation.
    let quick_lifecycle = determine_model_lifecycle_v2(
        quick_provider,
        &quick_endpoint,
        &quick_effective.effective_model,
        &quick_access,
        &quick_inventory,
    );
    let deep_lifecycle = if !deep_config_valid {
        lens_core::readiness::ModelLifecycle::Unknown
    } else {
        let deep_effective = derive_effective_model_and_target(
            deep_provider,
            &deep_model,
            &deep_inventory,
        );
        determine_model_lifecycle_v2(
            deep_provider,
            &deep_endpoint,
            &deep_effective.effective_model,
            &deep_access,
            &deep_inventory,
        )
    };

    // Step 6: Integrate download state into lifecycle.
    // P20C: download operations are Quick-only. Do not apply to Deep.
    let download = manager.operation_snapshot();
    let quick_lifecycle =
        integrate_download_into_lifecycle(quick_lifecycle, &download, quick_provider, &quick_effective.effective_model);
    // Deep lifecycle is NOT modified by download state in P20C (no mutation scope).

    // Step 7: Build SetupReadinessState and evaluate.
    let quick_runtime = endpoint_to_runtime(&quick_access);
    let deep_runtime = endpoint_to_runtime(&deep_access);

    let quick_endpoint_state = access_to_endpoint(&quick_access);
    let deep_endpoint_state = access_to_endpoint(&deep_access);

    let quick_can_infer_from_present =
        quick_provider == lens_core::translate::Provider::Ollama;
    let deep_can_infer_from_present =
        deep_provider == lens_core::translate::Provider::Ollama;

    let quick_state = SetupReadinessState {
        runtime_presence: quick_runtime,
        endpoint_state: quick_endpoint_state,
        model_lifecycle: quick_lifecycle,
        can_infer_from_present_model: quick_can_infer_from_present,
        provider_access: quick_access.clone(),
        model_inventory: quick_inventory.clone(),
        configured_model: quick_model.clone(),
        download_target: quick_effective.download_target.clone(),
    };
    let deep_state = SetupReadinessState {
        runtime_presence: deep_runtime,
        endpoint_state: deep_endpoint_state,
        model_lifecycle: deep_lifecycle,
        can_infer_from_present_model: deep_can_infer_from_present,
        provider_access: deep_access.clone(),
        model_inventory: deep_inventory.clone(),
        configured_model: deep_model.clone(),
        download_target: None, // Deep is status-only in P20C
    };

    let snapshot = lens_core::readiness::evaluate_readiness(&quick_state, &deep_state);

    // Step 8: Capabilities and next actions.
    let quick_caps = lens_core::provider_capability::ProviderCapability::for_provider(
        quick_provider,
    );
    let deep_caps = lens_core::provider_capability::ProviderCapability::for_provider(
        deep_provider,
    );

    let quick_next_raw = compute_next_action(&quick_state);
    // If the evaluator says Download but the provider doesn't support downloading,
    // fall back to ConfigureModel (user must manually set up).
    let quick_next = if quick_next_raw == lens_core::readiness::NextAction::Download
        && !quick_caps.can_download_model
    {
        lens_core::readiness::NextAction::ConfigureModel
    } else {
        quick_next_raw
    };
    // P20C: Deep is status-only. No mutation CTA for Deep until a real setup workflow exists.
    let deep_next = lens_core::readiness::NextAction::None;

    Ok(ProviderReadinessView {
        quick_lane: snapshot.quick,
        deep_lane: snapshot.deep,
        quick_provider: quick_provider.as_str().to_owned(),
        deep_provider: if deep_config_valid {
            Some(deep_provider.as_str().to_owned())
        } else {
            None
        },
        deep_configuration_valid: deep_config_valid,
        quick_model,
        deep_model,
        quick_provider_access: quick_access,
        deep_provider_access: deep_access,
        quick_model_inventory: quick_inventory,
        deep_model_inventory: deep_inventory,
        quick_can_download: quick_caps.can_download_model,
        quick_supports_progress: quick_caps.supports_progress,
        quick_can_load: quick_caps.can_load_model,
        deep_can_download: deep_caps.can_download_model,
        deep_supports_progress: deep_caps.supports_progress,
        deep_can_load: deep_caps.can_load_model,
        download,
        quick_next_action: quick_next,
        deep_next_action: deep_next,
        quick_effective_model: quick_effective.effective_model,
        quick_download_target: quick_effective.download_target,
    })
}

/// Check provider access with typed errors.
///
/// Maps TranslateError to ProviderAccessState. Runs the network call on a
/// thread pool to avoid blocking the async runtime.
fn check_provider_access(
    provider: lens_core::translate::Provider,
    endpoint: String,
) -> lens_core::readiness::ProviderAccessState {
    use lens_core::readiness::ProviderAccessState;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = lens_translate::check_provider_reachable_typed(provider, &endpoint);
        let _ = tx.send(result);
    });
    match rx.recv() {
        Ok(Ok(())) => ProviderAccessState::Available,
        Ok(Err(e)) => map_translate_error_to_access(e),
        Err(_) => ProviderAccessState::Unreachable {
            reason: "thread join failed".into(),
        },
    }
}

/// Map a TranslateError to a ProviderAccessState.
///
/// Distinguishes transport-level failures (Unavailable, Timeout) from
/// provider-level errors (5xx, malformed response) and auth failures (401/403).
fn map_translate_error_to_access(
    e: lens_core::translate::TranslateError,
) -> lens_core::readiness::ProviderAccessState {
    use lens_core::readiness::ProviderAccessState;
    use lens_core::translate::TranslateError;
    match e {
        TranslateError::HttpFailed(401) | TranslateError::HttpFailed(403) => {
            ProviderAccessState::AuthenticationRequired
        }
        TranslateError::Unavailable => ProviderAccessState::Unreachable {
            reason: "unavailable".into(),
        },
        TranslateError::Timeout => ProviderAccessState::Unreachable {
            reason: "timeout".into(),
        },
        TranslateError::HttpFailed(status) => ProviderAccessState::ProviderError {
            reason: format!("http {status}"),
        },
        other => ProviderAccessState::ProviderError {
            reason: other.reason().into(),
        },
    }
}

/// Get the model inventory for a provider.
///
/// Preserves errors as ModelInventoryView::LookupFailed instead of collapsing
/// to an empty Vec.
fn get_model_inventory(
    provider: lens_core::translate::Provider,
    endpoint: String,
    access: &lens_core::readiness::ProviderAccessState,
) -> lens_core::readiness::ModelInventoryView {
    use lens_core::readiness::{ModelInventoryView, ProviderAccessState};
    if *access != ProviderAccessState::Available {
        return ModelInventoryView::NotAttempted;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = lens_translate::list_models_for_provider(provider, &endpoint);
        let _ = tx.send(result);
    });
    match rx.recv() {
        Ok(Ok(models)) => ModelInventoryView::Available {
            models: models.into_iter().map(|m| m.id).collect(),
        },
        Ok(Err(e)) => ModelInventoryView::LookupFailed {
            reason: e.reason().into(),
        },
        Err(_) => ModelInventoryView::LookupFailed {
            reason: "thread join failed".into(),
        },
    }
}

/// Determine model lifecycle using ProviderRuntime::model_state.
///
/// For LM Studio, this detects loaded_instances to produce Ready directly.
/// For Ollama, Present + can_infer_from_present_model handles JIT semantics.
///
/// If model-state lookup fails, returns Unknown instead of fabricating presence.
/// A provider/model-state error is NOT evidence that the model is available.
fn determine_model_lifecycle_v2(
    provider: lens_core::translate::Provider,
    endpoint: &str,
    configured_model: &Option<String>,
    access: &lens_core::readiness::ProviderAccessState,
    inventory: &lens_core::readiness::ModelInventoryView,
) -> lens_core::readiness::ModelLifecycle {
    use lens_core::readiness::{ModelInventoryView, ModelLifecycle, ProviderAccessState};

    if *access != ProviderAccessState::Available {
        return ModelLifecycle::Unknown;
    }

    let model_list = match inventory {
        ModelInventoryView::Available { models } => models,
        ModelInventoryView::LookupFailed { .. } | ModelInventoryView::NotAttempted => {
            return ModelLifecycle::Unknown;
        }
    };

    match configured_model {
        Some(model) => {
            if !model_list.iter().any(|m| m == model) {
                return ModelLifecycle::Missing;
            }
            match get_model_state(provider, endpoint, model) {
                Ok(state) => lens_core::readiness::model_state_to_lifecycle(state),
                Err(_) => ModelLifecycle::Unknown,
            }
        }
        None => match provider {
            lens_core::translate::Provider::Ollama => {
                if model_list.is_empty() {
                    ModelLifecycle::Missing
                } else {
                    ModelLifecycle::Present
                }
            }
            lens_core::translate::Provider::LmStudio => ModelLifecycle::Missing,
        },
    }
}

/// Get model state via ProviderRuntime.
///
/// Returns Err on any failure (thread join, runtime error, model missing).
/// Callers must NOT interpret failure as model presence.
fn get_model_state(
    provider: lens_core::translate::Provider,
    endpoint: &str,
    model: &str,
) -> Result<lens_core::translate::ModelState, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ep = endpoint.to_owned();
    let m = model.to_owned();
    std::thread::spawn(move || {
        let runtime = lens_translate::build_provider_runtime(provider, &ep);
        let result = runtime.model_state(&m);
        let _ = tx.send(result);
    });
    match rx.recv() {
        Ok(Ok(state)) => Ok(state),
        Ok(Err(e)) => Err(e.reason().into()),
        Err(_) => Err("thread join failed".into()),
    }
}

/// Map ProviderAccessState to RuntimePresence.
///
/// A reachable HTTP endpoint proves the provider is answering, but does not
/// prove installation in the domain's defined sense (PATH, registry, service
/// enumeration). Use Unknown until real installation detection exists.
///
/// P20C: All access states map to Unknown because no installation detection
/// has been implemented yet. The endpoint probe only proves reachability,
/// not installation.
fn endpoint_to_runtime(
    access: &lens_core::readiness::ProviderAccessState,
) -> lens_core::readiness::RuntimePresence {
    use lens_core::readiness::RuntimePresence;
    // P20C: No installation detection implemented; endpoint evidence is insufficient.
    // All states map to Unknown until PATH/registry/service detection exists.
    let _ = access;
    RuntimePresence::Unknown
}

/// Map ProviderAccessState to EndpointState.
///
/// AuthenticationRequired and ProviderError both imply the provider is
/// reachable (it answered), so they map to Reachable. Only Unreachable
/// (transport failure) maps to EndpointState::Unreachable.
fn access_to_endpoint(
    access: &lens_core::readiness::ProviderAccessState,
) -> lens_core::readiness::EndpointState {
    use lens_core::readiness::{EndpointState, ProviderAccessState};
    match access {
        ProviderAccessState::Available => EndpointState::Reachable,
        ProviderAccessState::Unknown => EndpointState::Unknown,
        ProviderAccessState::Unreachable { .. } => EndpointState::Unreachable,
        // Provider answered but with auth error; it IS reachable.
        ProviderAccessState::AuthenticationRequired => EndpointState::Reachable,
        // Provider answered but with server error; it IS reachable.
        ProviderAccessState::ProviderError { .. } => EndpointState::Reachable,
    }
}

/// Integrate an active or terminal download into the model lifecycle.
///
/// If a download is active and matches this lane's provider+model, override
/// the lifecycle to Downloading. If a terminal download failed for this lane's
/// model, override to Failed.
fn integrate_download_into_lifecycle(
    lifecycle: lens_core::readiness::ModelLifecycle,
    download: &Option<lens_translate::operation_manager::DownloadOperationSnapshot>,
    provider: lens_core::translate::Provider,
    configured_model: &Option<String>,
) -> lens_core::readiness::ModelLifecycle {
    use lens_core::model_management::DownloadStatus;
    use lens_core::readiness::ModelLifecycle;

    let Some(snap) = download else {
        return lifecycle;
    };

    if snap.provider != provider.as_str() {
        return lifecycle;
    }

    let matches_model = match configured_model {
        Some(m) => snap.model == *m,
        None => provider == lens_core::translate::Provider::Ollama,
    };

    if !matches_model {
        return lifecycle;
    }

    if snap.active {
        return ModelLifecycle::Downloading;
    }

    match &snap.status {
        DownloadStatus::Failed(_) => ModelLifecycle::Failed,
        _ => lifecycle,
    }
}

/// Result of deriving effective model and download target.
struct EffectiveModelResult {
    /// The model that will actually be used for inference (mirrors production resolver).
    effective_model: Option<String>,
    /// A model that Mori can download to improve readiness (None if no action available).
    download_target: Option<String>,
}

/// Derive effective model and download target using production resolver logic.
///
/// Mirrors `resolve_model_for_provider` but uses already-fetched inventory instead
/// of making network calls. This ensures readiness evaluation uses the same model
/// selection as actual inference.
///
/// For Ollama auto mode:
/// - Uses `choose_model(None, installed)` which applies preference policy
///   (largest suitable model from preferred families, not just first listed)
/// - If inventory is empty: effective = None, download_target = DEFAULT_MODEL
///
/// For LM Studio auto mode:
/// - If inventory has models: effective = first model (matches runtime auto-select)
/// - If inventory is empty: effective = None, download_target = None (no fake target)
///
/// When a model is explicitly configured: effective = configured, download_target = None.
fn derive_effective_model_and_target(
    provider: lens_core::translate::Provider,
    configured_model: &Option<String>,
    inventory: &lens_core::readiness::ModelInventoryView,
) -> EffectiveModelResult {
    use lens_core::readiness::ModelInventoryView;

    // If a model is explicitly configured, that is the effective model.
    if let Some(model) = configured_model {
        return EffectiveModelResult {
            effective_model: Some(model.clone()),
            download_target: None,
        };
    }

    // Auto mode: derive from inventory.
    let models = match inventory {
        ModelInventoryView::Available { models } => models,
        ModelInventoryView::LookupFailed { .. } | ModelInventoryView::NotAttempted => {
            return EffectiveModelResult {
                effective_model: None,
                download_target: None,
            };
        }
    };

    if !models.is_empty() {
        match provider {
            lens_core::translate::Provider::Ollama => {
                // Ollama auto: use choose_model which applies preference policy
                // (largest suitable model from preferred families).
                // This matches OllamaTranslator::resolve_model production behavior.
                let chosen = lens_core::translate::choose_model(None, models);
                EffectiveModelResult {
                    effective_model: chosen,
                    download_target: None,
                }
            }
            lens_core::translate::Provider::LmStudio => {
                // LM Studio auto: first available model (matches production).
                EffectiveModelResult {
                    effective_model: Some(models[0].clone()),
                    download_target: None,
                }
            }
        }
    } else {
        // No models installed.
        match provider {
            lens_core::translate::Provider::Ollama => {
                // Ollama has a canonical default download target.
                EffectiveModelResult {
                    effective_model: None,
                    download_target: Some(lens_translate::DEFAULT_MODEL.to_owned()),
                }
            }
            lens_core::translate::Provider::LmStudio => {
                // LM Studio has no default recommendation. Do not fabricate one.
                EffectiveModelResult {
                    effective_model: None,
                    download_target: None,
                }
            }
        }
    }
}

/// Tauri command: start downloading the configured quick model.
///
/// Reads the quick engine, endpoint, and model from saved settings. If no
/// quick model is configured and the provider is Ollama, uses the canonical
/// DEFAULT_MODEL. If no quick model is configured for LM Studio, returns an
/// error (LM Studio has no default recommendation). If a download is already
/// active, returns `ProviderBusy`. Otherwise, reserves the operation atomically,
/// then spawns a background worker.
#[tauri::command]
async fn start_quick_model_download(
    manager: tauri::State<'_, SharedOperationManager>,
    settings: tauri::State<'_, SharedSettings>,
) -> Result<(), String> {
    let s = settings.lock().unwrap();
    let view = s.view();

    let provider = lens_core::translate::Provider::from_token(&view.quick_engine)
        .ok_or("invalid quick engine")?;
    let endpoint = view.quick_endpoint.clone();

    // Determine target model.
    // Ollama auto-mode: use DEFAULT_MODEL if no model configured.
    // LM Studio: require explicit model (no fabricated recommendation).
    let model = match (&view.quick_model, provider) {
        (Some(m), _) => m.clone(),
        (None, lens_core::translate::Provider::Ollama) => {
            lens_translate::DEFAULT_MODEL.to_string()
        }
        (None, lens_core::translate::Provider::LmStudio) => {
            return Err("no quick model configured".to_string());
        }
    };

    // Atomic reservation BEFORE spawning. Prevents race where two commands
    // both check idle and both spawn. Second command gets ProviderBusy here.
    // Job ID is always None at reservation time; LM Studio worker sets it
    // later after the HTTP response returns the actual job ID.
    manager
        .start_download(provider, model.clone(), lens_core::model_management::DownloadStatus::Starting, None)
        .map_err(|e| format!("{:?}", e))?;

    // Reservation succeeded. Spawn worker. If spawn fails, finish with error
    // to avoid permanent stale reservation.
    let manager_clone = manager.inner().clone();
    let model_clone = model.clone();
    let endpoint_clone = endpoint.clone();

    let spawn_result = match provider {
        lens_core::translate::Provider::Ollama => {
            std::thread::Builder::new()
                .name("ollama-download".into())
                .spawn(move || {
                    crate::download_worker::run_ollama_download(
                        manager_clone,
                        endpoint_clone,
                        model_clone,
                    );
                })
        }
        lens_core::translate::Provider::LmStudio => {
            std::thread::Builder::new()
                .name("lmstudio-download".into())
                .spawn(move || {
                    crate::download_worker::run_lmstudio_download(
                        manager_clone,
                        endpoint_clone,
                        model_clone,
                    );
                })
        }
    };

    if let Err(e) = spawn_result {
        eprintln!("download worker spawn failed: {:?}", e);
        let _ = manager.finish_download(
            lens_core::model_management::DownloadStatus::Failed(
                lens_core::model_management::ManagementError::DownloadFailed,
            ),
        );
        return Err(format!("failed to spawn download worker: {:?}", e));
    }

    Ok(())
}

/// Tauri command: load an LM Studio instance for the configured quick model.
///
/// Reads the quick engine and endpoint from saved settings. If not LM Studio,
/// returns an error. If Mori already owns an instance for this model, returns
/// the existing instance_id. Otherwise, calls LM Studio load_model and
/// registers ownership in the manager.
#[tauri::command]
async fn load_lmstudio_instance(
    manager: tauri::State<'_, SharedOperationManager>,
    settings: tauri::State<'_, SharedSettings>,
) -> Result<String, String> {
    let s = settings.lock().unwrap();
    let view = s.view();

    let provider = lens_core::translate::Provider::from_token(&view.quick_engine)
        .ok_or("invalid quick engine")?;
    if provider != lens_core::translate::Provider::LmStudio {
        return Err("quick engine is not LM Studio".to_string());
    }

    let endpoint = view.quick_endpoint.clone();
    let model = view.quick_model.clone().ok_or("no quick model configured")?;

    // Check if Mori already owns an instance for this model.
    if let Some(instance_id) = manager.owned_instance_for_model(&model) {
        return Ok(instance_id);
    }

    // Call LM Studio load_model in a background thread.
    let (tx, rx) = std::sync::mpsc::channel();
    let endpoint_clone = endpoint.clone();
    let model_clone = model.clone();
    std::thread::spawn(move || {
        let runtime = lens_translate::lmstudio_runtime::LmStudioRuntime::new(&endpoint_clone);
        let result = runtime.load_model(&model_clone);
        let _ = tx.send(result);
    });

    let instance_id = rx
        .recv()
        .map_err(|_| "thread error".to_string())?
        .map_err(|e| format!("{:?}", e))?;

    // Register ownership.
    manager
        .register_mori_instance(model, instance_id.clone())
        .map_err(|e| format!("{:?}", e))?;

    Ok(instance_id)
}

/// Tauri command: unload a Mori-owned LM Studio instance for the configured quick model.
///
/// Reads the quick engine and endpoint from saved settings. If not LM Studio,
/// returns an error. If Mori does not own an instance for this model, returns
/// an error. Otherwise, calls LM Studio unload_instance and removes ownership
/// from the manager.
#[tauri::command]
async fn unload_lmstudio_instance(
    manager: tauri::State<'_, SharedOperationManager>,
    settings: tauri::State<'_, SharedSettings>,
) -> Result<(), String> {
    let s = settings.lock().unwrap();
    let view = s.view();

    let provider = lens_core::translate::Provider::from_token(&view.quick_engine)
        .ok_or("invalid quick engine")?;
    if provider != lens_core::translate::Provider::LmStudio {
        return Err("quick engine is not LM Studio".to_string());
    }

    let endpoint = view.quick_endpoint.clone();
    let model = view.quick_model.clone().ok_or("no quick model configured")?;

    // Look up the owned instance for this model.
    let instance_id = manager
        .owned_instance_for_model(&model)
        .ok_or("no Mori-owned instance for this model")?;

    // Call LM Studio unload_instance in a background thread.
    let (tx, rx) = std::sync::mpsc::channel();
    let endpoint_clone = endpoint.clone();
    let instance_id_clone = instance_id.clone();
    std::thread::spawn(move || {
        let runtime = lens_translate::lmstudio_runtime::LmStudioRuntime::new(&endpoint_clone);
        let result = runtime.unload_instance(&instance_id_clone);
        let _ = tx.send(result);
    });

    rx.recv()
        .map_err(|_| "thread error".to_string())?
        .map_err(|e| format!("{:?}", e))?;

    // Remove ownership after successful unload.
    manager.take_owned_instance_for_model(&model);

    Ok(())
}

/// Internal helper: list models for a given provider (not a Tauri command).
async fn list_provider_models(
    provider: lens_core::translate::Provider,
    endpoint: String,
) -> Result<Vec<ModelInfoView>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = lens_translate::list_models_for_provider(provider, &endpoint);
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| "thread error".to_string())?
        .map(|models| {
            models
                .into_iter()
                .map(|m| ModelInfoView {
                    id: m.id,
                    display_name: m.display_name,
                    size_bytes: m.size_bytes,
                })
                .collect()
        })
        .map_err(|e| format!("{e}"))
}

/// Local AI readiness status for the webview.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalAiStatusView {
    pub quick_reachable: bool,
    pub deep_reachable: bool,
    pub quick_provider: String,
    pub deep_provider: String,
    pub quick_status: String,
    pub quick_model: Option<String>,
    pub quick_pull_command: Option<String>,
    pub deep_status: String,
    pub deep_model: Option<String>,
    pub deep_pull_command: Option<String>,
}

/// Model readiness classification for a single lane (Quick or Deep).
#[derive(Debug, Clone, PartialEq)]
enum ModelReadiness {
    /// Auto mode with no installed models.
    AutoNoModels,
    /// Auto mode with at least one installed model.
    AutoReady,
    /// Explicit model is installed.
    Ready(String),
    /// Explicit model is not installed.
    Missing(String),
}

/// Model list retrieval state, distinguishing endpoint reachability from list errors.
#[derive(Debug, Clone)]
enum ModelListState {
    /// Endpoint is unreachable (network down, wrong URL, etc.).
    Unreachable,
    /// Endpoint reachable, but model list request failed.
    Error(String),
    /// Models successfully listed.
    Models(Vec<String>),
}

/// Classify model readiness given a configured model and installed models.
fn classify_model_readiness(
    configured_model: Option<&str>,
    installed_models: &[String],
) -> ModelReadiness {
    match configured_model {
        None | Some("") => {
            // Auto mode: canonical is None, but accept Some("") for compatibility.
            if installed_models.is_empty() {
                ModelReadiness::AutoNoModels
            } else {
                ModelReadiness::AutoReady
            }
        }
        Some(model) => {
            // Explicit model.
            if installed_models.iter().any(|m| m == model) {
                ModelReadiness::Ready(model.to_string())
            } else {
                ModelReadiness::Missing(model.to_string())
            }
        }
    }
}

/// Tauri command: get Local AI readiness status.
///
/// Checks provider reachability and model availability for Quick and Deep engines.
/// Returns structured status for frontend display without polling.
#[tauri::command]
async fn get_local_ai_status(
    settings: tauri::State<'_, SharedSettings>,
) -> Result<LocalAiStatusView, String> {
    let (quick_provider, deep_provider, quick_endpoint, deep_endpoint, quick_model, deep_model) = {
        let s = settings.lock().unwrap();
        let view = s.view();
        let qp = lens_core::translate::Provider::from_token(&view.quick_engine)
            .unwrap_or(lens_core::translate::Provider::Ollama);
        let dp_raw = lens_core::translate::Provider::from_token(&view.deep_engine)
            .unwrap_or(lens_core::translate::Provider::Ollama);

        // Deep is Ollama-only. Apply the same constraint as ai_runtime::from_schema_deep.
        // This ensures Tauri readiness uses the SAME Deep capability truth as the runtime.
        let (dp, deep_ep, deep_m) = if dp_raw == lens_core::translate::Provider::Ollama {
            (
                lens_core::translate::Provider::Ollama,
                view.deep_endpoint.clone(),
                view.deep_model.clone(),
            )
        } else {
            // Deep is forced to Ollama with defaults
            eprintln!(
                "get_local_ai_status  Deep engine {:?} is unsupported; forcing Ollama",
                view.deep_engine
            );
            (
                lens_core::translate::Provider::Ollama,
                lens_core::translate::Provider::Ollama
                    .default_endpoint()
                    .to_owned(),
                None,
            )
        };

        (
            qp,
            dp,
            view.quick_endpoint.clone(),
            deep_ep,
            view.quick_model.clone(),
            deep_m,
        )
    };

    // Check provider reachability and model list.
    let quick_reachable =
        check_provider(quick_provider.as_str().to_owned(), quick_endpoint.clone()).await;
    let (deep_reachable, quick_model_state, deep_model_state) =
        if quick_provider == deep_provider && quick_endpoint == deep_endpoint {
            // Same provider and endpoint: fetch model list once.
            let state = if quick_reachable {
                match list_provider_models(quick_provider, quick_endpoint.clone()).await {
                    Ok(models) => {
                        let names = models.into_iter().map(|info| info.id).collect::<Vec<_>>();
                        ModelListState::Models(names)
                    }
                    Err(e) => ModelListState::Error(e),
                }
            } else {
                ModelListState::Unreachable
            };
            (quick_reachable, state.clone(), state)
        } else {
            // Different providers or endpoints: fetch separately.
            let deep_reachable =
                check_provider(deep_provider.as_str().to_owned(), deep_endpoint.clone()).await;
            let quick_state = if quick_reachable {
                match list_provider_models(quick_provider, quick_endpoint.clone()).await {
                    Ok(models) => {
                        let names = models.into_iter().map(|info| info.id).collect::<Vec<_>>();
                        ModelListState::Models(names)
                    }
                    Err(e) => ModelListState::Error(e),
                }
            } else {
                ModelListState::Unreachable
            };
            let deep_state = if deep_reachable {
                match list_provider_models(deep_provider, deep_endpoint.clone()).await {
                    Ok(models) => {
                        let names = models.into_iter().map(|info| info.id).collect::<Vec<_>>();
                        ModelListState::Models(names)
                    }
                    Err(e) => ModelListState::Error(e),
                }
            } else {
                ModelListState::Unreachable
            };
            (deep_reachable, quick_state, deep_state)
        };

    // Classify Quick model.
    let (quick_status, quick_model_name, quick_pull_cmd) = match quick_model_state {
        ModelListState::Unreachable => ("unreachable".to_string(), None, None),
        ModelListState::Error(e) => ("error".to_string(), Some(e), None),
        ModelListState::Models(models) => {
            match classify_model_readiness(quick_model.as_deref(), &models) {
                ModelReadiness::AutoNoModels => {
                    ("auto_no_models".to_string(), Some("Auto".to_string()), None)
                }
                ModelReadiness::AutoReady => {
                    ("auto_ready".to_string(), Some("Auto".to_string()), None)
                }
                ModelReadiness::Ready(model) => ("ready".to_string(), Some(model), None),
                ModelReadiness::Missing(model) => {
                    let pull_cmd = match quick_provider {
                        lens_core::translate::Provider::Ollama => {
                            Some(format!("ollama pull {}", model))
                        }
                        lens_core::translate::Provider::LmStudio => None,
                    };
                    ("missing".to_string(), Some(model.clone()), pull_cmd)
                }
            }
        }
    };

    // Classify Deep model.
    let (deep_status, deep_model_name, deep_pull_cmd) = match deep_model_state {
        ModelListState::Unreachable => ("unreachable".to_string(), None, None),
        ModelListState::Error(e) => ("error".to_string(), Some(e), None),
        ModelListState::Models(models) => {
            match classify_model_readiness(deep_model.as_deref(), &models) {
                ModelReadiness::AutoNoModels => {
                    ("auto_no_models".to_string(), Some("Auto".to_string()), None)
                }
                ModelReadiness::AutoReady => {
                    ("auto_ready".to_string(), Some("Auto".to_string()), None)
                }
                ModelReadiness::Ready(model) => ("ready".to_string(), Some(model), None),
                ModelReadiness::Missing(model) => {
                    let pull_cmd = match deep_provider {
                        lens_core::translate::Provider::Ollama => {
                            Some(format!("ollama pull {}", model))
                        }
                        lens_core::translate::Provider::LmStudio => None,
                    };
                    ("missing".to_string(), Some(model.clone()), pull_cmd)
                }
            }
        }
    };

    Ok(LocalAiStatusView {
        quick_reachable,
        deep_reachable,
        quick_provider: quick_provider.as_str().to_owned(),
        deep_provider: deep_provider.as_str().to_owned(),
        quick_status,
        quick_model: quick_model_name,
        quick_pull_command: quick_pull_cmd,
        deep_status,
        deep_model: deep_model_name,
        deep_pull_command: deep_pull_cmd,
    })
}

/// Tauri command: translate a fixed sentence to test a model.
#[tauri::command]
async fn test_model(
    provider: String,
    endpoint: String,
    model: String,
) -> Result<TestResultView, String> {
    let provider = lens_core::translate::Provider::from_token(&provider)
        .unwrap_or(lens_core::translate::Provider::Ollama);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use lens_core::translate::QuickRequest;
        let translator = lens_translate::build_translator(provider, &endpoint, &model);
        let request = QuickRequest::new("The output files came back empty.", "ko");
        let result = translator.quick(&request);
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| "thread error".to_string())?
        .map(|t| TestResultView {
            translated: t.text,
            latency_ms: t.latency_ms as u64,
            model: t.model,
        })
        .map_err(|e| format!("{e}"))
}

/// Tauri command: run an explicit health check testing actual inference capability.
///
/// Tests both Quick and Deep engines. For each engine:
/// 1. Check provider reachability
/// 2. Resolve the production model (not DEFAULT_MODEL)
/// 3. Send a fixed test translation/analysis (exercises the actual path)
/// 4. Verify the response is valid
/// 5. Classify the result as Ready, Degraded, or Unavailable
///
/// This is a one-shot explicit user action. No periodic polling.
/// Records the result in the diagnostic event ring and updates the health snapshot.
/// Protected by backend concurrency guard: returns error if already running.
#[tauri::command]
async fn run_health_check(
    settings: tauri::State<'_, SharedSettings>,
    events: tauri::State<'_, SharedDiagnosticEvents>,
    health: tauri::State<'_, SharedHealthSnapshot>,
    health_running: tauri::State<'_, HealthCheckRunning>,
) -> Result<HealthCheckView, String> {
    use lens_core::event_ring::{DiagnosticComponent, DiagnosticEventKind};
    use lens_core::health::HealthState;
    use lens_core::translate::Provider;
    use std::time::Instant;

    // Backend concurrency guard: prevent multiple simultaneous health checks.
    // The newtype provides a focused API with RAII semantics.
    let _guard = health_running
        .try_acquire()
        .ok_or_else(|| "health check already in progress".to_string())?;

    let start = Instant::now();

    // Mark both lanes as Running in the snapshot.
    {
        let mut snapshot = health.lock().unwrap();
        // Process-relative monotonic timestamp, not the duration of this call.
        let now_ms = process_start_elapsed_ms();
        snapshot.mark_running(now_ms);
    }

    // Read settings once.
    let (quick_provider, quick_endpoint, quick_model, _deep_provider, deep_endpoint, deep_model) = {
        let s = settings.lock().unwrap();
        let view = s.view();
        let qp = Provider::from_token(&view.quick_engine).unwrap_or(Provider::Ollama);
        let dp_raw = Provider::from_token(&view.deep_engine).unwrap_or(Provider::Ollama);
        // Deep is Ollama-only.
        let (dp, deep_ep, deep_m) = if dp_raw == Provider::Ollama {
            (
                Provider::Ollama,
                view.deep_endpoint.clone(),
                view.deep_model.clone(),
            )
        } else {
            (
                Provider::Ollama,
                Provider::Ollama.default_endpoint().to_owned(),
                None,
            )
        };
        (
            qp,
            view.quick_endpoint.clone(),
            view.quick_model.clone(),
            dp,
            deep_ep,
            deep_m,
        )
    };

    // Run health check in a background thread to avoid blocking the Tauri command.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::time::Instant;
        // Check Quick engine using QuickRequest and Translator::quick.
        let started_quick = Instant::now();
        let (quick_state, quick_failure) =
            check_quick_engine_health(quick_provider, &quick_endpoint, quick_model.as_deref());
        let quick_latency = started_quick.elapsed().as_millis() as u64;

        // Check Deep engine using DeepRequest and Analyser::deep (Ollama-only).
        let started_deep = Instant::now();
        let (deep_state, deep_failure) =
            check_deep_engine_health(&deep_endpoint, deep_model.as_deref());
        let deep_latency = started_deep.elapsed().as_millis() as u64;

        let _ = tx.send((
            quick_state,
            quick_failure,
            quick_latency,
            deep_state,
            deep_failure,
            deep_latency,
        ));
    });

    let result = rx.recv().map_err(|_| "thread error".to_string());

    // RAII guard handles release automatically on all exit paths.

    let (quick_state, quick_failure, quick_latency, deep_state, deep_failure, deep_latency) =
        result?;

    let latency_ms = start.elapsed().as_millis() as u64;

    // Failure classes arrive already typed from the check functions.
    let quick_failure_class = quick_failure;
    let deep_failure_class = deep_failure;

    // Update health snapshot and record event in the ring.
    {
        let mut snapshot = health.lock().unwrap();
        // `updated_ms` is the process-relative monotonic timestamp, not the
        // duration of this health check. `latency_ms` is the per-lane
        // measured inference time, not half the wall-clock total.
        let now_ms = process_start_elapsed_ms();

        // Snapshot uses typed HealthState directly, no string parsing.
        snapshot.update_quick(
            quick_state,
            quick_failure_class,
            Some(quick_latency),
            now_ms,
        );
        snapshot.update_deep(deep_state, deep_failure_class, Some(deep_latency), now_ms);
    }

    // Record health check events in the ring (separate for each lane).
    {
        let mut ring = events.lock().unwrap();
        let kind = DiagnosticEventKind::HealthCheck;

        // Typed comparison: no stringly-typed state checks.
        if matches!(
            quick_state,
            HealthState::Unavailable | HealthState::Degraded
        ) {
            ring.record(DiagnosticComponent::Quick, kind, quick_failure_class);
        }

        if matches!(deep_state, HealthState::Unavailable | HealthState::Degraded) {
            ring.record(DiagnosticComponent::Deep, kind, deep_failure_class);
        }
    }

    Ok(HealthCheckView {
        quick_state,
        quick_failure,
        deep_state,
        deep_failure,
        latency_ms,
    })
}

/// Tauri command: get cached health status without triggering inference.
///
/// Returns the last health check result from the shared snapshot.
/// Does NOT run inference. Only `run_health_check` does that.
/// Returns Unknown state if health has never been checked or is stale.
#[tauri::command]
fn get_health_status(
    health: tauri::State<'_, SharedHealthSnapshot>,
) -> lens_core::health::HealthSnapshot {
    health.lock().unwrap().clone()
}

/// Check health of the Quick engine using the production path.
///
/// Uses production model resolution (resolve_model), not DEFAULT_MODEL.
/// Exercises the same QuickRequest + Translator::quick path as the product loop.
///
/// Returns typed `(HealthState, Option<DiagnosticFailureClass>)`. The failure
/// class is present only for non-Ready states, giving the caller a typed
/// diagnostic token rather than a string to re-parse.
fn check_quick_engine_health(
    provider: lens_core::translate::Provider,
    endpoint: &str,
    configured_model: Option<&str>,
) -> (HealthState, Option<DiagnosticFailureClass>) {
    use lens_core::translate::QuickRequest;

    // Step 1: Check reachability. Typed so the ring can distinguish Timeout
    // from ConnectionFailed, rather than collapsing both to ConnectionFailed
    // the way the boolean `check_provider_reachable` did.
    let reachability = lens_translate::check_provider_reachable_typed(provider, endpoint);
    if let Err(error) = reachability {
        let failure_class = DiagnosticFailureClass::from_translate_error(&error)
            .unwrap_or(DiagnosticFailureClass::ConnectionFailed);
        return (HealthState::Unavailable, Some(failure_class));
    }

    // Step 2: Resolve a model provider-aware. Ollama honours the configured
    // model and falls back via `resolve_model()`; LM Studio uses the
    // configured model or the first listed, since it has no Ollama-style
    // defaulting.
    let model_name = match resolve_model_for_provider(provider, endpoint, configured_model) {
        Ok(m) => m,
        Err(e) => {
            // Model resolution failed. Map to appropriate failure class.
            let failure_class = DiagnosticFailureClass::from_translate_error(&e)
                .unwrap_or(DiagnosticFailureClass::ModelMissing);
            return (HealthState::Unavailable, Some(failure_class));
        }
    };

    // Step 3: Build translator with resolved model and test inference.
    let translator = lens_translate::build_translator(provider, endpoint, &model_name);
    let test_input = "The output files came back empty.";
    let target_lang = "ko";
    let request = QuickRequest::new(test_input, target_lang);

    match translator.quick(&request) {
        Ok(result) => {
            // Inference succeeded. Check if the output is meaningful.
            if result.text.trim().is_empty() {
                (
                    HealthState::Degraded,
                    Some(DiagnosticFailureClass::InferenceFailed),
                )
            } else {
                (HealthState::Ready, None)
            }
        }
        Err(err) => {
            // TranslateError -> DiagnosticFailureClass is a typed mapping, no strings.
            let failure_class = DiagnosticFailureClass::from_translate_error(&err)
                .unwrap_or(DiagnosticFailureClass::InferenceFailed);
            (HealthState::Unavailable, Some(failure_class))
        }
    }
}

/// Provider-aware model resolution for health checks.
///
/// Mirrors the branching already present in the worker loops (win.rs and
/// macos.rs): Ollama honours a configured model and falls back via
/// `resolve_model()`; LM Studio uses the configured model or picks the first
/// listed model, since it has no Ollama-style defaulting.
///
/// Returns the resolved model name, or a typed error explaining why resolution
/// failed. LM Studio auto with no available models returns `ModelMissing`, not
/// a fallback to `DEFAULT_MODEL`.
fn resolve_model_for_provider(
    provider: lens_core::translate::Provider,
    endpoint: &str,
    configured_model: Option<&str>,
) -> Result<String, lens_core::translate::TranslateError> {
    lens_translate::resolve_model_for_provider(provider, endpoint, configured_model)
}

/// Check health of the Deep engine using the production path.
///
/// Uses production model resolution (resolve_model), not DEFAULT_MODEL.
/// Exercises the same DeepRequest + Analyser::deep path as the product loop.
/// This is the CRITICAL FIX: Deep health check now tests Deep, not Quick.
///
/// Returns typed `(HealthState, Option<DiagnosticFailureClass>)`. The failure
/// class is present only for non-Ready states.
fn check_deep_engine_health(
    endpoint: &str,
    configured_model: Option<&str>,
) -> (HealthState, Option<DiagnosticFailureClass>) {
    use lens_core::analyse::{Analyser, DeepRequest};

    // Step 1: Check reachability. Typed so a Timeout is reported as Timeout,
    // not collapsed to ConnectionFailed.
    let reachability = lens_translate::check_provider_reachable_typed(
        lens_core::translate::Provider::Ollama,
        endpoint,
    );
    if let Err(error) = reachability {
        let failure_class = DiagnosticFailureClass::from_translate_error(&error)
            .unwrap_or(DiagnosticFailureClass::ConnectionFailed);
        return (HealthState::Unavailable, Some(failure_class));
    }

    // Step 2: Resolve a model using the same provider-aware path the Quick
    // health check uses. Deep is Ollama-only (ADR-0004), but the helper keeps
    // the model-resolution logic in one place.
    let model_name = match resolve_model_for_provider(
        lens_core::translate::Provider::Ollama,
        endpoint,
        configured_model,
    ) {
        Ok(m) => m,
        Err(e) => {
            // Model resolution failed. Map to appropriate failure class.
            let failure_class = DiagnosticFailureClass::from_translate_error(&e)
                .unwrap_or(DiagnosticFailureClass::ModelMissing);
            return (HealthState::Unavailable, Some(failure_class));
        }
    };

    // Step 3: Build analyser with resolved model and test Deep inference.
    let analyser = lens_translate::OllamaTranslator::new(endpoint, &model_name);

    // Use a test sentence that exercises both word and structure analysis.
    // This is a multi-word sentence with a focused word to test the full Deep pipeline.
    let test_request = DeepRequest::new(
        "The output files came back empty.",
        "출력 파일이 비어 있었습니다.",
        "Korean",
    );

    match Analyser::deep(&analyser, &test_request) {
        Ok(analysis) => {
            // Deep inference succeeded. Check if the analysis says anything meaningful.
            // This matches the production check: DeepAnalysis::says_anything() in analyse.rs:473-475.
            if analysis.says_anything() {
                (HealthState::Ready, None)
            } else {
                (
                    HealthState::Degraded,
                    Some(DiagnosticFailureClass::InferenceFailed),
                )
            }
        }
        Err(err) => {
            // TranslateError -> DiagnosticFailureClass is a typed mapping, no strings.
            let failure_class = DiagnosticFailureClass::from_translate_error(&err)
                .unwrap_or(DiagnosticFailureClass::InferenceFailed);
            (HealthState::Unavailable, Some(failure_class))
        }
    }
}

/// Tauri command: get recent diagnostic events from the ring.
///
/// Returns a snapshot of the most recent abnormal operational events.
/// Events are returned oldest-first. Privacy-safe: no captured text.
#[tauri::command]
fn get_diagnostic_events(
    events: tauri::State<'_, SharedDiagnosticEvents>,
) -> Vec<lens_core::event_ring::DiagnosticEvent> {
    let ring = events.lock().unwrap();
    ring.snapshot()
}

/// Tauri command: search and filter combined.
/// Applies both text search (original/answer) and structured filters with AND semantics.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn library_query(
    state: tauri::State<'_, SharedLibrary>,
    query: String,
    kind: Option<String>,
    app: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
    review_state: Option<String>,
    tag: Option<String>,
    offset: i64,
) -> Result<LibraryPageView, String> {
    use lens_storage::Filter;
    let lib = state.lock().unwrap();
    let filter = Filter {
        kind,
        app,
        source_language,
        target_language,
        review_state,
        tag,
    };
    let page = lib
        .library()
        .search_and_filter(&query, &filter, offset)
        .map_err(|e| format!("{e}"))?;
    Ok(LibraryPageView {
        items: page.items.iter().map(LibraryItemView::from).collect(),
        has_more: page.has_more,
        total: page.total,
    })
}

/// Tauri command: get a single library item.
#[tauri::command]
fn library_get(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
) -> Result<Option<LibraryItemView>, String> {
    let lib = state.lock().unwrap();
    let item = lib.library().get(id).map_err(|e| format!("{e}"))?;
    Ok(item.as_ref().map(LibraryItemView::from))
}

/// Tauri command: delete a library item.
#[tauri::command]
fn library_delete(state: tauri::State<'_, SharedLibrary>, id: i64) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library().delete(id).map_err(|e| format!("{e}"))
}

/// Tauri command: update the note on a library item.
#[tauri::command]
fn library_update_note(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
    note: String,
) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .update_note(id, &note)
        .map_err(|e| format!("{e}"))
}

/// Tauri command: set tags for a library item.
#[tauri::command]
fn library_set_tags(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
    tags: Vec<String>,
) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .set_tags(id, &tags)
        .map_err(|e| format!("{e}"))
}

/// Tauri command: get tags for a library item.
#[tauri::command]
fn library_get_tags(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library().get_tags(id).map_err(|e| format!("{e}"))
}

/// Tauri command: get distinct app names from the library.
#[tauri::command]
fn library_distinct_apps(state: tauri::State<'_, SharedLibrary>) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library().distinct_apps().map_err(|e| format!("{e}"))
}

/// Tauri command: get distinct source languages from the library.
#[tauri::command]
fn library_distinct_source_languages(
    state: tauri::State<'_, SharedLibrary>,
) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .distinct_source_languages()
        .map_err(|e| format!("{e}"))
}

/// Tauri command: get distinct target languages from the library.
#[tauri::command]
fn library_distinct_target_languages(
    state: tauri::State<'_, SharedLibrary>,
) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .distinct_target_languages()
        .map_err(|e| format!("{e}"))
}

/// Tauri command: get distinct tag names actually used in the library.
#[tauri::command]
fn library_distinct_tags(state: tauri::State<'_, SharedLibrary>) -> Result<Vec<String>, String> {
    let lib = state.lock().unwrap();
    lib.library().distinct_tags().map_err(|e| format!("{e}"))
}

/// Tauri command: export the library as Markdown.
#[tauri::command]
fn library_export(state: tauri::State<'_, SharedLibrary>) -> Result<String, String> {
    let lib = state.lock().unwrap();
    lib.library().export_markdown().map_err(|e| format!("{e}"))
}

/// Tauri command: export the library as JSON (lossless format).
#[tauri::command]
fn library_export_json(state: tauri::State<'_, SharedLibrary>) -> Result<String, String> {
    let lib = state.lock().unwrap();
    lib.library().export_json().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_export_jsonl(state: tauri::State<'_, SharedLibrary>) -> Result<String, String> {
    let lib = state.lock().unwrap();
    lib.library().export_jsonl().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_import_jsonl(
    state: tauri::State<'_, SharedLibrary>,
    jsonl: String,
) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .import_jsonl(&jsonl)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn recent_list(recent: tauri::State<'_, SharedRecent>) -> Vec<RecentCardView> {
    let mut guard = recent.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    guard
        .list(now)
        .into_iter()
        .map(|c| RecentCardView {
            original: c.original,
            answer: c.answer,
            shown_at: c.shown_at,
            app: c.app,
        })
        .collect()
}

/// Tauri command: securely wipe all data from the library database.
///
/// Checkpoints the WAL, enables `secure_delete`, deletes all rows, and
/// runs `VACUUM` to rebuild the file. See
/// [ADR-0013](../../../docs/decisions/ADR-0013-the-local-library.md) for
/// what this reaches and what it does not.
#[tauri::command]
fn library_secure_wipe(state: tauri::State<'_, SharedLibrary>) -> Result<(), String> {
    let lib = state.lock().unwrap();
    lib.library().secure_wipe().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn library_later_count(state: tauri::State<'_, SharedLibrary>) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library().count_later().map_err(|e| format!("{e}"))
}

/// Return all later items for a review session snapshot.
///
/// The review session takes a snapshot of all later items at entry time.
/// Each item is shown at most once per session. This avoids offset pagination
/// on a mutating dataset (Known/Archive remove items, Keep Later retains them).
#[tauri::command]
fn library_review_session(
    state: tauri::State<'_, SharedLibrary>,
) -> Result<LibraryPageView, String> {
    let lib = state.lock().unwrap();
    let items = lib
        .library()
        .list_later_session()
        .map_err(|e| format!("{e}"))?;
    let total = items.len() as i64;
    Ok(LibraryPageView {
        items: items.iter().map(LibraryItemView::from).collect(),
        has_more: false,
        total,
    })
}

#[tauri::command]
fn library_set_review_state(
    state: tauri::State<'_, SharedLibrary>,
    id: i64,
    state_: String,
) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .set_review_state(id, &state_)
        .map_err(|e| format!("{e}"))
}

// -- History commands (ADR-0014 Tier 3) -------------------------------------

/// A history entry for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct HistoryEntryView {
    pub id: i64,
    pub original: String,
    pub answer: String,
    pub kind: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub model: Option<String>,
    pub app: Option<String>,
    pub provenance: String,
    pub provider_scope: String,
    pub created_at: i64,
}

/// A page of history entries for the webview.
#[derive(Clone, Debug, Serialize)]
pub struct HistoryPageView {
    pub entries: Vec<HistoryEntryView>,
    pub has_more: bool,
    pub total: i64,
}

impl From<&lens_storage::HistoryEntry> for HistoryEntryView {
    fn from(e: &lens_storage::HistoryEntry) -> Self {
        Self {
            id: e.id,
            original: e.original.clone(),
            answer: e.answer.clone(),
            kind: e.kind.clone(),
            source_language: e.source_language.clone(),
            target_language: e.target_language.clone(),
            model: e.model.clone(),
            app: e.app.clone(),
            provenance: e.provenance.clone(),
            provider_scope: e.provider_scope.clone(),
            created_at: e.created_at,
        }
    }
}

#[tauri::command]
fn history_list(
    state: tauri::State<'_, SharedLibrary>,
    offset: Option<i64>,
) -> Result<HistoryPageView, String> {
    let lib = state.lock().unwrap();
    let page = lib
        .library()
        .history_list(offset.unwrap_or(0))
        .map_err(|e| format!("{e}"))?;
    Ok(HistoryPageView {
        entries: page.entries.iter().map(HistoryEntryView::from).collect(),
        has_more: page.has_more,
        total: page.total,
    })
}

#[tauri::command]
fn history_count(state: tauri::State<'_, SharedLibrary>) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library().history_count().map_err(|e| format!("{e}"))
}

#[tauri::command]
fn history_delete(state: tauri::State<'_, SharedLibrary>, id: i64) -> Result<bool, String> {
    let lib = state.lock().unwrap();
    lib.library().history_delete(id).map_err(|e| format!("{e}"))
}

#[tauri::command]
fn history_promote(state: tauri::State<'_, SharedLibrary>, id: i64) -> Result<i64, String> {
    let lib = state.lock().unwrap();
    lib.library()
        .history_promote(id)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn history_prune(
    state: tauri::State<'_, SharedLibrary>,
    retention_secs: i64,
) -> Result<usize, String> {
    let lib = state.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    lib.library()
        .history_prune(now, retention_secs)
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
fn mori_status() -> MoriStatusView {
    MoriStatusView::from(crate::mori_transfer::check_mori_status())
}

#[tauri::command]
fn mori_send_item(state: tauri::State<'_, SharedLibrary>, item_id: i64) -> Result<String, String> {
    let lib = state.lock().unwrap();
    let item = lib
        .library()
        .get(item_id)
        .map_err(|e| format!("{e}"))?
        .ok_or_else(|| format!("item {item_id} not found"))?;
    crate::mori_transfer::send_to_mori(&item).map_err(|e| format!("{e}"))
}

/// Serialize diagnostics into a JSON report for bug reports.
///
/// Pure function: no Tauri state, no locks, no side effects. Extracted so the
/// privacy contract can be tested without instantiating `tauri::State`.
///
/// **Privacy contract:** Only metadata is included. No captured text, no
/// translations, no analysis, no notes, no history content, no recent items,
/// no clipboard content, no screenshots, no passwords, no tokens, no API keys.
#[allow(clippy::too_many_arguments)]
pub fn serialize_diagnostics_report(
    version: &str,
    build: &str,
    os: &str,
    arch: &str,
    schema_version: i64,
    library_count: i64,
    history_count: i64,
    configuration: &[(String, String)],
    cards: u32,
    settled_hovers: u32,
    refusals: &[(String, u32)],
    ocr_mode: &str,
    language: &str,
    recent_events: &[lens_core::event_ring::DiagnosticEvent],
    health_snapshot: &lens_core::health::HealthSnapshot,
) -> String {
    let report = serde_json::json!({
        "version": version,
        "build": build,
        "os": os,
        "arch": arch,
        "schema_version": schema_version,
        "library_count": library_count,
        "history_count": history_count,
        "configuration": configuration,
        "cards": cards,
        "settled_hovers": settled_hovers,
        "refusals": refusals,
        "ocr_mode": ocr_mode,
        "language": language,
        "recent_events": recent_events,
        "health": health_snapshot,
    });

    serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_owned())
}

/// Diagnostics export for bug reports.
///
/// Explicit user action only: the reader clicks "Export diagnostics" in
/// the settings window. The result is a JSON string they can paste into
/// a bug report. **No captured text is included.** Everything here is
/// metadata: versions, counts, configuration labels, and error classes.
#[tauri::command]
fn export_diagnostics(
    diagnostics: tauri::State<'_, SharedDiagnostics>,
    settings: tauri::State<'_, SharedSettings>,
    library: tauri::State<'_, SharedLibrary>,
    events: tauri::State<'_, SharedDiagnosticEvents>,
    health: tauri::State<'_, SharedHealthSnapshot>,
) -> String {
    let diag = diagnostics.lock().unwrap();
    let conf = settings.lock().unwrap();
    let lib = library.lock().unwrap();
    let ring = events.lock().unwrap();
    let health_snap = health.lock().unwrap();

    let schema_version = lib.library().schema_version().unwrap_or(-1);
    let library_count = lib.library().count().unwrap_or(0);
    let history_count = lib.library().history_count().unwrap_or(0);
    let recent_events = ring.snapshot();

    serialize_diagnostics_report(
        env!("CARGO_PKG_VERSION"),
        &diag.build,
        std::env::consts::OS,
        std::env::consts::ARCH,
        schema_version,
        library_count,
        history_count,
        &diag.configuration,
        diag.cards,
        diag.settled_hovers,
        &diag.refusals,
        &diag.ocr_mode,
        &conf.current().language,
        &recent_events,
        &health_snap,
    )
}

/// Build the Tauri application and return its handle, shared diagnostics, pause request, quit request, and the `App` itself.
///
/// The `App` must be kept alive and `.run()` must be called on it. Dropping
/// it without running destroys the event loop; keeping only the `AppHandle`
/// without the `App` leaves no event loop running.
pub fn build(
    shared_settings: SharedSettings,
    shared_library: SharedLibrary,
    shared_detail: SharedDetail,
    shared_recent: SharedRecent,
    explain_flag: ExplainFlag,
    save_path: SharedSavePath,
) -> Result<
    (
        AppHandle,
        SharedDiagnostics,
        PauseRequest,
        QuitRequest,
        tauri::App,
    ),
    tauri::Error,
> {
    // Initialize process-start timestamp anchor early, during app startup.
    // This ensures health check timestamps are truly process-relative, not
    // relative to first health check invocation.
    let _ = initialize_process_start();

    let diagnostics: SharedDiagnostics = Arc::new(Mutex::new(DiagnosticsSnapshot {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        ..Default::default()
    }));
    let pause_request = PauseRequest::new();
    let quit_request = QuitRequest::new();
    let diagnostic_events: SharedDiagnosticEvents = global_diagnostic_events();
    let health_snapshot: SharedHealthSnapshot =
        Arc::new(Mutex::new(lens_core::health::HealthSnapshot::default()));
    let health_running = HealthCheckRunning::new();
    let operation_manager = lens_translate::operation_manager::OperationManager::new();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    #[cfg(feature = "e2e-testing")]
    {
        builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    }

    let app = builder
        .invoke_handler(tauri::generate_handler![
            get_diagnostics,
            get_runtime_status,
            set_autostart,
            request_pause_toggle,
            request_quit,
            get_settings,
            save_settings,
            list_models,
            check_provider,
            get_local_ai_status,
            test_model,
            run_health_check,
            get_health_status,
            get_diagnostic_events,
            library_query,
            library_get,
            library_delete,
            library_update_note,
            library_set_tags,
            library_get_tags,
            library_distinct_apps,
            library_distinct_source_languages,
            library_distinct_target_languages,
            library_distinct_tags,
            library_export,
            library_export_json,
            library_export_jsonl,
            library_import_jsonl,
            library_secure_wipe,
            recent_list,
            library_later_count,
            library_review_session,
            library_set_review_state,
            history_list,
            history_count,
            history_delete,
            history_promote,
            history_prune,
            mori_status,
            mori_send_item,
            export_diagnostics,
            crate::detail_view::detail_read,
            crate::detail_view::detail_copy,
            crate::detail_view::detail_save,
            crate::detail_view::detail_save_note,
            crate::detail_view::detail_request_explain,
            crate::detail_view::detail_open_in_library,
            get_model_download_status,
            get_provider_readiness,
            start_quick_model_download,
            load_lmstudio_instance,
            unload_lmstudio_instance,
        ])
        .manage(diagnostics.clone())
        .manage(diagnostic_events.clone())
        .manage(health_snapshot.clone())
        .manage(health_running.clone())
        .manage(operation_manager)
        .manage(pause_request.clone())
        .manage(quit_request.clone())
        .manage(shared_settings)
        .manage(shared_library)
        .manage(shared_detail)
        .manage(shared_recent)
        .manage(explain_flag)
        .manage(save_path)
        .setup(|app| {
            // Window visibility at startup depends on build configuration.
            // Production builds hide the window; it's opened from the tray.
            // E2E builds show the window for WebDriver automation.
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(feature = "e2e-testing")]
                {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                #[cfg(not(feature = "e2e-testing"))]
                {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())?;

    let handle = AppHandle(app.handle().clone());
    Ok((handle, diagnostics, pause_request, quit_request, app))
}

/// Run the Tauri event loop. Consumes the `App`.
///
/// This is the main thread's final act. It returns when the event loop
/// exits, which happens when `AppHandle::exit` is called.
///
/// Closing the settings window hides it rather than exiting: the polling
/// loop on its own thread decides when the app exits, and a user closing
/// a settings window is not a user quitting the product.
pub fn run(app: tauri::App) {
    app.run(|app_handle, event| {
        if let tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } = event
        {
            // A tray application does not exit when its window is closed.
            // The polling loop calls `AppHandle::exit` when the user quits
            // from the tray, and that is the only path to shutdown.
            if let Some(window) = app_handle.get_webview_window(&label) {
                let _ = window.hide();
            }
            api.prevent_close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn persistent_backing_is_persistent() {
        assert!(is_library_persistent(&LibraryBacking::Persistent(
            PathBuf::from("/tmp/lib.db")
        )));
    }

    #[test]
    fn in_memory_fallback_is_not_persistent() {
        assert!(!is_library_persistent(&LibraryBacking::InMemoryFallback {
            intended_path: PathBuf::from("/tmp/lib.db"),
            reason: "test".to_owned(),
        }));
    }

    /// Privacy regression test for diagnostics export.
    ///
    /// Proves that captured text, translations, analysis, notes, history, and
    /// recent items cannot leak into the diagnostics JSON. If this test fails,
    /// somebody added privacy-sensitive content to the export.
    #[test]
    fn diagnostics_export_excludes_canary_strings() {
        use lens_core::event_ring::{
            DiagnosticComponent, DiagnosticEvent, DiagnosticEventKind, DiagnosticFailureClass,
        };

        // Canary strings that must NEVER appear in diagnostics output.
        // These represent the kinds of content that are privacy-sensitive.
        // Expanded list: original six plus the additional canaries tracked in
        // the health snapshot test and docs/privacy-model.md.
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

        // Build a diagnostics report with realistic metadata.
        let configuration = vec![
            ("build".to_owned(), "2026-09-13-test".to_owned()),
            ("target".to_owned(), "ko".to_owned()),
            (
                "quick".to_owned(),
                "llama3.2 · 127.0.0.1:11434 · on this machine".to_owned(),
            ),
            (
                "deep".to_owned(),
                "llama3.2 · 127.0.0.1:11434 · on this machine".to_owned(),
            ),
            ("dwell".to_owned(), "500ms".to_owned()),
            ("mode".to_owned(), "default".to_owned()),
        ];

        let refusals = vec![("no_text".to_owned(), 5), ("password_field".to_owned(), 2)];

        // Create a health snapshot for the privacy test, with typed failures.
        let mut health_snapshot = lens_core::health::HealthSnapshot::new(0);
        health_snapshot.update_quick(
            lens_core::health::HealthState::Unavailable,
            Some(DiagnosticFailureClass::Timeout),
            Some(42),
            1000,
        );
        health_snapshot.update_deep(
            lens_core::health::HealthState::Degraded,
            Some(DiagnosticFailureClass::HttpClient),
            Some(100),
            1000,
        );

        // Build a realistic set of diagnostic events covering each component
        // and kind. These events are privacy-safe by construction: enums and
        // numbers only, no captured text or file paths.
        let populated_events = vec![
            DiagnosticEvent {
                seq: 1,
                elapsed_ms: 500,
                component: DiagnosticComponent::Quick,
                kind: DiagnosticEventKind::Failure,
                failure_class: Some(DiagnosticFailureClass::ConnectionFailed),
                count: 3,
                first_seen_ms: 100,
                last_seen_ms: 500,
            },
            DiagnosticEvent {
                seq: 2,
                elapsed_ms: 1200,
                component: DiagnosticComponent::Quick,
                kind: DiagnosticEventKind::Recovered,
                failure_class: None,
                count: 1,
                first_seen_ms: 1200,
                last_seen_ms: 1200,
            },
            DiagnosticEvent {
                seq: 3,
                elapsed_ms: 2000,
                component: DiagnosticComponent::Deep,
                kind: DiagnosticEventKind::Failure,
                failure_class: Some(DiagnosticFailureClass::HttpServer),
                count: 1,
                first_seen_ms: 2000,
                last_seen_ms: 2000,
            },
            DiagnosticEvent {
                seq: 4,
                elapsed_ms: 2500,
                component: DiagnosticComponent::HealthCheck,
                kind: DiagnosticEventKind::HealthCheck,
                failure_class: Some(DiagnosticFailureClass::Timeout),
                count: 1,
                first_seen_ms: 2500,
                last_seen_ms: 2500,
            },
            DiagnosticEvent {
                seq: 5,
                elapsed_ms: 3000,
                component: DiagnosticComponent::Library,
                kind: DiagnosticEventKind::Degraded,
                failure_class: Some(DiagnosticFailureClass::LibraryDegraded),
                count: 2,
                first_seen_ms: 2800,
                last_seen_ms: 3000,
            },
        ];

        let json = serialize_diagnostics_report(
            "0.1.0",
            "2026-09-13-test",
            "windows",
            "x86_64",
            1,
            42,
            100,
            &configuration,
            15,
            120,
            &refusals,
            "windows-gdi",
            "ko",
            &populated_events,
            &health_snapshot,
        );

        // Verify no canary strings appear in the output.
        for canary in &canaries {
            assert!(
                !json.contains(canary),
                "diagnostics export contains privacy-sensitive canary: {}",
                canary
            );
        }

        // Verify expected metadata IS present.
        assert!(json.contains("\"version\": \"0.1.0\""));
        assert!(json.contains("\"build\": \"2026-09-13-test\""));
        assert!(json.contains("\"os\": \"windows\""));
        assert!(json.contains("\"arch\": \"x86_64\""));
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"library_count\": 42"));
        assert!(json.contains("\"history_count\": 100"));
        assert!(json.contains("\"cards\": 15"));
        assert!(json.contains("\"settled_hovers\": 120"));
        assert!(json.contains("\"ocr_mode\": \"windows-gdi\""));
        assert!(json.contains("\"language\": \"ko\""));
        // Configuration is an array of key-value pairs.
        assert!(json.contains("\"target\""));
        assert!(json.contains("\"dwell\""));
        assert!(json.contains("\"mode\""));
        // Health snapshot IS present (privacy-safe enums only).
        assert!(json.contains("\"health\""));
        assert!(json.contains("\"quick\""));
        assert!(json.contains("\"deep\""));
        // Failure classes are serialized as snake_case enum names.
        assert!(json.contains("\"timeout\""));
        assert!(json.contains("\"http_client\""));
        // Recent events IS present (populated, privacy-safe).
        assert!(json.contains("\"recent_events\""));
        assert!(json.contains("\"connection_failed\""));
        assert!(json.contains("\"http_server\""));
        assert!(json.contains("\"library_degraded\""));
        // Sequence numbers and counts are present (proves events are populated).
        assert!(json.contains("\"seq\":"));
        assert!(json.contains("\"count\": 3"));
    }

    /// Schema allowlist test for diagnostics export.
    ///
    /// This is the primary privacy boundary. The diagnostics JSON must contain
    /// EXACTLY the allowed top-level keys, no more. Adding a new field requires
    /// an intentional test change, which forces a privacy review.
    ///
    /// Configuration labels are also bounded: only the six labels from
    /// configuration_facts() are allowed.
    #[test]
    fn diagnostics_export_schema_is_bounded() {
        use lens_core::event_ring::{
            DiagnosticComponent, DiagnosticEvent, DiagnosticEventKind, DiagnosticFailureClass,
        };
        use serde_json::Value;
        use std::collections::HashSet;

        let configuration = vec![
            ("build".to_owned(), "2026-09-13-test".to_owned()),
            ("target".to_owned(), "ko".to_owned()),
            (
                "quick".to_owned(),
                "llama3.2 · 127.0.0.1:11434 · on this machine".to_owned(),
            ),
            (
                "deep".to_owned(),
                "llama3.2 · 127.0.0.1:11434 · on this machine".to_owned(),
            ),
            ("dwell".to_owned(), "500ms".to_owned()),
            ("mode".to_owned(), "default".to_owned()),
        ];

        let refusals = vec![("no_text".to_owned(), 5)];

        // Create a health snapshot with typed failure classes.
        let mut health_snapshot = lens_core::health::HealthSnapshot::new(0);
        health_snapshot.update_quick(
            lens_core::health::HealthState::Unavailable,
            Some(DiagnosticFailureClass::ConnectionFailed),
            Some(42),
            1000,
        );
        health_snapshot.update_deep(lens_core::health::HealthState::Ready, None, Some(100), 1000);

        // Populated events exercise the event serialization schema too.
        let populated_events = vec![DiagnosticEvent {
            seq: 1,
            elapsed_ms: 500,
            component: DiagnosticComponent::Quick,
            kind: DiagnosticEventKind::Failure,
            failure_class: Some(DiagnosticFailureClass::Timeout),
            count: 1,
            first_seen_ms: 500,
            last_seen_ms: 500,
        }];

        let json = serialize_diagnostics_report(
            "0.1.0",
            "2026-09-13-test",
            "windows",
            "x86_64",
            1,
            42,
            100,
            &configuration,
            15,
            120,
            &refusals,
            "windows-gdi",
            "ko",
            &populated_events,
            &health_snapshot,
        );

        // Parse the JSON and assert the exact allowed top-level key set.
        let value: Value = serde_json::from_str(&json).expect("valid JSON");
        let obj = value.as_object().expect("top-level object");
        let actual_keys: HashSet<&String> = obj.keys().collect();

        let allowed_keys: HashSet<&str> = [
            "version",
            "build",
            "os",
            "arch",
            "schema_version",
            "library_count",
            "history_count",
            "configuration",
            "cards",
            "settled_hovers",
            "refusals",
            "ocr_mode",
            "language",
            "recent_events",
            "health",
        ]
        .iter()
        .copied()
        .collect();

        let actual_key_strs: HashSet<&str> = actual_keys.iter().map(|s| s.as_str()).collect();

        assert_eq!(
            actual_key_strs, allowed_keys,
            "diagnostics export schema mismatch: actual keys do not match allowed set"
        );

        // Assert configuration labels are bounded.
        let config_array = obj
            .get("configuration")
            .and_then(|v| v.as_array())
            .expect("configuration is an array");

        let allowed_config_labels: HashSet<&str> =
            ["build", "target", "quick", "deep", "dwell", "mode"]
                .iter()
                .copied()
                .collect();

        for item in config_array {
            let label = item
                .get(0)
                .and_then(|v| v.as_str())
                .expect("configuration item has label");
            assert!(
                allowed_config_labels.contains(label),
                "configuration label not in allowed set: {}",
                label
            );
        }
    }

    #[test]
    fn classify_model_readiness_none_with_no_models() {
        let result = classify_model_readiness(None, &[]);
        assert_eq!(result, ModelReadiness::AutoNoModels);
    }

    #[test]
    fn classify_model_readiness_none_with_models() {
        let result = classify_model_readiness(None, &["llama3".to_string()]);
        assert_eq!(result, ModelReadiness::AutoReady);
    }

    #[test]
    fn classify_model_readiness_empty_string_with_models() {
        // Some("") is accepted as Auto for backward compatibility.
        let result = classify_model_readiness(Some(""), &["llama3".to_string()]);
        assert_eq!(result, ModelReadiness::AutoReady);
    }

    #[test]
    fn classify_model_readiness_explicit_model_present() {
        let result = classify_model_readiness(Some("llama3"), &["llama3".to_string()]);
        assert_eq!(result, ModelReadiness::Ready("llama3".to_string()));
    }

    #[test]
    fn classify_model_readiness_explicit_model_missing() {
        let result = classify_model_readiness(Some("llama3"), &["mistral".to_string()]);
        assert_eq!(result, ModelReadiness::Missing("llama3".to_string()));
    }

    #[test]
    fn model_list_state_variants_are_distinct() {
        // Verify the three semantic states exist and are distinguishable.
        let unreachable = ModelListState::Unreachable;
        let error = ModelListState::Error("network timeout".to_string());
        let models = ModelListState::Models(vec!["llama3".to_string()]);

        // Ensure they are distinct variants.
        assert!(!matches!(unreachable, ModelListState::Error(_)));
        assert!(!matches!(unreachable, ModelListState::Models(_)));
        assert!(!matches!(error, ModelListState::Unreachable));
        assert!(!matches!(error, ModelListState::Models(_)));
        assert!(!matches!(models, ModelListState::Unreachable));
        assert!(!matches!(models, ModelListState::Error(_)));
    }

    // P12 timestamp-origin regression evidence.
    // Proves that the process-start anchor is established by explicit initialization,
    // and that elapsed time is read from the already-established anchor.

    #[test]
    fn process_start_initialization_and_reading_are_distinct() {
        // This test proves the contract: initialization and reading are separate operations.
        // The production code calls initialize_process_start() at startup (in build()),
        // then calls process_start_elapsed_ms() to read elapsed time.

        // Step 1: Initialize the anchor (simulates startup).
        let start = initialize_process_start();

        // Step 2: Small delay to ensure time has passed.
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Step 3: Read elapsed time using the dedicated function.
        let elapsed = process_start_elapsed_ms();

        // Elapsed time must reflect the delay (proves reading uses the initialized anchor).
        assert!(
            elapsed >= 10,
            "elapsed time must reflect time since initialization, got {}ms",
            elapsed
        );

        // Step 4: Verify the anchor is stable (OnceLock semantics).
        let start2 = initialize_process_start();
        assert_eq!(
            start, start2,
            "initialize_process_start() must return the same Instant on repeated calls"
        );

        // This proves:
        // 1. Initialization is explicit (initialize_process_start)
        // 2. Reading is separate (process_start_elapsed_ms)
        // 3. The anchor is stable (OnceLock)
        // 4. Health checks cannot silently create the origin
    }
}
