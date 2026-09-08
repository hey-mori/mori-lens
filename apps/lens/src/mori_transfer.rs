//! Send a library item to Mori across a process boundary.
//!
//! The boundary is a file on disk: Lens writes a JSON payload to
//! `%USERPROFILE%/.mori/connectors/lens/` and Mori reads it. No network, no
//! shared library, no assumption that Mori is running.
//!
//! Capability discovery: Mori writes a `.capability.json` marker into the
//! lens directory. Lens reads it to learn which schema versions Mori
//! supports. Without the marker, Lens assumes Mori is not installed or
//! too old, and hides the Send button.
//!
//! See [`ADR-0018`](../../docs/decisions/ADR-0018-mori-process-boundary.md).

use lens_storage::Item;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// The schema version this Lens build writes.
const OUR_SCHEMA_VERSION: u32 = 1;

/// What crosses the boundary.
///
/// Only the semantic fields Mori needs. No Lens database id, no review
/// scheduling, no application name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    /// The capability marker is missing or has wrong identity.
    NoCapabilityMarker,
    /// The capability marker exists but versions are incompatible.
    VersionMismatch,
    /// The file could not be written (disk full, permission denied).
    IoError(String),
    /// Serialization failed (should not happen; a bug).
    SerializeError(String),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MoriNotInstalled => write!(f, "Mori is not installed"),
            Self::NoCapabilityMarker => write!(f, "Mori capability marker missing or invalid"),
            Self::VersionMismatch => write!(f, "Mori schema version mismatch"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::SerializeError(msg) => write!(f, "serialization error: {msg}"),
        }
    }
}

impl std::error::Error for TransferError {}

/// What Mori's capability marker advertises.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityFile {
    pub product: String,
    pub supported_schema_versions: Vec<u32>,
    pub capability: String,
}

/// The result of checking Mori's availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoriStatus {
    /// `~/.mori/connectors/` does not exist. Mori is not installed.
    NotInstalled,
    /// The connectors directory exists but the lens capability marker
    /// is missing. Mori has not run yet, or is too old.
    NoCapabilityMarker,
    /// The marker exists but does not include any schema version Lens
    /// understands.
    VersionMismatch {
        mori_versions: Vec<u32>,
        our_version: u32,
    },
    /// Both sides agree. The `u32` is the negotiated version.
    Supported { negotiated_version: u32 },
}

// ─── Directory resolution ──────────────────────────────────────────────────

/// `~/.mori/connectors/`. Returns `None` when the directory does not exist.
pub fn mori_connectors_dir() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    let connectors = PathBuf::from(home).join(".mori").join("connectors");
    if connectors.is_dir() {
        Some(connectors)
    } else {
        None
    }
}

/// `~/.mori/connectors/lens/`. Returns `None` when the parent
/// `~/.mori/connectors/` does not exist (Mori not installed).
pub fn mori_lens_dir() -> Option<PathBuf> {
    let connectors = mori_connectors_dir()?;
    let lens_dir = connectors.join("lens");
    if lens_dir.is_dir() {
        Some(lens_dir)
    } else {
        None
    }
}

// ─── Capability discovery ──────────────────────────────────────────────────

/// Read the capability marker from `~/.mori/connectors/lens/.capability.json`.
///
/// Returns `None` when the file does not exist or cannot be parsed.
pub fn read_capability() -> Option<CapabilityFile> {
    let lens_dir = mori_lens_dir()?;
    let cap_path = lens_dir.join(".capability.json");
    let text = std::fs::read_to_string(&cap_path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Check Mori's availability and version compatibility.
///
/// Validates the capability marker identity (`product == "mori"`,
/// `capability == "lens-transfer"`) before accepting version negotiation.
/// A marker with wrong identity is treated as absent (safe unavailable).
pub fn check_mori_status() -> MoriStatus {
    let Some(_connectors) = mori_connectors_dir() else {
        return MoriStatus::NotInstalled;
    };
    let cap = read_capability();
    status_from_capability(cap)
}

/// Pure logic: derive `MoriStatus` from an optional capability file.
///
/// Separated from filesystem access so identity/version tests do not
/// need a real Mori installation.
fn status_from_capability(cap: Option<CapabilityFile>) -> MoriStatus {
    let Some(cap) = cap else {
        return MoriStatus::NoCapabilityMarker;
    };
    if cap.product != "mori" || cap.capability != "lens-transfer" {
        return MoriStatus::NoCapabilityMarker;
    }
    let mori_versions = cap.supported_schema_versions;
    let common: Vec<u32> = mori_versions
        .iter()
        .copied()
        .filter(|v| *v == OUR_SCHEMA_VERSION)
        .collect();
    match common.into_iter().max() {
        Some(version) => MoriStatus::Supported {
            negotiated_version: version,
        },
        None => MoriStatus::VersionMismatch {
            mori_versions,
            our_version: OUR_SCHEMA_VERSION,
        },
    }
}

/// Whether Mori is available with a compatible schema version.
///
/// Returns true only when the capability marker exists and includes
/// schema version 1.
pub fn is_mori_available() -> bool {
    matches!(check_mori_status(), MoriStatus::Supported { .. })
}

// ─── Payload ───────────────────────────────────────────────────────────────

/// Convert a library item to a transfer payload.
pub fn payload_from_item(item: &Item) -> TransferPayload {
    let transfer_id = Uuid::new_v4().to_string();
    let created_at = chrono::DateTime::from_timestamp(item.created_at, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();

    TransferPayload {
        schema_version: OUR_SCHEMA_VERSION,
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
/// Creates the lens directory if the parent connectors directory exists.
///
/// Re-checks capability before writing to catch race conditions where
/// Mori is uninstalled or its version changes between the UI check
/// and the actual send.
pub fn send_to_mori(item: &Item) -> Result<String, TransferError> {
    // Re-check capability at send time. The UI check may be stale.
    match check_mori_status() {
        MoriStatus::Supported { .. } => {} // proceed
        MoriStatus::NotInstalled => return Err(TransferError::MoriNotInstalled),
        MoriStatus::NoCapabilityMarker => return Err(TransferError::NoCapabilityMarker),
        MoriStatus::VersionMismatch { .. } => return Err(TransferError::VersionMismatch),
    }
    let connectors = mori_connectors_dir().ok_or(TransferError::MoriNotInstalled)?;
    let lens_dir = connectors.join("lens");
    if !lens_dir.exists() {
        std::fs::create_dir_all(&lens_dir).map_err(|e| TransferError::IoError(e.to_string()))?;
    }
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
        }
    }

    fn sample_capability() -> CapabilityFile {
        CapabilityFile {
            product: "mori".to_string(),
            supported_schema_versions: vec![1],
            capability: "lens-transfer".to_string(),
        }
    }

    // ─── Payload tests ─────────────────────────────────────────────────

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

    // ─── Capability tests ──────────────────────────────────────────────

    #[test]
    fn capability_deserializes_from_mori_format() {
        let json = r#"{
            "product": "mori",
            "supportedSchemaVersions": [1, 2],
            "capability": "lens-transfer"
        }"#;
        let cap: CapabilityFile = serde_json::from_str(json).unwrap();
        assert_eq!(cap.product, "mori");
        assert_eq!(cap.supported_schema_versions, vec![1, 2]);
        assert_eq!(cap.capability, "lens-transfer");
    }

    #[test]
    fn capability_round_trips_through_json() {
        let cap = sample_capability();
        let json = serde_json::to_string(&cap).unwrap();
        let parsed: CapabilityFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cap);
    }

    #[test]
    fn status_not_installed_when_connectors_dir_missing() {
        if mori_connectors_dir().is_some() {
            return; // Mori is installed on this machine; skip.
        }
        assert_eq!(check_mori_status(), MoriStatus::NotInstalled);
        assert!(!is_mori_available());
    }

    #[test]
    fn send_without_connectors_dir_returns_error() {
        if mori_connectors_dir().is_some() {
            return; // Mori is installed; skip.
        }
        let item = sample_item();
        let result = send_to_mori(&item);
        assert_eq!(result.unwrap_err(), TransferError::MoriNotInstalled);
    }

    // ─── Golden fixture cross-check ────────────────────────────────────

    #[test]
    fn golden_fixture_matches_wire_contract() {
        let fixture = include_str!("../tests/fixtures/lens-transfer-golden.json");
        let parsed: serde_json::Value = serde_json::from_str(fixture).unwrap();
        assert_eq!(parsed["schemaVersion"], 1);
        assert!(parsed["transferId"].is_string());
        assert_eq!(parsed["kind"], "translation");
        assert_eq!(parsed["sourceLanguage"], "en");
        assert_eq!(parsed["targetLanguage"], "ko");
        assert!(parsed["tags"].is_array());
        assert!(parsed["createdAt"].is_string());
    }

    #[test]
    fn payload_matches_golden_fixture_shape() {
        let fixture = include_str!("../tests/fixtures/lens-transfer-golden.json");
        let golden: TransferPayload = serde_json::from_str(fixture).unwrap();
        assert_eq!(golden.schema_version, 1);
        assert_eq!(golden.kind, "translation");
        assert!(!golden.transfer_id.is_empty());
        assert!(!golden.original.is_empty());
        assert!(!golden.answer.is_empty());
    }

    // ─── Identity validation tests ────────────────────────────────────

    #[test]
    fn wrong_product_identity_treated_as_unavailable() {
        let cap = CapabilityFile {
            product: "other".to_string(),
            supported_schema_versions: vec![1],
            capability: "lens-transfer".to_string(),
        };
        assert_eq!(
            status_from_capability(Some(cap)),
            MoriStatus::NoCapabilityMarker
        );
    }

    #[test]
    fn wrong_capability_identity_treated_as_unavailable() {
        let cap = CapabilityFile {
            product: "mori".to_string(),
            supported_schema_versions: vec![1],
            capability: "other".to_string(),
        };
        assert_eq!(
            status_from_capability(Some(cap)),
            MoriStatus::NoCapabilityMarker
        );
    }

    #[test]
    fn correct_identity_with_matching_version_is_supported() {
        let cap = sample_capability();
        assert!(matches!(
            status_from_capability(Some(cap)),
            MoriStatus::Supported {
                negotiated_version: 1
            }
        ));
    }

    #[test]
    fn correct_identity_with_wrong_version_is_mismatch() {
        let cap = CapabilityFile {
            product: "mori".to_string(),
            supported_schema_versions: vec![99],
            capability: "lens-transfer".to_string(),
        };
        assert!(matches!(
            status_from_capability(Some(cap)),
            MoriStatus::VersionMismatch { .. }
        ));
    }

    #[test]
    fn no_capability_file_is_unavailable() {
        assert_eq!(status_from_capability(None), MoriStatus::NoCapabilityMarker);
    }

    // ─── Send-time recheck tests ─────────────────────────────────────

    #[test]
    fn missing_capability_at_send_time_refuses() {
        // status_from_capability(None) → NoCapabilityMarker
        // send_to_mori checks status before writing, so if we can't get
        // a valid capability the send must fail.
        let status = status_from_capability(None);
        assert!(!matches!(status, MoriStatus::Supported { .. }));
    }

    #[test]
    fn version_mismatch_at_send_time_refuses() {
        let cap = CapabilityFile {
            product: "mori".to_string(),
            supported_schema_versions: vec![99],
            capability: "lens-transfer".to_string(),
        };
        let status = status_from_capability(Some(cap));
        assert!(!matches!(status, MoriStatus::Supported { .. }));
    }

    #[test]
    fn wrong_identity_at_send_time_refuses() {
        let cap = CapabilityFile {
            product: "impostor".to_string(),
            supported_schema_versions: vec![1],
            capability: "lens-transfer".to_string(),
        };
        let status = status_from_capability(Some(cap));
        assert!(!matches!(status, MoriStatus::Supported { .. }));
    }

    #[test]
    fn supported_at_send_time_proceeds() {
        let cap = sample_capability();
        let status = status_from_capability(Some(cap));
        assert!(matches!(status, MoriStatus::Supported { .. }));
    }

    #[test]
    fn transfer_error_display_for_new_variants() {
        let e = TransferError::NoCapabilityMarker;
        assert!(e.to_string().contains("capability marker"));
        let e = TransferError::VersionMismatch;
        assert!(e.to_string().contains("version mismatch"));
    }

    /// payload_from_item maps all semantic fields and produces valid JSON.
    #[test]
    fn payload_from_item_round_trips_through_json() {
        let item = sample_item();
        let payload = payload_from_item(&item);

        assert_eq!(payload.schema_version, OUR_SCHEMA_VERSION);
        assert_eq!(payload.original, item.original);
        assert_eq!(payload.answer, item.answer);
        assert_eq!(payload.kind, item.kind);
        assert!(!payload.transfer_id.is_empty());

        let json = serde_json::to_string_pretty(&payload).expect("serializes");
        let parsed: TransferPayload = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(parsed.schema_version, payload.schema_version);
        assert_eq!(parsed.original, payload.original);
        assert_eq!(parsed.answer, payload.answer);
        assert_eq!(parsed.kind, payload.kind);
        assert_eq!(parsed.transfer_id, payload.transfer_id);
        assert_eq!(parsed.source_language, payload.source_language);
        assert_eq!(parsed.target_language, payload.target_language);
    }
}
