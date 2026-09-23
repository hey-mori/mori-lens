//! Model management types for provider setup.
//!
//! Types for tracking model download progress, listing available models,
//! and managing model lifecycle. P20 first-card/provider readiness.

use serde::{Deserialize, Serialize};

/// Progress of a model download operation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes to download (None if unknown).
    pub total_bytes: Option<u64>,
}

impl DownloadProgress {
    /// Create a new download progress with known total.
    pub fn new(bytes_downloaded: u64, total_bytes: u64) -> Self {
        Self {
            bytes_downloaded,
            total_bytes: Some(total_bytes),
        }
    }

    /// Create a new download progress with unknown total.
    pub fn unknown_total(bytes_downloaded: u64) -> Self {
        Self {
            bytes_downloaded,
            total_bytes: None,
        }
    }

    /// Calculate the download percentage (0.0 to 100.0).
    /// Returns None if total_bytes is unknown.
    pub fn percentage(&self) -> Option<f64> {
        self.total_bytes.map(|total| {
            if total == 0 {
                100.0
            } else {
                (self.bytes_downloaded as f64 / total as f64) * 100.0
            }
        })
    }

    /// Check if the download is complete.
    pub fn is_complete(&self) -> bool {
        match self.total_bytes {
            Some(total) => self.bytes_downloaded >= total,
            None => false,
        }
    }
}

/// Status of a model download operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// Download has not started yet.
    NotStarted,
    /// Download is in progress.
    InProgress(DownloadProgress),
    /// Download completed successfully.
    Completed,
    /// Download failed with an error message.
    Failed(String),
    /// Download was cancelled by the user.
    Cancelled,
}

impl Default for DownloadStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

/// Information about an available model from a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model name/identifier (e.g., "llama3.2", "gemma2").
    pub name: String,
    /// Model size in bytes (if known).
    pub size_bytes: Option<u64>,
    /// Whether the model is already downloaded/installed locally.
    pub is_downloaded: bool,
    /// Model family/tag (e.g., "llama", "gemma", "mistral").
    pub family: Option<String>,
}

impl ModelInfo {
    /// Create a new ModelInfo for a downloaded model.
    pub fn downloaded(name: impl Into<String>, size_bytes: Option<u64>) -> Self {
        Self {
            name: name.into(),
            size_bytes,
            is_downloaded: true,
            family: None,
        }
    }

    /// Create a new ModelInfo for an available but not downloaded model.
    pub fn available(name: impl Into<String>, size_bytes: Option<u64>) -> Self {
        Self {
            name: name.into(),
            size_bytes,
            is_downloaded: false,
            family: None,
        }
    }

    /// Set the model family.
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }
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
    fn download_progress_is_complete() {
        let progress = DownloadProgress::new(100, 100);
        assert!(progress.is_complete());

        let incomplete = DownloadProgress::new(50, 100);
        assert!(!incomplete.is_complete());

        let unknown = DownloadProgress::unknown_total(50);
        assert!(!unknown.is_complete());
    }

    #[test]
    fn download_progress_zero_total() {
        let progress = DownloadProgress::new(0, 0);
        assert_eq!(progress.percentage(), Some(100.0));
        assert!(progress.is_complete());
    }

    #[test]
    fn download_status_default_is_not_started() {
        let status = DownloadStatus::default();
        assert_eq!(status, DownloadStatus::NotStarted);
    }

    #[test]
    fn model_info_downloaded() {
        let model = ModelInfo::downloaded("llama3.2", Some(4_000_000_000));
        assert!(model.is_downloaded);
        assert_eq!(model.name, "llama3.2");
        assert_eq!(model.size_bytes, Some(4_000_000_000));
    }

    #[test]
    fn model_info_available() {
        let model = ModelInfo::available("gemma2", Some(2_000_000_000));
        assert!(!model.is_downloaded);
        assert_eq!(model.name, "gemma2");
    }

    #[test]
    fn model_info_with_family() {
        let model = ModelInfo::available("llama3.2", None).with_family("llama");
        assert_eq!(model.family, Some("llama".to_string()));
    }

    #[test]
    fn download_status_serializes_to_snake_case() {
        let status = DownloadStatus::InProgress(DownloadProgress::new(50, 100));
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("in_progress"));
    }
}
