//! Dataset snapshot types for training run integrity.
//!
//! These types capture immutable dataset state at training run initiation
//! for reproducibility and audit purposes.

use serde::{Deserialize, Serialize};

// ============================================================================
// Snapshot Types
// ============================================================================

/// Parameters for snapshotting a dataset for a training run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDatasetForRunParams {
    /// Dataset ID to snapshot
    pub dataset_id: String,
    /// Optional tenant ID for isolation
    pub tenant_id: Option<String>,
    /// Whether to verify file integrity during snapshot
    pub verify_integrity: bool,
    /// Whether to require trusted status
    pub require_trusted: bool,
}

/// Snapshot of dataset state at training run initiation
///
/// Captures immutable dataset metadata for reproducibility and audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingRunDatasetSnapshot {
    /// Dataset ID
    pub dataset_id: String,
    /// Dataset version ID
    pub dataset_version_id: String,
    /// BLAKE3 hash of the version
    pub version_hash_b3: String,
    /// Trust state at snapshot time
    pub trust_state_at_snapshot: String,
    /// Validation status at snapshot time
    pub validation_status_at_snapshot: String,
    /// When the snapshot was taken
    pub snapshot_timestamp: String,
    /// Storage path at snapshot time
    pub storage_path: String,
    /// Version number
    pub version_number: i64,
    /// Manifest JSON if available
    pub manifest_json: Option<String>,
}

/// Result of verifying a dataset snapshot against current state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSnapshotVerification {
    /// Dataset version ID
    pub dataset_version_id: String,
    /// Original snapshot timestamp
    pub snapshot_timestamp: String,
    /// Whether the snapshot is still valid (no changes)
    pub is_valid: bool,
    /// List of detected changes since snapshot
    pub changes: Vec<String>,
    /// When verification was performed
    pub verified_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_params_defaults() {
        let params = SnapshotDatasetForRunParams {
            dataset_id: "ds-001".to_string(),
            tenant_id: None,
            verify_integrity: true,
            require_trusted: false,
        };
        assert!(params.verify_integrity);
        assert!(!params.require_trusted);
    }

    #[test]
    fn training_run_snapshot_fields() {
        let snapshot = TrainingRunDatasetSnapshot {
            dataset_id: "ds-001".to_string(),
            dataset_version_id: "dsv-001".to_string(),
            version_hash_b3: "a".repeat(64),
            trust_state_at_snapshot: "allowed".to_string(),
            validation_status_at_snapshot: "passed".to_string(),
            snapshot_timestamp: "2025-01-01T00:00:00Z".to_string(),
            storage_path: "/data/datasets/ds-001".to_string(),
            version_number: 1,
            manifest_json: None,
        };
        assert_eq!(snapshot.trust_state_at_snapshot, "allowed");
        assert_eq!(snapshot.version_number, 1);
    }

    #[test]
    fn verification_result_valid() {
        let verification = DatasetSnapshotVerification {
            dataset_version_id: "dsv-001".to_string(),
            snapshot_timestamp: "2025-01-01T00:00:00Z".to_string(),
            is_valid: true,
            changes: vec![],
            verified_at: "2025-01-02T00:00:00Z".to_string(),
        };
        assert!(verification.is_valid);
        assert!(verification.changes.is_empty());
    }

    #[test]
    fn verification_result_with_changes() {
        let verification = DatasetSnapshotVerification {
            dataset_version_id: "dsv-001".to_string(),
            snapshot_timestamp: "2025-01-01T00:00:00Z".to_string(),
            is_valid: false,
            changes: vec![
                "Hash changed from abc to def".to_string(),
                "Trust state changed from allowed to blocked".to_string(),
            ],
            verified_at: "2025-01-02T00:00:00Z".to_string(),
        };

        assert!(!verification.is_valid);
        assert_eq!(verification.changes.len(), 2);
        assert!(verification.changes[0].contains("Hash changed"));
        assert!(verification.changes[1].contains("Trust state"));
    }
}
