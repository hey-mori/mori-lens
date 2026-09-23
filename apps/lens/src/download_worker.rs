//! Background download workers for provider model management.
//!
//! Spawns OS threads that poll provider APIs and update the OperationManager.
//! Workers are fire-and-forget: they run until completion or error, then exit.
//! The OperationManager tracks the terminal state for frontend observation.

use std::thread;

use lens_core::model_management::{DownloadStatus, ManagementError};
use lens_core::translate::Provider;
use lens_translate::lmstudio_runtime::{DownloadStartResult, LmStudioRuntime};
use lens_translate::ollama_runtime::OllamaRuntime;
use lens_translate::operation_manager::OperationManager;

/// Spawn a background worker to download a model via Ollama.
///
/// The worker:
/// 1. Calls `pull_model()` with a progress callback that updates the manager.
/// 2. `pull_model` blocks until completion or error, streaming NDJSON events.
/// 3. Calls `finish_download()` with the terminal status.
///
/// The worker thread is detached and will exit when done. Errors are
/// recorded in the OperationManager as `DownloadStatus::Failed(_)`.
pub fn spawn_ollama_download_worker(
    manager: OperationManager,
    endpoint: String,
    model: String,
) {
    thread::spawn(move || {
        let runtime = OllamaRuntime::new(&endpoint);

        // Start tracking. If this fails, another download is already active.
        if let Err(e) = manager.start_download(
            Provider::Ollama,
            model.clone(),
            DownloadStatus::Starting,
            None,
        ) {
            eprintln!("ollama download worker: start_download failed: {:?}", e);
            return;
        }

        // pull_model blocks and streams progress via callback.
        let mut on_progress = |status: DownloadStatus| {
            let _ = manager.update_download_status(status);
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
    });
}

/// Spawn a background worker to download a model via LM Studio.
///
/// The worker:
/// 1. Calls `start_model_download()` to initiate the download.
/// 2. If a job_id is returned, polls `download_status(job_id)` every 2 seconds.
/// 3. Updates the OperationManager with progress on each poll.
/// 4. Calls `finish_download()` when status becomes terminal.
///
/// The worker thread is detached and will exit when done. Errors are
/// recorded in the OperationManager as `DownloadStatus::Failed(_)`.
pub fn spawn_lmstudio_download_worker(
    manager: OperationManager,
    endpoint: String,
    model: String,
) {
    thread::spawn(move || {
        let runtime = LmStudioRuntime::new(&endpoint);

        // Start the download.
        let start_result = match runtime.start_model_download(&model) {
            Ok(result) => result,
            Err(e) => {
                // Network error or other failure.
                let _ = manager.start_download(
                    Provider::LmStudio,
                    model.clone(),
                    DownloadStatus::Starting,
                    None,
                );
                let _ = manager.finish_download(DownloadStatus::Failed(e));
                return;
            }
        };

        // Handle the start result.
        let job_id = match start_result {
            DownloadStartResult::AlreadyPresent => {
                let _ = manager.start_download(
                    Provider::LmStudio,
                    model.clone(),
                    DownloadStatus::Starting,
                    None,
                );
                let _ = manager.finish_download(DownloadStatus::AlreadyPresent);
                return;
            }
            DownloadStartResult::TerminalCompleted => {
                let _ = manager.start_download(
                    Provider::LmStudio,
                    model.clone(),
                    DownloadStatus::Starting,
                    None,
                );
                let _ = manager.finish_download(DownloadStatus::Completed);
                return;
            }
            DownloadStartResult::Job {
                job_id,
                initial_status,
            } => {
                // Start tracking in the manager with the initial status.
                if let Err(e) = manager.start_download(
                    Provider::LmStudio,
                    model.clone(),
                    initial_status,
                    Some(job_id.clone()),
                ) {
                    eprintln!(
                        "lmstudio download worker: start_download failed: {:?}",
                        e
                    );
                    return;
                }
                job_id
            }
        };

        // Poll until terminal.
        loop {
            thread::sleep(std::time::Duration::from_secs(2));

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
    });
}
