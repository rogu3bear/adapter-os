//! Dataset integrity verification types.
//!
//! These types support pre-training integrity checks that verify
//! all dataset files match their stored BLAKE3 hashes.

use serde::{Deserialize, Serialize};

// ============================================================================
// Integrity Check Types
// ============================================================================

/// File mismatch information for integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetFileMismatch {
    pub file_name: String,
    pub file_path: String,
    pub expected_hash: String,
    pub actual_hash: String,
}

/// Result of dataset integrity verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetIntegrityResult {
    pub dataset_id: String,
    pub total_files: usize,
    pub verified_files: usize,
    pub mismatches: Vec<DatasetFileMismatch>,
    pub is_valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_result_valid() {
        let result = DatasetIntegrityResult {
            dataset_id: "ds-001".to_string(),
            total_files: 10,
            verified_files: 10,
            mismatches: vec![],
            is_valid: true,
        };
        assert!(result.is_valid);
        assert!(result.mismatches.is_empty());
        assert_eq!(result.total_files, result.verified_files);
    }

    #[test]
    fn integrity_result_with_mismatches() {
        let result = DatasetIntegrityResult {
            dataset_id: "ds-001".to_string(),
            total_files: 10,
            verified_files: 8,
            mismatches: vec![DatasetFileMismatch {
                file_name: "data.jsonl".to_string(),
                file_path: "/data/ds-001/data.jsonl".to_string(),
                expected_hash: "a".repeat(64),
                actual_hash: "b".repeat(64),
            }],
            is_valid: false,
        };
        assert!(!result.is_valid);
        assert_eq!(result.mismatches.len(), 1);
        assert_eq!(result.mismatches[0].file_name, "data.jsonl");
    }
}
