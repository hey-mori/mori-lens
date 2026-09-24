//! Operation manager for provider model management.
//!
//! Tracks process-local state for download and load operations, enforcing
//! safe ownership semantics and preventing duplicate operations.
//!
//! # Responsibilities
//!
//! The manager owns:
//! - Active download state (one at a time, globally)
//! - Duplicate download prevention
//! - Terminal download snapshot (bounded, last operation only)
//! - LM Studio job IDs for in-progress downloads (internal only)
//! - Mori-owned LM Studio instance IDs, keyed by model
//! - Provider/model association for active operations
//!
//! The manager does NOT own:
//! - Provider installation or lifecycle
//! - Model files on disk
//! - Persistent job history (process-local only)
//! - User-owned LM Studio instances (only Mori-created ones)
//!
//! # Ownership Model
//!
//! Mori Lens may only unload LM Studio instances it created. The manager
//! tracks which (model, instance_id) pairs were created by `load_model()`
//! and resolves ownership by model, not by frontend-supplied instance ID.
//!
//! After a process restart, the manager has no memory of prior operations.
//! This is intentional: the manager state is process-local, not serialized
//! to settings. On restart, refresh actual provider state from the runtime.
//!
//! # Lifecycle
//!
//! ```text
//! Idle
//! → start_download() → Starting
//! → update_download_status() → Downloading / Paused
//! → finish_download(terminal_status) → Completed / AlreadyPresent / Failed
//! → terminal snapshot remains readable via operation_snapshot()
//! → next start_download() replaces/clears the old terminal snapshot
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lens_core::model_management::{DownloadStatus, ManagementError};
use lens_core::translate::Provider;
use serde::Serialize;

/// A snapshot of the current or most recent download operation.
///
/// Suitable for frontend consumption. Does not expose provider internals
/// such as job IDs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DownloadOperationSnapshot {
    /// Which provider is running the operation (as "ollama" or "lmstudio").
    pub provider: String,
    /// Which model is being downloaded.
    pub model: String,
    /// Current or terminal status.
    pub status: DownloadStatus,
    /// Whether the operation is still active (non-terminal).
    pub active: bool,
}

/// Internal active download state. Includes provider job ID for backend use.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ActiveDownload {
    provider: Provider,
    model: String,
    status: DownloadStatus,
    /// LM Studio job ID, if applicable. Not exposed to frontend.
    job_id: Option<String>,
}

/// A Mori-owned LM Studio instance, keyed by model.
#[derive(Debug, Clone)]
struct OwnedInstance {
    model: String,
    instance_id: String,
}

/// Internal operation state.
#[derive(Debug, Default)]
struct OperationState {
    /// Active download, if any. Only one download at a time is allowed.
    active_download: Option<ActiveDownload>,
    /// Terminal snapshot of the last completed/failed download.
    /// Remains readable after `finish_download()` until the next operation starts.
    last_download: Option<DownloadOperationSnapshot>,
    /// Mori-owned LM Studio instances, keyed by model.
    /// At most one Mori-owned instance per model.
    owned_instances: HashMap<String, OwnedInstance>,
}

/// Process-local operation manager for provider model management.
///
/// Thread-safe via internal Mutex. Clone is cheap (Arc inside).
#[derive(Debug, Clone)]
pub struct OperationManager {
    state: Arc<Mutex<OperationState>>,
}

impl Default for OperationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationManager {
    /// Create a new operation manager with no active operations.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(OperationState::default())),
        }
    }

    /// Atomic snapshot of the current download operation.
    ///
    /// Returns the active download if one is in progress, or the terminal
    /// snapshot of the last completed/failed download, or `None` if idle
    /// with no prior operation.
    ///
    /// This is the primary accessor for frontend status queries. It does
    /// not expose provider internals such as job IDs.
    pub fn operation_snapshot(&self) -> Option<DownloadOperationSnapshot> {
        let state = self.state.lock().unwrap();
        if let Some(active) = &state.active_download {
            Some(DownloadOperationSnapshot {
                provider: active.provider.as_str().to_string(),
                model: active.model.clone(),
                status: active.status.clone(),
                active: true,
            })
        } else {
            state.last_download.clone()
        }
    }

    /// Internal snapshot including job ID, for backend worker use.
    #[allow(dead_code)]
    pub(crate) fn internal_snapshot(&self) -> Option<ActiveDownload> {
        let state = self.state.lock().unwrap();
        state.active_download.clone()
    }

    /// Check if a download is currently active.
    pub fn is_download_active(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.active_download.is_some()
    }

    /// Start tracking a new download operation.
    ///
    /// Returns an error if a download is already active (duplicate prevention).
    /// The manager enforces one active download globally.
    ///
    /// Starting a new operation clears any prior terminal snapshot.
    pub fn start_download(
        &self,
        provider: Provider,
        model: String,
        initial_status: DownloadStatus,
        job_id: Option<String>,
    ) -> Result<(), ManagementError> {
        let mut state = self.state.lock().unwrap();

        if state.active_download.is_some() {
            return Err(ManagementError::ProviderBusy);
        }

        // Reject non-terminal initial status.
        if initial_status.is_terminal() {
            return Err(ManagementError::InvalidResponse);
        }

        // Clear prior terminal snapshot when a new operation starts.
        state.last_download = None;

        state.active_download = Some(ActiveDownload {
            provider,
            model,
            status: initial_status,
            job_id,
        });

        Ok(())
    }

    /// Update the status of the active download.
    ///
    /// Returns an error if no download is active.
    /// Does not accept terminal statuses; use `finish_download()` for those.
    pub fn update_download_status(&self, status: DownloadStatus) -> Result<(), ManagementError> {
        let mut state = self.state.lock().unwrap();

        let download = state
            .active_download
            .as_mut()
            .ok_or(ManagementError::DownloadFailed)?;

        // Reject terminal statuses here; they belong in finish_download().
        if status.is_terminal() {
            return Err(ManagementError::InvalidResponse);
        }

        download.status = status;
        Ok(())
    }

    /// Finish the active download with a terminal status.
    ///
    /// The `final_status` must be terminal: `Completed`, `AlreadyPresent`,
    /// or `Failed(_)`. Non-terminal statuses are rejected.
    ///
    /// On success, the active download is removed from tracking and a
    /// terminal snapshot is stored for frontend observation. The snapshot
    /// remains readable until the next `start_download()` call.
    pub fn finish_download(&self, final_status: DownloadStatus) -> Result<(), ManagementError> {
        if !final_status.is_terminal() {
            return Err(ManagementError::InvalidResponse);
        }

        let mut state = self.state.lock().unwrap();

        let download = state
            .active_download
            .take()
            .ok_or(ManagementError::DownloadFailed)?;

        state.last_download = Some(DownloadOperationSnapshot {
            provider: download.provider.as_str().to_string(),
            model: download.model,
            status: final_status,
            active: false,
        });

        Ok(())
    }

    /// Register a Mori-owned LM Studio instance for a model.
    ///
    /// Call this only after successfully loading a model via LM Studio
    /// (POST /api/v1/models/load with valid `instance_id` in response).
    ///
    /// If Mori already owns an instance for this model, the registration
    /// is rejected to prevent duplicate VRAM use.
    pub fn register_mori_instance(
        &self,
        model: String,
        instance_id: String,
    ) -> Result<(), ManagementError> {
        let mut state = self.state.lock().unwrap();

        if state.owned_instances.contains_key(&model) {
            return Err(ManagementError::ProviderBusy);
        }

        state
            .owned_instances
            .insert(model.clone(), OwnedInstance { model, instance_id });

        Ok(())
    }

    /// Look up the Mori-owned instance ID for a model.
    ///
    /// Returns `Some(instance_id)` if Mori owns an instance for this model,
    /// `None` otherwise. Does not remove the ownership entry.
    pub fn owned_instance_for_model(&self, model: &str) -> Option<String> {
        let state = self.state.lock().unwrap();
        state
            .owned_instances
            .get(model)
            .map(|owned| owned.instance_id.clone())
    }

    /// Check if Mori owns an instance for a given model.
    pub fn owns_model_instance(&self, model: &str) -> bool {
        let state = self.state.lock().unwrap();
        state.owned_instances.contains_key(model)
    }

    /// Take (remove) the Mori-owned instance for a model.
    ///
    /// Returns the instance_id if Mori owned an instance for this model.
    /// The ownership entry is removed. Call this only after a successful
    /// unload HTTP call to the provider.
    ///
    /// If the unload fails, do not call this method; the ownership entry
    /// remains for retry.
    pub fn take_owned_instance_for_model(&self, model: &str) -> Option<String> {
        let mut state = self.state.lock().unwrap();
        state
            .owned_instances
            .remove(model)
            .map(|owned| owned.instance_id)
    }

    /// Get all Mori-owned (model, instance_id) pairs.
    ///
    /// Useful for cleanup on shutdown or for diagnostic purposes.
    pub fn mori_owned_instances(&self) -> Vec<(String, String)> {
        let state = self.state.lock().unwrap();
        state
            .owned_instances
            .values()
            .map(|owned| (owned.model.clone(), owned.instance_id.clone()))
            .collect()
    }

    /// Clear all tracked state.
    ///
    /// Removes active download tracking, terminal snapshot, and all owned
    /// instances. Useful for testing or when resetting after a provider restart.
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.active_download = None;
        state.last_download = None;
        state.owned_instances.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lens_core::model_management::{DownloadProgress, ManagementError};

    #[test]
    fn new_manager_is_idle() {
        let manager = OperationManager::new();
        assert!(!manager.is_download_active());
        assert_eq!(manager.operation_snapshot(), None);
    }

    #[test]
    fn start_download_tracks_operation() {
        let manager = OperationManager::new();
        let result = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );
        assert!(result.is_ok());
        assert!(manager.is_download_active());

        let snapshot = manager.operation_snapshot().unwrap();
        assert_eq!(snapshot.provider, "ollama");
        assert_eq!(snapshot.model, "llama3.1:8b");
        assert_eq!(snapshot.status, DownloadStatus::Starting);
        assert!(snapshot.active);
    }

    #[test]
    fn second_download_is_blocked() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result = manager.start_download(
            Provider::LmStudio,
            "mistral-7b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        assert_eq!(result, Err(ManagementError::ProviderBusy));
    }

    #[test]
    fn update_download_status_works() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let progress = DownloadProgress::new(50, 100);
        let result = manager.update_download_status(DownloadStatus::Downloading(progress));
        assert!(result.is_ok());

        let snapshot = manager.operation_snapshot().unwrap();
        match snapshot.status {
            DownloadStatus::Downloading(p) => {
                assert_eq!(p.bytes_downloaded, 50);
                assert_eq!(p.total_bytes, Some(100));
            }
            _ => panic!("Expected Downloading status"),
        }
        assert!(snapshot.active);
    }

    #[test]
    fn update_rejects_terminal_status() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result = manager.update_download_status(DownloadStatus::Completed);
        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn finish_download_retains_completed_snapshot() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result = manager.finish_download(DownloadStatus::Completed);
        assert!(result.is_ok());
        assert!(!manager.is_download_active());

        let snapshot = manager.operation_snapshot().unwrap();
        assert_eq!(snapshot.status, DownloadStatus::Completed);
        assert!(!snapshot.active);
        assert_eq!(snapshot.model, "llama3.1:8b");
    }

    #[test]
    fn finish_download_retains_failed_snapshot() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result =
            manager.finish_download(DownloadStatus::Failed(ManagementError::DownloadFailed));
        assert!(result.is_ok());

        let snapshot = manager.operation_snapshot().unwrap();
        assert_eq!(
            snapshot.status,
            DownloadStatus::Failed(ManagementError::DownloadFailed)
        );
        assert!(!snapshot.active);
    }

    #[test]
    fn finish_download_retains_already_present_snapshot() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::LmStudio,
            "mistral-7b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result = manager.finish_download(DownloadStatus::AlreadyPresent);
        assert!(result.is_ok());

        let snapshot = manager.operation_snapshot().unwrap();
        assert_eq!(snapshot.status, DownloadStatus::AlreadyPresent);
        assert!(!snapshot.active);
    }

    #[test]
    fn finish_download_rejects_starting() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result = manager.finish_download(DownloadStatus::Starting);
        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn finish_download_rejects_downloading() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result =
            manager.finish_download(DownloadStatus::Downloading(DownloadProgress::new(50, 100)));
        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn finish_download_rejects_paused() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::LmStudio,
            "mistral-7b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result =
            manager.finish_download(DownloadStatus::Paused(DownloadProgress::new(50, 100)));
        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn next_operation_clears_prior_terminal_snapshot() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );
        let _ = manager.finish_download(DownloadStatus::Completed);

        // Terminal snapshot is visible.
        assert!(manager.operation_snapshot().is_some());

        // Starting a new operation clears the old terminal snapshot.
        let _ = manager.start_download(
            Provider::LmStudio,
            "mistral-7b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let snapshot = manager.operation_snapshot().unwrap();
        assert_eq!(snapshot.model, "mistral-7b");
        assert!(snapshot.active);
    }

    #[test]
    fn start_download_rejects_terminal_initial_status() {
        let manager = OperationManager::new();
        let result = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Completed,
            None,
        );
        assert_eq!(result, Err(ManagementError::InvalidResponse));
    }

    #[test]
    fn lmstudio_job_id_tracked_internally() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::LmStudio,
            "mistral-7b".to_string(),
            DownloadStatus::Starting,
            Some("job_abc123".to_string()),
        );

        // Job ID is available via internal snapshot.
        let internal = manager.internal_snapshot().unwrap();
        assert_eq!(internal.job_id, Some("job_abc123".to_string()));

        // Job ID is NOT exposed via frontend snapshot.
        let snapshot = manager.operation_snapshot().unwrap();
        assert_eq!(snapshot.provider, "lmstudio");
        assert_eq!(snapshot.model, "mistral-7b");
        // No job_id field on DownloadOperationSnapshot.
    }

    #[test]
    fn register_mori_owned_instance_by_model() {
        let manager = OperationManager::new();
        let result =
            manager.register_mori_instance("mistral-7b".to_string(), "inst_abc123".to_string());
        assert!(result.is_ok());

        assert!(manager.owns_model_instance("mistral-7b"));
        assert!(!manager.owns_model_instance("other-model"));
        assert_eq!(
            manager.owned_instance_for_model("mistral-7b"),
            Some("inst_abc123".to_string())
        );
    }

    #[test]
    fn duplicate_registration_rejected() {
        let manager = OperationManager::new();
        let _ = manager.register_mori_instance("mistral-7b".to_string(), "inst_abc123".to_string());

        let result =
            manager.register_mori_instance("mistral-7b".to_string(), "inst_other".to_string());
        assert_eq!(result, Err(ManagementError::ProviderBusy));
    }

    #[test]
    fn non_owned_model_returns_none() {
        let manager = OperationManager::new();
        assert_eq!(manager.owned_instance_for_model("mistral-7b"), None);
        assert!(!manager.owns_model_instance("mistral-7b"));
    }

    #[test]
    fn take_owned_instance_removes_entry() {
        let manager = OperationManager::new();
        let _ = manager.register_mori_instance("mistral-7b".to_string(), "inst_abc123".to_string());

        let taken = manager.take_owned_instance_for_model("mistral-7b");
        assert_eq!(taken, Some("inst_abc123".to_string()));
        assert!(!manager.owns_model_instance("mistral-7b"));
        assert_eq!(manager.owned_instance_for_model("mistral-7b"), None);
    }

    #[test]
    fn take_nonexistent_returns_none() {
        let manager = OperationManager::new();
        let taken = manager.take_owned_instance_for_model("nonexistent");
        assert_eq!(taken, None);
    }

    #[test]
    fn new_manager_owns_nothing() {
        let manager = OperationManager::new();
        assert!(manager.mori_owned_instances().is_empty());
        assert!(!manager.owns_model_instance("anything"));
    }

    #[test]
    fn clear_removes_all_state() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );
        let _ = manager.finish_download(DownloadStatus::Completed);
        let _ = manager.register_mori_instance("mistral-7b".to_string(), "inst_abc123".to_string());

        manager.clear();

        assert!(!manager.is_download_active());
        assert_eq!(manager.operation_snapshot(), None);
        assert!(manager.mori_owned_instances().is_empty());
    }

    #[test]
    fn clone_sees_same_state() {
        let manager1 = OperationManager::new();
        let manager2 = manager1.clone();

        let _ = manager1.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        assert!(manager2.is_download_active());
    }

    #[test]
    fn thread_access() {
        use std::thread;

        let manager = OperationManager::new();
        let manager_clone = manager.clone();

        let handle = thread::spawn(move || {
            let _ = manager_clone.start_download(
                Provider::Ollama,
                "llama3.1:8b".to_string(),
                DownloadStatus::Starting,
                None,
            );
        });

        handle.join().unwrap();
        assert!(manager.is_download_active());
    }

    #[test]
    fn snapshot_serializes_to_json() {
        let snapshot = DownloadOperationSnapshot {
            provider: "ollama".to_string(),
            model: "llama3.1:8b".to_string(),
            status: DownloadStatus::Downloading(DownloadProgress::new(50, 100)),
            active: true,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("\"provider\":\"ollama\""));
        assert!(json.contains("\"model\":\"llama3.1:8b\""));
        assert!(json.contains("\"active\":true"));
    }
}
