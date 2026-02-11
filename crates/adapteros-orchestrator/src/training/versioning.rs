//! Adapter version state management for training.

use crate::training::job::DatasetVersionSelection;

/// Versioning context for training output.
#[derive(Debug, Clone)]
pub struct TrainingVersioningContext {
    pub adapter_version_id: String,
    pub version_label: String,
    pub branch: String,
    pub repo_id: String,
    pub repo_name: String,
    pub parent_version_id: Option<String>,
    pub draft_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_json: Option<String>,
    pub data_spec_hash: Option<String>,
}

/// Internal versioning snapshot used during training execution.
#[derive(Clone, Debug)]
pub(crate) struct VersioningSnapshot {
    pub adapter_version_id: Option<String>,
    pub version_label: Option<String>,
    pub target_branch: Option<String>,
    pub repo_name: Option<String>,
    pub repo_id: Option<String>,
    pub base_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_hash: Option<String>,
    pub dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
}

/// Deterministic combined data_spec_hash for multi-dataset jobs.
///
/// Input: (dataset_version_id, dataset_manifest_hash, weight)
/// - Sorted by dataset_version_id for stability.
/// - Weight hashed via IEEE-754 little-endian bytes to avoid formatting drift.
pub fn compute_combined_data_spec_hash(entries: &[(String, String, f32)]) -> String {
    let mut items = entries.to_vec();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = blake3::Hasher::new();
    for (id, hash, weight) in items {
        hasher.update(id.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b":");
        hasher.update(&weight.to_le_bytes());
        hasher.update(b";");
    }

    hasher.finalize().to_hex().to_string()
}

// Re-export canonical trust state normalization from the shared types crate.
pub(crate) use adapteros_api_types::training::canonical_trust_state;
