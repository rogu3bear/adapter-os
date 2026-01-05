//! Core type definitions for training datasets.
//!
//! This module contains the primary struct definitions for training datasets,
//! versions, files, evidence, and related entities.

use serde::{Deserialize, Serialize};

// ============================================================================
// Training Dataset
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrainingDataset {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub format: String,
    pub hash_b3: String,
    pub dataset_hash_b3: String,
    pub storage_path: String,
    pub status: String,
    pub validation_status: String,
    pub validation_errors: Option<String>,
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
    pub tenant_id: Option<String>,
    pub workspace_id: Option<String>,
    // Hash repair tracking (added in migration 0239)
    pub hash_needs_recompute: i32,
    pub hash_algorithm_version: i32,
    // Repository slug for filtering datasets by source repo (e.g., "org/repo-name")
    pub repo_slug: Option<String>,
    // Branch run tracking (added in migration 0248)
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    // Session lineage fields (migration 0256)
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub session_tags: Option<String>,
    // Scope metadata fields (migration 0257)
    pub scope_repo_id: Option<String>,
    pub scope_repo: Option<String>,
    pub scope_scan_root: Option<String>,
    pub scope_remote_url: Option<String>,
    // Aggregate metrics (migration 0259)
    pub scan_root_count: Option<i32>,
    pub total_scan_root_files: Option<i32>,
    pub total_scan_root_bytes: Option<i64>,
    pub scan_roots_content_hash: Option<String>,
    pub scan_roots_updated_at: Option<String>,
}

// ============================================================================
// Dataset Version
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrainingDatasetVersion {
    pub id: String,
    pub dataset_id: String,
    pub tenant_id: Option<String>,
    pub version_number: i64,
    pub version_label: Option<String>,
    pub storage_path: String,
    pub hash_b3: String,
    pub manifest_path: Option<String>,
    pub manifest_json: Option<String>,
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
    pub created_at: String,
    pub created_by: Option<String>,
    pub locked_at: Option<String>,
    pub soft_deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetVersionValidation {
    pub id: String,
    pub dataset_version_id: String,
    pub tier: String,
    pub status: String,
    pub signal: Option<String>,
    pub validation_errors_json: Option<String>,
    pub sample_row_ids_json: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetVersionOverride {
    pub id: String,
    pub dataset_version_id: String,
    pub override_state: String,
    pub reason: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

// ============================================================================
// Dataset Files
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetFile {
    pub id: String,
    pub dataset_id: String,
    pub file_name: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub hash_b3: String,
    pub mime_type: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetStatistics {
    pub dataset_id: String,
    pub num_examples: i32,
    pub avg_input_length: f64,
    pub avg_target_length: f64,
    pub language_distribution: Option<String>,
    pub file_type_distribution: Option<String>,
    pub total_tokens: i64,
    pub computed_at: String,
}

// ============================================================================
// Dataset Hash Inputs (for reproducibility)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetHashInputs {
    pub id: String,
    pub dataset_id: Option<String>,
    pub content_hash_b3: String,
    pub repo_id: Option<String>,
    pub repo_slug: Option<String>,
    pub commit_sha: Option<String>,
    pub branch: Option<String>,
    pub scan_root_path: Option<String>,
    pub remote_url: Option<String>,
    pub max_symbols: Option<i64>,
    pub include_private: Option<i32>,
    pub positive_weight: Option<f64>,
    pub negative_weight: Option<f64>,
    pub total_samples: i64,
    pub positive_samples: i64,
    pub negative_samples: i64,
    pub ingestion_mode: Option<String>,
    pub codegraph_version: Option<String>,
    pub generator: Option<String>,
    pub scope_config_json: Option<String>,
    pub additional_inputs_json: Option<String>,
    pub tenant_id: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
    /// HKDF algorithm version used for seed derivation (NULL for legacy data)
    pub hkdf_version: Option<i64>,
    /// Parser algorithm version used for directory scanning (NULL for legacy data)
    pub parser_version: Option<i64>,
    /// Path normalization version used for deterministic sorting (NULL for legacy data)
    pub path_normalization_version: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct CreateDatasetHashInputsParams {
    pub dataset_id: Option<String>,
    pub content_hash_b3: String,
    pub repo_id: Option<String>,
    pub repo_slug: Option<String>,
    pub commit_sha: Option<String>,
    pub branch: Option<String>,
    pub scan_root_path: Option<String>,
    pub remote_url: Option<String>,
    pub max_symbols: Option<i64>,
    pub include_private: Option<bool>,
    pub positive_weight: Option<f64>,
    pub negative_weight: Option<f64>,
    pub total_samples: i64,
    pub positive_samples: i64,
    pub negative_samples: i64,
    pub ingestion_mode: String,
    pub codegraph_version: Option<String>,
    pub generator: String,
    pub scope_config_json: Option<String>,
    pub additional_inputs_json: Option<String>,
    pub tenant_id: Option<String>,
    pub created_by: Option<String>,
    /// HKDF algorithm version used for seed derivation
    pub hkdf_version: Option<u32>,
    /// Parser algorithm version used for directory scanning
    pub parser_version: Option<u32>,
    /// Path normalization version used for deterministic sorting
    pub path_normalization_version: Option<u32>,
}

impl CreateDatasetHashInputsParams {
    pub fn new(
        content_hash_b3: impl Into<String>,
        total_samples: i64,
        positive_samples: i64,
        negative_samples: i64,
    ) -> Self {
        use adapteros_core::AlgorithmVersionBundle;
        let versions = AlgorithmVersionBundle::current();
        Self {
            dataset_id: None,
            content_hash_b3: content_hash_b3.into(),
            repo_id: None,
            repo_slug: None,
            commit_sha: None,
            branch: None,
            scan_root_path: None,
            remote_url: None,
            max_symbols: None,
            include_private: None,
            positive_weight: None,
            negative_weight: None,
            total_samples,
            positive_samples,
            negative_samples,
            ingestion_mode: "code_graph".to_string(),
            codegraph_version: versions.codegraph_version,
            generator: "code_ingestion_pipeline".to_string(),
            scope_config_json: None,
            additional_inputs_json: None,
            tenant_id: None,
            created_by: None,
            hkdf_version: Some(versions.hkdf_version),
            parser_version: Some(versions.parser_version),
            path_normalization_version: Some(versions.path_normalization_version),
        }
    }
}

// ============================================================================
// Dataset File Params
// ============================================================================

/// Parameters for inserting dataset file metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatasetFileParams {
    pub file_name: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub hash_b3: String,
    pub mime_type: Option<String>,
}

impl CreateDatasetFileParams {
    pub fn new(
        file_name: impl Into<String>,
        file_path: impl Into<String>,
        size_bytes: i64,
        hash_b3: impl Into<String>,
    ) -> Self {
        Self {
            file_name: file_name.into(),
            file_path: file_path.into(),
            size_bytes,
            hash_b3: hash_b3.into(),
            mime_type: None,
        }
    }

    pub fn mime_type(mut self, mime_type: Option<impl Into<String>>) -> Self {
        self.mime_type = mime_type.map(|value| value.into());
        self
    }
}

// ============================================================================
// Evidence
// ============================================================================

/// Evidence entry for datasets and adapters
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EvidenceEntry {
    pub id: String,
    pub dataset_id: Option<String>,
    pub adapter_id: Option<String>,
    pub evidence_type: String,
    pub reference: String,
    pub description: Option<String>,
    pub confidence: String,
    pub created_by: Option<String>,
    pub created_at: String,
    pub metadata_json: Option<String>,
}

/// Parameters for creating evidence entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEvidenceParams {
    pub dataset_id: Option<String>,
    pub adapter_id: Option<String>,
    pub evidence_type: String,
    pub reference: String,
    pub description: Option<String>,
    pub confidence: String,
    pub created_by: Option<String>,
    pub metadata_json: Option<String>,
}

/// Filter for listing evidence entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceFilter {
    pub dataset_id: Option<String>,
    pub adapter_id: Option<String>,
    pub evidence_type: Option<String>,
    pub confidence: Option<String>,
    pub limit: Option<i64>,
}

// ============================================================================
// Dataset-Adapter Links
// ============================================================================

/// Dataset-to-adapter link for tracking training lineage
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetAdapterLink {
    pub id: String,
    pub dataset_id: String,
    pub adapter_id: String,
    pub link_type: String,
    pub created_at: String,
}

// ============================================================================
// Adapter Training Lineage (migration 0258)
// ============================================================================

/// Adapter training lineage record for reverse lookups
///
/// Enables queries like:
/// - "Which adapters were trained on this dataset version?"
/// - "Which dataset versions contributed to this adapter?"
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterTrainingLineage {
    pub id: String,
    pub adapter_id: String,
    pub dataset_id: String,
    pub dataset_version_id: Option<String>,
    pub training_job_id: Option<String>,
    pub dataset_hash_b3_at_training: Option<String>,
    pub role: String,
    pub weight: Option<f64>,
    pub ordinal: i32,
    pub tenant_id: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
    pub metadata_json: Option<String>,
}

// ============================================================================
// Dataset Collection Sessions
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetCollectionSession {
    pub id: String,
    pub name: String,
    pub tags: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub parent_session_id: Option<String>,
    pub external_correlation_id: Option<String>,
    pub dataset_count: i64,
    pub adapter_count: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<f64>,
    pub initiated_by: Option<String>,
    pub tenant_id: Option<String>,
    pub error_message: Option<String>,
    pub error_details: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetSessionMembership {
    pub id: String,
    pub session_id: String,
    pub dataset_id: String,
    pub operation_type: String,
    pub ordinal: i32,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterSessionMembership {
    pub id: String,
    pub session_id: String,
    pub adapter_id: String,
    pub operation_type: String,
    pub ordinal: i32,
    pub added_at: String,
}
