//! Training dataset database operations

use crate::constants::TRAINING_DATASET_COLUMNS;
use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

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
    pub storage_path: String,
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
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, storage_path,
                validation_status, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, 'pending', ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(format)
        .bind(hash_b3)
        .bind(storage_path)
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset"))?;
        Ok(id)
    }

    /// Create a new training dataset using a precomputed ID (e.g., to align DB rows with storage paths)
    pub async fn create_training_dataset_with_id(
        &self,
        dataset_id: &str,
        name: &str,
        description: Option<&str>,
        format: &str,
        hash_b3: &str,
        storage_path: &str,
        created_by: Option<&str>,
    ) -> Result<String> {
        sqlx::query(
            "INSERT INTO training_datasets (
                id, name, description, format, hash_b3, storage_path,
                validation_status, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, 'pending', ?)",
        )
        .bind(dataset_id)
        .bind(name)
        .bind(description)
        .bind(format)
        .bind(hash_b3)
        .bind(storage_path)
        .bind(created_by)
        .execute(self.pool())
        .await
        .map_err(db_err("create training dataset"))?;
        Ok(dataset_id.to_string())
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
            "SELECT id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type, created_at
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
    use super::{derive_overall_safety_status, derive_trust_state};

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
