//! Background download workers for provider model management.
//!
//! These functions are called from threads already spawned by Tauri commands.
//! The command performs atomic reservation via `OperationManager::start_download()`
//! before spawning. The worker updates/resolves the reservation.
//!
//! Workers are fire-and-forget: they run until completion or error, then exit.
//! The OperationManager tracks the terminal state for frontend observation.

use std::thread;
use std::time::Duration;

use lens_core::model_management::{DownloadStatus, ManagementError};
use lens_translate::lmstudio_runtime::{DownloadStartResult, LmStudioRuntime};
use lens_translate::ollama_runtime::OllamaRuntime;
use lens_translate::operation_manager::OperationManager;

/// Poll interval while a download is actively progressing.
const ACTIVE_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Poll interval while a download is paused by the provider.
/// Longer interval avoids unnecessary API calls while the user/provider
/// decides whether to resume. Mori does not implement resume itself.
const PAUSED_POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Run an Ollama model download.
///
/// Assumes the OperationManager has already reserved the operation with
/// `start_download(Provider::Ollama, model, Starting, None)`.
///
/// Calls `pull_model()` with a progress callback that updates the manager.
/// On terminal success, calls `finish_download(Completed)` or
/// `finish_download(AlreadyPresent)`. On error, calls `finish_download(Failed)`.
pub fn run_ollama_download(
    manager: OperationManager,
    endpoint: String,
    model: String,
) {
    let runtime = OllamaRuntime::new(&endpoint);

    // pull_model blocks and streams progress via callback.
    let manager_ref = &manager;
    let mut on_progress = |status: DownloadStatus| {
        let _ = manager_ref.update_download_status(status);
    };

    match runtime.pull_model(&model, &mut on_progress) {
        Ok(status) => {
            // pull_model returns a terminal status (Completed, AlreadyPresent, or Failed).
            let terminal = match status {
                DownloadStatus::Completed => DownloadStatus::Completed,
                DownloadStatus::AlreadyPresent => DownloadStatus::AlreadyPresent,
                DownloadStatus::Failed(e) => DownloadStatus::Failed(e),
                other => {
                    // pull_model should only return terminal statuses.
                    eprintln!(
                        "ollama download worker: unexpected non-terminal status: {:?}",
                        other
                    );
                    DownloadStatus::Failed(ManagementError::InvalidResponse)
                }
            };
            let _ = manager.finish_download(terminal);
        }
        Err(e) => {
            // Network error or other failure.
            let _ = manager.finish_download(DownloadStatus::Failed(e));
        }
    }
}

/// Run an LM Studio model download.
///
/// Assumes the OperationManager has already reserved the operation with
/// `start_download(Provider::LmStudio, model, Starting, None)`.
///
/// Calls `start_model_download()` and handles the result:
/// - AlreadyPresent → finish terminal immediately
/// - TerminalCompleted → finish terminal immediately
/// - Job { job_id, initial_status } → update status and poll
///
/// While polling, adapts interval based on status:
/// - Downloading/Starting → poll every 2 seconds
/// - Paused → poll every 15 seconds (Mori does not implement resume)
/// - Terminal → worker exits
pub fn run_lmstudio_download(
    manager: OperationManager,
    endpoint: String,
    model: String,
) {
    let runtime = LmStudioRuntime::new(&endpoint);

    // Start the download.
    let start_result = match runtime.start_model_download(&model) {
        Ok(result) => result,
        Err(e) => {
            let _ = manager.finish_download(DownloadStatus::Failed(e));
            return;
        }
    };

    // Handle the start result.
    let job_id = match start_result {
        DownloadStartResult::AlreadyPresent => {
            let _ = manager.finish_download(DownloadStatus::AlreadyPresent);
            return;
        }
        DownloadStartResult::TerminalCompleted => {
            let _ = manager.finish_download(DownloadStatus::Completed);
            return;
        }
        DownloadStartResult::Job {
            job_id,
            initial_status,
        } => {
            // Update the reservation from Starting to the actual initial status.
            // Note: initial_status may be Paused if the provider reported paused.
            let _ = manager.update_download_status(initial_status.clone());
            job_id
        }
    };

    // Poll until terminal, adapting interval based on paused state.
    loop {
        let current_status = {
            let snapshot = manager.operation_snapshot();
            snapshot.map(|s| s.status)
        };

        let poll_interval = match &current_status {
            Some(DownloadStatus::Paused(_)) => PAUSED_POLL_INTERVAL,
            _ => ACTIVE_POLL_INTERVAL,
        };

        thread::sleep(poll_interval);

        match runtime.download_status(&job_id) {
            Ok(status) => {
                if status.is_terminal() {
                    let _ = manager.finish_download(status);
                    return;
                }
                let _ = manager.update_download_status(status);
            }
            Err(e) => {
                // Job not found or network error.
                let _ = manager.finish_download(DownloadStatus::Failed(e));
                return;
            }
        }
    }
}
