//! Model management types for provider setup.
//!
//! Types for tracking model download progress, listing available models,
//! and managing model lifecycle. P20 first-card/provider readiness.
//!
//! These types are provider-neutral. The platform adapter converts provider-specific
//! JSON shapes (Ollama streaming progress, LM Studio job status) into these types.

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
    pub fn with_rate(mut self, bytes_per_second: f64) -> Self {
        self.bytes_per_second = Some(bytes_per_second);
        // Compute ETA if we have total.
        if let Some(total) = self.total_bytes {
            let remaining = total.saturating_sub(self.bytes_downloaded);
            if bytes_per_second > 0.0 {
                self.estimated_completion_seconds = Some(remaining as f64 / bytes_per_second);
            }
        }
        self
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
    /// Currently neither Ollama nor LM Studio support pause, but this state
    /// is included for future providers.
    Paused(DownloadProgress),
    /// Download completed successfully.
    Completed,
    /// Model was already present locally before the download request.
    /// Ollama returns this when POST /api/pull is called for an existing model.
    AlreadyPresent,
    /// Download failed with an error message.
    Failed(String),
    /// Download was cancelled by the user or provider.
    CancelledOrAborted,
}

impl Default for DownloadStatus {
    fn default() -> Self {
        Self::Starting
    }
}

/// Information about an available model from a provider.
///
/// Provider-neutral representation. Ollama returns models with different
/// field names than LM Studio; adapters normalize both into this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model name/identifier (e.g., "llama3.2:latest", "gemma2-9b").
    /// For Ollama: the model name from /api/tags (e.g., "llama3.2:latest").
    /// For LM Studio: the model ID from /api/v1/models.
    pub name: String,
    /// Model size in bytes (if known).
    /// Ollama: size field from /api/tags.
    /// LM Studio: may not be available for all models.
    pub size_bytes: Option<u64>,
    /// Whether the model is already downloaded/installed locally.
    pub is_downloaded: bool,
    /// Model family/tag (e.g., "llama", "gemma", "mistral").
    /// Optional; not all providers expose this.
    pub family: Option<String>,
    /// Provider-specific digest or hash (for deduplication or versioning).
    /// Ollama: digest field from /api/tags.
    /// LM Studio: not available.
    pub digest: Option<String>,
    /// Whether the model is currently loaded in memory.
    /// LM Studio: can be determined from model state.
    /// Ollama: can be inferred from /api/ps but not directly in model info.
    pub is_loaded: Option<bool>,
}

impl ModelInfo {
    /// Create a new ModelInfo for a downloaded model.
    pub fn downloaded(name: impl Into<String>, size_bytes: Option<u64>) -> Self {
        Self {
            name: name.into(),
            size_bytes,
            is_downloaded: true,
            family: None,
            digest: None,
            is_loaded: None,
        }
    }

    /// Create a new ModelInfo for an available but not downloaded model.
    pub fn available(name: impl Into<String>, size_bytes: Option<u64>) -> Self {
        Self {
            name: name.into(),
            size_bytes,
            is_downloaded: false,
            family: None,
            digest: None,
            is_loaded: None,
        }
    }

    /// Set the model family.
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    /// Set the digest (Ollama-specific).
    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        self.digest = Some(digest.into());
        self
    }

    /// Set the loaded state.
    pub fn with_loaded(mut self, is_loaded: bool) -> Self {
        self.is_loaded = Some(is_loaded);
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
        let _failed = DownloadStatus::Failed("error".to_string());
        let _cancelled = DownloadStatus::CancelledOrAborted;
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
    fn model_info_with_digest() {
        let model = ModelInfo::downloaded("llama3.2", None).with_digest("sha256:abc123");
        assert_eq!(model.digest, Some("sha256:abc123".to_string()));
    }

    #[test]
    fn model_info_with_loaded() {
        let model = ModelInfo::downloaded("llama3.2", None).with_loaded(true);
        assert_eq!(model.is_loaded, Some(true));
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
}
