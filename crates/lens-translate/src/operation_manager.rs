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
//! - LM Studio job IDs for in-progress downloads
//! - Mori-owned LM Studio load instance IDs
//! - Provider/model association for active operations
//! - Terminal results and error states
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
//! tracks which instance_ids were created by `load_model_for_mori()` and
//! refuses to unload instances that existed before Mori started or were
//! created by other tools.
//!
//! After a process restart, the manager has no memory of prior operations.
//! This is intentional: the manager state is process-local, not serialized
//! to settings. On restart, refresh actual provider state from the runtime.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use lens_core::model_management::{DownloadStatus, ManagementError};
use lens_core::translate::Provider;

/// Tracks active operations and owned resources.
#[derive(Debug, Default)]
struct OperationState {
    /// Active download, if any. Only one download at a time is allowed.
    active_download: Option<ActiveDownload>,
    /// LM Studio instances created by Mori (safe to unload).
    mori_owned_instances: HashSet<String>,
}

/// An active download operation.
#[derive(Debug, Clone)]
struct ActiveDownload {
    provider: Provider,
    model: String,
    status: DownloadStatus,
    /// LM Studio job ID, if applicable.
    job_id: Option<String>,
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

    /// Check if a download is currently active.
    ///
    /// Returns true if any download is in progress, regardless of provider or model.
    pub fn is_download_active(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.active_download.is_some()
    }

    /// Get the status of the active download, if any.
    pub fn active_download_status(&self) -> Option<(Provider, String, DownloadStatus)> {
        let state = self.state.lock().unwrap();
        state
            .active_download
            .as_ref()
            .map(|d| (d.provider, d.model.clone(), d.status.clone()))
    }

    /// Start tracking a new download operation.
    ///
    /// Returns an error if a download is already active (duplicate prevention).
    /// The manager enforces one active download globally.
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
    pub fn update_download_status(&self, status: DownloadStatus) -> Result<(), ManagementError> {
        let mut state = self.state.lock().unwrap();

        let download = state
            .active_download
            .as_mut()
            .ok_or(ManagementError::DownloadFailed)?;

        download.status = status;
        Ok(())
    }

    /// Get the job ID of the active download, if any.
    ///
    /// Only LM Studio downloads have job IDs. Ollama uses streaming progress.
    pub fn active_download_job_id(&self) -> Option<String> {
        let state = self.state.lock().unwrap();
        state
            .active_download
            .as_ref()
            .and_then(|d| d.job_id.clone())
    }

    /// Complete the active download (success or failure).
    ///
    /// Removes the download from active tracking. Returns the final status.
    /// Returns an error if no download is active.
    pub fn complete_download(&self) -> Result<DownloadStatus, ManagementError> {
        let mut state = self.state.lock().unwrap();

        let download = state
            .active_download
            .take()
            .ok_or(ManagementError::DownloadFailed)?;

        Ok(download.status)
    }

    /// Register a Mori-owned LM Studio instance.
    ///
    /// Call this after successfully loading a model via LM Studio.
    /// The instance_id can later be unloaded via `unregister_mori_instance()`.
    pub fn register_mori_instance(&self, instance_id: String) {
        let mut state = self.state.lock().unwrap();
        state.mori_owned_instances.insert(instance_id);
    }

    /// Check if an instance is owned by Mori (safe to unload).
    pub fn is_mori_owned_instance(&self, instance_id: &str) -> bool {
        let state = self.state.lock().unwrap();
        state.mori_owned_instances.contains(instance_id)
    }

    /// Unregister a Mori-owned LM Studio instance.
    ///
    /// Call this after successfully unloading a Mori-owned instance.
    /// Returns true if the instance was registered (and has now been removed).
    /// Returns false if the instance was not registered (not owned by Mori).
    pub fn unregister_mori_instance(&self, instance_id: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        state.mori_owned_instances.remove(instance_id)
    }

    /// Get all Mori-owned instance IDs.
    ///
    /// Useful for cleanup on shutdown or for diagnostic purposes.
    pub fn mori_owned_instances(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state.mori_owned_instances.iter().cloned().collect()
    }

    /// Clear all tracked state.
    ///
    /// Removes active download tracking and all owned instances.
    /// Useful for testing or when resetting after a provider restart.
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.active_download = None;
        state.mori_owned_instances.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lens_core::model_management::{DownloadProgress, ManagementError};

    #[test]
    fn new_manager_has_no_active_download() {
        let manager = OperationManager::new();
        assert!(!manager.is_download_active());
        assert_eq!(manager.active_download_status(), None);
        assert_eq!(manager.active_download_job_id(), None);
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

        let (provider, model, status) = manager.active_download_status().unwrap();
        assert_eq!(provider, Provider::Ollama);
        assert_eq!(model, "llama3.1:8b");
        assert_eq!(status, DownloadStatus::Starting);
    }

    #[test]
    fn duplicate_download_is_rejected() {
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

        let (_, _, status) = manager.active_download_status().unwrap();
        match status {
            DownloadStatus::Downloading(p) => {
                assert_eq!(p.bytes_downloaded, 50);
                assert_eq!(p.total_bytes, Some(100));
            }
            _ => panic!("Expected Downloading status"),
        }
    }

    #[test]
    fn update_without_active_download_fails() {
        let manager = OperationManager::new();
        let result = manager.update_download_status(DownloadStatus::Completed);
        assert_eq!(result, Err(ManagementError::DownloadFailed));
    }

    #[test]
    fn complete_download_removes_tracking() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        let result = manager.complete_download();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), DownloadStatus::Starting);
        assert!(!manager.is_download_active());
    }

    #[test]
    fn complete_without_active_download_fails() {
        let manager = OperationManager::new();
        let result = manager.complete_download();
        assert_eq!(result, Err(ManagementError::DownloadFailed));
    }

    #[test]
    fn lmstudio_job_id_is_tracked() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::LmStudio,
            "mistral-7b".to_string(),
            DownloadStatus::Starting,
            Some("job_abc123".to_string()),
        );

        assert_eq!(
            manager.active_download_job_id(),
            Some("job_abc123".to_string())
        );
    }

    #[test]
    fn ollama_has_no_job_id() {
        let manager = OperationManager::new();
        let _ = manager.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        assert_eq!(manager.active_download_job_id(), None);
    }

    #[test]
    fn register_and_check_mori_instance() {
        let manager = OperationManager::new();
        manager.register_mori_instance("inst_abc123".to_string());

        assert!(manager.is_mori_owned_instance("inst_abc123"));
        assert!(!manager.is_mori_owned_instance("inst_other"));
    }

    #[test]
    fn unregister_mori_instance() {
        let manager = OperationManager::new();
        manager.register_mori_instance("inst_abc123".to_string());

        let removed = manager.unregister_mori_instance("inst_abc123");
        assert!(removed);
        assert!(!manager.is_mori_owned_instance("inst_abc123"));

        let removed_again = manager.unregister_mori_instance("inst_abc123");
        assert!(!removed_again);
    }

    #[test]
    fn list_mori_owned_instances() {
        let manager = OperationManager::new();
        manager.register_mori_instance("inst_1".to_string());
        manager.register_mori_instance("inst_2".to_string());

        let instances = manager.mori_owned_instances();
        assert_eq!(instances.len(), 2);
        assert!(instances.contains(&"inst_1".to_string()));
        assert!(instances.contains(&"inst_2".to_string()));
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
        manager.register_mori_instance("inst_abc123".to_string());

        manager.clear();

        assert!(!manager.is_download_active());
        assert!(!manager.is_mori_owned_instance("inst_abc123"));
        assert!(manager.mori_owned_instances().is_empty());
    }

    #[test]
    fn manager_is_cloneable() {
        let manager1 = OperationManager::new();
        let manager2 = manager1.clone();

        let _ = manager1.start_download(
            Provider::Ollama,
            "llama3.1:8b".to_string(),
            DownloadStatus::Starting,
            None,
        );

        // Both clones see the same state.
        assert!(manager2.is_download_active());
    }

    #[test]
    fn manager_is_thread_safe() {
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
}
