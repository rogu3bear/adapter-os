//! Dataset repository
//!
//! Replaces SQL queries with KV-based operations for dataset management.
//! Implements all CRUD operations, queries, and version management.

use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use crate::kv::indexing::{dataset_indexes, IndexManager};
use crate::models::{DatasetStatisticsKv, DatasetVersionKv, TrainingDatasetKv};
use std::sync::Arc;
use tracing::{error, info};

/// Paginated query result
#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// Dataset repository for KV storage
pub struct DatasetRepository {
    backend: Arc<dyn KvBackend>,
    index_manager: Arc<IndexManager>,
}

impl DatasetRepository {
    /// Create a new dataset repository
    pub fn new(backend: Arc<dyn KvBackend>, index_manager: Arc<IndexManager>) -> Self {
        Self {
            backend,
            index_manager,
        }
    }

    // ============================================================================
    // Training Dataset CRUD Operations
    // ============================================================================

    /// Create a new training dataset
    pub async fn create_dataset(&self, dataset: TrainingDatasetKv) -> Result<String, StorageError> {
        let id = dataset.id.clone();
        let key = dataset.primary_key();

        // Check if dataset already exists
        if self.backend.exists(&key).await? {
            return Err(StorageError::ConflictError(format!(
                "Dataset already exists: {}",
                id
            )));
        }

        // Serialize dataset
        let value = bincode::serialize(&dataset)?;

        // Store dataset
        self.backend.set(&key, value).await?;

        // Update all indexes
        self.update_dataset_indexes(&dataset, None).await?;

        info!(dataset_id = %id, tenant_id = %dataset.tenant_id, "Dataset created");
        Ok(id)
    }

    /// Get a dataset by ID
    pub async fn get_dataset(
        &self,
        tenant_id: &str,
        dataset_id: &str,
    ) -> Result<Option<TrainingDatasetKv>, StorageError> {
        let key = format!("dataset:{}", dataset_id);

        let bytes = match self.backend.get(&key).await? {
            Some(b) => b,
            None => return Ok(None),
        };

        let dataset: TrainingDatasetKv = bincode::deserialize(&bytes)?;

        // Verify tenant ownership
        if dataset.tenant_id != tenant_id {
            return Ok(None);
        }

        Ok(Some(dataset))
    }

    /// Update an existing dataset
    pub async fn update_dataset(&self, dataset: TrainingDatasetKv) -> Result<(), StorageError> {
        let key = dataset.primary_key();

        // Get old dataset for index updates
        let old_bytes = self
            .backend
            .get(&key)
            .await?
            .ok_or_else(|| StorageError::NotFound(dataset.id.clone()))?;

        let old_dataset: TrainingDatasetKv = bincode::deserialize(&old_bytes)?;

        // Serialize new dataset
        let new_value = bincode::serialize(&dataset)?;

        // Update in storage
        self.backend.set(&key, new_value).await?;

        // Update indexes (comparing old vs new values)
        self.update_dataset_indexes(&dataset, Some(&old_dataset))
            .await?;

        info!(dataset_id = %dataset.id, "Dataset updated");
        Ok(())
    }

    /// Delete a dataset
    pub async fn delete_dataset(
        &self,
        tenant_id: &str,
        dataset_id: &str,
    ) -> Result<bool, StorageError> {
        // Get dataset to verify tenant and clean up indexes
        let dataset = match self.get_dataset(tenant_id, dataset_id).await? {
            Some(d) => d,
            None => return Ok(false),
        };

        let key = dataset.primary_key();

        // Delete from storage
        let deleted = self.backend.delete(&key).await?;

        if deleted {
            // Remove from all indexes
            self.remove_dataset_from_indexes(&dataset).await?;
            info!(dataset_id = %dataset_id, "Dataset deleted");
        }

        Ok(deleted)
    }

    // ============================================================================
    // Dataset Query Operations
    // ============================================================================

    /// List all datasets for a tenant
    pub async fn list_datasets_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<TrainingDatasetKv>, StorageError> {
        let dataset_ids = self
            .index_manager
            .query_index(dataset_indexes::BY_TENANT, tenant_id)
            .await?;

        self.load_datasets(&dataset_ids).await
    }

    /// List datasets by validation status
    pub async fn list_datasets_by_validation_status(
        &self,
        tenant_id: &str,
        status: &str,
    ) -> Result<Vec<TrainingDatasetKv>, StorageError> {
        let dataset_ids = self
            .index_manager
            .query_index(dataset_indexes::BY_VALIDATION_STATUS, status)
            .await?;

        // Filter by tenant
        let datasets = self.load_datasets(&dataset_ids).await?;
        Ok(datasets
            .into_iter()
            .filter(|d| d.tenant_id == tenant_id)
            .collect())
    }

    /// Find dataset by content hash
    pub async fn find_dataset_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<TrainingDatasetKv>, StorageError> {
        let dataset_ids = self
            .index_manager
            .query_index(dataset_indexes::BY_HASH, hash)
            .await?;

        if dataset_ids.is_empty() {
            return Ok(None);
        }

        // Should only be one dataset per hash due to UNIQUE constraint
        let datasets = self.load_datasets(&dataset_ids).await?;
        Ok(datasets.into_iter().next())
    }

    // ============================================================================
    // Dataset Version CRUD Operations
    // ============================================================================

    /// Create a new dataset version
    pub async fn create_version(&self, version: DatasetVersionKv) -> Result<String, StorageError> {
        let id = version.id.clone();
        let key = version.primary_key();

        // Check if version already exists
        if self.backend.exists(&key).await? {
            return Err(StorageError::ConflictError(format!(
                "Dataset version already exists: {}",
                id
            )));
        }

        // Serialize version
        let value = bincode::serialize(&version)?;

        // Store version
        self.backend.set(&key, value).await?;

        // Update all indexes
        self.update_version_indexes(&version, None).await?;

        info!(
            version_id = %id,
            dataset_id = %version.dataset_id,
            tenant_id = %version.tenant_id,
            "Dataset version created"
        );
        Ok(id)
    }

    /// Get a dataset version by ID
    pub async fn get_version(
        &self,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<Option<DatasetVersionKv>, StorageError> {
        let key = format!("dataset_version:{}", version_id);

        let bytes = match self.backend.get(&key).await? {
            Some(b) => b,
            None => return Ok(None),
        };

        let version: DatasetVersionKv = bincode::deserialize(&bytes)?;

        // Verify tenant ownership
        if version.tenant_id != tenant_id {
            return Ok(None);
        }

        Ok(Some(version))
    }

    /// Update an existing dataset version
    pub async fn update_version(&self, version: DatasetVersionKv) -> Result<(), StorageError> {
        let key = version.primary_key();

        // Get old version for index updates
        let old_bytes = self
            .backend
            .get(&key)
            .await?
            .ok_or_else(|| StorageError::NotFound(version.id.clone()))?;

        let old_version: DatasetVersionKv = bincode::deserialize(&old_bytes)?;

        // Serialize new version
        let new_value = bincode::serialize(&version)?;

        // Update in storage
        self.backend.set(&key, new_value).await?;

        // Update indexes (comparing old vs new values)
        self.update_version_indexes(&version, Some(&old_version))
            .await?;

        info!(version_id = %version.id, "Dataset version updated");
        Ok(())
    }

    /// Delete a dataset version
    pub async fn delete_version(
        &self,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<bool, StorageError> {
        // Get version to verify tenant and clean up indexes
        let version = match self.get_version(tenant_id, version_id).await? {
            Some(v) => v,
            None => return Ok(false),
        };

        let key = version.primary_key();

        // Delete from storage
        let deleted = self.backend.delete(&key).await?;

        if deleted {
            // Remove from all indexes
            self.remove_version_from_indexes(&version).await?;
            info!(version_id = %version_id, "Dataset version deleted");
        }

        Ok(deleted)
    }

    // ============================================================================
    // Dataset Version Query Operations
    // ============================================================================

    /// List all versions for a dataset
    pub async fn list_versions_by_dataset(
        &self,
        tenant_id: &str,
        dataset_id: &str,
    ) -> Result<Vec<DatasetVersionKv>, StorageError> {
        let version_ids = self
            .index_manager
            .query_index(dataset_indexes::VERSION_BY_DATASET, dataset_id)
            .await?;

        // Filter by tenant
        let versions = self.load_versions(&version_ids).await?;
        Ok(versions
            .into_iter()
            .filter(|v| v.tenant_id == tenant_id)
            .collect())
    }

    /// List versions by trust state
    pub async fn list_versions_by_trust_state(
        &self,
        tenant_id: &str,
        trust_state: &str,
    ) -> Result<Vec<DatasetVersionKv>, StorageError> {
        let version_ids = self
            .index_manager
            .query_index(dataset_indexes::VERSION_BY_TRUST_STATE, trust_state)
            .await?;

        // Filter by tenant
        let versions = self.load_versions(&version_ids).await?;
        Ok(versions
            .into_iter()
            .filter(|v| v.tenant_id == tenant_id)
            .collect())
    }

    /// List versions by validation status
    pub async fn list_versions_by_validation_status(
        &self,
        tenant_id: &str,
        status: &str,
    ) -> Result<Vec<DatasetVersionKv>, StorageError> {
        let version_ids = self
            .index_manager
            .query_index(dataset_indexes::VERSION_BY_VALIDATION_STATUS, status)
            .await?;

        // Filter by tenant
        let versions = self.load_versions(&version_ids).await?;
        Ok(versions
            .into_iter()
            .filter(|v| v.tenant_id == tenant_id)
            .collect())
    }

    /// Find dataset version by content hash
    pub async fn find_version_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<DatasetVersionKv>, StorageError> {
        let version_ids = self
            .index_manager
            .query_index(dataset_indexes::VERSION_BY_HASH, hash)
            .await?;

        if version_ids.is_empty() {
            return Ok(None);
        }

        // Should only be one version per hash due to UNIQUE constraint
        let versions = self.load_versions(&version_ids).await?;
        Ok(versions.into_iter().next())
    }

    // ============================================================================
    // Dataset Statistics Operations
    // ============================================================================

    /// Store dataset statistics
    pub async fn set_statistics(&self, stats: DatasetStatisticsKv) -> Result<(), StorageError> {
        let key = stats.primary_key();
        let value = bincode::serialize(&stats)?;
        self.backend.set(&key, value).await?;
        info!(dataset_id = %stats.dataset_id, "Dataset statistics stored");
        Ok(())
    }

    /// Get dataset statistics
    pub async fn get_statistics(
        &self,
        dataset_id: &str,
    ) -> Result<Option<DatasetStatisticsKv>, StorageError> {
        let key = format!("dataset:{}:stats", dataset_id);

        let bytes = match self.backend.get(&key).await? {
            Some(b) => b,
            None => return Ok(None),
        };

        let stats: DatasetStatisticsKv = bincode::deserialize(&bytes)?;
        Ok(Some(stats))
    }

    /// Delete dataset statistics
    pub async fn delete_statistics(&self, dataset_id: &str) -> Result<bool, StorageError> {
        let key = format!("dataset:{}:stats", dataset_id);
        self.backend.delete(&key).await
    }

    // ============================================================================
    // Paginated Queries
    // ============================================================================

    /// List datasets with pagination
    pub async fn list_datasets_paginated(
        &self,
        tenant_id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<TrainingDatasetKv>, StorageError> {
        let mut all_ids = self
            .index_manager
            .query_index(dataset_indexes::BY_TENANT, tenant_id)
            .await?;

        // Sort IDs for consistent pagination
        all_ids.sort();

        // Find cursor position
        let start_index = if let Some(cursor_id) = cursor {
            all_ids
                .iter()
                .position(|id| id == cursor_id)
                .map(|pos| pos + 1)
                .unwrap_or(0)
        } else {
            0
        };

        // Get page of IDs
        let page_ids: Vec<String> = all_ids
            .iter()
            .skip(start_index)
            .take(limit)
            .cloned()
            .collect();

        let has_more = start_index + page_ids.len() < all_ids.len();
        let next_cursor = if has_more {
            page_ids.last().cloned()
        } else {
            None
        };

        // Load datasets
        let items = self.load_datasets(&page_ids).await?;

        Ok(PaginatedResult {
            items,
            next_cursor,
            has_more,
        })
    }

    // ============================================================================
    // Internal Helper Methods
    // ============================================================================

    /// Load multiple datasets by ID
    async fn load_datasets(
        &self,
        dataset_ids: &[String],
    ) -> Result<Vec<TrainingDatasetKv>, StorageError> {
        let keys: Vec<String> = dataset_ids
            .iter()
            .map(|id| format!("dataset:{}", id))
            .collect();

        let values = self.backend.batch_get(&keys).await?;

        let mut datasets = Vec::new();
        for (id, value_opt) in dataset_ids.iter().zip(values.iter()) {
            if let Some(bytes) = value_opt {
                match bincode::deserialize::<TrainingDatasetKv>(bytes) {
                    Ok(dataset) => datasets.push(dataset),
                    Err(e) => {
                        error!(dataset_id = %id, error = %e, "Failed to deserialize dataset");
                    }
                }
            }
        }

        Ok(datasets)
    }

    /// Load multiple versions by ID
    async fn load_versions(
        &self,
        version_ids: &[String],
    ) -> Result<Vec<DatasetVersionKv>, StorageError> {
        let keys: Vec<String> = version_ids
            .iter()
            .map(|id| format!("dataset_version:{}", id))
            .collect();

        let values = self.backend.batch_get(&keys).await?;

        let mut versions = Vec::new();
        for (id, value_opt) in version_ids.iter().zip(values.iter()) {
            if let Some(bytes) = value_opt {
                match bincode::deserialize::<DatasetVersionKv>(bytes) {
                    Ok(version) => versions.push(version),
                    Err(e) => {
                        error!(version_id = %id, error = %e, "Failed to deserialize dataset version");
                    }
                }
            }
        }

        Ok(versions)
    }

    /// Update all secondary indexes for a dataset
    async fn update_dataset_indexes(
        &self,
        dataset: &TrainingDatasetKv,
        old_dataset: Option<&TrainingDatasetKv>,
    ) -> Result<(), StorageError> {
        let entity_id = &dataset.id;
        let should_add_new = old_dataset.is_none();

        // Tenant index
        if should_add_new {
            self.index_manager
                .add_to_index(dataset_indexes::BY_TENANT, &dataset.tenant_id, entity_id)
                .await?;
        }

        // Validation status index
        let old_status = old_dataset.map(|d| d.validation_status.as_str());
        self.index_manager
            .update_index(
                dataset_indexes::BY_VALIDATION_STATUS,
                old_status,
                &dataset.validation_status,
                entity_id,
            )
            .await?;

        // Hash index
        if should_add_new {
            self.index_manager
                .add_to_index(dataset_indexes::BY_HASH, &dataset.hash_b3, entity_id)
                .await?;
        }

        Ok(())
    }

    /// Remove dataset from all indexes
    async fn remove_dataset_from_indexes(
        &self,
        dataset: &TrainingDatasetKv,
    ) -> Result<(), StorageError> {
        let entity_id = &dataset.id;

        self.index_manager
            .remove_from_index(dataset_indexes::BY_TENANT, &dataset.tenant_id, entity_id)
            .await?;

        self.index_manager
            .remove_from_index(
                dataset_indexes::BY_VALIDATION_STATUS,
                &dataset.validation_status,
                entity_id,
            )
            .await?;

        self.index_manager
            .remove_from_index(dataset_indexes::BY_HASH, &dataset.hash_b3, entity_id)
            .await?;

        Ok(())
    }

    /// Update all secondary indexes for a dataset version
    async fn update_version_indexes(
        &self,
        version: &DatasetVersionKv,
        old_version: Option<&DatasetVersionKv>,
    ) -> Result<(), StorageError> {
        let entity_id = &version.id;
        let should_add_new = old_version.is_none();

        // Tenant index
        if should_add_new {
            self.index_manager
                .add_to_index(
                    dataset_indexes::VERSION_BY_TENANT,
                    &version.tenant_id,
                    entity_id,
                )
                .await?;
        }

        // Dataset index (for querying all versions of a dataset)
        if should_add_new {
            self.index_manager
                .add_to_index(
                    dataset_indexes::VERSION_BY_DATASET,
                    &version.dataset_id,
                    entity_id,
                )
                .await?;
        }

        // Trust state index
        let old_trust = old_version.map(|v| v.trust_state.as_str());
        self.index_manager
            .update_index(
                dataset_indexes::VERSION_BY_TRUST_STATE,
                old_trust,
                &version.trust_state,
                entity_id,
            )
            .await?;

        // Validation status index
        let old_status = old_version.map(|v| v.validation_status.as_str());
        self.index_manager
            .update_index(
                dataset_indexes::VERSION_BY_VALIDATION_STATUS,
                old_status,
                &version.validation_status,
                entity_id,
            )
            .await?;

        // Hash index
        if should_add_new {
            self.index_manager
                .add_to_index(
                    dataset_indexes::VERSION_BY_HASH,
                    &version.hash_b3,
                    entity_id,
                )
                .await?;
        }

        Ok(())
    }

    /// Remove version from all indexes
    async fn remove_version_from_indexes(
        &self,
        version: &DatasetVersionKv,
    ) -> Result<(), StorageError> {
        let entity_id = &version.id;

        self.index_manager
            .remove_from_index(
                dataset_indexes::VERSION_BY_TENANT,
                &version.tenant_id,
                entity_id,
            )
            .await?;

        self.index_manager
            .remove_from_index(
                dataset_indexes::VERSION_BY_DATASET,
                &version.dataset_id,
                entity_id,
            )
            .await?;

        self.index_manager
            .remove_from_index(
                dataset_indexes::VERSION_BY_TRUST_STATE,
                &version.trust_state,
                entity_id,
            )
            .await?;

        self.index_manager
            .remove_from_index(
                dataset_indexes::VERSION_BY_VALIDATION_STATUS,
                &version.validation_status,
                entity_id,
            )
            .await?;

        self.index_manager
            .remove_from_index(
                dataset_indexes::VERSION_BY_HASH,
                &version.hash_b3,
                entity_id,
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::indexing::IndexManager;
    use crate::redb::RedbBackend;
    use chrono::Utc;

    fn sample_dataset(id: &str, tenant_id: &str) -> TrainingDatasetKv {
        let now = Utc::now().to_rfc3339();
        TrainingDatasetKv {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            name: "Test Dataset".to_string(),
            description: Some("A test dataset".to_string()),
            format: "jsonl".to_string(),
            hash_b3: "b3:testdataset".to_string(),
            dataset_hash_b3: Some("b3:testdataset".to_string()),
            storage_path: "/var/datasets/test".to_string(),
            status: "ready".to_string(),
            validation_status: "valid".to_string(),
            validation_errors: None,
            validation_errors_json: None,
            file_count: 10,
            total_size_bytes: 1048576,
            metadata_json: None,
            created_by: Some("test-user".to_string()),
            created_at: now.clone(),
            updated_at: now,
            dataset_type: Some("training".to_string()),
            purpose: Some("fine-tuning".to_string()),
            source_location: None,
            collection_method: None,
            ownership: Some("tenant-a".to_string()),
            workspace_id: Some("workspace-1".to_string()),
            hash_needs_recompute: 0,
            hash_algorithm_version: 2,
            repo_slug: Some("org/repo".to_string()),
            branch: Some("main".to_string()),
            commit_sha: Some("deadbeef".to_string()),
            // Session lineage fields
            session_id: None,
            session_name: None,
            session_tags: None,
            // Scope metadata fields
            scope_repo_id: None,
            scope_repo: None,
            scope_scan_root: None,
            scope_remote_url: None,
            // Aggregate metrics
            scan_root_count: None,
            total_scan_root_files: None,
            total_scan_root_bytes: None,
            scan_roots_content_hash: None,
            scan_roots_updated_at: None,
        }
    }

    fn sample_version(id: &str, dataset_id: &str, tenant_id: &str) -> DatasetVersionKv {
        let now = Utc::now().to_rfc3339();
        DatasetVersionKv {
            id: id.to_string(),
            dataset_id: dataset_id.to_string(),
            tenant_id: tenant_id.to_string(),
            version_number: 1,
            version_label: Some("v1.0".to_string()),
            storage_path: "/var/datasets/test/v1".to_string(),
            hash_b3: "b3:testversion".to_string(),
            manifest_path: None,
            manifest_json: None,
            validation_status: "valid".to_string(),
            validation_errors_json: None,
            pii_status: "clean".to_string(),
            toxicity_status: "clean".to_string(),
            leak_status: "clean".to_string(),
            anomaly_status: "clean".to_string(),
            overall_safety_status: "clean".to_string(),
            trust_state: "allowed".to_string(),
            overall_trust_status: "allowed".to_string(),
            sensitivity: None,
            created_at: now.clone(),
            created_by: Some("test-user".to_string()),
            locked_at: None,
            soft_deleted_at: None,
        }
    }

    fn repo_in_memory() -> (DatasetRepository, Arc<dyn KvBackend>, Arc<IndexManager>) {
        let backend = Arc::new(RedbBackend::open_in_memory().unwrap());
        let index_manager = Arc::new(IndexManager::new(backend.clone()));
        let repo = DatasetRepository::new(backend.clone(), index_manager.clone());
        (repo, backend, index_manager)
    }

    #[tokio::test]
    async fn test_create_and_read_dataset() {
        let (repo, backend, _indexes) = repo_in_memory();
        let dataset = sample_dataset("dataset-1", "tenant-a");

        repo.create_dataset(dataset.clone()).await.unwrap();

        // Verify stored
        assert!(backend.get("dataset:dataset-1").await.unwrap().is_some());

        let fetched = repo
            .get_dataset("tenant-a", "dataset-1")
            .await
            .unwrap()
            .expect("dataset readable");
        assert_eq!(fetched.id, "dataset-1");
        assert_eq!(fetched.name, "Test Dataset");
    }

    #[tokio::test]
    async fn test_create_and_read_version() {
        let (repo, backend, _indexes) = repo_in_memory();
        let dataset = sample_dataset("dataset-1", "tenant-a");
        let version = sample_version("version-1", "dataset-1", "tenant-a");

        repo.create_dataset(dataset).await.unwrap();
        repo.create_version(version.clone()).await.unwrap();

        // Verify stored
        assert!(backend
            .get("dataset_version:version-1")
            .await
            .unwrap()
            .is_some());

        let fetched = repo
            .get_version("tenant-a", "version-1")
            .await
            .unwrap()
            .expect("version readable");
        assert_eq!(fetched.id, "version-1");
        assert_eq!(fetched.dataset_id, "dataset-1");
    }

    #[tokio::test]
    async fn test_list_versions_by_dataset() {
        let (repo, _backend, _indexes) = repo_in_memory();
        let dataset = sample_dataset("dataset-1", "tenant-a");
        let version1 = sample_version("version-1", "dataset-1", "tenant-a");
        let mut version2 = sample_version("version-2", "dataset-1", "tenant-a");
        version2.version_number = 2;

        repo.create_dataset(dataset).await.unwrap();
        repo.create_version(version1).await.unwrap();
        repo.create_version(version2).await.unwrap();

        let versions = repo
            .list_versions_by_dataset("tenant-a", "dataset-1")
            .await
            .unwrap();

        assert_eq!(versions.len(), 2);
        assert!(versions.iter().any(|v| v.id == "version-1"));
        assert!(versions.iter().any(|v| v.id == "version-2"));
    }

    #[tokio::test]
    async fn test_list_datasets_by_tenant() {
        let (repo, _backend, _indexes) = repo_in_memory();
        let dataset1 = sample_dataset("dataset-1", "tenant-a");
        let dataset2 = sample_dataset("dataset-2", "tenant-a");
        let dataset3 = sample_dataset("dataset-3", "tenant-b");

        repo.create_dataset(dataset1).await.unwrap();
        repo.create_dataset(dataset2).await.unwrap();
        repo.create_dataset(dataset3).await.unwrap();

        let datasets = repo.list_datasets_by_tenant("tenant-a").await.unwrap();

        assert_eq!(datasets.len(), 2);
        assert!(datasets.iter().any(|d| d.id == "dataset-1"));
        assert!(datasets.iter().any(|d| d.id == "dataset-2"));
        assert!(!datasets.iter().any(|d| d.id == "dataset-3"));
    }

    #[tokio::test]
    async fn test_delete_dataset() {
        let (repo, backend, _indexes) = repo_in_memory();
        let dataset = sample_dataset("dataset-1", "tenant-a");

        repo.create_dataset(dataset.clone()).await.unwrap();

        let deleted = repo.delete_dataset("tenant-a", "dataset-1").await.unwrap();
        assert!(deleted);

        assert!(backend.get("dataset:dataset-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_statistics() {
        let (repo, backend, _indexes) = repo_in_memory();
        let stats = DatasetStatisticsKv {
            dataset_id: "dataset-1".to_string(),
            num_examples: 1000,
            avg_input_length: 128.5,
            avg_target_length: 64.2,
            language_distribution: Some(r#"{"en": 0.8, "es": 0.2}"#.to_string()),
            file_type_distribution: Some(r#"{"jsonl": 1.0}"#.to_string()),
            total_tokens: 128000,
            computed_at: Utc::now().to_rfc3339(),
        };

        repo.set_statistics(stats.clone()).await.unwrap();

        // Verify stored
        assert!(backend
            .get("dataset:dataset-1:stats")
            .await
            .unwrap()
            .is_some());

        let fetched = repo
            .get_statistics("dataset-1")
            .await
            .unwrap()
            .expect("stats readable");
        assert_eq!(fetched.num_examples, 1000);
        assert_eq!(fetched.total_tokens, 128000);
    }
}
