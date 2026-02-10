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

// Submodules
mod integrity;
mod scan_roots;
mod snapshot;
mod trust;
mod types;
mod validation;

// Public re-exports
pub use integrity::{DatasetFileMismatch, DatasetIntegrityResult};
pub use scan_roots::{
    CreateDatasetScanRootParams, CreateDatasetScanRootParamsBuilder, DatasetScanRoot,
    DatasetScanRootStats,
};
pub use snapshot::{
    DatasetSnapshotVerification, SnapshotDatasetForRunParams, TrainingRunDatasetSnapshot,
};
pub use types::{
    AdapterSessionMembership, AdapterTrainingLineage, CreateDatasetFileParams,
    CreateDatasetHashInputsParams, CreateEvidenceParams, DatasetAdapterLink,
    DatasetCollectionSession, DatasetFile, DatasetHashInputs, DatasetSessionMembership,
    DatasetStatistics, DatasetVersionOverride, DatasetVersionValidation, EvidenceEntry,
    EvidenceFilter, TrainingDataset, TrainingDatasetVersion,
};
pub use validation::{
    validate_category, validate_format, validate_hash_b3, validate_status, VALID_CATEGORIES,
    VALID_FORMATS, VALID_STATUSES,
};

// Internal imports from submodules
use trust::{derive_overall_safety_status, derive_trust_state};

use crate::constants::{
    DATASET_SCAN_ROOT_COLUMNS, TRAINING_DATASET_COLUMNS, TRAINING_DATASET_ROW_COLUMNS,
};
use crate::new_id;
use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::{
    extract_repo_identifier_from_metadata, normalize_repo_id, normalize_repo_slug,
    sanitize_optional, sanitize_repo_identifier, sanitize_repo_slug,
};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{Sqlite, Transaction};
use std::collections::{HashMap, HashSet};

// Normalization functions imported from adapteros_core (via adapteros-infra-common)

pub(crate) fn normalize_optional_value(value: Option<&str>) -> Option<String> {
    sanitize_optional(value)
}

fn normalize_session_tags(tags: Option<&[String]>) -> Option<String> {
    let tags = tags?;
    let mut normalized: Vec<String> = tags
        .iter()
        .filter_map(|tag| {
            let trimmed = tag.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();
    normalized.sort();
    normalized.dedup();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.join(","))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ScanRootInput {
    path: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    file_count: Option<u64>,
    #[serde(default)]
    byte_count: Option<u64>,
    #[serde(default, alias = "content_hash_b3")]
    content_hash: Option<String>,
    #[serde(default)]
    scanned_at: Option<String>,
}

fn parse_scan_roots_from_metadata_json(metadata_json: Option<&str>) -> Vec<ScanRootInput> {
    let raw = match metadata_json {
        Some(raw) if !raw.trim().is_empty() => raw,
        _ => return Vec::new(),
    };

    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(val) => val,
        Err(_) => return Vec::new(),
    };

    if let Some(scan_roots) = value.get("scan_roots") {
        if let Ok(parsed) = serde_json::from_value::<Vec<ScanRootInput>>(scan_roots.clone()) {
            let roots: Vec<ScanRootInput> = parsed
                .into_iter()
                .filter_map(|mut root| {
                    let trimmed = root.path.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    if trimmed != root.path {
                        root.path = trimmed.to_string();
                    }
                    Some(root)
                })
                .collect();
            if !roots.is_empty() {
                return roots;
            }
        }
    }

    Vec::new()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct RepoScopeInput {
    #[serde(default)]
    include_paths: Vec<String>,
    #[serde(default)]
    exclude_paths: Vec<String>,
    #[serde(default)]
    include_extensions: Vec<String>,
    #[serde(default)]
    exclude_extensions: Vec<String>,
}

impl RepoScopeInput {
    fn has_filters(&self) -> bool {
        !self.include_paths.is_empty()
            || !self.exclude_paths.is_empty()
            || !self.include_extensions.is_empty()
            || !self.exclude_extensions.is_empty()
    }

    fn normalized(&self) -> Self {
        Self {
            include_paths: normalize_scope_list(&self.include_paths),
            exclude_paths: normalize_scope_list(&self.exclude_paths),
            include_extensions: normalize_scope_list(&self.include_extensions),
            exclude_extensions: normalize_scope_list(&self.exclude_extensions),
        }
    }
}

fn normalize_scope_list(values: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn parse_metadata_json(metadata_json: Option<&str>) -> Option<Value> {
    let raw = metadata_json?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

fn parse_repo_scope_from_value(value: &Value) -> Option<RepoScopeInput> {
    if let Some(scope_value) = value.get("repo_scope") {
        return serde_json::from_value(scope_value.clone()).ok();
    }

    value
        .get("repo_scope_json")
        .and_then(|raw| raw.as_str())
        .and_then(|raw| serde_json::from_str::<RepoScopeInput>(raw).ok())
}

fn compute_scope_suffix(metadata_json: Option<&str>) -> Option<String> {
    let metadata = parse_metadata_json(metadata_json)?;
    let repo_scope = parse_repo_scope_from_value(&metadata);
    let scan_roots = parse_scan_roots_from_metadata_json(metadata_json);

    let scan_root_relative = metadata
        .get("scan_root_relative")
        .and_then(|v| v.as_str())
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());

    let repo_root_path = metadata
        .get("repo_root_path")
        .and_then(|v| v.as_str())
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string());

    let mut scan_root_paths: Vec<String> = if !scan_roots.is_empty() {
        scan_roots.into_iter().map(|root| root.path).collect()
    } else {
        metadata
            .get("scan_root_path")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .into_iter()
            .collect()
    };

    scan_root_paths.retain(|p| !p.trim().is_empty());

    let repo_scope_active = repo_scope.as_ref().is_some_and(RepoScopeInput::has_filters);
    let scan_root_scoped = if scan_root_paths.len() > 1 || scan_root_relative.is_some() {
        true
    } else if let (Some(root), Some(scan_root)) =
        (repo_root_path.as_deref(), scan_root_paths.first())
    {
        root != scan_root
    } else {
        false
    };

    if !repo_scope_active && !scan_root_scoped {
        return None;
    }

    let mut hasher = blake3::Hasher::new();
    if let Some(scope) = repo_scope {
        let normalized = scope.normalized();
        if let Ok(scope_json) = serde_json::to_string(&normalized) {
            hasher.update(scope_json.as_bytes());
        }
    }

    if !scan_root_paths.is_empty() {
        scan_root_paths.sort();
        scan_root_paths.dedup();
        let joined = scan_root_paths.join("|");
        hasher.update(joined.as_bytes());
    }

    if let Some(relative) = scan_root_relative {
        hasher.update(relative.as_bytes());
    }

    let hash = hasher.finalize().to_hex().to_string();
    Some(format!("scope-{}", &hash[..8]))
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
    /// Git branch name for branch-specific datasets (e.g., "main", "feature/xyz")
    pub branch: Option<String>,
    /// Git commit SHA at dataset creation time for reproducibility
    pub commit_sha: Option<String>,
    /// Correlation ID for tracing dataset lineage across runs
    pub correlation_id: Option<String>,
    // Session lineage fields (migration 0256)
    /// Session ID that created this dataset (for atomic rollback/grouping)
    pub session_id: Option<String>,
    /// Human-readable session name for display/filtering
    pub session_name: Option<String>,
    /// Comma-separated tags for session categorization
    pub session_tags: Option<String>,
    // Scope metadata fields (migration 0257)
    /// Repository identifier for scoped queries (e.g., "github.com/org/repo")
    pub scope_repo_id: Option<String>,
    /// Repository name (human-readable)
    pub scope_repo: Option<String>,
    /// Primary scan root path used during ingestion
    pub scope_scan_root: Option<String>,
    /// Remote URL of the repository
    pub scope_remote_url: Option<String>,
    // Aggregate metrics (migration 0259) - typically computed, not user-provided
    /// Number of scan roots used
    pub scan_root_count: Option<i64>,
    /// Total files across all scan roots
    pub total_scan_root_files: Option<i64>,
    /// Total bytes across all scan roots
    pub total_scan_root_bytes: Option<i64>,
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
    branch: Option<String>,
    commit_sha: Option<String>,
    correlation_id: Option<String>,
    // Session lineage fields (migration 0256)
    session_id: Option<String>,
    session_name: Option<String>,
    session_tags: Option<String>,
    // Scope metadata fields (migration 0257)
    scope_repo_id: Option<String>,
    scope_repo: Option<String>,
    scope_scan_root: Option<String>,
    scope_remote_url: Option<String>,
    // Aggregate metrics (migration 0259)
    scan_root_count: Option<i64>,
    total_scan_root_files: Option<i64>,
    total_scan_root_bytes: Option<i64>,
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

    /// Set the git branch name for branch-specific datasets (e.g., "main", "feature/xyz")
    pub fn branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    /// Set the git commit SHA at dataset creation time for reproducibility
    pub fn commit_sha(mut self, commit_sha: impl Into<String>) -> Self {
        self.commit_sha = Some(commit_sha.into());
        self
    }

    /// Set a correlation ID for tracing (optional)
    pub fn correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    // Session lineage fields (migration 0256)

    /// Set the session ID that created this dataset (for atomic rollback/grouping)
    pub fn session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the human-readable session name for display/filtering
    pub fn session_name(mut self, session_name: impl Into<String>) -> Self {
        self.session_name = Some(session_name.into());
        self
    }

    /// Set the comma-separated tags for session categorization
    pub fn session_tags(mut self, session_tags: impl Into<String>) -> Self {
        self.session_tags = Some(session_tags.into());
        self
    }

    // Scope metadata fields (migration 0257)

    /// Set the repository identifier for scoped queries (e.g., "github.com/org/repo")
    pub fn scope_repo_id(mut self, scope_repo_id: impl Into<String>) -> Self {
        self.scope_repo_id = Some(scope_repo_id.into());
        self
    }

    /// Set the repository name (human-readable)
    pub fn scope_repo(mut self, scope_repo: impl Into<String>) -> Self {
        self.scope_repo = Some(scope_repo.into());
        self
    }

    /// Set the primary scan root path used during ingestion
    pub fn scope_scan_root(mut self, scope_scan_root: impl Into<String>) -> Self {
        self.scope_scan_root = Some(scope_scan_root.into());
        self
    }

    /// Set the remote URL of the repository
    pub fn scope_remote_url(mut self, scope_remote_url: impl Into<String>) -> Self {
        self.scope_remote_url = Some(scope_remote_url.into());
        self
    }

    // Aggregate metrics (migration 0259)

    /// Set the number of scan roots used
    pub fn scan_root_count(mut self, count: i64) -> Self {
        self.scan_root_count = Some(count);
        self
    }

    /// Set the total files across all scan roots
    pub fn total_scan_root_files(mut self, count: i64) -> Self {
        self.total_scan_root_files = Some(count);
        self
    }

    /// Set the total bytes across all scan roots
    pub fn total_scan_root_bytes(mut self, bytes: i64) -> Self {
        self.total_scan_root_bytes = Some(bytes);
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
            branch: normalize_optional_value(self.branch.as_deref()),
            commit_sha: normalize_optional_value(self.commit_sha.as_deref()),
            correlation_id: normalize_optional_value(self.correlation_id.as_deref()),
            // Session lineage fields (migration 0256)
            session_id: normalize_optional_value(self.session_id.as_deref()),
            session_name: normalize_optional_value(self.session_name.as_deref()),
            session_tags: normalize_optional_value(self.session_tags.as_deref()),
            // Scope metadata fields (migration 0257)
            scope_repo_id: normalize_optional_value(self.scope_repo_id.as_deref()),
            scope_repo: normalize_optional_value(self.scope_repo.as_deref()),
            scope_scan_root: normalize_optional_value(self.scope_scan_root.as_deref()),
            scope_remote_url: normalize_optional_value(self.scope_remote_url.as_deref()),
            // Aggregate metrics (migration 0259)
            scan_root_count: self.scan_root_count,
            total_scan_root_files: self.total_scan_root_files,
            total_scan_root_bytes: self.total_scan_root_bytes,
        })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

// Note: Snapshot types moved to snapshot.rs
// Note: Trust derivation functions moved to trust.rs
// Note: Core types moved to types.rs
fn merge_metadata_with_category(
    metadata_json: Option<&str>,
    category: Option<&str>,
) -> Option<String> {
    match (metadata_json, category) {
        (None, None) => None,
        (Some(raw), None) => Some(raw.to_string()),
        (None, Some(category)) => Some(serde_json::json!({ "category": category }).to_string()),
        (Some(raw), Some(category)) => {
            if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(raw) {
                if let Some(obj) = value.as_object_mut() {
                    obj.entry("category".to_string())
                        .or_insert_with(|| serde_json::Value::String(category.to_string()));
                    return Some(value.to_string());
                }
            }
            Some(raw.to_string())
        }
    }
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
        repo_slug: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Dst);
        let final_status = status.unwrap_or("uploaded");
        let final_dataset_hash = dataset_hash_b3.unwrap_or(hash_b3);
        let repo_slug = sanitize_repo_slug(repo_slug).map(|slug| normalize_repo_slug(&slug));
        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, dataset_hash_b3, storage_path,
                status, validation_status, created_by, workspace_id, repo_slug
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)",
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
        .bind(repo_slug)
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
        repo_slug: Option<&str>,
    ) -> Result<String> {
        let final_status = status.unwrap_or("uploaded");
        let final_dataset_hash = dataset_hash_b3.unwrap_or(hash_b3);
        let repo_slug = sanitize_repo_slug(repo_slug).map(|slug| normalize_repo_slug(&slug));
        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, dataset_hash_b3, storage_path,
                status, validation_status, created_by, workspace_id, repo_slug
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)",
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
        .bind(repo_slug)
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
        let id = params.id.clone().unwrap_or_else(|| new_id(IdPrefix::Dst));
        let final_dataset_hash = params.dataset_hash_b3.as_deref().unwrap_or(&params.hash_b3);
        let metadata_json = merge_metadata_with_category(
            params.metadata_json.as_deref(),
            params.category.as_deref(),
        );
        let repo_slug =
            sanitize_repo_slug(params.repo_slug.as_deref()).map(|slug| normalize_repo_slug(&slug));
        let branch = normalize_optional_value(params.branch.as_deref());
        let commit_sha = normalize_optional_value(params.commit_sha.as_deref());

        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, dataset_hash_b3, storage_path,
                status, validation_status, created_by, tenant_id, workspace_id,
                dataset_type, purpose, source_location, collection_method, ownership,
                metadata_json, repo_slug, branch, commit_sha, correlation_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(&metadata_json)
        .bind(&repo_slug)
        .bind(&branch)
        .bind(&commit_sha)
        .bind(&params.correlation_id)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset from params"))?;

        if let Some(tenant_id) = params.tenant_id.as_deref() {
            self.dual_write_dataset_to_kv(tenant_id, &id).await?;
        }

        Ok(id)
    }

    /// Create a training dataset and its initial version from validated parameters.
    ///
    /// Returns tuple of (dataset_id, version_id) for the newly created records.
    pub async fn create_training_dataset_from_params_with_version(
        &self,
        params: &CreateDatasetParams,
        version_label: Option<&str>,
        version_storage_path: &str,
        version_hash_b3: &str,
        manifest_path: Option<&str>,
        manifest_json: Option<&str>,
    ) -> Result<(String, String)> {
        let dataset_id = self.create_training_dataset_from_params(params).await?;
        let version_id = self
            .create_training_dataset_version(
                &dataset_id,
                params.tenant_id.as_deref(),
                version_label,
                version_storage_path,
                version_hash_b3,
                manifest_path,
                manifest_json,
                params.created_by.as_deref(),
            )
            .await?;
        Ok((dataset_id, version_id))
    }

    /// Create a training dataset record for repository-based ingestion.
    ///
    /// This is a convenience method that wraps `create_training_dataset_from_params`
    /// with repo-specific defaults:
    /// - `collection_method`: "code_ingestion_pipeline" (automated code ingestion)
    /// - `dataset_type`: "codebase"
    /// - `category`: "codebase"
    /// - `format`: "custom" (repo-derived, not standard file format)
    /// - `status`: "processing" (will be updated after ingestion completes)
    ///
    /// Returns tuple of (dataset_id, version_id) for the newly created records.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (dataset_id, version_id) = db.create_dataset_from_repo(
    ///     "adapter-os",
    ///     "https://github.com/org/adapter-os",
    ///     "/path/to/scan-root",
    ///     "abc123def456",
    ///     "main",
    ///     Some("tenant-123"),
    ///     Some("user@example.com"),
    /// ).await?;
    /// ```
    pub async fn create_dataset_from_repo(
        &self,
        repo_name: &str,
        repo_url: &str,
        repo_path: &str,
        commit_sha: &str,
        branch: Option<&str>,
        tenant_id: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<(String, String)> {
        self.create_dataset_from_repo_with_slug(
            repo_name, None, repo_url, repo_path, commit_sha, branch, tenant_id, created_by,
        )
        .await
    }

    /// Create a training dataset record for repository-based ingestion with an explicit repo_slug.
    ///
    /// If `repo_slug` is provided, it is normalized and used for naming and storage paths.
    /// Otherwise, the slug is derived from `repo_name`.
    pub async fn create_dataset_from_repo_with_slug(
        &self,
        repo_name: &str,
        repo_slug: Option<&str>,
        repo_url: &str,
        repo_path: &str,
        commit_sha: &str,
        branch: Option<&str>,
        tenant_id: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<(String, String)> {
        // Generate a dataset hash from repo content identifiers
        let mut hasher = blake3::Hasher::new();
        hasher.update(repo_url.as_bytes());
        hasher.update(commit_sha.as_bytes());
        hasher.update(repo_path.as_bytes());
        let dataset_hash = hasher.finalize().to_hex().to_string();

        let repo_url_opt = if repo_url.trim().is_empty() {
            None
        } else {
            Some(repo_url)
        };

        self.create_codebase_dataset_from_repo_with_hash(
            repo_name,
            repo_url_opt,
            repo_path,
            commit_sha,
            branch,
            repo_slug,
            &dataset_hash,
            None,
            tenant_id,
            created_by,
        )
        .await
    }

    /// Create a codebase dataset record with explicit dataset hash and metadata.
    ///
    /// This variant is used by code ingestion pipelines that already compute
    /// a deterministic dataset hash (e.g., from generated samples).
    pub async fn create_codebase_dataset_from_repo_with_hash(
        &self,
        repo_name: &str,
        repo_url: Option<&str>,
        repo_path: &str,
        commit_sha: &str,
        branch: Option<&str>,
        repo_slug: Option<&str>,
        dataset_hash_b3: &str,
        metadata_json: Option<&str>,
        tenant_id: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<(String, String)> {
        let repo_slug = sanitize_repo_slug(repo_slug)
            .map(|slug| normalize_repo_slug(&slug))
            .unwrap_or_else(|| normalize_repo_slug(repo_name));

        // Storage path for dataset artifacts (scope-specific when scoped)
        let scope_suffix = compute_scope_suffix(metadata_json);
        let storage_path = match scope_suffix.as_deref() {
            Some(suffix) => format!("/datasets/repo/{}/{}/{}", repo_slug, commit_sha, suffix),
            None => format!("/datasets/repo/{}/{}", repo_slug, commit_sha),
        };

        self.create_codebase_dataset_from_repo_with_hashes(
            repo_name,
            repo_url,
            repo_path,
            commit_sha,
            branch,
            Some(&repo_slug),
            dataset_hash_b3,
            dataset_hash_b3,
            &storage_path,
            metadata_json,
            tenant_id,
            created_by,
        )
        .await
    }

    /// Create a codebase dataset record with explicit dataset + file hashes.
    ///
    /// This variant is used when the dataset content hash and stored artifact hash
    /// are computed separately (e.g., canonical dataset bytes stored in storage).
    pub async fn create_codebase_dataset_from_repo_with_hashes(
        &self,
        repo_name: &str,
        repo_url: Option<&str>,
        repo_path: &str,
        commit_sha: &str,
        branch: Option<&str>,
        repo_slug: Option<&str>,
        dataset_hash_b3: &str,
        dataset_file_hash_b3: &str,
        dataset_storage_path: &str,
        metadata_json: Option<&str>,
        tenant_id: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<(String, String)> {
        validate_hash_b3(dataset_hash_b3)?;
        validate_hash_b3(dataset_file_hash_b3)?;

        let repo_slug = sanitize_repo_slug(repo_slug)
            .map(|slug| normalize_repo_slug(&slug))
            .unwrap_or_else(|| normalize_repo_slug(repo_name));

        let short_commit = commit_sha.get(0..8).unwrap_or(commit_sha);
        let scope_suffix = compute_scope_suffix(metadata_json);
        let dataset_name = match scope_suffix.as_deref() {
            Some(suffix) => format!("{}-{}-{}", repo_slug, short_commit, suffix),
            None => format!("{}-{}", repo_slug, short_commit),
        };

        let storage_path = if dataset_storage_path.trim().is_empty() {
            match scope_suffix.as_deref() {
                Some(suffix) => format!("/datasets/repo/{}/{}/{}", repo_slug, commit_sha, suffix),
                None => format!("/datasets/repo/{}/{}", repo_slug, commit_sha),
            }
        } else {
            dataset_storage_path.to_string()
        };

        let source_location = repo_url
            .filter(|url| !url.trim().is_empty())
            .unwrap_or(repo_path);

        let mut params = CreateDatasetParams::builder()
            .name(&dataset_name)
            .format("custom")
            .hash_b3(dataset_file_hash_b3)
            .dataset_hash_b3(dataset_hash_b3)
            .storage_path(&storage_path)
            .dataset_type("codebase")
            .category("codebase")
            .collection_method("code_ingestion_pipeline")
            .purpose(&format!("Repository code ingestion from {}", repo_name))
            .source_location(source_location)
            .status("processing")
            .repo_slug(&repo_slug)
            .commit_sha(commit_sha);

        if let Some(raw) = metadata_json.filter(|raw| !raw.trim().is_empty()) {
            params = params.metadata_json(raw);

            // Extract session and scope metadata from JSON for first-class columns
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {
                // Session lineage fields (migration 0256)
                if let Some(session_id) = parsed.get("session_id").and_then(|v| v.as_str()) {
                    params = params.session_id(session_id);
                }
                if let Some(session_name) = parsed.get("session_name").and_then(|v| v.as_str()) {
                    params = params.session_name(session_name);
                }
                if let Some(session_tags) = parsed.get("session_tags") {
                    if let Some(tags_array) = session_tags.as_array() {
                        let tags: Vec<&str> =
                            tags_array.iter().filter_map(|v| v.as_str()).collect();
                        if !tags.is_empty() {
                            params = params.session_tags(tags.join(","));
                        }
                    } else if let Some(tags_str) = session_tags.as_str() {
                        params = params.session_tags(tags_str);
                    }
                }

                // Scope metadata fields (migration 0257)
                if let Some(repo_scope) = parsed.get("repo_scope") {
                    if let Some(repo_id) =
                        repo_scope.get("repo_identifier").and_then(|v| v.as_str())
                    {
                        params = params.scope_repo_id(repo_id);
                    }
                    if let Some(scan_root) = repo_scope.get("scan_root").and_then(|v| v.as_str()) {
                        params = params.scope_scan_root(scan_root);
                    }
                }
                // Also check top-level scope fields for compatibility
                if let Some(scope_repo_id) = parsed.get("scope_repo_id").and_then(|v| v.as_str()) {
                    params = params.scope_repo_id(scope_repo_id);
                }
                if let Some(scope_scan_root) =
                    parsed.get("scope_scan_root").and_then(|v| v.as_str())
                {
                    params = params.scope_scan_root(scope_scan_root);
                }
            }
        }

        // Set scope_repo and scope_remote_url from function parameters
        params = params.scope_repo(&repo_slug);
        if let Some(url) = repo_url.filter(|u| !u.trim().is_empty()) {
            params = params.scope_remote_url(url);
        }

        let params = if let Some(branch) = branch.filter(|b| !b.trim().is_empty()) {
            params.branch(branch)
        } else {
            params
        };

        let params = if let Some(user) = created_by {
            params.created_by(user)
        } else {
            params
        };

        let params = if let Some(tid) = tenant_id {
            params.tenant_id(tid)
        } else {
            params
        };

        let params = params.build().map_err(|e| {
            adapteros_core::AosError::validation(format!("Failed to build dataset params: {}", e))
        })?;

        let dataset_id = self.create_training_dataset_from_params(&params).await?;

        let version_id = self
            .create_training_dataset_version(
                &dataset_id,
                tenant_id,
                Some(&format!("v1-{}", short_commit)),
                &storage_path,
                dataset_file_hash_b3,
                None,
                None,
                created_by,
            )
            .await?;

        let mut scan_roots = parse_scan_roots_from_metadata_json(metadata_json);
        if scan_roots.is_empty() && !repo_path.trim().is_empty() {
            scan_roots.push(ScanRootInput {
                path: repo_path.to_string(),
                label: Some("primary".to_string()),
                file_count: None,
                byte_count: None,
                content_hash: None,
                scanned_at: None,
            });
        }

        for (ordinal, root) in scan_roots.into_iter().enumerate() {
            let mut scan_root_builder =
                CreateDatasetScanRootParams::builder(&dataset_id, root.path)
                    .dataset_version_id(&version_id)
                    .ordinal(ordinal as i32)
                    .repo_name(repo_name)
                    .repo_slug(&repo_slug)
                    .commit_sha(commit_sha);

            if let Some(label) = root.label {
                scan_root_builder = scan_root_builder.label(label);
            }
            if let Some(file_count) = root.file_count {
                scan_root_builder = scan_root_builder.file_count(file_count);
            }
            if let Some(byte_count) = root.byte_count {
                scan_root_builder = scan_root_builder.byte_count(byte_count);
            }
            if let Some(hash) = root.content_hash {
                scan_root_builder = scan_root_builder.content_hash_b3(hash);
            }
            if let Some(scanned_at) = root.scanned_at {
                scan_root_builder = scan_root_builder.scanned_at(scanned_at);
            }

            if let Some(branch) = branch.filter(|b| !b.trim().is_empty()) {
                scan_root_builder = scan_root_builder.branch(branch);
            }

            if let Some(url) = repo_url.filter(|u| !u.trim().is_empty()) {
                scan_root_builder = scan_root_builder.remote_url(url);
            }

            if let Some(tid) = tenant_id {
                scan_root_builder = scan_root_builder.tenant_id(tid);
            }

            if let Some(user) = created_by {
                scan_root_builder = scan_root_builder.created_by(user);
            }

            let scan_root_params = scan_root_builder.build();
            self.insert_dataset_scan_root(&scan_root_params).await?;
        }

        tracing::info!(
            dataset_id = %dataset_id,
            version_id = %version_id,
            repo_name = %repo_name,
            repo_slug = %repo_slug,
            commit = %short_commit,
            "Created dataset record for repository ingestion"
        );

        Ok((dataset_id, version_id))
    }

    /// Ensure a dataset collection session row exists.
    pub async fn ensure_dataset_collection_session(
        &self,
        session_id: &str,
        session_name: Option<&str>,
        session_tags: Option<&[String]>,
        tenant_id: Option<&str>,
    ) -> Result<()> {
        let provided_name = sanitize_optional(session_name);
        let session_name = provided_name
            .clone()
            .unwrap_or_else(|| session_id.to_string());
        let tags = normalize_session_tags(session_tags);

        sqlx::query(
            "INSERT OR IGNORE INTO dataset_collection_sessions (id, name, tags, tenant_id)
             VALUES (?, ?, ?, ?)",
        )
        .bind(session_id)
        .bind(&session_name)
        .bind(&tags)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(db_err("insert dataset collection session"))?;

        if let Some(name) = provided_name {
            sqlx::query(
                "UPDATE dataset_collection_sessions
                 SET name = ?, updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&name)
            .bind(session_id)
            .execute(self.pool())
            .await
            .map_err(db_err("update dataset collection session name"))?;
        }

        if let Some(tags) = &tags {
            sqlx::query(
                "UPDATE dataset_collection_sessions
                 SET tags = ?, updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(tags)
            .bind(session_id)
            .execute(self.pool())
            .await
            .map_err(db_err("update dataset collection session tags"))?;
        }

        if let Some(tenant_id) = tenant_id {
            sqlx::query(
                "UPDATE dataset_collection_sessions
                 SET tenant_id = COALESCE(NULLIF(tenant_id, ''), ?), updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(tenant_id)
            .bind(session_id)
            .execute(self.pool())
            .await
            .map_err(db_err("update dataset collection session tenant"))?;
        }

        Ok(())
    }

    /// Link a dataset to a collection session and update counts.
    pub async fn link_dataset_to_collection_session(
        &self,
        session_id: &str,
        dataset_id: &str,
        operation_type: Option<&str>,
        ordinal: Option<i32>,
    ) -> Result<u64> {
        let id = new_id(IdPrefix::Dst);
        let operation_type = operation_type.unwrap_or("created");
        let ordinal = ordinal.unwrap_or(0);

        let result = sqlx::query(
            "INSERT OR IGNORE INTO dataset_session_membership (
                id, session_id, dataset_id, operation_type, ordinal
            ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(dataset_id)
        .bind(operation_type)
        .bind(ordinal)
        .execute(self.pool())
        .await
        .map_err(db_err("insert dataset session membership"))?;

        if result.rows_affected() > 0 {
            sqlx::query(
                "UPDATE dataset_collection_sessions
                 SET dataset_count = dataset_count + 1, updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(session_id)
            .execute(self.pool())
            .await
            .map_err(db_err("update dataset session dataset_count"))?;
        }

        Ok(result.rows_affected())
    }

    /// Link an adapter to a collection session and update counts.
    pub async fn link_adapter_to_collection_session(
        &self,
        session_id: &str,
        adapter_id: &str,
        operation_type: Option<&str>,
        ordinal: Option<i32>,
    ) -> Result<u64> {
        let id = new_id(IdPrefix::Dst);
        let operation_type = operation_type.unwrap_or("trained");
        let ordinal = ordinal.unwrap_or(0);

        let result = sqlx::query(
            "INSERT OR IGNORE INTO adapter_session_membership (
                id, session_id, adapter_id, operation_type, ordinal
            ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(adapter_id)
        .bind(operation_type)
        .bind(ordinal)
        .execute(self.pool())
        .await
        .map_err(db_err("insert adapter session membership"))?;

        if result.rows_affected() > 0 {
            sqlx::query(
                "UPDATE dataset_collection_sessions
                 SET adapter_count = adapter_count + 1, updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(session_id)
            .execute(self.pool())
            .await
            .map_err(db_err("update dataset session adapter_count"))?;
        }

        Ok(result.rows_affected())
    }

    /// Create a training dataset record for a branch run with explicit dataset hash.
    ///
    /// This enforces a non-empty branch name while preserving the standard
    /// codebase dataset naming and storage conventions.
    pub async fn create_dataset_for_branch_run(
        &self,
        repo_name: &str,
        repo_url: Option<&str>,
        repo_path: &str,
        commit_sha: &str,
        branch: &str,
        repo_slug: Option<&str>,
        dataset_hash_b3: &str,
        metadata_json: Option<&str>,
        tenant_id: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<(String, String)> {
        let branch = branch.trim();
        if branch.is_empty() {
            return Err(AosError::Validation(
                "branch is required for branch-run dataset creation".to_string(),
            ));
        }

        self.create_codebase_dataset_from_repo_with_hash(
            repo_name,
            repo_url,
            repo_path,
            commit_sha,
            Some(branch),
            repo_slug,
            dataset_hash_b3,
            metadata_json,
            tenant_id,
            created_by,
        )
        .await
    }

    /// Record the structured inputs used to compute a dataset content hash.
    pub async fn record_dataset_hash_inputs(
        &self,
        params: &CreateDatasetHashInputsParams,
    ) -> Result<String> {
        validate_hash_b3(&params.content_hash_b3)?;

        let id = new_id(IdPrefix::Dst);
        let include_private_flag = params
            .include_private
            .map(|value| if value { 1 } else { 0 });

        let result = sqlx::query(
            "INSERT OR IGNORE INTO dataset_hash_inputs (
                id, dataset_id, content_hash_b3, repo_id, repo_slug, commit_sha, branch,
                scan_root_path, remote_url, max_symbols, include_private, positive_weight,
                negative_weight, total_samples, positive_samples, negative_samples, ingestion_mode,
                codegraph_version, generator, scope_config_json, additional_inputs_json,
                tenant_id, created_by, hkdf_version, parser_version, path_normalization_version
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.dataset_id)
        .bind(&params.content_hash_b3)
        .bind(&params.repo_id)
        .bind(&params.repo_slug)
        .bind(&params.commit_sha)
        .bind(&params.branch)
        .bind(&params.scan_root_path)
        .bind(&params.remote_url)
        .bind(params.max_symbols)
        .bind(include_private_flag)
        .bind(params.positive_weight)
        .bind(params.negative_weight)
        .bind(params.total_samples)
        .bind(params.positive_samples)
        .bind(params.negative_samples)
        .bind(&params.ingestion_mode)
        .bind(&params.codegraph_version)
        .bind(&params.generator)
        .bind(&params.scope_config_json)
        .bind(&params.additional_inputs_json)
        .bind(&params.tenant_id)
        .bind(&params.created_by)
        .bind(params.hkdf_version.map(|v| v as i64))
        .bind(params.parser_version.map(|v| v as i64))
        .bind(params.path_normalization_version.map(|v| v as i64))
        .execute(self.pool())
        .await
        .map_err(db_err("record dataset hash inputs"))?;

        if result.rows_affected() == 0 {
            if let Some(dataset_id) = params.dataset_id.as_deref() {
                if let Some(existing_id) = self
                    .get_dataset_hash_inputs_id(dataset_id, &params.content_hash_b3)
                    .await?
                {
                    return Ok(existing_id);
                }
            }
        }

        Ok(id)
    }

    async fn get_dataset_hash_inputs_id(
        &self,
        dataset_id: &str,
        content_hash_b3: &str,
    ) -> Result<Option<String>> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM dataset_hash_inputs WHERE dataset_id = ? AND content_hash_b3 = ? LIMIT 1",
        )
        .bind(dataset_id)
        .bind(content_hash_b3)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("fetch dataset hash inputs id"))?;

        Ok(id)
    }

    /// Get dataset hash inputs by dataset_id and content_hash_b3.
    pub async fn get_dataset_hash_inputs_by_content_hash(
        &self,
        dataset_id: &str,
        content_hash_b3: &str,
    ) -> Result<Option<DatasetHashInputs>> {
        let inputs: Option<DatasetHashInputs> = sqlx::query_as(
            "SELECT id, dataset_id, content_hash_b3, repo_id, repo_slug, commit_sha, branch,
                    scan_root_path, remote_url, max_symbols, include_private, positive_weight,
                    negative_weight, total_samples, positive_samples, negative_samples,
                    ingestion_mode, codegraph_version, generator, scope_config_json,
                    additional_inputs_json, tenant_id, created_at, created_by,
                    hkdf_version, parser_version, path_normalization_version
             FROM dataset_hash_inputs
             WHERE dataset_id = ? AND content_hash_b3 = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(dataset_id)
        .bind(content_hash_b3)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("fetch dataset hash inputs by content hash"))?;

        Ok(inputs)
    }

    /// Get dataset hash inputs by dataset_id for algorithm version compatibility checks.
    ///
    /// Returns the most recent hash inputs record for the given dataset, which contains
    /// algorithm version information needed for deterministic replay validation.
    pub async fn get_dataset_hash_inputs_by_dataset_id(
        &self,
        dataset_id: &str,
    ) -> Result<Option<DatasetHashInputs>> {
        let inputs: Option<DatasetHashInputs> = sqlx::query_as(
            "SELECT id, dataset_id, content_hash_b3, repo_id, repo_slug, commit_sha, branch,
                    scan_root_path, remote_url, max_symbols, include_private, positive_weight,
                    negative_weight, total_samples, positive_samples, negative_samples,
                    ingestion_mode, codegraph_version, generator, scope_config_json,
                    additional_inputs_json, tenant_id, created_at, created_by,
                    hkdf_version, parser_version, path_normalization_version
             FROM dataset_hash_inputs
             WHERE dataset_id = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(dataset_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("fetch dataset hash inputs by dataset_id"))?;

        Ok(inputs)
    }

    /// Create a training dataset record for scan-root ingestion runs.
    ///
    /// This helper creates:
    /// - a dataset row
    /// - an initial dataset version
    /// - one dataset_scan_roots row per scan root
    pub async fn create_dataset_from_scan_roots(
        &self,
        repo_name: &str,
        repo_url: &str,
        scan_roots: &[ScanRootRunInput],
        commit_sha: &str,
        branch: Option<&str>,
        tenant_id: Option<&str>,
        created_by: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<(String, String)> {
        if scan_roots.is_empty() {
            return Err(AosError::Validation(
                "scan_roots cannot be empty for scan-root dataset creation".to_string(),
            ));
        }

        let mut hasher = blake3::Hasher::new();
        let repo_identity = if repo_url.trim().is_empty() {
            repo_name
        } else {
            repo_url
        };
        hasher.update(repo_identity.as_bytes());
        hasher.update(commit_sha.as_bytes());

        let mut sorted_roots: Vec<&ScanRootRunInput> = scan_roots.iter().collect();
        sorted_roots.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.label.cmp(&b.label)));
        for root in sorted_roots {
            let path = root.path.trim();
            if path.is_empty() {
                return Err(AosError::Validation(
                    "scan_root path cannot be empty".to_string(),
                ));
            }
            hasher.update(path.as_bytes());
            if let Some(ref label) = root.label {
                hasher.update(label.as_bytes());
            }
            if let Some(ref content_hash) = root.content_hash_b3 {
                hasher.update(content_hash.as_bytes());
            }
        }

        let dataset_hash = hasher.finalize().to_hex().to_string();
        let repo_slug = normalize_repo_slug(repo_name);
        let short_commit = commit_sha.get(0..8).unwrap_or(commit_sha);
        let dataset_name = format!("{}-{}", repo_slug, short_commit);
        let storage_path = format!("/datasets/scan-root/{}/{}", repo_slug, dataset_hash);

        let source_location = if repo_url.trim().is_empty() {
            scan_roots
                .first()
                .map(|root| root.path.as_str())
                .unwrap_or(repo_name)
        } else {
            repo_url
        };

        let params = CreateDatasetParams::builder()
            .name(&dataset_name)
            .format("custom")
            .hash_b3(&dataset_hash)
            .storage_path(&storage_path)
            .dataset_type("codebase")
            .category("codebase")
            .collection_method("code_ingestion_pipeline")
            .purpose(&format!("Repository code ingestion from {}", repo_name))
            .source_location(source_location)
            .status("processing")
            .repo_slug(&repo_slug)
            .commit_sha(commit_sha);

        let params = if let Some(branch) = branch.filter(|b| !b.trim().is_empty()) {
            params.branch(branch)
        } else {
            params
        };

        let params = if let Some(user) = created_by {
            params.created_by(user)
        } else {
            params
        };

        let params = if let Some(tid) = tenant_id {
            params.tenant_id(tid)
        } else {
            params
        };

        let params = params.build().map_err(|e| {
            adapteros_core::AosError::validation(format!(
                "Failed to build scan-root dataset params: {}",
                e
            ))
        })?;

        let dataset_id = self.create_training_dataset_from_params(&params).await?;

        let version_id = self
            .create_training_dataset_version(
                &dataset_id,
                tenant_id,
                Some(&format!("v1-{}", short_commit)),
                &storage_path,
                &dataset_hash,
                None,
                None,
                created_by,
            )
            .await?;

        let mut roots = Vec::with_capacity(scan_roots.len());
        for (idx, root) in scan_roots.iter().enumerate() {
            let path = root.path.trim();
            if path.is_empty() {
                return Err(AosError::Validation(
                    "scan_root path cannot be empty".to_string(),
                ));
            }

            let mut builder = CreateDatasetScanRootParams::builder(&dataset_id, path)
                .dataset_version_id(&version_id)
                .repo_name(repo_name)
                .repo_slug(&repo_slug)
                .commit_sha(commit_sha)
                .ordinal(root.ordinal.unwrap_or(idx as i32));

            if let Some(ref label) = root.label {
                builder = builder.label(label.clone());
            }
            if let Some(count) = root.file_count {
                builder = builder.file_count(count);
            }
            if let Some(count) = root.byte_count {
                builder = builder.byte_count(count);
            }
            if let Some(ref hash) = root.content_hash_b3 {
                builder = builder.content_hash_b3(hash.clone());
            }
            if let Some(ref scanned_at) = root.scanned_at {
                builder = builder.scanned_at(scanned_at.clone());
            }
            if let Some(ref metadata_json) = root.metadata_json {
                builder = builder.metadata_json(metadata_json.clone());
            }
            if let Some(session_id) = session_id {
                builder = builder.session_id(session_id);
            }
            if let Some(branch) = branch.filter(|b| !b.trim().is_empty()) {
                builder = builder.branch(branch);
            }
            if !repo_url.trim().is_empty() {
                builder = builder.remote_url(repo_url);
            }
            if let Some(tid) = tenant_id {
                builder = builder.tenant_id(tid);
            }
            if let Some(user) = created_by {
                builder = builder.created_by(user);
            }

            roots.push(builder.build());
        }

        self.bulk_insert_dataset_scan_roots(&roots).await?;

        tracing::info!(
            dataset_id = %dataset_id,
            version_id = %version_id,
            repo_name = %repo_name,
            commit = %short_commit,
            scan_root_count = roots.len(),
            "Created dataset record for scan-root ingestion"
        );

        Ok((dataset_id, version_id))
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
        let mut resolved_storage_path = storage_path.to_string();
        if !storage_path.trim().is_empty() {
            if let Ok(files) = self.get_dataset_files(dataset_id).await {
                if files.len() == 1 {
                    let file = &files[0];
                    if file.hash_b3 == hash_b3 && !file.file_path.trim().is_empty() {
                        resolved_storage_path = file.file_path.clone();
                    }
                }
            }
        }

        let next_version: (i64,) = sqlx::query_as(
            "SELECT COALESCE(MAX(version_number), 0) + 1 FROM training_dataset_versions WHERE dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_one(self.pool())
        .await
        .map_err(db_err("next dataset version number"))?;

        let version_id = new_id(IdPrefix::Dst);

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
        .bind(&resolved_storage_path)
        .bind(hash_b3)
        .bind(manifest_path)
        .bind(manifest_json)
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset version"))?;

        self.dual_write_dataset_version_to_kv(&version_id).await?;

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

    /// Record a dataset version for an ingestion run and link session rows.
    ///
    /// Creates a new dataset version and updates any rows/scan roots tagged
    /// with the given session_id to point at the new version.
    pub async fn record_dataset_version_for_run(
        &self,
        dataset_id: &str,
        session_id: &str,
        version_label: Option<&str>,
        version_hash_b3: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<String> {
        let dataset = self
            .get_training_dataset(dataset_id)
            .await?
            .ok_or_else(|| AosError::Validation("dataset not found".to_string()))?;

        let label = normalize_optional_value(version_label).or_else(|| {
            let trimmed = session_id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("run-{}", trimmed.get(0..8).unwrap_or(trimmed)))
            }
        });

        let hash_b3 = version_hash_b3
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .unwrap_or_else(|| dataset.hash_b3.clone());

        let created_by = created_by.or(dataset.created_by.as_deref());

        let version_id = self
            .create_training_dataset_version(
                &dataset.id,
                dataset.tenant_id.as_deref(),
                label.as_deref(),
                &dataset.storage_path,
                &hash_b3,
                None,
                None,
                created_by,
            )
            .await?;

        if !session_id.trim().is_empty() {
            self.update_session_version(session_id, &version_id).await?;
            self.update_dataset_scan_root_session_version(session_id, &version_id)
                .await?;
        }

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

        self.dual_write_dataset_version_to_kv(version_id).await?;

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

    /// Get correlation ID for a dataset (best-effort).
    pub async fn get_dataset_correlation_id(&self, dataset_id: &str) -> Result<Option<String>> {
        let correlation_id =
            sqlx::query_scalar("SELECT correlation_id FROM training_datasets WHERE id = ? LIMIT 1")
                .bind(dataset_id)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err("get dataset correlation id"))?;

        Ok(correlation_id)
    }

    /// Resolve correlation ID from a dataset version ID (best-effort).
    pub async fn get_dataset_correlation_id_from_version(
        &self,
        dataset_version_id: &str,
    ) -> Result<Option<String>> {
        let correlation_id = sqlx::query_scalar(
            "SELECT td.correlation_id
             FROM training_dataset_versions v
             JOIN training_datasets td ON td.id = v.dataset_id
             WHERE v.id = ?
             LIMIT 1",
        )
        .bind(dataset_version_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset correlation id from version"))?;

        Ok(correlation_id)
    }

    /// Get training dataset by ID with BLAKE3 hash verification.
    ///
    /// Fetches the dataset and verifies that the file at `storage_path`
    /// matches the stored `hash_b3` content hash. Returns an error if:
    /// - The file does not exist at storage_path
    /// - The computed hash does not match the stored hash
    /// - The dataset is not found
    ///
    /// This method is critical for ensuring data integrity before training
    /// operations that depend on dataset content being unmodified.
    ///
    /// # Errors
    ///
    /// Returns `AosError::Validation` with specific message for:
    /// - `DATASET_NOT_FOUND` - Dataset ID does not exist
    /// - `DATASET_FILE_MISSING` - File at storage_path does not exist
    /// - `DATASET_HASH_MISMATCH` - Computed hash differs from stored hash
    pub async fn get_training_dataset_with_verification(
        &self,
        dataset_id: &str,
    ) -> Result<TrainingDataset> {
        let dataset = self
            .get_training_dataset(dataset_id)
            .await?
            .ok_or_else(|| {
                AosError::Validation(format!(
                    "DATASET_NOT_FOUND: Dataset with id '{}' does not exist",
                    dataset_id
                ))
            })?;

        // Verify file exists at storage_path
        let storage_path = std::path::Path::new(&dataset.storage_path);
        if !storage_path.exists() {
            return Err(AosError::Validation(format!(
                "DATASET_FILE_MISSING: File does not exist at storage_path '{}' for dataset '{}'",
                dataset.storage_path, dataset_id
            )));
        }

        // Compute BLAKE3 hash of file content
        let file_content = tokio::fs::read(storage_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to read dataset file at '{}': {}",
                dataset.storage_path, e
            ))
        })?;

        let computed_hash = blake3::hash(&file_content).to_hex().to_string();

        // Compare with stored hash_b3 (the content hash)
        if computed_hash != dataset.hash_b3 {
            return Err(AosError::Validation(format!(
                "DATASET_HASH_MISMATCH: Content hash mismatch for dataset '{}'. \
                 Expected '{}', computed '{}'. File may have been modified or corrupted.",
                dataset_id, dataset.hash_b3, computed_hash
            )));
        }

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

    // ============================================================================
    // Branch Run Dataset Operations
    // ============================================================================

    /// List training datasets for a specific branch within a tenant.
    ///
    /// Filters datasets by tenant_id and branch name, returning only datasets
    /// created for the specified branch run.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    /// * `branch` - The git branch name to filter by (e.g., "main", "feature/xyz")
    /// * `limit` - Maximum number of datasets to return
    ///
    /// # Returns
    /// Vector of training datasets for the branch, ordered by creation date (newest first)
    pub async fn list_training_datasets_for_branch(
        &self,
        tenant_id: &str,
        branch: &str,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE tenant_id = ? AND branch = ? \
             ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(tenant_id)
        .bind(branch)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list training datasets for branch: {}",
                e
            ))
        })?;
        Ok(datasets)
    }

    /// List training datasets for a specific repository and branch.
    ///
    /// Filters datasets by repo_slug and branch, useful for finding all datasets
    /// generated from a particular repository branch.
    ///
    /// # Arguments
    /// * `repo_slug` - The repository slug (e.g., "org/repo-name")
    /// * `branch` - The git branch name (e.g., "main", "feature/xyz")
    /// * `limit` - Maximum number of datasets to return
    ///
    /// # Returns
    /// Vector of training datasets for the repo/branch, ordered by creation date (newest first)
    pub async fn list_training_datasets_for_repo_branch(
        &self,
        repo_slug: &str,
        branch: &str,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE repo_slug = ? AND branch = ? \
             ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(repo_slug)
        .bind(branch)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list training datasets for repo branch: {}",
                e
            ))
        })?;
        Ok(datasets)
    }

    /// Get the latest dataset for a specific repository branch.
    ///
    /// Returns the most recently created dataset for the given repo/branch combination.
    /// Useful for getting the current training dataset for a branch.
    ///
    /// # Arguments
    /// * `repo_slug` - The repository slug (e.g., "org/repo-name")
    /// * `branch` - The git branch name (e.g., "main", "feature/xyz")
    ///
    /// # Returns
    /// The most recent training dataset for the repo/branch, if any exists
    pub async fn get_latest_dataset_for_repo_branch(
        &self,
        repo_slug: &str,
        branch: &str,
    ) -> Result<Option<TrainingDataset>> {
        let dataset = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE repo_slug = ? AND branch = ? \
             ORDER BY created_at DESC LIMIT 1",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(repo_slug)
        .bind(branch)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get latest dataset for repo branch: {}",
                e
            ))
        })?;
        Ok(dataset)
    }

    /// Get a dataset by commit SHA.
    ///
    /// Returns the dataset created at a specific commit. Useful for reproducibility
    /// and finding the exact dataset used for a particular training run.
    ///
    /// # Arguments
    /// * `commit_sha` - The git commit SHA to search for
    ///
    /// # Returns
    /// The training dataset created at that commit, if any exists
    pub async fn get_dataset_by_commit_sha(
        &self,
        commit_sha: &str,
    ) -> Result<Option<TrainingDataset>> {
        let dataset = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE commit_sha = ? LIMIT 1",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(commit_sha)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get dataset by commit SHA: {}", e)))?;
        Ok(dataset)
    }

    async fn sync_dataset_to_kv_if_possible(&self, dataset_id: &str) -> Result<()> {
        let Some(dataset) = self.get_training_dataset(dataset_id).await? else {
            return Ok(());
        };
        let Some(tenant_id) = dataset.tenant_id.as_deref() else {
            return Ok(());
        };
        self.sync_dataset_to_kv(tenant_id, dataset_id).await
    }

    /// Update the branch and commit information for a dataset.
    ///
    /// Useful for associating an existing dataset with branch run metadata
    /// after initial creation.
    ///
    /// # Arguments
    /// * `dataset_id` - The dataset ID to update
    /// * `branch` - The git branch name (optional, updates only if Some)
    /// * `commit_sha` - The git commit SHA (optional, updates only if Some)
    pub async fn update_dataset_branch_info(
        &self,
        dataset_id: &str,
        branch: Option<&str>,
        commit_sha: Option<&str>,
    ) -> Result<()> {
        let branch = normalize_optional_value(branch);
        let commit_sha = normalize_optional_value(commit_sha);
        sqlx::query(
            "UPDATE training_datasets
             SET branch = COALESCE(?, branch),
                 commit_sha = COALESCE(?, commit_sha),
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(branch)
        .bind(commit_sha)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update dataset branch info: {}", e)))?;

        self.sync_dataset_to_kv_if_possible(dataset_id).await?;
        Ok(())
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
        let tenant_id = self
            .get_training_dataset(dataset_id)
            .await?
            .and_then(|dataset| dataset.tenant_id);
        sqlx::query("DELETE FROM training_datasets WHERE id = ?")
            .bind(dataset_id)
            .execute(self.pool())
            .await
            .map_err(db_err("delete training dataset"))?;
        if let Some(tenant_id) = tenant_id {
            self.delete_dataset_from_kv(&tenant_id, dataset_id).await?;
        }
        Ok(())
    }

    /// Rollback all training datasets associated with a session.
    ///
    /// This is an atomic operation that deletes all datasets created during
    /// a training session. Use this to undo a failed or unwanted ingestion run.
    ///
    /// # Algorithm
    /// 1. Query all datasets with matching session_id
    /// 2. Delete each dataset (including KV sync cleanup)
    /// 3. Return count of deleted datasets
    ///
    /// # Arguments
    /// * `session_id` - The session identifier to rollback
    ///
    /// # Returns
    /// Count of datasets that were deleted
    ///
    /// # Note
    /// This does not delete associated rows (codebase_dataset_rows, dataset_scan_roots).
    /// Those should be cleaned up separately if needed using:
    /// - `delete_session_rows()` for codebase_dataset_rows
    /// - `delete_dataset_scan_roots_by_session()` for dataset_scan_roots
    ///
    /// For a complete session rollback, call all three methods.
    pub async fn rollback_training_session(&self, session_id: &str) -> Result<u64> {
        // Query all datasets with this session_id
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE session_id = ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(session_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("query datasets for session rollback"))?;

        if datasets.is_empty() {
            return Ok(0);
        }

        let mut deleted = 0u64;
        for dataset in &datasets {
            // Use existing delete method which handles KV cleanup
            match self.delete_training_dataset(&dataset.id).await {
                Ok(()) => {
                    deleted += 1;
                }
                Err(e) => {
                    // Log but continue with other deletions
                    tracing::warn!(
                        error = %e,
                        dataset_id = %dataset.id,
                        session_id = %session_id,
                        "Failed to delete dataset during session rollback"
                    );
                }
            }
        }

        tracing::info!(
            session_id = %session_id,
            deleted = deleted,
            total = datasets.len(),
            "Training session rollback complete"
        );

        Ok(deleted)
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
        let id = new_id(IdPrefix::Dst);
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

        self.sync_dataset_to_kv_if_possible(dataset_id).await?;

        Ok(id)
    }

    /// Insert dataset file records in bulk, skipping existing file names.
    pub async fn insert_dataset_files(
        &self,
        dataset_id: &str,
        files: &[CreateDatasetFileParams],
    ) -> Result<usize> {
        if files.is_empty() {
            return Ok(0);
        }

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(db_err("begin dataset file insert transaction"))?;

        let existing_names: Vec<String> =
            sqlx::query_scalar("SELECT file_name FROM dataset_files WHERE dataset_id = ?")
                .bind(dataset_id)
                .fetch_all(tx.as_mut())
                .await
                .map_err(db_err("list dataset files"))?;

        let mut existing: HashSet<String> = existing_names.into_iter().collect();
        let mut seen: HashSet<String> = HashSet::new();
        let mut inserted = 0usize;
        let mut size_delta: i64 = 0;

        for file in files {
            let file_name = file.file_name.trim();
            if file_name.is_empty() {
                continue;
            }
            if existing.contains(file_name) || !seen.insert(file_name.to_string()) {
                continue;
            }

            let id = new_id(IdPrefix::Dst);
            sqlx::query(
                "INSERT INTO dataset_files (id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(dataset_id)
            .bind(file_name)
            .bind(&file.file_path)
            .bind(file.size_bytes)
            .bind(&file.hash_b3)
            .bind(&file.mime_type)
            .execute(tx.as_mut())
            .await
            .map_err(db_err("insert dataset files"))?;

            inserted += 1;
            size_delta = size_delta.saturating_add(file.size_bytes);
            existing.insert(file_name.to_string());
        }

        if inserted > 0 {
            sqlx::query(
                "UPDATE training_datasets
                 SET file_count = file_count + ?,
                     total_size_bytes = total_size_bytes + ?,
                     updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(inserted as i64)
            .bind(size_delta)
            .bind(dataset_id)
            .execute(tx.as_mut())
            .await
            .map_err(db_err("update dataset file counts"))?;
        }

        tx.commit().await.map_err(db_err("commit dataset files"))?;

        if inserted > 0 {
            self.sync_dataset_to_kv_if_possible(dataset_id).await?;
        }

        Ok(inserted)
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
        self.sync_dataset_to_kv_if_possible(dataset_id).await?;
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

    /// Get a single dataset file by dataset ID and file ID.
    pub async fn get_dataset_file(
        &self,
        dataset_id: &str,
        file_id: &str,
    ) -> Result<Option<DatasetFile>> {
        let file = sqlx::query_as::<_, DatasetFile>(
            "SELECT id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type, created_at
             FROM dataset_files
             WHERE dataset_id = ? AND id = ?",
        )
        .bind(dataset_id)
        .bind(file_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset file"))?;
        Ok(file)
    }

    /// Get files in dataset scoped to a specific workspace.
    ///
    /// This method joins with training_datasets to ensure the dataset belongs to the
    /// specified workspace, providing proper tenant isolation.
    pub async fn get_dataset_files_for_workspace(
        &self,
        workspace_id: &str,
        dataset_id: &str,
    ) -> Result<Vec<DatasetFile>> {
        let files = sqlx::query_as::<_, DatasetFile>(
            "SELECT df.id, df.dataset_id, df.file_name, df.file_path, df.size_bytes,
                    df.hash_b3, df.mime_type, df.created_at
             FROM dataset_files df
             INNER JOIN training_datasets td ON df.dataset_id = td.id
             WHERE df.dataset_id = ? AND td.workspace_id = ?
             ORDER BY df.created_at ASC",
        )
        .bind(dataset_id)
        .bind(workspace_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("get dataset files for workspace"))?;
        Ok(files)
    }

    /// Get a single dataset file scoped to a workspace.
    ///
    /// Returns None if the file doesn't exist or the dataset doesn't belong to the workspace.
    pub async fn get_dataset_file_for_workspace(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        file_id: &str,
    ) -> Result<Option<DatasetFile>> {
        let file = sqlx::query_as::<_, DatasetFile>(
            "SELECT df.id, df.dataset_id, df.file_name, df.file_path, df.size_bytes,
                    df.hash_b3, df.mime_type, df.created_at
             FROM dataset_files df
             INNER JOIN training_datasets td ON df.dataset_id = td.id
             WHERE df.dataset_id = ? AND df.id = ? AND td.workspace_id = ?",
        )
        .bind(dataset_id)
        .bind(file_id)
        .bind(workspace_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("get dataset file for workspace"))?;
        Ok(file)
    }

    /// Get files for all datasets in a workspace.
    ///
    /// Useful for workspace-wide file listing operations.
    pub async fn list_all_files_for_workspace(
        &self,
        workspace_id: &str,
        limit: i64,
    ) -> Result<Vec<DatasetFile>> {
        let files = sqlx::query_as::<_, DatasetFile>(
            "SELECT df.id, df.dataset_id, df.file_name, df.file_path, df.size_bytes,
                    df.hash_b3, df.mime_type, df.created_at
             FROM dataset_files df
             INNER JOIN training_datasets td ON df.dataset_id = td.id
             WHERE td.workspace_id = ?
             ORDER BY df.created_at DESC
             LIMIT ?",
        )
        .bind(workspace_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list all files for workspace"))?;
        Ok(files)
    }

    /// Count total files in a workspace.
    pub async fn count_files_for_workspace(&self, workspace_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dataset_files df
             INNER JOIN training_datasets td ON df.dataset_id = td.id
             WHERE td.workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0);
        Ok(count)
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

    /// Sum total dataset bytes for a specific workspace.
    pub async fn sum_dataset_sizes_for_workspace(&self, workspace_id: &str) -> Result<i64> {
        let total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(total_size_bytes), 0) as total FROM training_datasets WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0);
        Ok(total)
    }

    /// Count datasets in a workspace.
    pub async fn count_datasets_for_workspace(&self, workspace_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) as cnt FROM training_datasets WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0);
        Ok(count)
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
        errors_json: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE training_datasets
             SET validation_status = ?, validation_errors = ?, validation_errors_json = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(status)
        .bind(errors)
        .bind(errors_json)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset validation"))?;
        self.sync_dataset_to_kv_if_possible(dataset_id).await?;
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

        self.sync_dataset_version_to_kv(dataset_version_id).await?;

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

        self.sync_dataset_version_to_kv(dataset_version_id).await?;

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
    ///
    /// Optional determinism fields:
    /// - `validation_seed_hex`: Hex-encoded 32-byte seed used for validation
    /// - `determinism_mode`: The determinism mode (strict, best_effort, non_deterministic)
    /// - `validation_hash_b3`: BLAKE3 hash of validation output for reproducibility checks
    pub async fn record_dataset_version_validation_run(
        &self,
        dataset_version_id: &str,
        tier: &str,
        status: &str,
        signal: Option<&str>,
        validation_errors_json: Option<&str>,
        sample_row_ids_json: Option<&str>,
        created_by: Option<&str>,
        validation_seed_hex: Option<&str>,
        determinism_mode: Option<&str>,
        validation_hash_b3: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Dst);
        let is_deterministic = validation_seed_hex.is_some() as i32;
        sqlx::query(
            "INSERT INTO dataset_version_validations (
                id, dataset_version_id, tier, status, signal, validation_errors_json,
                sample_row_ids_json, created_by, validation_seed_hex, determinism_mode,
                validation_hash_b3, is_deterministic
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(dataset_version_id)
        .bind(tier)
        .bind(status)
        .bind(signal)
        .bind(validation_errors_json)
        .bind(sample_row_ids_json)
        .bind(created_by)
        .bind(validation_seed_hex)
        .bind(determinism_mode)
        .bind(validation_hash_b3)
        .bind(is_deterministic)
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

        let id = new_id(IdPrefix::Dst);
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

    /// List validation runs for a dataset version, ordered by most recent first.
    ///
    /// This provides an audit trail of all safety/validation checks performed on the version.
    pub async fn list_dataset_version_validation_runs(
        &self,
        dataset_version_id: &str,
        limit: i64,
    ) -> Result<Vec<DatasetVersionValidation>> {
        let validations = sqlx::query_as::<_, DatasetVersionValidation>(
            "SELECT id, dataset_version_id, tier, status, signal, validation_errors_json,
                    sample_row_ids_json, created_at, created_by
             FROM dataset_version_validations
             WHERE dataset_version_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(dataset_version_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list dataset version validation runs"))?;
        Ok(validations)
    }

    /// List trust override history for a dataset version, ordered by most recent first.
    ///
    /// This provides an audit trail of all admin overrides applied to the version.
    pub async fn list_dataset_version_overrides(
        &self,
        dataset_version_id: &str,
        limit: i64,
    ) -> Result<Vec<DatasetVersionOverride>> {
        let overrides = sqlx::query_as::<_, DatasetVersionOverride>(
            "SELECT id, dataset_version_id, override_state, reason, created_by, created_at
             FROM dataset_version_overrides
             WHERE dataset_version_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(dataset_version_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list dataset version overrides"))?;
        Ok(overrides)
    }

    /// Compute effective trust_state by layering overrides over derived trust_state.
    pub async fn get_effective_trust_state(
        &self,
        dataset_version_id: &str,
    ) -> Result<Option<String>> {
        let version = self
            .get_training_dataset_version(dataset_version_id)
            .await?;

        let version = match version {
            Some(v) => v,
            None => return Ok(None),
        };

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
            version.trust_state
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

        let version = match version {
            Some(v) => v,
            None => return Ok(None),
        };

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
            version.trust_state
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
        self.sync_dataset_to_kv_if_possible(dataset_id).await?;
        Ok(())
    }

    /// Update dataset repo_slug tag
    ///
    /// Sets or clears the repository slug for a dataset, used for filtering
    /// datasets by their source repository (e.g., "org/repo-name").
    pub async fn update_dataset_repo_slug(
        &self,
        dataset_id: &str,
        repo_slug: Option<&str>,
    ) -> Result<()> {
        let repo_slug = sanitize_repo_slug(repo_slug).map(|slug| normalize_repo_slug(&slug));
        sqlx::query(
            "UPDATE training_datasets
             SET repo_slug = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(repo_slug)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(db_err("update dataset repo_slug"))?;
        self.sync_dataset_to_kv_if_possible(dataset_id).await?;
        Ok(())
    }

    /// List training datasets by repo_slug
    ///
    /// Returns all datasets tagged with the specified repository slug.
    /// Useful for finding all datasets derived from a specific source repository.
    ///
    /// # Arguments
    /// * `repo_slug` - The repository slug to filter by (e.g., "org/repo-name")
    /// * `limit` - Maximum number of datasets to return
    ///
    /// # Returns
    /// Vector of training datasets with matching repo_slug, ordered by creation date (newest first)
    pub async fn list_training_datasets_by_repo_slug(
        &self,
        repo_slug: &str,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE repo_slug = ? ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(repo_slug)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list training datasets by repo_slug"))?;
        Ok(datasets)
    }

    /// List training datasets by repo_slug for a specific tenant
    ///
    /// Returns datasets tagged with the specified repository slug within a tenant.
    /// Enforces tenant isolation for multi-tenant deployments.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    /// * `repo_slug` - The repository slug to filter by (e.g., "org/repo-name")
    /// * `limit` - Maximum number of datasets to return
    ///
    /// # Returns
    /// Vector of training datasets with matching repo_slug and tenant_id
    pub async fn list_training_datasets_by_repo_slug_for_tenant(
        &self,
        tenant_id: &str,
        repo_slug: &str,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(&format!(
            "SELECT {} FROM training_datasets WHERE tenant_id = ? AND repo_slug = ? ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS
        ))
        .bind(tenant_id)
        .bind(repo_slug)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list training datasets by repo_slug for tenant"))?;
        Ok(datasets)
    }

    /// Get a codebase dataset by repository source location.
    ///
    /// Finds a dataset associated with a codebase by matching the `source_location` field.
    /// This is used by version listing endpoints to find datasets for codebase adapter workflows.
    ///
    /// # Arguments
    /// * `source_location` - The source location (repo path, URL, etc.) to match
    /// * `tenant_id` - Optional tenant ID to filter by (for tenant isolation)
    ///
    /// # Returns
    /// The first matching training dataset, or None if not found
    pub async fn get_codebase_dataset_by_repo(
        &self,
        source_location: &str,
        tenant_id: Option<&str>,
    ) -> Result<Option<TrainingDataset>> {
        let dataset = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE source_location = ? AND tenant_id = ?
                 ORDER BY created_at DESC LIMIT 1",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(source_location)
            .bind(tid)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err("get codebase dataset by repo (with tenant)"))?
        } else {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE source_location = ?
                 ORDER BY created_at DESC LIMIT 1",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(source_location)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err("get codebase dataset by repo"))?
        };
        Ok(dataset)
    }

    /// List codebase datasets by repository source location.
    pub async fn list_codebase_datasets_by_repo(
        &self,
        source_location: &str,
        tenant_id: Option<&str>,
    ) -> Result<Vec<TrainingDataset>> {
        let datasets = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE source_location = ? AND tenant_id = ?
                 ORDER BY created_at DESC",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(source_location)
            .bind(tid)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list codebase datasets by repo (with tenant)"))?
        } else {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE source_location = ?
                 ORDER BY created_at DESC",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(source_location)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list codebase datasets by repo"))?
        };

        Ok(datasets)
    }

    /// Get a codebase dataset by repository identifier stored in metadata_json.
    pub async fn get_codebase_dataset_by_repo_identifier(
        &self,
        repo_identifier: &str,
        tenant_id: Option<&str>,
    ) -> Result<Option<TrainingDataset>> {
        let trimmed = repo_identifier.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let normalized = normalize_repo_id(trimmed);

        let dataset = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE tenant_id = ?
                   AND (
                     json_extract(metadata_json, '$.repo_identifier') = ?
                     OR json_extract(metadata_json, '$.scope_repo_id') = ?
                     OR json_extract(metadata_json, '$.repo_id') = ?
                   )
                 ORDER BY created_at DESC LIMIT 1",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(tid)
            .bind(&normalized)
            .bind(&normalized)
            .bind(&normalized)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err(
                "get codebase dataset by repo identifier (with tenant)",
            ))?
        } else {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE (
                     json_extract(metadata_json, '$.repo_identifier') = ?
                     OR json_extract(metadata_json, '$.scope_repo_id') = ?
                     OR json_extract(metadata_json, '$.repo_id') = ?
                 )
                 ORDER BY created_at DESC LIMIT 1",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(&normalized)
            .bind(&normalized)
            .bind(&normalized)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err("get codebase dataset by repo identifier"))?
        };

        Ok(dataset)
    }

    /// List codebase datasets by repository identifier stored in metadata_json.
    pub async fn list_codebase_datasets_by_repo_identifier(
        &self,
        repo_identifier: &str,
        tenant_id: Option<&str>,
    ) -> Result<Vec<TrainingDataset>> {
        let trimmed = repo_identifier.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let normalized = normalize_repo_id(trimmed);

        let datasets = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE tenant_id = ?
                   AND (
                     json_extract(metadata_json, '$.repo_identifier') = ?
                     OR json_extract(metadata_json, '$.scope_repo_id') = ?
                     OR json_extract(metadata_json, '$.repo_id') = ?
                   )
                 ORDER BY created_at DESC",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(tid)
            .bind(&normalized)
            .bind(&normalized)
            .bind(&normalized)
            .fetch_all(self.pool())
            .await
            .map_err(db_err(
                "list codebase datasets by repo identifier (with tenant)",
            ))?
        } else {
            sqlx::query_as::<_, TrainingDataset>(&format!(
                "SELECT {} FROM training_datasets
                 WHERE (
                     json_extract(metadata_json, '$.repo_identifier') = ?
                     OR json_extract(metadata_json, '$.scope_repo_id') = ?
                     OR json_extract(metadata_json, '$.repo_id') = ?
                 )
                 ORDER BY created_at DESC",
                TRAINING_DATASET_COLUMNS
            ))
            .bind(&normalized)
            .bind(&normalized)
            .bind(&normalized)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list codebase datasets by repo identifier"))?
        };

        Ok(datasets)
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
        let id = new_id(IdPrefix::Dst);
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
        let id = new_id(IdPrefix::Dst);
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
        self.sync_dataset_to_kv_if_possible(dataset_id).await?;
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

    #[test]
    fn training_jsonl_accepts_input_target_schema_with_metadata() {
        let data =
            br#"{"input":"hi","target":"there","metadata":{"source":"unit_test","split":"eval","weight":2.0}}"#;
        let (rows, parse_errors, dropped) = super::build_training_rows_from_jsonl_bytes(
            "fixture.jsonl",
            data,
            "dst-test",
            "dsv-test",
            Some("tenant-test"),
            Some("user-test"),
            Some("upload"),
        );

        assert_eq!(parse_errors, 0);
        assert_eq!(dropped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt, "hi");
        assert_eq!(rows[0].response, "there");
        assert_eq!(rows[0].split, "eval");
        assert_eq!(rows[0].weight, 2.0);
        assert_eq!(rows[0].source_file.as_deref(), Some("fixture.jsonl"));
        assert_eq!(rows[0].source_line, Some(1));
        assert!(rows[0]
            .metadata_json
            .as_deref()
            .unwrap_or_default()
            .contains("unit_test"));
        assert_eq!(rows[0].tenant_id.as_deref(), Some("tenant-test"));
        assert_eq!(rows[0].created_by.as_deref(), Some("user-test"));
        assert_eq!(rows[0].source_type.as_deref(), Some("upload"));
    }

    #[test]
    fn training_jsonl_accepts_prompt_response_schema_and_defaults_split() {
        let data = br#"{"prompt":"p","response":"r"}"#;
        let (rows, parse_errors, dropped) = super::build_training_rows_from_jsonl_bytes(
            "fixture.jsonl",
            data,
            "dst-test",
            "dsv-test",
            None,
            None,
            None,
        );
        assert_eq!(parse_errors, 0);
        assert_eq!(dropped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt, "p");
        assert_eq!(rows[0].response, "r");
        assert_eq!(rows[0].split, "train");
        assert_eq!(rows[0].weight, 1.0);
    }

    #[test]
    fn training_jsonl_accepts_openai_prompt_completion_schema() {
        let data = br#"{"prompt":"p","completion":"c"}"#;
        let (rows, parse_errors, dropped) = super::build_training_rows_from_jsonl_bytes(
            "fixture.jsonl",
            data,
            "dst-test",
            "dsv-test",
            None,
            None,
            None,
        );
        assert_eq!(parse_errors, 0);
        assert_eq!(dropped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt, "p");
        assert_eq!(rows[0].response, "c");
    }

    #[test]
    fn training_jsonl_accepts_raw_text_schema() {
        let data = br#"{"text":"hello"}"#;
        let (rows, parse_errors, dropped) = super::build_training_rows_from_jsonl_bytes(
            "fixture.jsonl",
            data,
            "dst-test",
            "dsv-test",
            None,
            None,
            None,
        );
        assert_eq!(parse_errors, 0);
        assert_eq!(dropped, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt, "hello");
        assert!(rows[0].response.is_empty());
    }
}

// ==============================================================================
// Workstream 9: Dataset Integrity Pre-Training Check
// ==============================================================================

// Note: DatasetFileMismatch and DatasetIntegrityResult moved to integrity.rs

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
// Training Dataset Rows (Deterministic Runs)
// ==============================================================================

/// Sample role for training examples
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SampleRole {
    /// Positive examples teach knowledge
    #[default]
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

/// A single row in a general training dataset.
///
/// Stores canonical prompt/response pairs for uploaded or synthetic datasets.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrainingDatasetRow {
    pub id: String,
    pub dataset_id: String,
    pub dataset_version_id: Option<String>,
    pub session_id: Option<String>,
    pub prompt: String,
    pub response: String,
    pub weight: f64,
    pub split: String,
    pub sample_role: String,
    pub content_hash_b3: String,
    pub source_type: Option<String>,
    pub source_file: Option<String>,
    pub source_line: Option<i32>,
    pub tenant_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
}

/// Parameters for creating a training dataset row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTrainingDatasetRowParams {
    pub row_id: Option<String>,
    pub dataset_id: String,
    pub dataset_version_id: Option<String>,
    pub session_id: Option<String>,
    pub prompt: String,
    pub response: String,
    pub weight: f64,
    pub split: String,
    pub sample_role: SampleRole,
    pub source_type: Option<String>,
    pub source_file: Option<String>,
    pub source_line: Option<i32>,
    pub tenant_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_by: Option<String>,
}

impl CreateTrainingDatasetRowParams {
    /// Create minimal training dataset row parameters with defaults.
    pub fn new(
        dataset_id: impl Into<String>,
        prompt: impl Into<String>,
        response: impl Into<String>,
    ) -> Self {
        Self {
            row_id: None,
            dataset_id: dataset_id.into(),
            dataset_version_id: None,
            session_id: None,
            prompt: prompt.into(),
            response: response.into(),
            weight: 1.0,
            split: "train".to_string(),
            sample_role: SampleRole::Positive,
            source_type: None,
            source_file: None,
            source_line: None,
            tenant_id: None,
            metadata_json: None,
            created_by: None,
        }
    }

    /// Create a builder for training dataset row parameters.
    pub fn builder(
        dataset_id: impl Into<String>,
        prompt: impl Into<String>,
        response: impl Into<String>,
    ) -> CreateTrainingDatasetRowParamsBuilder {
        CreateTrainingDatasetRowParamsBuilder::new(dataset_id, prompt, response)
    }
}

/// Builder for creating `CreateTrainingDatasetRowParams`.
#[derive(Debug, Default)]
pub struct CreateTrainingDatasetRowParamsBuilder {
    row_id: Option<String>,
    dataset_id: String,
    dataset_version_id: Option<String>,
    session_id: Option<String>,
    prompt: String,
    response: String,
    weight: f64,
    split: Option<String>,
    sample_role: SampleRole,
    source_type: Option<String>,
    source_file: Option<String>,
    source_line: Option<i32>,
    tenant_id: Option<String>,
    metadata_json: Option<String>,
    created_by: Option<String>,
}

impl CreateTrainingDatasetRowParamsBuilder {
    /// Create a new builder with required fields.
    pub fn new(
        dataset_id: impl Into<String>,
        prompt: impl Into<String>,
        response: impl Into<String>,
    ) -> Self {
        Self {
            dataset_id: dataset_id.into(),
            prompt: prompt.into(),
            response: response.into(),
            weight: 1.0,
            split: None,
            sample_role: SampleRole::Positive,
            ..Default::default()
        }
    }

    pub fn row_id(mut self, row_id: impl Into<String>) -> Self {
        self.row_id = Some(row_id.into());
        self
    }

    pub fn dataset_version_id(mut self, id: impl Into<String>) -> Self {
        self.dataset_version_id = Some(id.into());
        self
    }

    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    pub fn split(mut self, split: impl Into<String>) -> Self {
        self.split = Some(split.into());
        self
    }

    pub fn sample_role(mut self, role: SampleRole) -> Self {
        self.sample_role = role;
        self
    }

    pub fn source_type(mut self, source_type: impl Into<String>) -> Self {
        self.source_type = Some(source_type.into());
        self
    }

    pub fn source_file(mut self, source_file: impl Into<String>) -> Self {
        self.source_file = Some(source_file.into());
        self
    }

    pub fn source_line(mut self, source_line: i32) -> Self {
        self.source_line = Some(source_line);
        self
    }

    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    pub fn metadata_json(mut self, metadata_json: impl Into<String>) -> Self {
        self.metadata_json = Some(metadata_json.into());
        self
    }

    pub fn created_by(mut self, created_by: impl Into<String>) -> Self {
        self.created_by = Some(created_by.into());
        self
    }

    pub fn build(self) -> CreateTrainingDatasetRowParams {
        let split = self
            .split
            .unwrap_or_else(|| "train".to_string())
            .to_lowercase();

        CreateTrainingDatasetRowParams {
            row_id: self.row_id,
            dataset_id: self.dataset_id,
            dataset_version_id: self.dataset_version_id,
            session_id: self.session_id,
            prompt: self.prompt,
            response: self.response,
            weight: self.weight,
            split,
            sample_role: self.sample_role,
            source_type: self.source_type,
            source_file: self.source_file,
            source_line: self.source_line,
            tenant_id: self.tenant_id,
            metadata_json: self.metadata_json,
            created_by: self.created_by,
        }
    }
}

/// Build training dataset rows from JSONL bytes.
///
/// Returns a tuple of (rows, parse_errors, dropped).
pub fn build_training_rows_from_jsonl_bytes(
    file_name: &str,
    data: &[u8],
    dataset_id: &str,
    dataset_version_id: &str,
    tenant_id: Option<&str>,
    created_by: Option<&str>,
    source_type: Option<&str>,
) -> (Vec<CreateTrainingDatasetRowParams>, usize, usize) {
    let mut rows = Vec::new();
    let mut parse_errors = 0usize;
    let mut dropped = 0usize;
    let default_split = "train";

    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(_) => {
            return (rows, 1, 0);
        }
    };

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                parse_errors += 1;
                continue;
            }
        };

        let Some(object) = value.as_object() else {
            dropped += 1;
            continue;
        };

        let metadata_obj = object.get("metadata").and_then(|v| v.as_object());

        // Accept a few common JSONL dialects:
        // - OpenAI fine-tune: {prompt, completion}
        // - AdapterOS UI + training: {prompt, response} / {input, target}
        // - Raw: {text}
        let prompt = object
            .get("prompt")
            .or_else(|| object.get("input"))
            .or_else(|| object.get("question"))
            .or_else(|| object.get("text"))
            .or_else(|| metadata_obj.and_then(|m| m.get("prompt")))
            .or_else(|| metadata_obj.and_then(|m| m.get("input")))
            .or_else(|| metadata_obj.and_then(|m| m.get("question")))
            .or_else(|| metadata_obj.and_then(|m| m.get("text")))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let response = object
            .get("response")
            .or_else(|| object.get("target"))
            .or_else(|| object.get("output"))
            .or_else(|| object.get("answer"))
            .or_else(|| object.get("completion"))
            .or_else(|| metadata_obj.and_then(|m| m.get("response")))
            .or_else(|| metadata_obj.and_then(|m| m.get("target")))
            .or_else(|| metadata_obj.and_then(|m| m.get("output")))
            .or_else(|| metadata_obj.and_then(|m| m.get("answer")))
            .or_else(|| metadata_obj.and_then(|m| m.get("completion")))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_default();

        let Some(prompt) = prompt else {
            dropped += 1;
            continue;
        };

        let source_line = i32::try_from(line_idx + 1).ok();

        let split = extract_string_field(object, metadata_obj, "split")
            .unwrap_or_else(|| default_split.to_string());

        let weight = object
            .get("weight")
            .or_else(|| metadata_obj.and_then(|m| m.get("weight")))
            .and_then(|v| match v {
                Value::Number(num) => num.as_f64(),
                Value::String(text) => text.trim().parse::<f64>().ok(),
                _ => None,
            })
            .unwrap_or(1.0);

        let sample_role = object
            .get("sample_role")
            .or_else(|| object.get("role"))
            .or_else(|| metadata_obj.and_then(|m| m.get("sample_role")))
            .or_else(|| metadata_obj.and_then(|m| m.get("role")))
            .and_then(|v| v.as_str())
            .and_then(SampleRole::from_str)
            .unwrap_or_else(|| {
                if weight.is_sign_negative() {
                    SampleRole::Negative
                } else {
                    SampleRole::Positive
                }
            });

        let metadata_json = metadata_obj.and_then(|m| {
            if m.is_empty() {
                None
            } else {
                serde_json::to_string(m).ok()
            }
        });

        let mut builder = CreateTrainingDatasetRowParams::builder(dataset_id, prompt, response)
            .dataset_version_id(dataset_version_id)
            .weight(weight)
            .split(split)
            .sample_role(sample_role)
            .source_file(file_name);

        if let Some(source_line) = source_line {
            builder = builder.source_line(source_line);
        }

        if let Some(source_type) = source_type {
            builder = builder.source_type(source_type);
        }
        if let Some(tenant_id) = tenant_id {
            builder = builder.tenant_id(tenant_id);
        }
        if let Some(created_by) = created_by {
            builder = builder.created_by(created_by);
        }
        if let Some(metadata_json) = metadata_json {
            builder = builder.metadata_json(metadata_json);
        }

        rows.push(builder.build());
    }

    (rows, parse_errors, dropped)
}

fn extract_string_field(
    object: &Map<String, Value>,
    metadata: Option<&Map<String, Value>>,
    key: &str,
) -> Option<String> {
    object
        .get(key)
        .or_else(|| metadata.and_then(|m| m.get(key)))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn extract_i32_field(
    object: &Map<String, Value>,
    metadata: Option<&Map<String, Value>>,
    key: &str,
) -> Option<i32> {
    let value = object
        .get(key)
        .or_else(|| metadata.and_then(|m| m.get(key)))?;
    match value {
        Value::Number(num) => num.as_i64().and_then(|v| i32::try_from(v).ok()),
        Value::String(text) => text.trim().parse::<i32>().ok(),
        _ => None,
    }
}

fn extract_bool_field(
    object: &Map<String, Value>,
    metadata: Option<&Map<String, Value>>,
    key: &str,
) -> Option<bool> {
    let value = object
        .get(key)
        .or_else(|| metadata.and_then(|m| m.get(key)))?;
    match value {
        Value::Bool(value) => Some(*value),
        Value::Number(num) => num.as_i64().map(|v| v != 0),
        Value::String(text) => {
            let normalized = text.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "true" | "1" | "yes" | "y" => Some(true),
                "false" | "0" | "no" | "n" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Build codebase dataset rows from JSONL bytes.
///
/// Returns a tuple of (rows, parse_errors, dropped).
pub fn build_codebase_rows_from_jsonl_bytes(
    _file_name: &str,
    data: &[u8],
) -> (Vec<CodebaseDatasetRowInput>, usize, usize) {
    let mut rows = Vec::new();
    let mut parse_errors = 0usize;
    let mut dropped = 0usize;

    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(_) => {
            return (rows, 1, 0);
        }
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                parse_errors += 1;
                continue;
            }
        };

        let Some(object) = value.as_object() else {
            dropped += 1;
            continue;
        };

        let prompt = object
            .get("prompt")
            .or_else(|| object.get("input"))
            .or_else(|| object.get("question"))
            .or_else(|| object.get("text"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string());

        let response = object
            .get("response")
            .or_else(|| object.get("output"))
            .or_else(|| object.get("answer"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string());

        let (Some(prompt), Some(response)) = (prompt, response) else {
            dropped += 1;
            continue;
        };

        let metadata_obj = object.get("metadata").and_then(|v| v.as_object());

        let weight = object.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);

        let sample_role = object
            .get("sample_role")
            .or_else(|| object.get("role"))
            .or_else(|| metadata_obj.and_then(|m| m.get("sample_role")))
            .or_else(|| metadata_obj.and_then(|m| m.get("role")))
            .and_then(|v| v.as_str())
            .and_then(SampleRole::from_str)
            .unwrap_or_else(|| {
                if weight.is_sign_negative() {
                    SampleRole::Negative
                } else {
                    SampleRole::Positive
                }
            });

        let symbol_kind = extract_string_field(object, metadata_obj, "symbol_kind");
        let language = extract_string_field(object, metadata_obj, "language");
        let file_path = extract_string_field(object, metadata_obj, "file_path");
        let qualified_name = extract_string_field(object, metadata_obj, "qualified_name");
        let start_line = extract_i32_field(object, metadata_obj, "start_line");
        let end_line = extract_i32_field(object, metadata_obj, "end_line");
        let has_docstring = extract_bool_field(object, metadata_obj, "has_docstring")
            .or_else(|| extract_bool_field(object, metadata_obj, "docstring_present"))
            .unwrap_or(false);

        let metadata_json = metadata_obj.and_then(|m| {
            if m.is_empty() {
                None
            } else {
                serde_json::to_string(m).ok()
            }
        });

        rows.push(CodebaseDatasetRowInput {
            prompt,
            response,
            weight,
            sample_role,
            symbol_kind,
            language,
            file_path,
            start_line,
            end_line,
            qualified_name,
            has_docstring,
            metadata_json,
        });
    }

    (rows, parse_errors, dropped)
}

fn metadata_map_from_json(metadata_json: Option<&str>) -> Map<String, Value> {
    let Some(raw) = metadata_json else {
        return Map::new();
    };

    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn compute_training_dataset_row_id(
    params: &CreateTrainingDatasetRowParams,
    dataset_hash_b3: &str,
) -> String {
    let meta = metadata_map_from_json(params.metadata_json.as_deref());
    let meta_bytes = serde_json::to_vec(&meta).unwrap_or_default();
    let weight = params.weight as f32;
    // Namespace row IDs by dataset hash to prevent collisions across datasets.
    B3Hash::hash_multi(&[
        dataset_hash_b3.as_bytes(),
        params.dataset_id.as_bytes(),
        params.prompt.as_bytes(),
        params.response.as_bytes(),
        &meta_bytes,
        params.split.as_bytes(),
        &weight.to_be_bytes(),
    ])
    .to_hex()
}

fn compute_training_dataset_content_hash(params: &CreateTrainingDatasetRowParams) -> String {
    let hash_input = format!("{}:{}:{}", params.prompt, params.response, params.weight);
    blake3::hash(hash_input.as_bytes()).to_hex().to_string()
}

fn merge_training_config_hash_metadata(
    metadata_json: Option<&str>,
    training_config_hash: &str,
) -> Option<String> {
    let trimmed_hash = training_config_hash.trim();
    if trimmed_hash.is_empty() {
        return metadata_json.map(str::to_string);
    }

    let mut map = match metadata_json {
        Some(raw) if !raw.trim().is_empty() => match serde_json::from_str::<Value>(raw) {
            Ok(Value::Object(map)) => map,
            _ => return Some(raw.to_string()),
        },
        _ => Map::new(),
    };

    map.entry("training_config_hash".to_string())
        .or_insert_with(|| Value::String(trimmed_hash.to_string()));

    serde_json::to_string(&Value::Object(map))
        .ok()
        .or_else(|| metadata_json.map(str::to_string))
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

impl CreateCodebaseDatasetRowParams {
    /// Build a codebase dataset row from a streaming sample payload.
    pub fn from_stream_sample(
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        session_id: Option<&str>,
        prompt: &str,
        response: &str,
        weight: f64,
        metadata: &HashMap<String, String>,
        repo_name: Option<&str>,
        repo_slug: Option<&str>,
        repo_identifier: Option<&str>,
        project_name: Option<&str>,
        commit_sha: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Self {
        let sample_role = metadata
            .get("sample_role")
            .and_then(|role| SampleRole::from_str(role))
            .unwrap_or(SampleRole::Positive);

        let docstring_present = metadata
            .get("docstring_present")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "y"))
            .unwrap_or(false);

        let metadata_json = if metadata.is_empty() {
            None
        } else {
            serde_json::to_string(metadata).ok()
        };

        let commit_sha = commit_sha
            .map(|v| v.to_string())
            .or_else(|| metadata.get("repo_commit").cloned());

        let repo_slug = repo_slug
            .map(|v| v.to_string())
            .or_else(|| metadata.get("repo_slug").cloned());

        let project_name = project_name
            .map(|v| v.to_string())
            .or_else(|| metadata.get("project").cloned());

        let repo_identifier = repo_identifier
            .map(|v| v.to_string())
            .or_else(|| metadata.get("repo_identifier").cloned())
            .or_else(|| metadata.get("scope_repo_id").cloned())
            .or_else(|| metadata.get("repo_id").cloned());

        Self {
            dataset_id: dataset_id.to_string(),
            dataset_version_id: dataset_version_id.map(|v| v.to_string()),
            session_id: session_id.map(|v| v.to_string()),
            prompt: prompt.to_string(),
            response: response.to_string(),
            weight,
            sample_role,
            symbol_kind: metadata.get("symbol_kind").cloned(),
            language: metadata.get("language").cloned(),
            file_path: metadata.get("file_path").cloned(),
            start_line: metadata
                .get("start_line")
                .and_then(|v| v.parse::<i32>().ok()),
            end_line: metadata.get("end_line").and_then(|v| v.parse::<i32>().ok()),
            qualified_name: metadata.get("qualified_name").cloned(),
            commit_sha,
            repo_name: repo_name.map(|v| v.to_string()),
            repo_slug,
            repo_identifier,
            project_name,
            has_docstring: docstring_present,
            metadata_json,
            tenant_id: tenant_id.map(|v| v.to_string()),
        }
    }
}

/// Input payload for building codebase dataset rows for an ingestion run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseDatasetRowInput {
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
    pub has_docstring: bool,
    pub metadata_json: Option<String>,
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
    // Training Dataset Row Operations
    // ============================================================================

    async fn fetch_dataset_hash_b3(&self, dataset_id: &str) -> Result<String> {
        let hash: Option<String> = sqlx::query_scalar(
            "SELECT COALESCE(dataset_hash_b3, hash_b3) FROM training_datasets WHERE id = ?",
        )
        .bind(dataset_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db_err("fetch dataset hash"))?;

        hash.ok_or_else(|| AosError::Validation("dataset not found".to_string()))
    }

    /// Insert a single training dataset row.
    ///
    /// The content_hash_b3 is computed from prompt, response, and weight to
    /// enable deduplication checks.
    pub async fn insert_training_dataset_row(
        &self,
        params: &CreateTrainingDatasetRowParams,
    ) -> Result<String> {
        let dataset_hash_b3 = self.fetch_dataset_hash_b3(&params.dataset_id).await?;
        let id = params
            .row_id
            .clone()
            .unwrap_or_else(|| compute_training_dataset_row_id(params, &dataset_hash_b3));
        let content_hash_b3 = compute_training_dataset_content_hash(params);

        sqlx::query(
            "INSERT OR IGNORE INTO training_dataset_rows (
                id, dataset_id, dataset_version_id, session_id, prompt, response, weight,
                split, sample_role, content_hash_b3, source_type, source_file, source_line,
                tenant_id, metadata_json, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.dataset_id)
        .bind(&params.dataset_version_id)
        .bind(&params.session_id)
        .bind(&params.prompt)
        .bind(&params.response)
        .bind(params.weight)
        .bind(&params.split)
        .bind(params.sample_role.as_str())
        .bind(&content_hash_b3)
        .bind(&params.source_type)
        .bind(&params.source_file)
        .bind(params.source_line)
        .bind(&params.tenant_id)
        .bind(&params.metadata_json)
        .bind(&params.created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("insert training dataset row"))?;

        Ok(id)
    }

    /// Bulk insert training dataset rows for deterministic runs.
    pub async fn bulk_insert_training_dataset_rows(
        &self,
        rows: &[CreateTrainingDatasetRowParams],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        let mut dataset_hashes: HashMap<String, String> = HashMap::new();
        for params in rows {
            if !dataset_hashes.contains_key(&params.dataset_id) {
                let hash = self.fetch_dataset_hash_b3(&params.dataset_id).await?;
                dataset_hashes.insert(params.dataset_id.clone(), hash);
            }
        }

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(db_err("begin training_dataset_rows transaction"))?;

        let mut count = 0;
        for params in rows {
            let dataset_hash_b3 = dataset_hashes.get(&params.dataset_id).ok_or_else(|| {
                AosError::Validation(format!(
                    "dataset hash missing for dataset_id {}",
                    params.dataset_id
                ))
            })?;
            let id = params
                .row_id
                .clone()
                .unwrap_or_else(|| compute_training_dataset_row_id(params, dataset_hash_b3));
            let content_hash_b3 = compute_training_dataset_content_hash(params);

            let result = sqlx::query(
                "INSERT OR IGNORE INTO training_dataset_rows (
                    id, dataset_id, dataset_version_id, session_id, prompt, response, weight,
                    split, sample_role, content_hash_b3, source_type, source_file, source_line,
                    tenant_id, metadata_json, created_by
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&params.dataset_id)
            .bind(&params.dataset_version_id)
            .bind(&params.session_id)
            .bind(&params.prompt)
            .bind(&params.response)
            .bind(params.weight)
            .bind(&params.split)
            .bind(params.sample_role.as_str())
            .bind(&content_hash_b3)
            .bind(&params.source_type)
            .bind(&params.source_file)
            .bind(params.source_line)
            .bind(&params.tenant_id)
            .bind(&params.metadata_json)
            .bind(&params.created_by)
            .execute(&mut *tx)
            .await
            .map_err(db_err("bulk insert training dataset row"))?;

            count += result.rows_affected() as usize;
        }

        tx.commit()
            .await
            .map_err(db_err("commit training_dataset_rows transaction"))?;
        Ok(count)
    }

    /// List training dataset rows deterministically (content hash ASC, id ASC).
    pub async fn list_training_dataset_rows(
        &self,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TrainingDatasetRow>> {
        let rows = if let Some(version_id) = dataset_version_id {
            sqlx::query_as::<_, TrainingDatasetRow>(&format!(
                "SELECT {} FROM training_dataset_rows \
                 WHERE dataset_id = ? AND dataset_version_id = ? \
                 ORDER BY content_hash_b3 ASC, id ASC LIMIT ? OFFSET ?",
                TRAINING_DATASET_ROW_COLUMNS
            ))
            .bind(dataset_id)
            .bind(version_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list training dataset rows by version"))?
        } else {
            sqlx::query_as::<_, TrainingDatasetRow>(&format!(
                "SELECT {} FROM training_dataset_rows \
                 WHERE dataset_id = ? \
                 ORDER BY content_hash_b3 ASC, id ASC LIMIT ? OFFSET ?",
                TRAINING_DATASET_ROW_COLUMNS
            ))
            .bind(dataset_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list training dataset rows"))?
        };

        Ok(rows)
    }

    /// Count training dataset rows, optionally scoped to a dataset version.
    pub async fn count_training_dataset_rows(
        &self,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
    ) -> Result<i64> {
        let count = if let Some(version_id) = dataset_version_id {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM training_dataset_rows WHERE dataset_id = ? AND dataset_version_id = ?",
            )
            .bind(dataset_id)
            .bind(version_id)
            .fetch_one(self.pool())
            .await
            .map_err(db_err("count training dataset rows by version"))?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM training_dataset_rows WHERE dataset_id = ?")
                .bind(dataset_id)
                .fetch_one(self.pool())
                .await
                .map_err(db_err("count training dataset rows"))?
        };

        Ok(count)
    }

    // ============================================================================
    // Codebase Dataset Row Operations
    // ============================================================================

    async fn fetch_dataset_repo_slug(&self, dataset_id: &str) -> Result<Option<String>> {
        let repo_slug: Option<Option<String>> =
            sqlx::query_scalar("SELECT repo_slug FROM training_datasets WHERE id = ?")
                .bind(dataset_id)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err("fetch dataset repo_slug"))?;

        Ok(repo_slug.flatten())
    }

    async fn fetch_dataset_source_location(&self, dataset_id: &str) -> Result<Option<String>> {
        let source_location: Option<Option<String>> =
            sqlx::query_scalar("SELECT source_location FROM training_datasets WHERE id = ?")
                .bind(dataset_id)
                .fetch_optional(self.pool())
                .await
                .map_err(db_err("fetch dataset source_location"))?;

        Ok(source_location.flatten())
    }

    /// Insert codebase dataset rows for a single ingestion run.
    ///
    /// Builds row parameters from a shared run context and bulk-inserts them
    /// into the `codebase_dataset_rows` table.
    pub async fn insert_codebase_dataset_rows_for_run(
        &self,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        session_id: Option<&str>,
        repo_name: Option<&str>,
        repo_slug: Option<&str>,
        repo_identifier: Option<&str>,
        project_name: Option<&str>,
        commit_sha: Option<&str>,
        rows: &[CodebaseDatasetRowInput],
        tenant_id: Option<&str>,
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        let repo_name = sanitize_optional(repo_name);
        let repo_slug = sanitize_repo_slug(repo_slug).map(|slug| normalize_repo_slug(&slug));
        let repo_identifier = sanitize_repo_identifier(repo_identifier);
        let project_name = sanitize_optional(project_name);
        let commit_sha = sanitize_optional(commit_sha);
        let session_id = sanitize_optional(session_id);
        let tenant_id = tenant_id.map(str::to_string);

        let params: Vec<CreateCodebaseDatasetRowParams> = rows
            .iter()
            .map(|row| CreateCodebaseDatasetRowParams {
                dataset_id: dataset_id.to_string(),
                dataset_version_id: dataset_version_id.map(str::to_string),
                session_id: session_id.clone(),
                prompt: row.prompt.clone(),
                response: row.response.clone(),
                weight: row.weight,
                sample_role: row.sample_role,
                symbol_kind: row.symbol_kind.clone(),
                language: row.language.clone(),
                file_path: row.file_path.clone(),
                start_line: row.start_line,
                end_line: row.end_line,
                qualified_name: row.qualified_name.clone(),
                commit_sha: commit_sha.clone(),
                repo_name: repo_name.clone(),
                repo_slug: repo_slug.clone(),
                repo_identifier: repo_identifier.clone(),
                project_name: project_name.clone(),
                has_docstring: row.has_docstring,
                metadata_json: row.metadata_json.clone(),
                tenant_id: tenant_id.clone(),
            })
            .collect();

        self.bulk_insert_codebase_dataset_rows(&params).await
    }

    /// Insert codebase dataset rows with a training config hash applied to metadata.
    pub async fn insert_codebase_dataset_rows_for_training_config(
        &self,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        session_id: Option<&str>,
        repo_name: Option<&str>,
        repo_slug: Option<&str>,
        repo_identifier: Option<&str>,
        project_name: Option<&str>,
        commit_sha: Option<&str>,
        training_config_hash: &str,
        rows: &[CodebaseDatasetRowInput],
        tenant_id: Option<&str>,
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        if training_config_hash.trim().is_empty() {
            return self
                .insert_codebase_dataset_rows_for_run(
                    dataset_id,
                    dataset_version_id,
                    session_id,
                    repo_name,
                    repo_slug,
                    repo_identifier,
                    project_name,
                    commit_sha,
                    rows,
                    tenant_id,
                )
                .await;
        }

        let enriched_rows: Vec<CodebaseDatasetRowInput> = rows
            .iter()
            .cloned()
            .map(|mut row| {
                row.metadata_json = merge_training_config_hash_metadata(
                    row.metadata_json.as_deref(),
                    training_config_hash,
                );
                row
            })
            .collect();

        self.insert_codebase_dataset_rows_for_run(
            dataset_id,
            dataset_version_id,
            session_id,
            repo_name,
            repo_slug,
            repo_identifier,
            project_name,
            commit_sha,
            &enriched_rows,
            tenant_id,
        )
        .await
    }

    /// Insert a single codebase dataset row.
    ///
    /// The content_hash_b3 is computed from prompt, response, and weight to
    /// enable deduplication checks.
    pub async fn insert_codebase_dataset_row(
        &self,
        params: &CreateCodebaseDatasetRowParams,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Dst);

        // Compute content hash for deduplication
        let hash_input = format!("{}:{}:{}", params.prompt, params.response, params.weight);
        let content_hash = blake3::hash(hash_input.as_bytes());
        let content_hash_b3 = content_hash.to_hex().to_string();

        let mut repo_slug =
            sanitize_repo_slug(params.repo_slug.as_deref()).map(|slug| normalize_repo_slug(&slug));
        if repo_slug.is_none() {
            let dataset_slug = self.fetch_dataset_repo_slug(&params.dataset_id).await?;
            repo_slug =
                sanitize_repo_slug(dataset_slug.as_deref()).map(|slug| normalize_repo_slug(&slug));
        }
        if repo_slug.is_none() {
            if let Some(repo_name) = params.repo_name.as_deref() {
                repo_slug = Some(normalize_repo_slug(repo_name));
            }
        }

        let mut repo_identifier = sanitize_repo_identifier(params.repo_identifier.as_deref())
            .or_else(|| extract_repo_identifier_from_metadata(params.metadata_json.as_deref()));

        if repo_identifier.is_none() {
            let source_location = self
                .fetch_dataset_source_location(&params.dataset_id)
                .await?;
            repo_identifier = sanitize_repo_identifier(source_location.as_deref());
        }

        if repo_identifier.is_none() {
            if let Some(slug) = repo_slug.as_deref() {
                repo_identifier = Some(normalize_repo_id(&format!("repo:{}", slug)));
            } else if let Some(repo_name) = params.repo_name.as_deref() {
                let slug = normalize_repo_slug(repo_name);
                repo_identifier = Some(normalize_repo_id(&format!("repo:{}", slug)));
            }
        }

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
        .bind(&repo_slug)
        .bind(&repo_identifier)
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

        let mut dataset_repo_slugs: HashMap<String, Option<String>> = HashMap::new();
        let mut dataset_source_locations: HashMap<String, Option<String>> = HashMap::new();
        for params in rows {
            if sanitize_repo_slug(params.repo_slug.as_deref()).is_none() {
                dataset_repo_slugs
                    .entry(params.dataset_id.clone())
                    .or_insert(None);
            }
            let repo_identifier_missing =
                sanitize_repo_identifier(params.repo_identifier.as_deref()).is_none()
                    && extract_repo_identifier_from_metadata(params.metadata_json.as_deref())
                        .is_none();
            if repo_identifier_missing {
                dataset_source_locations
                    .entry(params.dataset_id.clone())
                    .or_insert(None);
            }
        }

        if !dataset_repo_slugs.is_empty() {
            let dataset_ids: Vec<String> = dataset_repo_slugs.keys().cloned().collect();
            for dataset_id in dataset_ids {
                let dataset_slug = self.fetch_dataset_repo_slug(&dataset_id).await?;
                dataset_repo_slugs.insert(
                    dataset_id,
                    sanitize_repo_slug(dataset_slug.as_deref())
                        .map(|slug| normalize_repo_slug(&slug)),
                );
            }
        }

        if !dataset_source_locations.is_empty() {
            let dataset_ids: Vec<String> = dataset_source_locations.keys().cloned().collect();
            for dataset_id in dataset_ids {
                let source_location = self.fetch_dataset_source_location(&dataset_id).await?;
                dataset_source_locations.insert(dataset_id, source_location);
            }
        }

        // Use a transaction for atomic insertion
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(db_err("begin transaction"))?;
        let mut count = 0;

        for params in rows {
            let id = new_id(IdPrefix::Dst);

            // Compute content hash for deduplication
            let hash_input = format!("{}:{}:{}", params.prompt, params.response, params.weight);
            let content_hash = blake3::hash(hash_input.as_bytes());
            let content_hash_b3 = content_hash.to_hex().to_string();

            let mut repo_slug = sanitize_repo_slug(params.repo_slug.as_deref())
                .map(|slug| normalize_repo_slug(&slug));
            if repo_slug.is_none() {
                if let Some(slug) = dataset_repo_slugs.get(&params.dataset_id) {
                    repo_slug = slug.clone();
                }
            }
            if repo_slug.is_none() {
                if let Some(repo_name) = params.repo_name.as_deref() {
                    repo_slug = Some(normalize_repo_slug(repo_name));
                }
            }

            let mut repo_identifier = sanitize_repo_identifier(params.repo_identifier.as_deref())
                .or_else(|| extract_repo_identifier_from_metadata(params.metadata_json.as_deref()));
            if repo_identifier.is_none() {
                if let Some(source_location) = dataset_source_locations
                    .get(&params.dataset_id)
                    .and_then(|location| location.as_deref())
                {
                    repo_identifier = sanitize_repo_identifier(Some(source_location));
                }
            }
            if repo_identifier.is_none() {
                if let Some(slug) = repo_slug.as_deref() {
                    repo_identifier = Some(normalize_repo_id(&format!("repo:{}", slug)));
                } else if let Some(repo_name) = params.repo_name.as_deref() {
                    let slug = normalize_repo_slug(repo_name);
                    repo_identifier = Some(normalize_repo_id(&format!("repo:{}", slug)));
                }
            }

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
            .bind(&repo_slug)
            .bind(&repo_identifier)
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
    pub async fn get_codebase_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>> {
        #[allow(clippy::type_complexity)]
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
        #[allow(clippy::type_complexity)]
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

    /// List codebase dataset rows by repository slug.
    ///
    /// Returns all training examples associated with a specific repository slug,
    /// useful for finding all rows from a particular slugged repo (e.g., "org/repo-name").
    ///
    /// # Arguments
    /// * `repo_slug` - The repository slug to filter by (e.g., "org/repo-name")
    /// * `limit` - Maximum number of rows to return
    /// * `offset` - Number of rows to skip for pagination
    ///
    /// # Returns
    /// Vector of codebase dataset rows with matching repo_slug, ordered by creation date (newest first)
    pub async fn list_codebase_dataset_rows_by_repo_slug(
        &self,
        repo_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows \
             WHERE repo_slug = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(repo_slug)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list codebase dataset rows by repo_slug"))?;

        Ok(rows)
    }

    /// List codebase dataset rows by repository slug for a specific tenant.
    ///
    /// Returns training examples associated with a repository slug within a tenant,
    /// enforcing tenant isolation for multi-tenant deployments.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    /// * `repo_slug` - The repository slug to filter by (e.g., "org/repo-name")
    /// * `limit` - Maximum number of rows to return
    /// * `offset` - Number of rows to skip for pagination
    ///
    /// # Returns
    /// Vector of codebase dataset rows with matching repo_slug and tenant_id
    pub async fn list_codebase_dataset_rows_by_repo_slug_for_tenant(
        &self,
        tenant_id: &str,
        repo_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows \
             WHERE tenant_id = ? AND repo_slug = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(tenant_id)
        .bind(repo_slug)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("list codebase dataset rows by repo_slug for tenant"))?;

        Ok(rows)
    }

    /// Count codebase dataset rows by repository slug.
    ///
    /// Returns the total number of training examples associated with a repository slug,
    /// optionally filtered by tenant.
    ///
    /// # Arguments
    /// * `repo_slug` - The repository slug to filter by (e.g., "org/repo-name")
    /// * `tenant_id` - Optional tenant ID for tenant-scoped counting
    ///
    /// # Returns
    /// Count of matching rows
    pub async fn count_codebase_dataset_rows_by_repo_slug(
        &self,
        repo_slug: &str,
        tenant_id: Option<&str>,
    ) -> Result<i64> {
        let count: (i64,) = if let Some(tid) = tenant_id {
            sqlx::query_as(
                "SELECT COUNT(*) FROM codebase_dataset_rows \
                 WHERE tenant_id = ? AND repo_slug = ?",
            )
            .bind(tid)
            .bind(repo_slug)
            .fetch_one(self.pool())
            .await
            .map_err(db_err(
                "count codebase dataset rows by repo_slug for tenant",
            ))?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM codebase_dataset_rows WHERE repo_slug = ?")
                .bind(repo_slug)
                .fetch_one(self.pool())
                .await
                .map_err(db_err("count codebase dataset rows by repo_slug"))?
        };
        Ok(count.0)
    }

    /// List codebase dataset rows by repository slug and dataset ID.
    ///
    /// Returns training examples for a specific repository slug within a dataset,
    /// useful for filtering rows when a dataset contains data from multiple repos.
    ///
    /// # Arguments
    /// * `dataset_id` - The dataset ID to filter by
    /// * `repo_slug` - The repository slug to filter by (e.g., "org/repo-name")
    /// * `limit` - Maximum number of rows to return
    /// * `offset` - Number of rows to skip for pagination
    ///
    /// # Returns
    /// Vector of codebase dataset rows with matching dataset_id and repo_slug
    pub async fn list_codebase_dataset_rows_by_dataset_and_repo_slug(
        &self,
        dataset_id: &str,
        repo_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CodebaseDatasetRow>> {
        let rows = sqlx::query_as::<_, CodebaseDatasetRow>(&format!(
            "SELECT {} FROM codebase_dataset_rows \
             WHERE dataset_id = ? AND repo_slug = ? \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
            CODEBASE_DATASET_ROW_COLUMNS
        ))
        .bind(dataset_id)
        .bind(repo_slug)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(db_err(
            "list codebase dataset rows by dataset and repo_slug",
        ))?;

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

/// Input metadata for creating scan-root dataset records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRootRunInput {
    /// Absolute or relative path to the scan root directory
    pub path: String,
    /// Optional label describing this scan root's role
    pub label: Option<String>,
    /// Number of files processed from this scan root
    pub file_count: Option<u64>,
    /// Total bytes ingested from this scan root
    pub byte_count: Option<u64>,
    /// BLAKE3 hash of the scan root's content at ingestion time
    pub content_hash_b3: Option<String>,
    /// Timestamp when this scan root was processed
    pub scanned_at: Option<String>,
    /// Optional ordering for scan roots
    pub ordinal: Option<i32>,
    /// Additional metadata as JSON
    pub metadata_json: Option<String>,
}

// ============================================================================
// Adapter Training Lineage Operations (migration 0258)
// ============================================================================

impl Db {
    /// Record adapter training lineage (called from training completion)
    ///
    /// Creates an entry in adapter_training_lineage to establish provenance
    /// between an adapter and the dataset version it was trained on.
    pub async fn record_adapter_training_lineage(
        &self,
        adapter_id: &str,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        training_job_id: Option<&str>,
        dataset_hash_b3: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Dst);

        sqlx::query(
            "INSERT INTO adapter_training_lineage (
                id, adapter_id, dataset_id, dataset_version_id, training_job_id,
                dataset_hash_b3_at_training, role, ordinal, tenant_id
            ) VALUES (?, ?, ?, ?, ?, ?, 'primary', 0, ?)
            ON CONFLICT(adapter_id, dataset_id, dataset_version_id) DO UPDATE SET
                training_job_id = COALESCE(excluded.training_job_id, adapter_training_lineage.training_job_id),
                dataset_hash_b3_at_training = COALESCE(excluded.dataset_hash_b3_at_training, adapter_training_lineage.dataset_hash_b3_at_training)",
        )
        .bind(&id)
        .bind(adapter_id)
        .bind(dataset_id)
        .bind(dataset_version_id)
        .bind(training_job_id)
        .bind(dataset_hash_b3)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(db_err("record adapter training lineage"))?;

        Ok(id)
    }

    /// Get all adapters trained on a specific dataset version
    pub async fn get_adapters_for_dataset_version(
        &self,
        dataset_version_id: &str,
    ) -> Result<Vec<AdapterTrainingLineage>> {
        use crate::constants::ADAPTER_TRAINING_LINEAGE_COLUMNS;

        let query = format!(
            "SELECT {} FROM adapter_training_lineage WHERE dataset_version_id = ? ORDER BY created_at DESC",
            ADAPTER_TRAINING_LINEAGE_COLUMNS
        );

        let lineage = sqlx::query_as::<_, AdapterTrainingLineage>(&query)
            .bind(dataset_version_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("get adapters for dataset version"))?;

        Ok(lineage)
    }

    /// Get all training lineage records for a specific dataset (any version)
    pub async fn get_training_lineage_for_dataset(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<AdapterTrainingLineage>> {
        use crate::constants::ADAPTER_TRAINING_LINEAGE_COLUMNS;

        let query = format!(
            "SELECT {} FROM adapter_training_lineage WHERE dataset_id = ? ORDER BY created_at DESC",
            ADAPTER_TRAINING_LINEAGE_COLUMNS
        );

        let lineage = sqlx::query_as::<_, AdapterTrainingLineage>(&query)
            .bind(dataset_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("get training lineage for dataset"))?;

        Ok(lineage)
    }

    /// Get lineage for a specific adapter
    pub async fn get_lineage_for_adapter(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<AdapterTrainingLineage>> {
        use crate::constants::ADAPTER_TRAINING_LINEAGE_COLUMNS;

        let query = format!(
            "SELECT {} FROM adapter_training_lineage WHERE adapter_id = ? ORDER BY ordinal",
            ADAPTER_TRAINING_LINEAGE_COLUMNS
        );

        let lineage = sqlx::query_as::<_, AdapterTrainingLineage>(&query)
            .bind(adapter_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("get lineage for adapter"))?;

        Ok(lineage)
    }

    /// Get all datasets in a session
    pub async fn get_datasets_by_session(
        &self,
        session_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<Vec<TrainingDataset>> {
        use crate::constants::TRAINING_DATASET_COLUMNS;

        let query = if tenant_id.is_some() {
            format!(
                "SELECT {} FROM training_datasets WHERE session_id = ? AND tenant_id = ? ORDER BY created_at DESC",
                TRAINING_DATASET_COLUMNS
            )
        } else {
            format!(
                "SELECT {} FROM training_datasets WHERE session_id = ? ORDER BY created_at DESC",
                TRAINING_DATASET_COLUMNS
            )
        };

        let mut query_builder = sqlx::query_as::<_, TrainingDataset>(&query).bind(session_id);

        if let Some(tid) = tenant_id {
            query_builder = query_builder.bind(tid);
        }

        let datasets = query_builder
            .fetch_all(self.pool())
            .await
            .map_err(db_err("get datasets by session"))?;

        Ok(datasets)
    }

    /// Get datasets by scope (repo, branch, commit)
    ///
    /// Supports partial matching - all provided parameters must match.
    pub async fn get_datasets_by_scope(
        &self,
        repo_slug: Option<&str>,
        branch: Option<&str>,
        commit_sha: Option<&str>,
        tenant_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        use crate::constants::TRAINING_DATASET_COLUMNS;

        let mut conditions = vec!["1=1".to_string()];
        let mut params: Vec<String> = vec![];

        if let Some(repo) = repo_slug {
            conditions.push("repo_slug = ?".to_string());
            params.push(repo.to_string());
        }
        if let Some(br) = branch {
            conditions.push("branch = ?".to_string());
            params.push(br.trim().to_string());
        }
        if let Some(commit) = commit_sha {
            conditions.push("commit_sha = ?".to_string());
            params.push(commit.trim().to_string());
        }
        if let Some(tid) = tenant_id {
            conditions.push("tenant_id = ?".to_string());
            params.push(tid.to_string());
        }

        let query = format!(
            "SELECT {} FROM training_datasets WHERE {} ORDER BY created_at DESC LIMIT ?",
            TRAINING_DATASET_COLUMNS,
            conditions.join(" AND ")
        );

        // Build the query with dynamic bindings
        let mut query_builder = sqlx::query_as::<_, TrainingDataset>(&query);
        for param in &params {
            query_builder = query_builder.bind(param);
        }
        query_builder = query_builder.bind(limit);

        let datasets = query_builder
            .fetch_all(self.pool())
            .await
            .map_err(db_err("get datasets by scope"))?;

        Ok(datasets)
    }
}
