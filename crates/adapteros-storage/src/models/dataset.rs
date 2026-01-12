//! Dataset KV model
//!
//! This module defines the key-value representation of training datasets,
//! matching the database schema from adapteros-db.

use serde::{Deserialize, Serialize};

/// Key-value representation of a training dataset
///
/// This struct matches the TrainingDataset struct from adapteros-db/src/training_datasets.rs
/// All fields are preserved for zero-loss migration from SQL to KV storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingDatasetKv {
    // Core fields
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub format: String,
    pub hash_b3: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
    pub storage_path: String,
    pub status: String,
    pub validation_status: String,
    pub validation_errors: Option<String>,
    pub validation_errors_json: Option<String>,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub metadata_json: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,

    // Dataset Lab extensions for enhanced metadata tracking
    pub dataset_type: Option<String>,
    pub purpose: Option<String>,
    pub source_location: Option<String>,
    pub collection_method: Option<String>,
    pub ownership: Option<String>,
    pub workspace_id: Option<String>,
    // Hash repair tracking fields
    #[serde(default)]
    pub hash_needs_recompute: i32,
    #[serde(default)]
    pub hash_algorithm_version: i32,
    // Repository tracking for deterministic runs
    #[serde(default)]
    pub repo_slug: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub commit_sha: Option<String>,

    // Session lineage fields (migration 0256)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_tags: Option<String>,

    // Scope metadata fields (migration 0257)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_repo_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_scan_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_remote_url: Option<String>,

    // Aggregate metrics (migration 0259)
    #[serde(default)]
    pub scan_root_count: Option<i64>,
    #[serde(default)]
    pub total_scan_root_files: Option<i64>,
    #[serde(default)]
    pub total_scan_root_bytes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_roots_content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_roots_updated_at: Option<String>,
}

impl TrainingDatasetKv {
    /// Get the primary key for this dataset
    pub fn primary_key(&self) -> String {
        format!("dataset:{}", self.id)
    }

    /// Get the tenant-scoped key for this dataset
    pub fn tenant_key(&self) -> String {
        format!("tenant:{}:dataset:{}", self.tenant_id, self.id)
    }

    /// Get hash-based lookup key
    pub fn hash_key(&self) -> String {
        let hash = self.dataset_hash_b3.as_deref().unwrap_or(&self.hash_b3);
        format!("dataset:hash:{}", hash)
    }
}

/// Key-value representation of a dataset version
///
/// This struct matches the TrainingDatasetVersion struct from adapteros-db/src/training_datasets.rs
/// All fields are preserved for zero-loss migration from SQL to KV storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetVersionKv {
    // Core fields
    pub id: String,
    pub dataset_id: String,
    pub tenant_id: String,
    pub version_number: i64,
    pub version_label: Option<String>,
    pub storage_path: String,
    pub hash_b3: String,
    pub manifest_path: Option<String>,
    pub manifest_json: Option<String>,

    // Validation and safety status
    pub validation_status: String,
    pub validation_errors_json: Option<String>,
    pub pii_status: String,
    pub toxicity_status: String,
    pub leak_status: String,
    pub anomaly_status: String,
    pub overall_safety_status: String,
    pub trust_state: String,
    pub overall_trust_status: String,
    pub sensitivity: Option<String>,

    // Metadata
    pub created_at: String,
    pub created_by: Option<String>,
    pub locked_at: Option<String>,
    pub soft_deleted_at: Option<String>,
}

impl DatasetVersionKv {
    /// Get the primary key for this dataset version
    pub fn primary_key(&self) -> String {
        format!("dataset_version:{}", self.id)
    }

    /// Get the dataset-scoped key for this version
    pub fn dataset_key(&self) -> String {
        format!("dataset:{}:version:{}", self.dataset_id, self.id)
    }

    /// Get the tenant-scoped key for this version
    pub fn tenant_key(&self) -> String {
        format!("tenant:{}:dataset_version:{}", self.tenant_id, self.id)
    }

    /// Get hash-based lookup key
    pub fn hash_key(&self) -> String {
        format!("dataset_version:hash:{}", self.hash_b3)
    }
}

/// Key-value representation of dataset statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetStatisticsKv {
    pub dataset_id: String,
    pub num_examples: i32,
    pub avg_input_length: f64,
    pub avg_target_length: f64,
    pub language_distribution: Option<String>,
    pub file_type_distribution: Option<String>,
    pub total_tokens: i64,
    pub computed_at: String,
}

impl DatasetStatisticsKv {
    /// Get the primary key for dataset statistics
    pub fn primary_key(&self) -> String {
        format!("dataset:{}:stats", self.dataset_id)
    }
}
