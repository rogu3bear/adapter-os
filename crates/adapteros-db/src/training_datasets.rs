//! Training dataset database operations
//!
//! This module provides database operations for training datasets, including:
//! - Dataset creation with validation via builder pattern
//! - Dataset version management with trust state derivation
//! - Evidence and lineage tracking
//! - Integrity verification before training
//!
//! # Dataset Creation
//!
//! Use `CreateDatasetParams` with the builder pattern for validated dataset creation:
//!
//! ```ignore
//! use adapteros_db::training_datasets::CreateDatasetParams;
//!
//! let params = CreateDatasetParams::builder()
//!     .name("my-dataset")
//!     .format("jsonl")
//!     .hash_b3("abc123...") // 64 hex chars
//!     .storage_path("/data/datasets/my-dataset")
//!     .tenant_id("tenant-123")
//!     .build()?;
//!
//! let dataset_id = db.create_training_dataset_from_params(&params).await?;
//! ```

use crate::constants::TRAINING_DATASET_COLUMNS;
use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

// ============================================================================
// Dataset Format and Status Validation
// ============================================================================

/// Valid dataset format types
pub const VALID_FORMATS: &[&str] = &["patches", "jsonl", "txt", "custom", "parquet", "csv"];

/// Valid dataset status values
pub const VALID_STATUSES: &[&str] = &["uploaded", "processing", "ready", "failed"];

/// Validate dataset format
pub fn validate_format(format: &str) -> Result<()> {
    if VALID_FORMATS.contains(&format) {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "Invalid dataset format '{}'. Must be one of: {}",
            format,
            VALID_FORMATS.join(", ")
        )))
    }
}

/// Validate dataset status
pub fn validate_status(status: &str) -> Result<()> {
    if VALID_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "Invalid dataset status '{}'. Must be one of: {}",
            status,
            VALID_STATUSES.join(", ")
        )))
    }
}

/// Validate BLAKE3 hash format (64 hex characters)
pub fn validate_hash_b3(hash: &str) -> Result<()> {
    if hash.len() != 64 {
        return Err(AosError::Validation(format!(
            "Invalid hash_b3 length: expected 64 hex characters, got {}",
            hash.len()
        )));
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AosError::Validation(
            "Invalid hash_b3: must contain only hexadecimal characters".to_string(),
        ));
    }
    Ok(())
}

// ============================================================================
// Dataset Creation Parameters and Builder
// ============================================================================

/// Parameters for creating a training dataset
///
/// Use the builder pattern via `CreateDatasetParams::builder()` for validated construction.
#[derive(Debug, Clone)]
pub struct CreateDatasetParams {
    /// Optional pre-generated ID (UUIDv7 generated if not provided)
    pub id: Option<String>,
    /// Dataset name (required)
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Dataset format: patches, jsonl, txt, custom, parquet, csv (required)
    pub format: String,
    /// BLAKE3 hash of the dataset archive/file (required)
    pub hash_b3: String,
    /// Content-derived hash for deduplication (defaults to hash_b3)
    pub dataset_hash_b3: Option<String>,
    /// Storage path for dataset files (required)
    pub storage_path: String,
    /// Dataset status (defaults to "uploaded")
    pub status: String,
    /// User/system that created the dataset
    pub created_by: Option<String>,
    /// Tenant ID for multi-tenant isolation
    pub tenant_id: Option<String>,
    /// Workspace ID within tenant
    pub workspace_id: Option<String>,
    /// Dataset type classification
    pub dataset_type: Option<String>,
    /// Purpose/use case description
    pub purpose: Option<String>,
    /// Source location (URL, path, etc.)
    pub source_location: Option<String>,
    /// How the dataset was collected
    pub collection_method: Option<String>,
    /// Ownership information
    pub ownership: Option<String>,
    /// Additional metadata as JSON
    pub metadata_json: Option<String>,
    /// Dataset category (codebase, metrics, synthetic, upload, etc.)
    pub category: Option<String>,
    /// Repository slug for filtering by source repo (e.g., "org/repo-name")
    pub repo_slug: Option<String>,
}

/// Valid dataset categories
pub const VALID_CATEGORIES: &[&str] = &["codebase", "metrics", "synthetic", "upload", "patches", "general", "other"];

/// Validate dataset category
pub fn validate_category(category: &str) -> Result<()> {
    if VALID_CATEGORIES.contains(&category) {
        Ok(())
    } else {
        Err(AosError::Validation(format!(
            "Invalid dataset category '{}'. Must be one of: {}",
            category,
            VALID_CATEGORIES.join(", ")
        )))
    }
}

/// Builder for creating `CreateDatasetParams` with validation
#[derive(Debug, Default)]
pub struct CreateDatasetParamsBuilder {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    format: Option<String>,
    hash_b3: Option<String>,
    dataset_hash_b3: Option<String>,
    storage_path: Option<String>,
    status: Option<String>,
    created_by: Option<String>,
    tenant_id: Option<String>,
    workspace_id: Option<String>,
    dataset_type: Option<String>,
    purpose: Option<String>,
    source_location: Option<String>,
    collection_method: Option<String>,
    ownership: Option<String>,
    metadata_json: Option<String>,
    category: Option<String>,
    repo_slug: Option<String>,
}

impl CreateDatasetParams {
    /// Create a new builder for dataset creation parameters
    pub fn builder() -> CreateDatasetParamsBuilder {
        CreateDatasetParamsBuilder::default()
    }
}

impl CreateDatasetParamsBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a pre-generated dataset ID (optional, UUIDv7 generated if not set)
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the dataset name (required)
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the dataset description (optional)
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the dataset format (required): patches, jsonl, txt, custom, parquet, csv
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Set the BLAKE3 hash of the dataset (required)
    pub fn hash_b3(mut self, hash_b3: impl Into<String>) -> Self {
        self.hash_b3 = Some(hash_b3.into());
        self
    }

    /// Set the content-derived hash for deduplication (optional, defaults to hash_b3)
    pub fn dataset_hash_b3(mut self, dataset_hash_b3: impl Into<String>) -> Self {
        self.dataset_hash_b3 = Some(dataset_hash_b3.into());
        self
    }

    /// Set the storage path (required)
    pub fn storage_path(mut self, storage_path: impl Into<String>) -> Self {
        self.storage_path = Some(storage_path.into());
        self
    }

    /// Set the dataset status (optional, defaults to "uploaded")
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    /// Set the creator identifier (optional)
    pub fn created_by(mut self, created_by: impl Into<String>) -> Self {
        self.created_by = Some(created_by.into());
        self
    }

    /// Set the tenant ID for multi-tenant isolation (recommended)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the workspace ID within the tenant (optional)
    pub fn workspace_id(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    /// Set the dataset type classification (optional)
    pub fn dataset_type(mut self, dataset_type: impl Into<String>) -> Self {
        self.dataset_type = Some(dataset_type.into());
        self
    }

    /// Set the purpose/use case (optional)
    pub fn purpose(mut self, purpose: impl Into<String>) -> Self {
        self.purpose = Some(purpose.into());
        self
    }

    /// Set the source location (optional)
    pub fn source_location(mut self, source_location: impl Into<String>) -> Self {
        self.source_location = Some(source_location.into());
        self
    }

    /// Set the collection method (optional)
    pub fn collection_method(mut self, collection_method: impl Into<String>) -> Self {
        self.collection_method = Some(collection_method.into());
        self
    }

    /// Set ownership information (optional)
    pub fn ownership(mut self, ownership: impl Into<String>) -> Self {
        self.ownership = Some(ownership.into());
        self
    }

    /// Set additional metadata as JSON (optional)
    pub fn metadata_json(mut self, metadata_json: impl Into<String>) -> Self {
        self.metadata_json = Some(metadata_json.into());
        self
    }

    /// Set the dataset category (optional): codebase, metrics, synthetic, upload, patches, general, other
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the repository slug for filtering by source repo (e.g., "org/repo-name")
    pub fn repo_slug(mut self, repo_slug: impl Into<String>) -> Self {
        self.repo_slug = Some(repo_slug.into());
        self
    }

    /// Build and validate the dataset creation parameters
    pub fn build(self) -> Result<CreateDatasetParams> {
        // Validate required fields
        let name = self
            .name
            .ok_or_else(|| AosError::validation("name is required"))?;
        if name.trim().is_empty() {
            return Err(AosError::validation("name cannot be empty"));
        }

        let format = self
            .format
            .ok_or_else(|| AosError::validation("format is required"))?;
        validate_format(&format)?;

        let hash_b3 = self
            .hash_b3
            .ok_or_else(|| AosError::validation("hash_b3 is required"))?;
        validate_hash_b3(&hash_b3)?;

        let storage_path = self
            .storage_path
            .ok_or_else(|| AosError::validation("storage_path is required"))?;
        if storage_path.trim().is_empty() {
            return Err(AosError::validation("storage_path cannot be empty"));
        }

        // Validate optional fields if provided
        let status = self.status.unwrap_or_else(|| "uploaded".to_string());
        validate_status(&status)?;

        // Validate dataset_hash_b3 if provided
        if let Some(ref dh) = self.dataset_hash_b3 {
            validate_hash_b3(dh)?;
        }

        // Validate category if provided
        if let Some(ref cat) = self.category {
            validate_category(cat)?;
        }

        Ok(CreateDatasetParams {
            id: self.id,
            name,
            description: self.description,
            format,
            hash_b3,
            dataset_hash_b3: self.dataset_hash_b3,
            storage_path,
            status,
            created_by: self.created_by,
            tenant_id: self.tenant_id,
            workspace_id: self.workspace_id,
            dataset_type: self.dataset_type,
            purpose: self.purpose,
            source_location: self.source_location,
            collection_method: self.collection_method,
            ownership: self.ownership,
            metadata_json: self.metadata_json,
            category: self.category,
            repo_slug: self.repo_slug,
        })
    }
}

// ============================================================================
// Dataset Snapshot Types for Training Run Integrity
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

// ============================================================================
// Core Types
// ============================================================================

/// Derive aggregate safety status from individual signals.
fn derive_overall_safety_status(
    pii_status: &str,
    toxicity_status: &str,
    leak_status: &str,
    anomaly_status: &str,
) -> String {
    let signals = [pii_status, toxicity_status, leak_status, anomaly_status];
    if signals
        .iter()
        .any(|s| s.eq_ignore_ascii_case("block") || s.eq_ignore_ascii_case("unsafe"))
    {
        "block".to_string()
    } else if signals.iter().any(|s| s.eq_ignore_ascii_case("warn")) {
        "warn".to_string()
    } else if signals.iter().all(|s| s.eq_ignore_ascii_case("unknown")) {
        "unknown".to_string()
    } else {
        "clean".to_string()
    }
}

/// Derive trust_state for a dataset version.
///
/// Canonical semantics:
/// - `allowed`: validation passed and no safety warnings.
/// - `allowed_with_warning`: validation passed but at least one safety signal warned.
/// - `needs_approval`: validation is pending/validating or any safety signal is unresolved.
/// - `blocked`: validation failed/invalid or any safety signal blocked.
/// - `unknown`: trust not evaluated (explicit `validation_status == unknown`).
///
/// Training gates block `blocked`, `needs_approval`, and `unknown`. Adapter trust
/// aggregates per-dataset trust using `map_dataset_trust_to_adapter_trust` in
/// `adapter_repositories.rs` (priority: blocked > warn > unknown > allowed).
fn derive_trust_state(
    validation_status: &str,
    pii_status: &str,
    toxicity_status: &str,
    leak_status: &str,
    anomaly_status: &str,
    override_state: Option<&str>,
) -> String {
    if let Some(ov) = override_state {
        return ov.trim().to_ascii_lowercase();
    }

    let validation_lower = validation_status.trim().to_ascii_lowercase();
    if validation_lower == "invalid" || validation_lower == "failed" {
        return "blocked".to_string();
    }

    let safety_block = [pii_status, toxicity_status, leak_status, anomaly_status]
        .iter()
        .any(|s| s.eq_ignore_ascii_case("block") || s.eq_ignore_ascii_case("unsafe"));
    if safety_block {
        return "blocked".to_string();
    }

    let safety_warn = [pii_status, toxicity_status, leak_status, anomaly_status]
        .iter()
        .any(|s| s.eq_ignore_ascii_case("warn"));

    let safety_unknown = [pii_status, toxicity_status, leak_status, anomaly_status]
        .iter()
        .any(|s| s.eq_ignore_ascii_case("unknown"));

    if validation_lower == "unknown" {
        return "unknown".to_string();
    }

    if validation_lower == "pending" || validation_lower == "validating" {
        return "needs_approval".to_string();
    }

    if safety_unknown {
        return "needs_approval".to_string();
    }

    if safety_warn {
        "allowed_with_warning".to_string()
    } else {
        "allowed".to_string()
    }
}

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
    // Repository slug for filtering datasets by source repo (e.g., "org/repo-name")
    pub repo_slug: Option<String>,
    // Hash repair tracking (added in migration 0239)
    pub hash_needs_recompute: i32,
    pub hash_algorithm_version: i32,
}

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

/// Dataset-to-adapter link for tracking training lineage
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DatasetAdapterLink {
    pub id: String,
    pub dataset_id: String,
    pub adapter_id: String,
    pub link_type: String,
    pub created_at: String,
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

impl Db {
    /// Create a new training dataset
    pub async fn create_training_dataset(
        &self,
        name: &str,
        description: Option<&str>,
        format: &str,
        hash_b3: &str,
        storage_path: &str,
        created_by: Option<&str>,
        workspace_id: Option<&str>,
        status: Option<&str>,
        dataset_hash_b3: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let final_status = status.unwrap_or("uploaded");
        let final_dataset_hash = dataset_hash_b3.unwrap_or(hash_b3);
        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, dataset_hash_b3, storage_path,
                status, validation_status, created_by, workspace_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(format)
        .bind(hash_b3)
        .bind(final_dataset_hash)
        .bind(storage_path)
        .bind(final_status)
        .bind(created_by)
        .bind(workspace_id)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset"))?;
        Ok(id)
    }

    /// Create a new training dataset using a precomputed ID (e.g., to align DB rows with storage paths)
    ///
    /// Note: Consider using `create_training_dataset_from_params` for validated creation
    /// with all extended fields.
    pub async fn create_training_dataset_with_id(
        &self,
        dataset_id: &str,
        name: &str,
        description: Option<&str>,
        format: &str,
        hash_b3: &str,
        storage_path: &str,
        created_by: Option<&str>,
        workspace_id: Option<&str>,
        status: Option<&str>,
        dataset_hash_b3: Option<&str>,
    ) -> Result<String> {
        let final_status = status.unwrap_or("uploaded");
        let final_dataset_hash = dataset_hash_b3.unwrap_or(hash_b3);
        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, dataset_hash_b3, storage_path,
                status, validation_status, created_by, workspace_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
        )
        .bind(dataset_id)
        .bind(name)
        .bind(description)
        .bind(format)
        .bind(hash_b3)
        .bind(final_dataset_hash)
        .bind(storage_path)
        .bind(final_status)
        .bind(created_by)
        .bind(workspace_id)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset"))?;
        Ok(dataset_id.to_string())
    }

    /// Create a training dataset from validated parameters
    ///
    /// This is the preferred method for dataset creation as it:
    /// - Validates all inputs before insertion
    /// - Supports all extended fields including tenant_id
    /// - Uses the builder pattern for clear, fluent API
    ///
    /// # Example
    ///
    /// ```ignore
    /// let params = CreateDatasetParams::builder()
    ///     .name("my-dataset")
    ///     .format("jsonl")
    ///     .hash_b3("a".repeat(64)) // 64 hex chars
    ///     .storage_path("/data/datasets/my-dataset")
    ///     .tenant_id("tenant-123")
    ///     .build()?;
    ///
    /// let dataset_id = db.create_training_dataset_from_params(&params).await?;
    /// ```
    pub async fn create_training_dataset_from_params(
        &self,
        params: &CreateDatasetParams,
    ) -> Result<String> {
        let id = params
            .id
            .clone()
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        let final_dataset_hash = params.dataset_hash_b3.as_deref().unwrap_or(&params.hash_b3);

        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, dataset_hash_b3, storage_path,
                status, validation_status, created_by, tenant_id, workspace_id,
                dataset_type, purpose, source_location, collection_method, ownership,
                metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.name)
        .bind(&params.description)
        .bind(&params.format)
        .bind(&params.hash_b3)
        .bind(final_dataset_hash)
        .bind(&params.storage_path)
        .bind(&params.status)
        .bind(&params.created_by)
        .bind(&params.tenant_id)
        .bind(&params.workspace_id)
        .bind(&params.dataset_type)
        .bind(&params.purpose)
        .bind(&params.source_location)
        .bind(&params.collection_method)
        .bind(&params.ownership)
        .bind(&params.metadata_json)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset from params"))?;

        Ok(id)
    }

    /// Create a new dataset version aligned to a dataset record.
    /// Version numbers are monotonically increasing per dataset; start at 1.
    pub async fn create_training_dataset_version(
        &self,
        dataset_id: &str,
        tenant_id: Option<&str>,
        version_label: Option<&str>,
        storage_path: &str,
        hash_b3: &str,
        manifest_path: Option<&str>,
        manifest_json: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<String> {
        let next_version: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(version_number), 0) + 1 FROM training_dataset_versions WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("next dataset version number"))?;

        let version_id = Uuid::now_v7().to_string();

        sqlx::query(
            "INSERT INTO training_dataset_versions (
                id, dataset_id, tenant_id, version_number, version_label, storage_path, hash_b3,
                manifest_path, manifest_json, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&version_id)
        .bind(dataset_id)
        .bind(tenant_id)
        .bind(next_version.0)
        .bind(version_label)
        .bind(storage_path)
        .bind(hash_b3)
        .bind(manifest_path)
        .bind(manifest_json)
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset version"))?;

        Ok(version_id)
    }

    /// Ensure at least one version exists for a dataset; returns the latest version id.
    pub async fn ensure_dataset_version_exists(&self, dataset_id: &str) -> Result<String> {
        if let Some(ver) = self
            .get_latest_dataset_version_for_dataset(dataset_id)
            .await?
        {
            return Ok(ver.id);
        }

        let dataset = self
            .get_training_dataset(dataset_id)
            .await?
            .ok_or_else(|| AosError::Validation("dataset not found".to_string()))?;

        let version_id = self
            .create_training_dataset_version(
                &dataset.id,
                dataset.tenant_id.as_deref(),
                None,
                &dataset.storage_path,
                &dataset.hash_b3,
                None,
                None,
                dataset.created_by.as_deref(),
            )
            .await?;
        Ok(version_id)
    }

    /// Create a dataset version using a caller-provided version ID (aligns storage layout).
    pub async fn create_training_dataset_version_with_id(
        &self,
        version_id: &str,
        dataset_id: &str,
        tenant_id: Option<&str>,
        version_label: Option<&str>,
        storage_path: &str,
        hash_b3: &str,
        manifest_path: Option<&str>,
        manifest_json: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<String> {
        let next_version: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(version_number), 0) + 1 FROM training_dataset_versions WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("next dataset version number"))?;

        sqlx::query(
            "INSERT INTO training_dataset_versions (
                id, dataset_id, tenant_id, version_number, version_label, storage_path, hash_b3,
                manifest_path, manifest_json, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(version_id)
        .bind(dataset_id)
        .bind(tenant_id)
        .bind(next_version.0)
        .bind(version_label)
        .bind(storage_path)
        .bind(hash_b3)
        .bind(manifest_path)
        .bind(manifest_json)
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset version (with id)"))?;

        Ok(version_id.to_string())
    }

    /// Fetch a dataset version by ID.
    pub async fn get_training_dataset_version(
        &self,
        version_id: &str,
    ) -> Result<Option<TrainingDatasetVersion>> {
        let row = sqlx::query_as::<_, TrainingDatasetVersion>(
            "SELECT id, dataset_id, tenant_id, version_number, version_label, storage_path, hash_b3,
                    manifest_path, manifest_json, validation_status, validation_errors_json,
                    pii_status, toxicity_status, leak_status, anomaly_status, overall_safety_status,
                    trust_state, overall_trust_status, sensitivity, created_at, created_by,
                    locked_at, soft_deleted_at
             FROM training_dataset_versions WHERE id = ?",
        )
        .bind(version_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get training dataset version"))?;

        Ok(row)
    }

    /// Fetch manifest JSON for a dataset version (if stored inline).
    pub async fn get_dataset_version_manifest(
        &self,
        dataset_version_id: &str,
    ) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT manifest_json FROM training_dataset_versions WHERE id = ?",
        )
        .bind(dataset_version_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset version manifest"))?;

        Ok(row.map(|tuple| tuple.0))
    }

    /// Fetch a dataset version while enforcing tenant isolation.
    pub async fn get_training_dataset_version_for_tenant(
        &self,
        version_id: &str,
        tenant_id: &str,
    ) -> Result<Option<TrainingDatasetVersion>> {
        let version = self.get_training_dataset_version(version_id).await?;
        if let Some(ver) = version {
            if let Some(ref version_tenant) = ver.tenant_id {
                if version_tenant != tenant_id {
                    return Err(AosError::Authz(
                        "Dataset version belongs to different tenant".into(),
                    ));
                }
            }
            Ok(Some(ver))
        } else {
            Ok(None)
        }
    }

    /// Fetch the latest version for a dataset (by version_number DESC).
    pub async fn get_latest_dataset_version_for_dataset(
        &self,
        dataset_id: &str,
    ) -> Result<Option<TrainingDatasetVersion>> {
        let row = sqlx::query_as::<_, TrainingDatasetVersion>(
            "SELECT id, dataset_id, tenant_id, version_number, version_label, storage_path, hash_b3,
                    manifest_path, manifest_json, validation_status, validation_errors_json,
                    pii_status, toxicity_status, leak_status, anomaly_status, overall_safety_status,
                    trust_state, overall_trust_status, sensitivity, created_at, created_by,
                    locked_at, soft_deleted_at
             FROM training_dataset_versions
             WHERE dataset_id = ?
             ORDER BY version_number DESC
             LIMIT 1",
        )
        .bind(dataset_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get latest dataset version"))?;

        Ok(row)
    }

    /// Compute effective trust_state for a dataset version by applying the most recent override (if any).
    async fn effective_trust_state_for_version(
        &self,
        version: &TrainingDatasetVersion,
    ) -> Result<String> {
        let override_state: Option<(String,)> = sqlx::query_as(
            "SELECT override_state FROM dataset_version_overrides
             WHERE dataset_version_id = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(&version.id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset version override for effective trust"))?;

        Ok(override_state
            .map(|(ov,)| ov)
            .unwrap_or_else(|| version.trust_state.clone()))
    }

    /// Fetch the latest trusted dataset version (allowed/allowed_with_warning). Falls back to latest version if none trusted.
    pub async fn get_latest_trusted_dataset_version_for_dataset(
        &self,
        dataset_id: &str,
    ) -> Result<Option<(TrainingDatasetVersion, String)>> {
        let versions = sqlx::query_as::<_, TrainingDatasetVersion>(
            "SELECT id, dataset_id, tenant_id, version_number, version_label, storage_path, hash_b3,
                    manifest_path, manifest_json, validation_status, validation_errors_json,
                    pii_status, toxicity_status, leak_status, anomaly_status, overall_safety_status,
                    trust_state, overall_trust_status, sensitivity, created_at, created_by,
                    locked_at, soft_deleted_at
             FROM training_dataset_versions
             WHERE dataset_id = ?
               AND soft_deleted_at IS NULL
             ORDER BY version_number DESC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list dataset versions for trust selection"))?;

        for version in versions.iter() {
            let trust_state = self.effective_trust_state_for_version(version).await?;
            let trust_lower = trust_state.to_ascii_lowercase();
            if trust_lower == "allowed" || trust_lower == "allowed_with_warning" {
                return Ok(Some((version.clone(), trust_state)));
            }
        }

        if let Some(version) = versions.into_iter().next() {
            let trust_state = self.effective_trust_state_for_version(&version).await?;
            return Ok(Some((version, trust_state)));
        }

        Ok(None)
    }

    /// List dataset versions for a dataset with effective trust_state applied (ordered DESC by version_number).
    pub async fn list_dataset_versions_for_dataset(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<(TrainingDatasetVersion, String)>> {
        let versions = sqlx::query_as::<_, TrainingDatasetVersion>(
            "SELECT id, dataset_id, tenant_id, version_number, version_label, storage_path, hash_b3,
                    manifest_path, manifest_json, validation_status, validation_errors_json,
                    pii_status, toxicity_status, leak_status, anomaly_status, overall_safety_status,
                    trust_state, overall_trust_status, sensitivity, created_at, created_by,
                    locked_at, soft_deleted_at
             FROM training_dataset_versions
             WHERE dataset_id = ?
               AND soft_deleted_at IS NULL
             ORDER BY version_number DESC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list dataset versions for dataset"))?;

        let mut result = Vec::with_capacity(versions.len());
        for version in versions {
            let trust_state = self.effective_trust_state_for_version(&version).await?;
            result.push((version, trust_state));
        }

        Ok(result)
    }

    /// Get training dataset by ID
    pub async fn get_training_dataset(&self, dataset_id: &str) -> Result<Option<TrainingDataset>> {
        let dataset = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE id = ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(dataset_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get training dataset"))?;
        Ok(dataset)
    }

    /// Find a dataset by hash within a workspace (helps deduplicate uploads).
    pub async fn get_dataset_by_hash_and_workspace(
        &self,
        dataset_hash_b3: &str,
        workspace_id: &str,
    ) -> Result<Option<TrainingDataset>> {
        let dataset = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE dataset_hash_b3 = ? AND workspace_id = ? LIMIT 1",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(dataset_hash_b3)
        .bind(workspace_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset by hash+workspace"))?;
        Ok(dataset)
    }

    /// List all training datasets (DEPRECATED - use list_training_datasets_for_tenant instead)
    ///
    /// WARNING: This method returns ALL datasets across ALL tenants without filtering.
    /// This breaks multi-tenant isolation and should only be used in very specific cases
    /// like system administration or migration scripts where cross-tenant access is required.
    ///
    /// For normal operations, use `list_training_datasets_for_tenant()` which enforces tenant isolation.
    #[deprecated(
        since = "0.3.0",
        note = "Use list_training_datasets_for_tenant() for tenant isolation"
    )]
    pub async fn list_training_datasets(&self, limit: i64) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list training datasets"))?;
        Ok(datasets)
    }

    /// List training datasets for a specific tenant
    ///
    /// Filters datasets by tenant_id, returning only datasets belonging to the specified tenant.
    /// This is used for tenant isolation in multi-tenant deployments.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    /// * `limit` - Maximum number of datasets to return
    ///
    /// # Returns
    /// Vector of training datasets belonging to the tenant, ordered by creation date (newest first)
    pub async fn list_training_datasets_for_tenant(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE tenant_id = ? ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list training datasets for tenant: {}",
                e
            ))
        })?;
        Ok(datasets)
    }

    /// List datasets scoped to a workspace and tenant.
    pub async fn list_training_datasets_for_workspace(
        &self,
        tenant_id: &str,
        workspace_id: &str,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE tenant_id = ? AND workspace_id = ? \
             ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(tenant_id)
        .bind(workspace_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list training datasets for workspace: {}",
                e
            ))
        })?;
        Ok(datasets)
    }

    /// List ALL training datasets across ALL tenants for system-level operations.
    ///
    /// This method is explicitly designed for system-level operations that require
    /// cross-tenant visibility, such as:
    /// - Storage cleanup and orphaned file detection
    /// - System-wide storage quota monitoring
    /// - Dataset archival jobs
    /// - Administrative reporting
    ///
    /// For normal tenant-scoped operations, use `list_training_datasets_for_tenant()` instead.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of datasets to return
    ///
    /// # Returns
    /// Vector of all training datasets ordered by creation date (newest first)
    pub async fn list_all_training_datasets_system(
        &self,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list all training datasets (system)"))?;
        Ok(datasets)
    }

    /// Delete training dataset
    pub async fn delete_training_dataset(&self, dataset_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM training_datasets WHERE id = ?")
            .bind(dataset_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete training dataset"))?;
        Ok(())
    }

    /// Check if a dataset can be safely deleted.
    /// Returns error if dataset is in use by adapters or active training jobs.
    /// This is a guard to prevent accidental deletion of datasets that are still being used.
    pub async fn validate_dataset_deletion(&self, dataset_id: &str) -> Result<()> {
        // Check adapter links via dataset_adapter_links table
        let usage_count = self.count_dataset_usage(dataset_id).await?;
        if usage_count > 0 {
            return Err(AosError::Validation(format!(
                "Cannot delete dataset: {} adapter(s) are using it. Unlink adapters first.",
                usage_count
            )));
        }

        // Check active training jobs that reference this dataset
        let active_jobs: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM repository_training_jobs
             WHERE dataset_id = ? AND status IN ('pending', 'running', 'queued')",
        )
        .bind(dataset_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("check active training jobs for dataset"))?;

        if active_jobs.0 > 0 {
            return Err(AosError::Validation(format!(
                "Cannot delete dataset: {} active training job(s). Wait for completion.",
                active_jobs.0
            )));
        }

        Ok(())
    }

    /// Add file to dataset
    pub async fn add_dataset_file(
        &self,
        dataset_id: &str,
        file_name: &str,
        file_path: &str,
        size_bytes: i64,
        hash_b3: &str,
        mime_type: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO dataset_files (id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(dataset_id)
        .bind(file_name)
        .bind(file_path)
        .bind(size_bytes)
        .bind(hash_b3)
        .bind(mime_type)
        .execute(self.pool())
        .await
        .map_err(db_err("add dataset file"))?;

        // Update dataset file count and size
        sqlx::query(
            "UPDATE training_datasets
             SET file_count = file_count + 1,
                 total_size_bytes = total_size_bytes + ?,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(size_bytes)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset file count"))?;

        Ok(id)
    }

    /// Update dataset lifecycle status (uploaded|processing|ready|failed)
    pub async fn update_dataset_status(&self, dataset_id: &str, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE training_datasets SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset status"))?;
        Ok(())
    }

    /// Get files in dataset
    pub async fn get_dataset_files(&self, dataset_id: &str) -> Result<Vec<DatasetFile>> {
        let files = sqlx::query_as::<_, DatasetFile>(
            "SELECT id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type, created_at
             FROM dataset_files
             WHERE dataset_id = ?
             ORDER BY created_at ASC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get dataset files"))?;
        Ok(files)
    }

    /// Sum total dataset bytes for a tenant across all datasets.
    pub async fn sum_dataset_sizes_for_tenant(&self, tenant_id: &str) -> Result<i64> {
        let total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(total_size_bytes), 0) as total FROM training_datasets WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0);
        Ok(total)
    }

    /// Count dataset versions for a tenant.
    pub async fn count_dataset_versions_for_tenant(&self, tenant_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) as cnt FROM training_dataset_versions WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0);
        Ok(count)
    }

    /// List all dataset versions (used by reconciler).
    pub async fn list_all_dataset_versions(&self) -> Result<Vec<TrainingDatasetVersion>> {
        let versions = sqlx::query_as::<_, TrainingDatasetVersion>(
            "SELECT id, dataset_id, tenant_id, version_number, version_label, storage_path,
                    hash_b3, manifest_path, manifest_json, validation_status,
                    validation_errors_json, pii_status, toxicity_status, leak_status,
                    anomaly_status, overall_safety_status, trust_state, overall_trust_status,
                    sensitivity, created_at, created_by, locked_at, soft_deleted_at
             FROM training_dataset_versions",
        )
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list dataset versions"))?;
        Ok(versions)
    }

    /// List all dataset files (used by reconciler for orphan detection).
    pub async fn list_all_dataset_files(&self) -> Result<Vec<DatasetFile>> {
        let files = sqlx::query_as::<_, DatasetFile>(
            "SELECT id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type, created_at,
                    updated_at, upload_completed_at, compression_format, original_size_bytes,
                    row_count, encoding, line_ending, metadata_json, validation_status,
                    validation_errors_json, source_type, created_by
             FROM dataset_files",
        )
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list dataset files"))?;
        Ok(files)
    }

    /// Update dataset validation status
    pub async fn update_dataset_validation(
        &self,
        dataset_id: &str,
        status: &str,
        errors: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE training_datasets
             SET validation_status = ?, validation_errors = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(status)
        .bind(errors)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset validation"))?;
        Ok(())
    }

    /// Update structural validation status for a dataset version and recompute trust.
    pub async fn update_dataset_version_structural_validation(
        &self,
        dataset_version_id: &str,
        status: &str,
        errors_json: Option<&str>,
    ) -> Result<String> {
        let prev_effective = self.get_effective_trust_state(dataset_version_id).await?;

        let current = self
            .get_training_dataset_version(dataset_version_id)
            .await?
            .ok_or_else(|| AosError::Validation("dataset version not found".to_string()))?;

        let trust_state = derive_trust_state(
            status,
            &current.pii_status,
            &current.toxicity_status,
            &current.leak_status,
            &current.anomaly_status,
            None,
        );
        let overall_safety = derive_overall_safety_status(
            &current.pii_status,
            &current.toxicity_status,
            &current.leak_status,
            &current.anomaly_status,
        );

        sqlx::query(
            "UPDATE training_dataset_versions
             SET validation_status = ?, validation_errors_json = ?, overall_safety_status = ?,
                 trust_state = ?, overall_trust_status = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(errors_json)
        .bind(&overall_safety)
        .bind(&trust_state)
        .bind(&trust_state)
        .bind(dataset_version_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset version validation"))?;

        if let Some(new_effective) = self.get_effective_trust_state(dataset_version_id).await? {
            self.propagate_dataset_trust_change(
                dataset_version_id,
                prev_effective.as_deref(),
                &new_effective,
            )
            .await?;
        }

        Ok(trust_state)
    }

    /// Update semantic/safety statuses and recompute trust for a dataset version.
    pub async fn update_dataset_version_safety_status(
        &self,
        dataset_version_id: &str,
        pii_status: Option<&str>,
        toxicity_status: Option<&str>,
        leak_status: Option<&str>,
        anomaly_status: Option<&str>,
    ) -> Result<String> {
        let prev_effective = self.get_effective_trust_state(dataset_version_id).await?;

        let mut current = self
            .get_training_dataset_version(dataset_version_id)
            .await?
            .ok_or_else(|| AosError::Validation("dataset version not found".to_string()))?;

        if let Some(p) = pii_status {
            current.pii_status = p.to_string();
        }
        if let Some(t) = toxicity_status {
            current.toxicity_status = t.to_string();
        }
        if let Some(l) = leak_status {
            current.leak_status = l.to_string();
        }
        if let Some(a) = anomaly_status {
            current.anomaly_status = a.to_string();
        }

        let trust_state = derive_trust_state(
            &current.validation_status,
            &current.pii_status,
            &current.toxicity_status,
            &current.leak_status,
            &current.anomaly_status,
            None,
        );
        let overall_safety = derive_overall_safety_status(
            &current.pii_status,
            &current.toxicity_status,
            &current.leak_status,
            &current.anomaly_status,
        );

        sqlx::query(
            "UPDATE training_dataset_versions
             SET pii_status = ?, toxicity_status = ?, leak_status = ?, anomaly_status = ?,
                 overall_safety_status = ?, trust_state = ?, overall_trust_status = ?
             WHERE id = ?",
        )
        .bind(&current.pii_status)
        .bind(&current.toxicity_status)
        .bind(&current.leak_status)
        .bind(&current.anomaly_status)
        .bind(&overall_safety)
        .bind(&trust_state)
        .bind(&trust_state)
        .bind(dataset_version_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset version safety status"))?;

        if let Some(new_effective) = self.get_effective_trust_state(dataset_version_id).await? {
            self.propagate_dataset_trust_change(
                dataset_version_id,
                prev_effective.as_deref(),
                &new_effective,
            )
            .await?;
        }

        Ok(trust_state)
    }

    /// Record a validation run for observability/audit.
    pub async fn record_dataset_version_validation_run(
        &self,
        dataset_version_id: &str,
        tier: &str,
        status: &str,
        signal: Option<&str>,
        validation_errors_json: Option<&str>,
        sample_row_ids_json: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO dataset_version_validations (
                id, dataset_version_id, tier, status, signal, validation_errors_json,
                sample_row_ids_json, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(dataset_version_id)
        .bind(tier)
        .bind(status)
        .bind(signal)
        .bind(validation_errors_json)
        .bind(sample_row_ids_json)
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("record dataset version validation run"))?;
        Ok(id)
    }

    /// Record an admin override for trust_state.
    pub async fn create_dataset_version_override(
        &self,
        dataset_version_id: &str,
        override_state: &str,
        reason: Option<&str>,
        created_by: &str,
    ) -> Result<String> {
        let prev_effective = self.get_effective_trust_state(dataset_version_id).await?;

        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO dataset_version_overrides (
                id, dataset_version_id, override_state, reason, created_by
            ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(dataset_version_id)
        .bind(override_state)
        .bind(reason)
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("create dataset version override"))?;

        if let Some(new_effective) = self.get_effective_trust_state(dataset_version_id).await? {
            self.propagate_dataset_trust_change(
                dataset_version_id,
                prev_effective.as_deref(),
                &new_effective,
            )
            .await?;
        }

        Ok(id)
    }

    /// Compute effective trust_state by layering overrides over derived trust_state.
    pub async fn get_effective_trust_state(
        &self,
        dataset_version_id: &str,
    ) -> Result<Option<String>> {
        let version = self
            .get_training_dataset_version(dataset_version_id)
            .await?;

        if version.is_none() {
            return Ok(None);
        }

        let override_state: Option<(String,)> = sqlx::query_as(
            "SELECT override_state FROM dataset_version_overrides
             WHERE dataset_version_id = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(dataset_version_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset version override"))?;

        let effective = if let Some((ov,)) = override_state {
            ov
        } else {
            version.unwrap().trust_state
        };

        Ok(Some(effective))
    }

    /// Compute effective trust_state using an existing transaction.
    ///
    /// This variant avoids acquiring a new pool connection, which is critical
    /// when called within an outer transaction to prevent pool exhaustion.
    pub async fn get_effective_trust_state_with_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        dataset_version_id: &str,
    ) -> Result<Option<String>> {
        // Inline version lookup to avoid needing another executor variant
        let version: Option<TrainingDatasetVersion> = sqlx::query_as(
            "SELECT id, dataset_id, tenant_id, version_number, version_label, storage_path, hash_b3,
                    manifest_path, manifest_json, validation_status, validation_errors_json,
                    pii_status, toxicity_status, leak_status, anomaly_status, overall_safety_status,
                    trust_state, overall_trust_status, sensitivity, created_at, created_by,
                    locked_at, soft_deleted_at
             FROM training_dataset_versions WHERE id = ?",
        )
        .bind(dataset_version_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err("get training dataset version"))?;

        if version.is_none() {
            return Ok(None);
        }

        let override_state: Option<(String,)> = sqlx::query_as(
            "SELECT override_state FROM dataset_version_overrides
             WHERE dataset_version_id = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(dataset_version_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err("get dataset version override"))?;

        let effective = if let Some((ov,)) = override_state {
            ov
        } else {
            version.unwrap().trust_state
        };

        Ok(Some(effective))
    }

    /// Update dataset storage path
    pub async fn update_dataset_storage_path(
        &self,
        dataset_id: &str,
        storage_path: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE training_datasets
             SET storage_path = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(storage_path)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset storage path"))?;
        Ok(())
    }

    /// Store dataset statistics
    pub async fn store_dataset_statistics(
        &self,
        dataset_id: &str,
        num_examples: i32,
        avg_input_length: f64,
        avg_target_length: f64,
        language_distribution: Option<&str>,
        file_type_distribution: Option<&str>,
        total_tokens: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO dataset_statistics (
                dataset_id, num_examples, avg_input_length, avg_target_length,
                language_distribution, file_type_distribution, total_tokens, computed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(dataset_id)
        .bind(num_examples)
        .bind(avg_input_length)
        .bind(avg_target_length)
        .bind(language_distribution)
        .bind(file_type_distribution)
        .bind(total_tokens)
        .execute(self.pool())
        .await
        .map_err(db_err("store dataset statistics"))?;
        Ok(())
    }

    /// Get dataset statistics
    pub async fn get_dataset_statistics(
        &self,
        dataset_id: &str,
    ) -> Result<Option<DatasetStatistics>> {
        let stats = sqlx::query_as::<_, DatasetStatistics>(
            "SELECT dataset_id, num_examples, avg_input_length, avg_target_length,
                    language_distribution, file_type_distribution, total_tokens, computed_at
             FROM dataset_statistics
             WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset statistics"))?;
        Ok(stats)
    }

    // ============================================================================
    // Evidence Entries Operations
    // ============================================================================

    /// Create evidence entry for dataset or adapter
    pub async fn create_evidence_entry(
        &self,
        dataset_id: Option<&str>,
        adapter_id: Option<&str>,
        evidence_type: &str,
        reference: &str,
        description: Option<&str>,
        confidence: &str,
        created_by: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO evidence_entries (
                id, dataset_id, adapter_id, evidence_type, reference,
                description, confidence, created_by, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(dataset_id)
        .bind(adapter_id)
        .bind(evidence_type)
        .bind(reference)
        .bind(description)
        .bind(confidence)
        .bind(created_by)
        .bind(metadata_json)
        .execute(self.pool())
        .await
        .map_err(db_err("create evidence entry"))?;
        Ok(id)
    }

    /// Create evidence entry with params struct
    pub async fn create_evidence_entry_with_params(
        &self,
        params: &CreateEvidenceParams,
    ) -> Result<String> {
        self.create_evidence_entry(
            params.dataset_id.as_deref(),
            params.adapter_id.as_deref(),
            &params.evidence_type,
            &params.reference,
            params.description.as_deref(),
            &params.confidence,
            params.created_by.as_deref(),
            params.metadata_json.as_deref(),
        )
        .await
    }

    /// List evidence entries with optional filters
    pub async fn list_evidence_entries(
        &self,
        filter: &EvidenceFilter,
    ) -> Result<Vec<EvidenceEntry>> {
        let mut query = String::from(
            "SELECT id, dataset_id, adapter_id, evidence_type, reference,
                    description, confidence, created_by, created_at, metadata_json
             FROM evidence_entries WHERE 1=1",
        );
        let mut bindings = Vec::new();

        if let Some(ref dataset_id) = filter.dataset_id {
            query.push_str(" AND dataset_id = ?");
            bindings.push(dataset_id.clone());
        }
        if let Some(ref adapter_id) = filter.adapter_id {
            query.push_str(" AND adapter_id = ?");
            bindings.push(adapter_id.clone());
        }
        if let Some(ref evidence_type) = filter.evidence_type {
            query.push_str(" AND evidence_type = ?");
            bindings.push(evidence_type.clone());
        }
        if let Some(ref confidence) = filter.confidence {
            query.push_str(" AND confidence = ?");
            bindings.push(confidence.clone());
        }

        query.push_str(" ORDER BY created_at DESC");

        let limit = filter.limit.unwrap_or(100).min(500);
        query.push_str(&format!(" LIMIT {}", limit));

        let mut sqlx_query = sqlx::query_as::<_, EvidenceEntry>(&query);
        for binding in bindings {
            sqlx_query = sqlx_query.bind(binding);
        }

        let entries = sqlx_query
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list evidence entries"))?;
        Ok(entries)
    }

    /// Get single evidence entry by ID
    pub async fn get_evidence_entry(&self, id: &str) -> Result<Option<EvidenceEntry>> {
        let entry = sqlx::query_as::<_, EvidenceEntry>(
            "SELECT id, dataset_id, adapter_id, evidence_type, reference,
                    description, confidence, created_by, created_at, metadata_json
             FROM evidence_entries
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get evidence entry"))?;
        Ok(entry)
    }

    /// Get evidence entries for a dataset
    pub async fn get_dataset_evidence(&self, dataset_id: &str) -> Result<Vec<EvidenceEntry>> {
        let entries = sqlx::query_as::<_, EvidenceEntry>(
            "SELECT id, dataset_id, adapter_id, evidence_type, reference,
                    description, confidence, created_by, created_at, metadata_json
             FROM evidence_entries
             WHERE dataset_id = ?
             ORDER BY created_at DESC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get dataset evidence"))?;
        Ok(entries)
    }

    /// Get evidence entries for an adapter
    pub async fn get_adapter_evidence(&self, adapter_id: &str) -> Result<Vec<EvidenceEntry>> {
        let entries = sqlx::query_as::<_, EvidenceEntry>(
            "SELECT id, dataset_id, adapter_id, evidence_type, reference,
                    description, confidence, created_by, created_at, metadata_json
             FROM evidence_entries
             WHERE adapter_id = ?
             ORDER BY created_at DESC",
        )
        .bind(adapter_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get adapter evidence"))?;
        Ok(entries)
    }

    /// Count evidence entries for a dataset
    pub async fn count_dataset_evidence(&self, dataset_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM evidence_entries WHERE dataset_id = ?")
                .bind(dataset_id)
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to count dataset evidence: {}", e))
                })?;
        Ok(count.0)
    }

    /// Count evidence entries for an adapter
    pub async fn count_adapter_evidence(&self, adapter_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM evidence_entries WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to count adapter evidence: {}", e))
                })?;
        Ok(count.0)
    }

    /// Delete evidence entry
    pub async fn delete_evidence_entry(&self, entry_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM evidence_entries WHERE id = ?")
            .bind(entry_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete evidence entry"))?;
        Ok(())
    }

    // ============================================================================
    // Dataset-Adapter Links Operations
    // ============================================================================

    /// Create link between dataset and adapter
    pub async fn create_dataset_adapter_link(
        &self,
        dataset_id: &str,
        adapter_id: &str,
        link_type: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO dataset_adapter_links (id, dataset_id, adapter_id, link_type)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(dataset_id, adapter_id, link_type) DO NOTHING",
        )
        .bind(&id)
        .bind(dataset_id)
        .bind(adapter_id)
        .bind(link_type)
        .execute(self.pool())
        .await
        .map_err(db_err("create dataset-adapter link"))?;
        Ok(id)
    }

    /// Get adapters linked to a dataset
    pub async fn get_dataset_adapters(&self, dataset_id: &str) -> Result<Vec<DatasetAdapterLink>> {
        let links = sqlx::query_as::<_, DatasetAdapterLink>(
            "SELECT id, dataset_id, adapter_id, link_type, created_at
             FROM dataset_adapter_links
             WHERE dataset_id = ?
             ORDER BY created_at DESC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get dataset adapters"))?;
        Ok(links)
    }

    /// Alias for get_dataset_adapters
    pub async fn get_adapters_for_dataset(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<DatasetAdapterLink>> {
        self.get_dataset_adapters(dataset_id).await
    }

    /// Get datasets linked to an adapter
    pub async fn get_adapter_datasets(&self, adapter_id: &str) -> Result<Vec<DatasetAdapterLink>> {
        let links = sqlx::query_as::<_, DatasetAdapterLink>(
            "SELECT id, dataset_id, adapter_id, link_type, created_at
             FROM dataset_adapter_links
             WHERE adapter_id = ?
             ORDER BY created_at DESC",
        )
        .bind(adapter_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get adapter datasets"))?;
        Ok(links)
    }

    /// Alias for get_adapter_datasets
    pub async fn get_datasets_for_adapter(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<DatasetAdapterLink>> {
        self.get_adapter_datasets(adapter_id).await
    }

    /// Count adapters using a dataset
    pub async fn count_dataset_usage(&self, dataset_id: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT adapter_id) FROM dataset_adapter_links WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("count dataset usage"))?;
        Ok(count.0)
    }

    /// Delete dataset-adapter link
    pub async fn delete_dataset_adapter_link(&self, link_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM dataset_adapter_links WHERE id = ?")
            .bind(link_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to delete dataset-adapter link: {}", e))
            })?;
        Ok(())
    }

    /// Update dataset extended fields for enhanced tracking
    pub async fn update_dataset_extended_fields(
        &self,
        dataset_id: &str,
        dataset_type: Option<&str>,
        purpose: Option<&str>,
        source_location: Option<&str>,
        collection_method: Option<&str>,
        ownership: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE training_datasets
             SET dataset_type = COALESCE(?, dataset_type),
                 purpose = COALESCE(?, purpose),
                 source_location = COALESCE(?, source_location),
                 collection_method = COALESCE(?, collection_method),
                 ownership = COALESCE(?, ownership),
                 tenant_id = COALESCE(?, tenant_id),
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(dataset_type)
        .bind(purpose)
        .bind(source_location)
        .bind(collection_method)
        .bind(ownership)
        .bind(tenant_id)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update dataset extended fields: {}", e))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        derive_overall_safety_status, derive_trust_state, validate_format, validate_hash_b3,
        validate_status, CreateDatasetParams, DatasetSnapshotVerification,
        SnapshotDatasetForRunParams, TrainingRunDatasetSnapshot,
    };

    #[test]
    fn trust_blocks_on_invalid() {
        let trust = derive_trust_state("invalid", "clean", "clean", "clean", "clean", None);
        assert_eq!(trust, "blocked");
    }

    #[test]
    fn trust_blocks_on_safety_block() {
        let trust = derive_trust_state("valid", "block", "clean", "clean", "clean", None);
        assert_eq!(trust, "blocked");
    }

    #[test]
    fn trust_warns_on_warn() {
        let trust = derive_trust_state("valid", "warn", "clean", "clean", "clean", None);
        assert_eq!(trust, "allowed_with_warning");
    }

    #[test]
    fn trust_needs_approval_when_pending_validation() {
        let trust = derive_trust_state("pending", "clean", "clean", "clean", "clean", None);
        assert_eq!(trust, "needs_approval");
    }

    #[test]
    fn trust_needs_approval_on_unknown() {
        let trust = derive_trust_state("valid", "unknown", "clean", "clean", "clean", None);
        assert_eq!(trust, "needs_approval");
    }

    #[test]
    fn trust_unknown_when_validation_unknown() {
        let trust = derive_trust_state("unknown", "unknown", "unknown", "unknown", "unknown", None);
        assert_eq!(trust, "unknown");
    }

    #[test]
    fn trust_allows_with_warning_when_warn_present() {
        let trust = derive_trust_state("valid", "clean", "warn", "clean", "clean", None);
        assert_eq!(trust, "allowed_with_warning");
    }

    #[test]
    fn trust_allows_when_clean_and_valid() {
        let trust = derive_trust_state("valid", "clean", "clean", "clean", "clean", None);
        assert_eq!(trust, "allowed");
    }

    #[test]
    fn safety_aggregates_block() {
        let safety = derive_overall_safety_status("clean", "block", "clean", "clean");
        assert_eq!(safety, "block");
    }

    #[test]
    fn safety_warn_when_warn_present() {
        let safety = derive_overall_safety_status("clean", "warn", "clean", "clean");
        assert_eq!(safety, "warn");
    }

    // ============================================================================
    // Validation Function Tests
    // ============================================================================

    #[test]
    fn test_validate_format_valid() {
        assert!(validate_format("jsonl").is_ok());
        assert!(validate_format("patches").is_ok());
        assert!(validate_format("txt").is_ok());
        assert!(validate_format("custom").is_ok());
        assert!(validate_format("parquet").is_ok());
        assert!(validate_format("csv").is_ok());
    }

    #[test]
    fn test_validate_format_invalid() {
        let result = validate_format("xml");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid dataset format"));
    }

    #[test]
    fn test_validate_status_valid() {
        assert!(validate_status("uploaded").is_ok());
        assert!(validate_status("processing").is_ok());
        assert!(validate_status("ready").is_ok());
        assert!(validate_status("failed").is_ok());
    }

    #[test]
    fn test_validate_status_invalid() {
        let result = validate_status("pending");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid dataset status"));
    }

    #[test]
    fn test_validate_hash_b3_valid() {
        // Valid 64 hex character hash
        let hash = "a".repeat(64);
        assert!(validate_hash_b3(&hash).is_ok());

        // Mixed case hex
        let hash2 = "abcdef0123456789ABCDEF0123456789abcdef0123456789ABCDEF0123456789";
        assert!(validate_hash_b3(hash2).is_ok());
    }

    #[test]
    fn test_validate_hash_b3_invalid_length() {
        let result = validate_hash_b3("abc123");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 64 hex characters"));
    }

    #[test]
    fn test_validate_hash_b3_invalid_chars() {
        // Non-hex characters
        let hash = "g".repeat(64);
        let result = validate_hash_b3(&hash);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("hexadecimal"));
    }

    // ============================================================================
    // Builder Tests
    // ============================================================================

    #[test]
    fn test_builder_success() {
        let hash = "a".repeat(64);
        let result = CreateDatasetParams::builder()
            .name("test-dataset")
            .format("jsonl")
            .hash_b3(&hash)
            .storage_path("/data/test")
            .build();

        assert!(result.is_ok());
        let params = result.unwrap();
        assert_eq!(params.name, "test-dataset");
        assert_eq!(params.format, "jsonl");
        assert_eq!(params.hash_b3, hash);
        assert_eq!(params.storage_path, "/data/test");
        assert_eq!(params.status, "uploaded"); // default
    }

    #[test]
    fn test_builder_missing_name() {
        let hash = "a".repeat(64);
        let result = CreateDatasetParams::builder()
            .format("jsonl")
            .hash_b3(&hash)
            .storage_path("/data/test")
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name is required"));
    }

    #[test]
    fn test_builder_empty_name() {
        let hash = "a".repeat(64);
        let result = CreateDatasetParams::builder()
            .name("  ")
            .format("jsonl")
            .hash_b3(&hash)
            .storage_path("/data/test")
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("name cannot be empty"));
    }

    #[test]
    fn test_builder_invalid_format() {
        let hash = "a".repeat(64);
        let result = CreateDatasetParams::builder()
            .name("test")
            .format("xml")
            .hash_b3(&hash)
            .storage_path("/data/test")
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid dataset format"));
    }

    #[test]
    fn test_builder_invalid_status() {
        let hash = "a".repeat(64);
        let result = CreateDatasetParams::builder()
            .name("test")
            .format("jsonl")
            .hash_b3(&hash)
            .storage_path("/data/test")
            .status("invalid_status")
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid dataset status"));
    }

    #[test]
    fn test_builder_with_all_fields() {
        let hash = "a".repeat(64);
        let result = CreateDatasetParams::builder()
            .id("custom-id")
            .name("test-dataset")
            .description("Test description")
            .format("jsonl")
            .hash_b3(&hash)
            .dataset_hash_b3(&hash)
            .storage_path("/data/test")
            .status("processing")
            .created_by("user-123")
            .tenant_id("tenant-456")
            .workspace_id("workspace-789")
            .dataset_type("training")
            .purpose("fine-tuning")
            .source_location("s3://bucket/path")
            .collection_method("automated")
            .ownership("team-ml")
            .metadata_json(r#"{"key": "value"}"#)
            .build();

        assert!(result.is_ok());
        let params = result.unwrap();
        assert_eq!(params.id, Some("custom-id".to_string()));
        assert_eq!(params.description, Some("Test description".to_string()));
        assert_eq!(params.status, "processing");
        assert_eq!(params.tenant_id, Some("tenant-456".to_string()));
    }

    // ============================================================================
    // Snapshot Types Tests
    // ============================================================================

    #[test]
    fn snapshot_params_defaults() {
        let params = SnapshotDatasetForRunParams {
            dataset_id: "test-dataset-id".to_string(),
            tenant_id: None,
            verify_integrity: false,
            require_trusted: false,
        };
        assert_eq!(params.dataset_id, "test-dataset-id");
        assert!(!params.verify_integrity);
        assert!(!params.require_trusted);
    }

    #[test]
    fn snapshot_params_with_tenant() {
        let params = SnapshotDatasetForRunParams {
            dataset_id: "test-dataset-id".to_string(),
            tenant_id: Some("tenant-123".to_string()),
            verify_integrity: true,
            require_trusted: true,
        };
        assert_eq!(params.tenant_id, Some("tenant-123".to_string()));
        assert!(params.verify_integrity);
        assert!(params.require_trusted);
    }

    #[test]
    fn snapshot_struct_serialization() {
        let snapshot = TrainingRunDatasetSnapshot {
            dataset_id: "ds-001".to_string(),
            dataset_version_id: "dsv-001".to_string(),
            version_hash_b3: "abc123".to_string(),
            trust_state_at_snapshot: "allowed".to_string(),
            validation_status_at_snapshot: "valid".to_string(),
            snapshot_timestamp: "2025-01-01T00:00:00Z".to_string(),
            storage_path: "/data/datasets/ds-001".to_string(),
            version_number: 1,
            manifest_json: Some(r#"{"files": []}"#.to_string()),
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: TrainingRunDatasetSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.dataset_id, snapshot.dataset_id);
        assert_eq!(deserialized.dataset_version_id, snapshot.dataset_version_id);
        assert_eq!(deserialized.version_hash_b3, snapshot.version_hash_b3);
        assert_eq!(
            deserialized.trust_state_at_snapshot,
            snapshot.trust_state_at_snapshot
        );
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

// ==============================================================================
// Workstream 9: Dataset Integrity Pre-Training Check
// ==============================================================================

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

impl Db {
    /// Verify dataset integrity before training
    ///
    /// Workstream 9: Checks that all dataset files match their stored BLAKE3 hashes.
    /// This prevents training on corrupted or tampered data.
    ///
    /// # Arguments
    /// * `dataset_id` - The dataset ID to verify
    ///
    /// # Returns
    /// Result containing integrity verification details
    pub async fn verify_dataset_integrity(
        &self,
        dataset_id: &str,
    ) -> Result<DatasetIntegrityResult> {
        // Get all files for this dataset
        let files = self.get_dataset_files(dataset_id).await?;
        let total_files = files.len();
        let mut verified_files = 0;
        let mut mismatches = Vec::new();

        for file in files {
            // Read file from disk
            let file_contents = match tokio::fs::read(&file.file_path).await {
                Ok(contents) => contents,
                Err(e) => {
                    // File not found or unreadable is a mismatch
                    mismatches.push(DatasetFileMismatch {
                        file_name: file.file_name.clone(),
                        file_path: file.file_path.clone(),
                        expected_hash: file.hash_b3.clone(),
                        actual_hash: format!("ERROR: {}", e),
                    });
                    continue;
                }
            };

            // Compute BLAKE3 hash
            let actual_hash = blake3::hash(&file_contents);
            let actual_hash_hex = actual_hash.to_hex().to_string();

            // Compare with stored hash
            if actual_hash_hex != file.hash_b3 {
                mismatches.push(DatasetFileMismatch {
                    file_name: file.file_name,
                    file_path: file.file_path,
                    expected_hash: file.hash_b3,
                    actual_hash: actual_hash_hex,
                });
            } else {
                verified_files += 1;
            }
        }

        let is_valid = mismatches.is_empty();

        Ok(DatasetIntegrityResult {
            dataset_id: dataset_id.to_string(),
            total_files,
            verified_files,
            mismatches,
            is_valid,
        })
    }
}

// ==============================================================================
// Codebase Dataset Rows (Session-based ingestion)
// ==============================================================================

/// Sample role for training examples
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleRole {
    /// Positive examples teach knowledge
    Positive,
    /// Negative examples teach abstention (what NOT to do)
    Negative,
}

impl SampleRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "positive" => Some(Self::Positive),
            "negative" => Some(Self::Negative),
            _ => None,
        }
    }
}

/// A single row in a codebase dataset, representing one training example.
///
/// Each row contains a prompt/response pair extracted from code symbols
/// during codebase ingestion. Rows are grouped by session_id for atomic
/// operations and progress tracking.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CodebaseDatasetRow {
    pub id: String,
    pub dataset_id: String,
    pub dataset_version_id: Option<String>,
    pub session_id: Option<String>,
    pub prompt: String,
    pub response: String,
    pub weight: f64,
    pub sample_role: String,
    pub symbol_kind: Option<String>,
    pub language: Option<String>,
    pub file_path: Option<String>,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub qualified_name: Option<String>,
    pub commit_sha: Option<String>,
    pub repo_name: Option<String>,
    pub repo_slug: Option<String>,
    pub repo_identifier: Option<String>,
    pub project_name: Option<String>,
    pub has_docstring: i32,
    pub content_hash_b3: String,
    pub metadata_json: Option<String>,
    pub tenant_id: Option<String>,
    pub created_at: String,
}

/// Columns for codebase dataset row SELECT queries
pub const CODEBASE_DATASET_ROW_COLUMNS: &str =
    "id, dataset_id, dataset_version_id, session_id, prompt, response, weight, sample_role, \
     symbol_kind, language, file_path, start_line, end_line, qualified_name, \
     commit_sha, repo_name, repo_slug, repo_identifier, project_name, has_docstring, content_hash_b3, \
     metadata_json, tenant_id, created_at";

/// Parameters for creating a codebase dataset row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCodebaseDatasetRowParams {
    pub dataset_id: String,
    pub dataset_version_id: Option<String>,
    pub session_id: Option<String>,
    pub prompt: String,
    pub response: String,
    pub weight: f64,
    pub sample_role: SampleRole,
    pub symbol_kind: Option<String>,
    pub language: Option<String>,
    pub file_path: Option<String>,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub qualified_name: Option<String>,
    pub commit_sha: Option<String>,
    pub repo_name: Option<String>,
    pub repo_slug: Option<String>,
    pub repo_identifier: Option<String>,
    pub project_name: Option<String>,
    pub has_docstring: bool,
    pub metadata_json: Option<String>,
    pub tenant_id: Option<String>,
}

/// Summary statistics for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub dataset_id: String,
    pub row_count: i64,
    pub positive_count: i64,
    pub negative_count: i64,
    pub earliest_created_at: Option<String>,
    pub latest_created_at: Option<String>,
}

impl Db {
    // ============================================================================
    // Codebase Dataset Row Operations
    // ============================================================================

    /// Insert a single codebase dataset row.
    ///
    /// The content_hash_b3 is computed from prompt, response, and weight to
    /// enable deduplication checks.
    pub async fn insert_codebase_dataset_row(
        &self,
        params: &CreateCodebaseDatasetRowParams,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        // Compute content hash for deduplication
        let hash_input = format!("{}:{}:{}", params.prompt, params.response, params.weight);
        let content_hash = blake3::hash(hash_input.as_bytes());
        let content_hash_b3 = content_hash.to_hex().to_string();

        sqlx::query(
            "INSERT INTO codebase_dataset_rows (
                id, dataset_id, dataset_version_id, session_id, prompt, response, weight,
                sample_role, symbol_kind, language, file_path, start_line, end_line,
                qualified_name, commit_sha, repo_name, repo_slug, repo_identifier, project_name,
                has_docstring, content_hash_b3, metadata_json, tenant_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.dataset_id)
        .bind(&params.dataset_version_id)
        .bind(&params.session_id)
        .bind(&params.prompt)
        .bind(&params.response)
        .bind(params.weight)
        .bind(params.sample_role.as_str())
        .bind(&params.symbol_kind)
        .bind(&params.language)
        .bind(&params.file_path)
        .bind(params.start_line)
        .bind(params.end_line)
        .bind(&params.qualified_name)
        .bind(&params.commit_sha)
        .bind(&params.repo_name)
        .bind(&params.repo_slug)
        .bind(&params.repo_identifier)
        .bind(&params.project_name)
        .bind(if params.has_docstring { 1 } else { 0 })
        .bind(&content_hash_b3)
        .bind(&params.metadata_json)
        .bind(&params.tenant_id)
        .execute(self.pool())
        .await
        .map_err(db_err("insert codebase dataset row"))?;

        Ok(id)
    }

    /// Bulk insert codebase dataset rows for efficiency.
    ///
    /// All rows are inserted with the same session_id for atomic grouping.
    /// Returns the number of rows inserted.
    pub async fn bulk_insert_codebase_dataset_rows(
        &self,
        rows: &[CreateCodebaseDatasetRowParams],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        // Use a transaction for atomic insertion
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(db_err("begin transaction"))?;
        let mut count = 0;

        for params in rows {
            let id = Uuid::now_v7().to_string();

            // Compute content hash for deduplication
            let hash_input = format!("{}:{}:{}", params.prompt, params.response, params.weight);
            let content_hash = blake3::hash(hash_input.as_bytes());
            let content_hash_b3 = content_hash.to_hex().to_string();

            sqlx::query(
                "INSERT INTO codebase_dataset_rows (
                    id, dataset_id, dataset_version_id, session_id, prompt, response, weight,
                    sample_role, symbol_kind, language, file_path, start_line, end_line,
                    qualified_name, commit_sha, repo_name, repo_slug, repo_identifier, project_name,
                    has_docstring, content_hash_b3, metadata_json, tenant_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&params.dataset_id)
            .bind(&params.dataset_version_id)
            .bind(&params.session_id)
            .bind(&params.prompt)
            .bind(&params.response)
            .bind(params.weight)
            .bind(params.sample_role.as_str())
            .bind(&params.symbol_kind)
            .bind(&params.language)
            .bind(&params.file_path)
            .bind(params.start_line)
            .bind(params.end_line)
            .bind(&params.qualified_name)
            .bind(&params.commit_sha)
            .bind(&params.repo_name)
            .bind(&params.repo_slug)
            .bind(&params.repo_identifier)
            .bind(&params.project_name)
            .bind(if params.has_docstring { 1 } else { 0 })
            .bind(&content_hash_b3)
            .bind(&params.metadata_json)
            .bind(&params.tenant_id)
            .execute(&mut *tx)
            .await
            .map_err(db_err("bulk insert codebase dataset row"))?;

            count += 1;
        }

        tx.commit().await.map_err(db_err("commit transaction"))?;
        Ok(count)
    }

    /// Get a codebase dataset row by ID.
    pub async fn get_codebase_dataset_row(
        &self,
        row_id: &str,
    ) -> Result<Option<CodebaseDatasetRow>> {
        let row = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows WHERE id = ?",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(row_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get codebase dataset row"))?;

        Ok(row)
    }

    /// List all rows for a dataset, optionally filtered by session.
    pub async fn list_codebase_dataset_rows(
        &self,
        dataset_id: &str,
        session_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = if let Some(sid) = session_id {
            sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
                "SELECT {} FROM codebase_dataset_rows \
                 WHERE dataset_id = ? AND session_id = ? \
                 ORDER BY created_at ASC LIMIT ? OFFSET ?",
                CODEBASE_DATASET_ROW_COLUMNS
            ))
            .bind(dataset_id)
            .bind(sid)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list codebase dataset rows by session"))?
        } else {
            sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
                "SELECT {} FROM codebase_dataset_rows \
                 WHERE dataset_id = ? \
                 ORDER BY created_at ASC LIMIT ? OFFSET ?",
                CODEBASE_DATASET_ROW_COLUMNS
            ))
            .bind(dataset_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list codebase dataset rows"))?
        };

        Ok(rows)
    }

    /// Get all rows for a specific session.
    pub async fn get_rows_by_session(&self, session_id: &str) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows \
             WHERE session_id = ? \
             ORDER BY created_at ASC",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(session_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get rows by session"))?;

        Ok(rows)
    }

    /// Count rows for a dataset, optionally filtered by session.
    pub async fn count_codebase_dataset_rows(
        &self,
        dataset_id: &str,
        session_id: Option<&str>,
    ) -> Result<i64> {
        let count: (i64,) = if let Some(sid) = session_id {
            sqlx::query_as(
                "SELECT COUNT(*) FROM codebase_dataset_rows \
                 WHERE dataset_id = ? AND session_id = ?",
            )
            .bind(dataset_id)
            .bind(sid)
            .fetch_one(self.pool())
            .await
            .map_err(db_err("count codebase dataset rows by session"))?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM codebase_dataset_rows WHERE dataset_id = ?")
                .bind(dataset_id)
                .fetch_one(self.pool())
                .await
                .map_err(db_err("count codebase dataset rows"))?
        };

        Ok(count.0)
    }

    /// Count rows by sample role for a session.
    pub async fn count_rows_by_role_for_session(&self, session_id: &str) -> Result<(i64, i64)> {
        let positive: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM codebase_dataset_rows \
             WHERE session_id = ? AND sample_role = 'positive'",
        )
        .bind(session_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("count positive rows for session"))?;

        let negative: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM codebase_dataset_rows \
             WHERE session_id = ? AND sample_role = 'negative'",
        )
        .bind(session_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("count negative rows for session"))?;

        Ok((positive.0, negative.0))
    }

    /// Get codebase ingestion session summary statistics.
    pub async fn get_codebase_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let result: Option<(String, i64, i64, i64, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT
                dataset_id,
                COUNT(*) as row_count,
                SUM(CASE WHEN sample_role = 'positive' THEN 1 ELSE 0 END) as positive_count,
                SUM(CASE WHEN sample_role = 'negative' THEN 1 ELSE 0 END) as negative_count,
                MIN(created_at) as earliest_created_at,
                MAX(created_at) as latest_created_at
             FROM codebase_dataset_rows
             WHERE session_id = ?
             GROUP BY dataset_id",
            )
            .bind(session_id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err("get session summary"))?;

        Ok(result.map(
            |(dataset_id, row_count, positive_count, negative_count, earliest, latest)| {
                SessionSummary {
                    session_id: session_id.to_string(),
                    dataset_id,
                    row_count,
                    positive_count,
                    negative_count,
                    earliest_created_at: earliest,
                    latest_created_at: latest,
                }
            },
        ))
    }

    /// List all sessions for a dataset.
    pub async fn list_sessions_for_dataset(&self, dataset_id: &str) -> Result<Vec<SessionSummary>> {
        let rows: Vec<(String, i64, i64, i64, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT
                session_id,
                COUNT(*) as row_count,
                SUM(CASE WHEN sample_role = 'positive' THEN 1 ELSE 0 END) as positive_count,
                SUM(CASE WHEN sample_role = 'negative' THEN 1 ELSE 0 END) as negative_count,
                MIN(created_at) as earliest_created_at,
                MAX(created_at) as latest_created_at
             FROM codebase_dataset_rows
             WHERE dataset_id = ? AND session_id IS NOT NULL
             GROUP BY session_id
             ORDER BY MIN(created_at) DESC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list sessions for dataset"))?;

        Ok(rows
            .into_iter()
            .map(
                |(session_id, row_count, positive_count, negative_count, earliest, latest)| {
                    SessionSummary {
                        session_id,
                        dataset_id: dataset_id.to_string(),
                        row_count,
                        positive_count,
                        negative_count,
                        earliest_created_at: earliest,
                        latest_created_at: latest,
                    }
                },
            )
            .collect())
    }

    /// Delete all rows for a session (atomic rollback).
    ///
    /// This enables undoing a failed or unwanted ingestion run.
    pub async fn delete_session_rows(&self, session_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM codebase_dataset_rows WHERE session_id = ?")
            .bind(session_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete session rows"))?;

        Ok(result.rows_affected())
    }

    /// Delete all rows for a dataset.
    pub async fn delete_all_codebase_dataset_rows(&self, dataset_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM codebase_dataset_rows WHERE dataset_id = ?")
            .bind(dataset_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete all codebase dataset rows"))?;

        Ok(result.rows_affected())
    }

    /// Check for duplicate rows by content hash.
    ///
    /// Returns true if a row with the same content hash already exists
    /// in the dataset (across any session).
    pub async fn check_duplicate_row(
        &self,
        dataset_id: &str,
        content_hash_b3: &str,
    ) -> Result<bool> {
        let exists: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM codebase_dataset_rows \
             WHERE dataset_id = ? AND content_hash_b3 = ?",
        )
        .bind(dataset_id)
        .bind(content_hash_b3)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("check duplicate row"))?;

        Ok(exists.0 > 0)
    }

    /// Get rows by file path for a dataset.
    ///
    /// Useful for finding all training examples from a specific source file.
    pub async fn get_rows_by_file(
        &self,
        dataset_id: &str,
        file_path: &str,
    ) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows \
             WHERE dataset_id = ? AND file_path = ? \
             ORDER BY start_line ASC",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(dataset_id)
        .bind(file_path)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get rows by file"))?;

        Ok(rows)
    }

    /// Get rows by symbol for a dataset.
    ///
    /// Useful for finding the training example for a specific function/struct.
    pub async fn get_rows_by_symbol(
        &self,
        dataset_id: &str,
        qualified_name: &str,
    ) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows \
             WHERE dataset_id = ? AND qualified_name = ? \
             ORDER BY created_at DESC",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(dataset_id)
        .bind(qualified_name)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get rows by symbol"))?;

        Ok(rows)
    }

    /// Get rows by repository identifier.
    ///
    /// Useful for finding all training examples from a specific repository.
    pub async fn get_rows_by_repo_identifier(
        &self,
        repo_identifier: &str,
    ) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows \
             WHERE repo_identifier = ? \
             ORDER BY created_at DESC",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(repo_identifier)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get rows by repo identifier"))?;

        Ok(rows)
    }

    /// Update the dataset_version_id for all rows in a session.
    ///
    /// Used when creating a new dataset version from ingested rows.
    pub async fn update_session_version(
        &self,
        session_id: &str,
        dataset_version_id: &str,
    ) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE codebase_dataset_rows SET dataset_version_id = ? WHERE session_id = ?",
        )
        .bind(dataset_version_id)
        .bind(session_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update session version"))?;

        Ok(result.rows_affected())
    }
}
