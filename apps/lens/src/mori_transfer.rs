//! Send a library item to Mori across a process boundary.
//!
//! The boundary is a file on disk: Lens writes a JSON payload to
//! `%USERPROFILE%/.mori/connectors/lens/` and Mori reads it. No network, no
//! shared library, no assumption that Mori is running.
//!
//! See [`ADR-0018`](../../docs/decisions/ADR-0018-mori-process-boundary.md).

use lens_storage::Item;
use serde::Serialize;
use std::path::PathBuf;
use uuid::Uuid;

/// What crosses the boundary.
///
/// Only the semantic fields Mori needs. No Lens database id, no review
/// scheduling, no application name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPayload {
    pub schema_version: u32,
    pub transfer_id: String,
    pub original: String,
    pub answer: String,
    pub kind: String,
    pub source_language: String,
    pub target_language: String,
    pub note: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

/// Why a transfer did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    /// Mori's connector directory does not exist.
    MoriNotInstalled,
    /// The file could not be written (disk full, permission denied).
    IoError(String),
    /// Serialization failed (should not happen; a bug).
    SerializeError(String),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MoriNotInstalled => write!(f, "Mori is not installed"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::SerializeError(msg) => write!(f, "serialization error: {msg}"),
        }
    }
}

impl std::error::Error for TransferError {}

/// The directory where Lens writes transfer files.
///
/// Returns `None` if the parent `~/.mori/connectors/` does not exist, which
/// means Mori is not installed or not this version.
pub fn mori_connectors_dir() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE").ok()?;
    let connectors = PathBuf::from(home).join(".mori").join("connectors");
    if connectors.is_dir() {
        Some(connectors)
    } else {
        None
    }
}

/// The Lens-specific subdirectory inside the connectors directory.
///
/// Creates it if it does not exist. Returns `None` if the parent does not
/// exist (Mori not installed).
pub fn mori_lens_dir() -> Option<PathBuf> {
    let connectors = mori_connectors_dir()?;
    let lens_dir = connectors.join("lens");
    if !lens_dir.exists() {
        std::fs::create_dir_all(&lens_dir).ok()?;
    }
    Some(lens_dir)
}

/// Whether Mori appears to be installed (the connectors directory exists).
pub fn is_mori_available() -> bool {
    mori_connectors_dir().is_some()
}

/// Convert a library item to a transfer payload.
pub fn payload_from_item(item: &Item) -> TransferPayload {
    let transfer_id = Uuid::new_v4().to_string();
    let created_at = chrono::DateTime::from_timestamp(item.created_at, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();

    TransferPayload {
        schema_version: 1,
        transfer_id,
        original: item.original.clone(),
        answer: item.answer.clone(),
        kind: item.kind.clone(),
        source_language: item.source_language.clone().unwrap_or_default(),
        target_language: item.target_language.clone().unwrap_or_default(),
        note: item.note.clone().unwrap_or_default(),
        tags: item.tags.clone().unwrap_or_default(),
        created_at,
    }
}

/// Write the payload to the Mori connector directory.
///
/// The file is `{transfer_id}.json` in `~/.mori/connectors/lens/`.
pub fn send_to_mori(item: &Item) -> Result<String, TransferError> {
    let lens_dir = mori_lens_dir().ok_or(TransferError::MoriNotInstalled)?;
    let payload = payload_from_item(item);
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| TransferError::SerializeError(e.to_string()))?;
    let file_path = lens_dir.join(format!("{}.json", payload.transfer_id));
    std::fs::write(&file_path, json).map_err(|e| TransferError::IoError(e.to_string()))?;
    Ok(payload.transfer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_item() -> Item {
        Item {
            id: 1,
            original: "The output files came back empty.".to_string(),
            answer: "출력 파일이 비어 있었습니다.".to_string(),
            caveat: None,
            app: None,
            created_at: 1_725_372_600,
            kind: "translation".to_string(),
            source_language: Some("en".to_string()),
            target_language: Some("ko".to_string()),
            model: None,
            note: Some("a note".to_string()),
            tags: Some(vec!["cargo".to_string(), "build".to_string()]),
            review_state: None,
            next_review_at: None,
            review_count: None,
        }
    }

    #[test]
    fn payload_omits_empty_optionals_as_empty_strings() {
        let item = Item {
            source_language: None,
            target_language: None,
            note: None,
            tags: None,
            ..sample_item()
        };
        let payload = payload_from_item(&item);
        assert_eq!(payload.source_language, "");
        assert_eq!(payload.target_language, "");
        assert_eq!(payload.note, "");
        assert!(payload.tags.is_empty());
    }

    #[test]
    fn payload_preserves_kind() {
        let item = sample_item();
        let payload = payload_from_item(&item);
        assert_eq!(payload.kind, "translation");
    }

    #[test]
    fn payload_carries_tags() {
        let item = sample_item();
        let payload = payload_from_item(&item);
        assert_eq!(payload.tags, vec!["cargo", "build"]);
    }

    #[test]
    fn payload_schema_version_is_one() {
        let item = sample_item();
        let payload = payload_from_item(&item);
        assert_eq!(payload.schema_version, 1);
    }

    #[test]
    fn payload_transfer_id_is_uuid_format() {
        let item = sample_item();
        let payload = payload_from_item(&item);
        assert_eq!(payload.transfer_id.len(), 36);
        assert_eq!(payload.transfer_id.matches('-').count(), 4);
    }

    #[test]
    fn payload_serializes_to_expected_json_shape() {
        let item = sample_item();
        let payload = payload_from_item(&item);
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"schemaVersion\":1"));
        assert!(json.contains("\"transferId\":"));
        assert!(json.contains("\"original\":\"The output files came back empty.\""));
        assert!(json.contains("\"kind\":\"translation\""));
        assert!(json.contains("\"sourceLanguage\":\"en\""));
        assert!(json.contains("\"targetLanguage\":\"ko\""));
        assert!(json.contains("\"note\":\"a note\""));
        assert!(json.contains("\"tags\":[\"cargo\",\"build\"]"));
    }

    #[test]
    fn mori_not_installed_returns_none() {
        // In CI/test environments, ~/.mori/connectors does not exist.
        // This test documents that the function returns None gracefully.
        // If Mori IS installed on the test machine, this test is skipped.
        if mori_connectors_dir().is_some() {
            return;
        }
        assert!(!is_mori_available());
    }

    #[test]
    fn send_without_mori_returns_error() {
        if mori_connectors_dir().is_some() {
            return;
        }
        let item = sample_item();
        let result = send_to_mori(&item);
        assert_eq!(result.unwrap_err(), TransferError::MoriNotInstalled);
    }
}
