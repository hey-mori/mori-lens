//! Model management types for provider setup.
//!
//! Types for tracking model download progress and managing download operations.
//! P20 first-card/provider readiness.
//!
//! These types are provider-neutral. The platform adapter converts provider-specific
//! JSON shapes (Ollama streaming progress, LM Studio job status) into these types.
//!
//! For installed model inventory, use `lens_core::translate::ModelInfo` which is
//! returned by `ProviderRuntime::list_models()`.

use serde::{Deserialize, Serialize};

/// Progress of a model download operation.
///
/// Provider-neutral representation. Adapters convert provider-specific
/// progress fields into this shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes to download (None if unknown).
    pub total_bytes: Option<u64>,
    /// Download rate in bytes per second (provider-computed or adapter-computed).
    /// None if rate cannot be determined.
    pub bytes_per_second: Option<f64>,
    /// Estimated seconds until completion.
    /// Derived from bytes_per_second and remaining bytes.
    /// None if ETA cannot be determined.
    pub estimated_completion_seconds: Option<f64>,
}

impl DownloadProgress {
    /// Create a new download progress with known total.
    pub fn new(bytes_downloaded: u64, total_bytes: u64) -> Self {
        Self {
            bytes_downloaded,
            total_bytes: Some(total_bytes),
            bytes_per_second: None,
            estimated_completion_seconds: None,
        }
    }

    /// Create a new download progress with unknown total.
    pub fn unknown_total(bytes_downloaded: u64) -> Self {
        Self {
            bytes_downloaded,
            total_bytes: None,
            bytes_per_second: None,
            estimated_completion_seconds: None,
        }
    }

    /// Set the download rate.
    ///
    /// Normalizes invalid values: NaN, Infinity, negative, and zero rates are
    /// rejected and stored as `None`. Only finite positive rates are preserved.
    /// ETA is computed only from a valid rate.
    pub fn with_rate(mut self, bytes_per_second: f64) -> Self {
        // Normalize: only finite positive rates are valid.
        if bytes_per_second.is_finite() && bytes_per_second > 0.0 {
            self.bytes_per_second = Some(bytes_per_second);
            // Compute ETA if we have total.
            if let Some(total) = self.total_bytes {
                let remaining = total.saturating_sub(self.bytes_downloaded);
                self.estimated_completion_seconds = Some(remaining as f64 / bytes_per_second);
            }
        } else {
            self.bytes_per_second = None;
            self.estimated_completion_seconds = None;
        }
        self
    }

    /// Calculate the download percentage (0.0 to 100.0).
    /// Returns None if total_bytes is unknown or zero (malformed).
    /// Clamped to 0..=100 for presentation safety.
    pub fn percentage(&self) -> Option<f64> {
        self.total_bytes.and_then(|total| {
            if total == 0 {
                None // Malformed or incomplete provider progress
            } else {
                let pct = (self.bytes_downloaded as f64 / total as f64) * 100.0;
                Some(pct.clamp(0.0, 100.0))
            }
        })
    }

    /// Check if bytes downloaded has reached or exceeded total bytes.
    ///
    /// This is byte-level arithmetic only, not provider operation completion.
    /// For terminal operation status, use `DownloadStatus::is_terminal()`.
    /// Returns false if total_bytes is unknown or zero (malformed).
    pub fn bytes_reached_total(&self) -> bool {
        match self.total_bytes {
            Some(total) if total > 0 => self.bytes_downloaded >= total,
            _ => false,
        }
    }
}

/// Normalized download status.
///
/// Provider-neutral states that cover both Ollama and LM Studio download flows.
/// Adapters map provider-specific states into these variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// Download request has been submitted but no data received yet.
    Starting,
    /// Download is in progress.
    Downloading(DownloadProgress),
    /// Download is paused (provider supports pause/resume).
    /// LM Studio may report this state.
    Paused(DownloadProgress),
    /// Download completed successfully.
    Completed,
    /// Provider explicitly indicated the requested model was already local.
    /// Currently used by LM Studio `already_downloaded` response.
    AlreadyPresent,
    /// Download failed with a typed error.
    Failed(ManagementError),
}

impl DownloadStatus {
    /// Whether this status represents a terminal operation state.
    ///
    /// Terminal: `Completed`, `AlreadyPresent`, `Failed(_)`.
    /// Non-terminal: `Starting`, `Downloading(_)`, `Paused(_)`.
    ///
    /// Terminality is determined by provider operation semantics, not byte
    /// arithmetic. A download where bytes_reached_total() is true but the
    /// provider has not confirmed completion is still non-terminal.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DownloadStatus::Completed | DownloadStatus::AlreadyPresent | DownloadStatus::Failed(_)
        )
    }
}

impl Default for DownloadStatus {
    fn default() -> Self {
        Self::Starting
    }
}

/// Typed management operation error.
///
/// Closed enum to prevent raw provider error bodies from leaking into
/// privacy-sensitive surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagementError {
    /// Provider endpoint is not reachable.
    EndpointUnreachable,
    /// Provider requires authentication but none was provided or it was invalid.
    Unauthorized,
    /// Requested model was not found (model lookup/load 404).
    ModelNotFound,
    /// Provider returned an invalid or malformed response.
    InvalidResponse,
    /// Download operation failed.
    DownloadFailed,
    /// Load operation failed.
    LoadFailed,
    /// Unload operation failed.
    UnloadFailed,
    /// Operation is not supported by this provider.
    UnsupportedOperation,
    /// Provider is busy or rate-limited.
    ProviderBusy,
    /// Referenced operation or resource was not found (download job 404, instance 404).
    OperationNotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_progress_percentage_known_total() {
        let progress = DownloadProgress::new(50, 100);
        assert_eq!(progress.percentage(), Some(50.0));
    }

    #[test]
    fn download_progress_percentage_unknown_total() {
        let progress = DownloadProgress::unknown_total(50);
        assert_eq!(progress.percentage(), None);
    }

    #[test]
    fn download_progress_percentage_zero_total_is_none() {
        // Zero total is malformed, not 100%
        let progress = DownloadProgress::new(0, 0);
        assert_eq!(progress.percentage(), None);
    }

    #[test]
    fn download_progress_percentage_clamped_to_100() {
        // bytes_downloaded > total (malformed or in-flight) clamps to 100
        let progress = DownloadProgress::new(150, 100);
        assert_eq!(progress.percentage(), Some(100.0));
    }

    #[test]
    fn download_progress_percentage_clamped_to_0() {
        // Negative would be impossible with u64, but test the clamp
        let progress = DownloadProgress::new(0, 100);
        assert_eq!(progress.percentage(), Some(0.0));
    }

    #[test]
    fn download_progress_is_complete_only_with_positive_total() {
        let progress = DownloadProgress::new(100, 100);
        assert!(progress.bytes_reached_total());

        let incomplete = DownloadProgress::new(50, 100);
        assert!(!incomplete.bytes_reached_total());

        let unknown = DownloadProgress::unknown_total(50);
        assert!(!unknown.bytes_reached_total());

        // Zero total is not complete
        let zero_total = DownloadProgress::new(0, 0);
        assert!(!zero_total.bytes_reached_total());
    }

    #[test]
    fn download_progress_with_rate_computes_eta() {
        let progress = DownloadProgress::new(50, 100).with_rate(10.0);
        assert_eq!(progress.bytes_per_second, Some(10.0));
        assert_eq!(progress.estimated_completion_seconds, Some(5.0));
    }

    #[test]
    fn download_progress_with_rate_unknown_total_no_eta() {
        let progress = DownloadProgress::unknown_total(50).with_rate(10.0);
        assert_eq!(progress.bytes_per_second, Some(10.0));
        assert_eq!(progress.estimated_completion_seconds, None);
    }

    #[test]
    fn download_progress_with_rate_rejects_nan() {
        let progress = DownloadProgress::new(50, 100).with_rate(f64::NAN);
        assert_eq!(progress.bytes_per_second, None);
        assert_eq!(progress.estimated_completion_seconds, None);
    }

    #[test]
    fn download_progress_with_rate_rejects_infinity() {
        let progress = DownloadProgress::new(50, 100).with_rate(f64::INFINITY);
        assert_eq!(progress.bytes_per_second, None);
        assert_eq!(progress.estimated_completion_seconds, None);
    }

    #[test]
    fn download_progress_with_rate_rejects_zero() {
        let progress = DownloadProgress::new(50, 100).with_rate(0.0);
        assert_eq!(progress.bytes_per_second, None);
        assert_eq!(progress.estimated_completion_seconds, None);
    }

    #[test]
    fn download_progress_with_rate_rejects_negative() {
        let progress = DownloadProgress::new(50, 100).with_rate(-10.0);
        assert_eq!(progress.bytes_per_second, None);
        assert_eq!(progress.estimated_completion_seconds, None);
    }

    #[test]
    fn download_progress_invalid_rate_serializes_cleanly() {
        // A rate that was invalid should serialize with null bytes_per_second,
        // not with NaN or Infinity which would break JSON.
        let progress = DownloadProgress::new(50, 100).with_rate(f64::NAN);
        let json = serde_json::to_string(&progress).unwrap();
        assert!(!json.contains("NaN"));
        assert!(!json.contains("Infinity"));
        assert!(json.contains("\"bytes_per_second\":null"));
    }

    #[test]
    fn download_status_default_is_starting() {
        let status = DownloadStatus::default();
        assert_eq!(status, DownloadStatus::Starting);
    }

    #[test]
    fn download_status_all_variants_exist() {
        // Verify all normalized states exist.
        let _starting = DownloadStatus::Starting;
        let _downloading = DownloadStatus::Downloading(DownloadProgress::new(50, 100));
        let _paused = DownloadStatus::Paused(DownloadProgress::new(50, 100));
        let _completed = DownloadStatus::Completed;
        let _already_present = DownloadStatus::AlreadyPresent;
        let _failed = DownloadStatus::Failed(ManagementError::DownloadFailed);
    }

    #[test]
    fn management_error_variants_are_closed() {
        // Verify all error variants exist and serialize.
        let errors = vec![
            ManagementError::EndpointUnreachable,
            ManagementError::Unauthorized,
            ManagementError::ModelNotFound,
            ManagementError::InvalidResponse,
            ManagementError::DownloadFailed,
            ManagementError::LoadFailed,
            ManagementError::UnloadFailed,
            ManagementError::UnsupportedOperation,
            ManagementError::ProviderBusy,
            ManagementError::OperationNotFound,
        ];
        for error in errors {
            let json = serde_json::to_string(&error).unwrap();
            assert!(!json.is_empty());
        }
    }

    #[test]
    fn download_status_terminal_states() {
        assert!(DownloadStatus::Completed.is_terminal());
        assert!(DownloadStatus::AlreadyPresent.is_terminal());
        assert!(DownloadStatus::Failed(ManagementError::DownloadFailed).is_terminal());

        assert!(!DownloadStatus::Starting.is_terminal());
        assert!(!DownloadStatus::Downloading(DownloadProgress::new(50, 100)).is_terminal());
        assert!(!DownloadStatus::Paused(DownloadProgress::new(50, 100)).is_terminal());
    }

    #[test]
    fn download_status_serializes_to_snake_case() {
        let status = DownloadStatus::Downloading(DownloadProgress::new(50, 100));
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("downloading"));
    }

    #[test]
    fn download_progress_serializes_all_fields() {
        let progress = DownloadProgress::new(50, 100).with_rate(10.0);
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"bytes_downloaded\":50"));
        assert!(json.contains("\"total_bytes\":100"));
        assert!(json.contains("\"bytes_per_second\":10.0"));
        assert!(json.contains("\"estimated_completion_seconds\":5.0"));
    }

    #[test]
    fn download_status_starting_exact_shape() {
        let status = DownloadStatus::Starting;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"starting\"");
    }

    #[test]
    fn download_status_completed_exact_shape() {
        let status = DownloadStatus::Completed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"completed\"");
    }

    #[test]
    fn download_status_already_present_exact_shape() {
        let status = DownloadStatus::AlreadyPresent;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"already_present\"");
    }

    #[test]
    fn download_status_downloading_exact_shape() {
        let progress = DownloadProgress {
            bytes_downloaded: 50,
            total_bytes: Some(100),
            bytes_per_second: Some(10.0),
            estimated_completion_seconds: Some(5.0),
        };
        let status = DownloadStatus::Downloading(progress);
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(
            json,
            "{\"downloading\":{\"bytes_downloaded\":50,\"total_bytes\":100,\"bytes_per_second\":10.0,\"estimated_completion_seconds\":5.0}}"
        );
    }

    #[test]
    fn download_status_paused_exact_shape() {
        let progress = DownloadProgress {
            bytes_downloaded: 75,
            total_bytes: Some(200),
            bytes_per_second: None,
            estimated_completion_seconds: None,
        };
        let status = DownloadStatus::Paused(progress);
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(
            json,
            "{\"paused\":{\"bytes_downloaded\":75,\"total_bytes\":200,\"bytes_per_second\":null,\"estimated_completion_seconds\":null}}"
        );
    }

    #[test]
    fn download_status_failed_exact_shape() {
        let status = DownloadStatus::Failed(ManagementError::DownloadFailed);
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "{\"failed\":\"download_failed\"}");
    }

    #[test]
    fn management_error_all_variants_exact_shape() {
        let cases = vec![
            (
                ManagementError::EndpointUnreachable,
                "\"endpoint_unreachable\"",
            ),
            (ManagementError::Unauthorized, "\"unauthorized\""),
            (ManagementError::ModelNotFound, "\"model_not_found\""),
            (ManagementError::InvalidResponse, "\"invalid_response\""),
            (ManagementError::DownloadFailed, "\"download_failed\""),
            (ManagementError::LoadFailed, "\"load_failed\""),
            (ManagementError::UnloadFailed, "\"unload_failed\""),
            (
                ManagementError::UnsupportedOperation,
                "\"unsupported_operation\"",
            ),
            (ManagementError::ProviderBusy, "\"provider_busy\""),
            (
                ManagementError::OperationNotFound,
                "\"operation_not_found\"",
            ),
        ];
        for (error, expected) in cases {
            let json = serde_json::to_string(&error).unwrap();
            assert_eq!(json, expected, "Mismatch for {:?}", error);
        }
    }
}
