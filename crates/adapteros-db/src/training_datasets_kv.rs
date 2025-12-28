//! Training datasets KV storage integration
//!
//! This module adds storage mode routing for training datasets,
//! enabling dual-write to SQL and KV backends.

use crate::kv_backend::KvBackend;
use crate::training_datasets::{TrainingDataset, TrainingDatasetVersion};
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_storage::kv::indexing::IndexManager;
use adapteros_storage::models::{DatasetVersionKv, TrainingDatasetKv};
use adapteros_storage::repos::dataset::DatasetRepository;
use std::sync::Arc;
use tracing::{debug, warn};

// ============================================================================
// SQL ↔ KV Conversion Functions
// ============================================================================

/// Convert SQL TrainingDataset to KV representation
pub fn kv_dataset_from_record(record: &TrainingDataset, tenant_id: &str) -> TrainingDatasetKv {
    TrainingDatasetKv {
        id: record.id.clone(),
        tenant_id: record
            .tenant_id
            .clone()
            .unwrap_or_else(|| tenant_id.to_string()),
        name: record.name.clone(),
        description: record.description.clone(),
        format: record.format.clone(),
        hash_b3: record.hash_b3.clone(),
        dataset_hash_b3: Some(record.dataset_hash_b3.clone()),
        storage_path: record.storage_path.clone(),
        status: record.status.clone(),
        validation_status: record.validation_status.clone(),
        validation_errors: record.validation_errors.clone(),
        file_count: record.file_count,
        total_size_bytes: record.total_size_bytes,
        metadata_json: record.metadata_json.clone(),
        created_by: record.created_by.clone(),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        dataset_type: record.dataset_type.clone(),
        purpose: record.purpose.clone(),
        source_location: record.source_location.clone(),
        collection_method: record.collection_method.clone(),
        ownership: record.ownership.clone(),
        workspace_id: record.workspace_id.clone(),
    }
}

/// Convert KV TrainingDatasetKv to SQL representation
pub fn dataset_record_from_kv(kv: &TrainingDatasetKv) -> TrainingDataset {
    TrainingDataset {
        id: kv.id.clone(),
        name: kv.name.clone(),
        description: kv.description.clone(),
        file_count: kv.file_count,
        total_size_bytes: kv.total_size_bytes,
        format: kv.format.clone(),
        hash_b3: kv.hash_b3.clone(),
        dataset_hash_b3: kv
            .dataset_hash_b3
            .clone()
            .unwrap_or_else(|| kv.hash_b3.clone()),
        storage_path: kv.storage_path.clone(),
        status: kv.status.clone(),
        validation_status: kv.validation_status.clone(),
        validation_errors: kv.validation_errors.clone(),
        metadata_json: kv.metadata_json.clone(),
        created_by: kv.created_by.clone(),
        created_at: kv.created_at.clone(),
        updated_at: kv.updated_at.clone(),
        dataset_type: kv.dataset_type.clone(),
        purpose: kv.purpose.clone(),
        source_location: kv.source_location.clone(),
        collection_method: kv.collection_method.clone(),
        ownership: kv.ownership.clone(),
        tenant_id: Some(kv.tenant_id.clone()),
        workspace_id: kv.workspace_id.clone(),
    }
}

/// Convert SQL TrainingDatasetVersion to KV representation
pub fn kv_version_from_record(record: &TrainingDatasetVersion) -> DatasetVersionKv {
    DatasetVersionKv {
        id: record.id.clone(),
        dataset_id: record.dataset_id.clone(),
        tenant_id: record.tenant_id.clone().unwrap_or_default(),
        version_number: record.version_number,
        version_label: record.version_label.clone(),
        storage_path: record.storage_path.clone(),
        hash_b3: record.hash_b3.clone(),
        manifest_path: record.manifest_path.clone(),
        manifest_json: record.manifest_json.clone(),
        validation_status: record.validation_status.clone(),
        validation_errors_json: record.validation_errors_json.clone(),
        pii_status: record.pii_status.clone(),
        toxicity_status: record.toxicity_status.clone(),
        leak_status: record.leak_status.clone(),
        anomaly_status: record.anomaly_status.clone(),
        overall_safety_status: record.overall_safety_status.clone(),
        trust_state: record.trust_state.clone(),
        overall_trust_status: record.overall_trust_status.clone(),
        sensitivity: record.sensitivity.clone(),
        created_at: record.created_at.clone(),
        created_by: record.created_by.clone(),
        locked_at: record.locked_at.clone(),
        soft_deleted_at: record.soft_deleted_at.clone(),
    }
}

/// Convert KV DatasetVersionKv to SQL representation
pub fn version_record_from_kv(kv: &DatasetVersionKv) -> TrainingDatasetVersion {
    TrainingDatasetVersion {
        id: kv.id.clone(),
        dataset_id: kv.dataset_id.clone(),
        tenant_id: Some(kv.tenant_id.clone()),
        version_number: kv.version_number,
        version_label: kv.version_label.clone(),
        storage_path: kv.storage_path.clone(),
        hash_b3: kv.hash_b3.clone(),
        manifest_path: kv.manifest_path.clone(),
        manifest_json: kv.manifest_json.clone(),
        validation_status: kv.validation_status.clone(),
        validation_errors_json: kv.validation_errors_json.clone(),
        pii_status: kv.pii_status.clone(),
        toxicity_status: kv.toxicity_status.clone(),
        leak_status: kv.leak_status.clone(),
        anomaly_status: kv.anomaly_status.clone(),
        overall_safety_status: kv.overall_safety_status.clone(),
        trust_state: kv.trust_state.clone(),
        overall_trust_status: kv.overall_trust_status.clone(),
        sensitivity: kv.sensitivity.clone(),
        created_at: kv.created_at.clone(),
        created_by: kv.created_by.clone(),
        locked_at: kv.locked_at.clone(),
        soft_deleted_at: kv.soft_deleted_at.clone(),
    }
}

// ============================================================================
// Db Extension Methods for Dataset KV Operations
// ============================================================================

impl Db {
    /// Get the dataset KV repository if KV operations are enabled
    pub fn get_dataset_kv_repo(&self) -> Option<DatasetRepository> {
        if self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv() {
            self.kv_backend().map(|kv| {
                let backend: Arc<dyn KvBackend> = kv.clone();
                let index_manager = Arc::new(IndexManager::new(backend.clone()));
                DatasetRepository::new(backend, index_manager)
            })
        } else {
            None
        }
    }

    /// Get training dataset with storage mode routing
    ///
    /// Reads from KV first if enabled, falls back to SQL if allowed.
    pub async fn get_training_dataset_routed(
        &self,
        tenant_id: &str,
        dataset_id: &str,
    ) -> Result<Option<TrainingDataset>> {
        // Try KV first if read_from_kv is enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_dataset_kv_repo() {
                match repo.get_dataset(tenant_id, dataset_id).await {
                    Ok(Some(kv_dataset)) => {
                        debug!(
                            dataset_id = %dataset_id,
                            tenant_id = %tenant_id,
                            mode = "kv-primary",
                            "Retrieved dataset from KV"
                        );
                        return Ok(Some(dataset_record_from_kv(&kv_dataset)));
                    }
                    Ok(None) if !self.storage_mode().sql_fallback_enabled() => {
                        return Ok(None);
                    }
                    Ok(None) => {
                        self.record_kv_read_fallback("datasets.get.empty");
                        debug!(
                            dataset_id = %dataset_id,
                            mode = "kv-fallback",
                            "KV returned None, falling back to SQL"
                        );
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("datasets.get.error");
                        warn!(
                            error = %e,
                            dataset_id = %dataset_id,
                            mode = "kv-fallback",
                            "KV read failed, falling back to SQL"
                        );
                    }
                    Err(e) => {
                        return Err(AosError::Database(format!(
                            "KV read failed for dataset: {}",
                            e
                        )));
                    }
                }
            }
        }

        // SQL fallback or primary read
        if self.storage_mode().read_from_sql() {
            return self.get_training_dataset(dataset_id).await;
        }

        Ok(None)
    }

    /// List training datasets for tenant with storage mode routing
    pub async fn list_training_datasets_for_tenant_routed(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<TrainingDataset>> {
        // Try KV first if read_from_kv is enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_dataset_kv_repo() {
                match repo.list_datasets_by_tenant(tenant_id).await {
                    Ok(kv_datasets) if !kv_datasets.is_empty() => {
                        debug!(
                            tenant_id = %tenant_id,
                            count = kv_datasets.len(),
                            mode = "kv-primary",
                            "Retrieved datasets from KV"
                        );
                        let mut datasets: Vec<TrainingDataset> = kv_datasets
                            .into_iter()
                            .map(|kv| dataset_record_from_kv(&kv))
                            .collect();
                        // Sort by created_at DESC and apply limit
                        datasets.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                        datasets.truncate(limit as usize);
                        return Ok(datasets);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("datasets.list_for_tenant.empty");
                        debug!(
                            tenant_id = %tenant_id,
                            mode = "kv-fallback",
                            "KV returned empty list, falling back to SQL"
                        );
                    }
                    Ok(datasets) => {
                        return Ok(datasets
                            .into_iter()
                            .map(|kv| dataset_record_from_kv(&kv))
                            .collect());
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("datasets.list_for_tenant.error");
                        warn!(
                            error = %e,
                            tenant_id = %tenant_id,
                            mode = "kv-fallback",
                            "KV read failed, falling back to SQL"
                        );
                    }
                    Err(e) => {
                        return Err(AosError::Database(format!(
                            "KV read failed for datasets: {}",
                            e
                        )));
                    }
                }
            }
        }

        // SQL fallback or primary read
        if self.storage_mode().read_from_sql() {
            return self
                .list_training_datasets_for_tenant(tenant_id, limit)
                .await;
        }

        Ok(vec![])
    }

    /// Dual-write dataset to KV after SQL write
    ///
    /// Call this after creating a dataset in SQL to ensure KV consistency.
    pub async fn dual_write_dataset_to_kv(&self, tenant_id: &str, dataset_id: &str) -> Result<()> {
        if !self.storage_mode().write_to_kv() {
            return Ok(());
        }

        let Some(repo) = self.get_dataset_kv_repo() else {
            return Ok(());
        };

        // Fetch the dataset from SQL to get full record
        let dataset = self.get_training_dataset(dataset_id).await?;
        let Some(dataset) = dataset else {
            warn!(
                dataset_id = %dataset_id,
                "Dataset not found in SQL for dual-write"
            );
            return Ok(());
        };

        // Convert and write to KV
        let kv_dataset = kv_dataset_from_record(&dataset, tenant_id);
        if let Err(e) = repo.create_dataset(kv_dataset).await {
            self.record_kv_write_fallback("datasets.dual_write");
            warn!(
                error = %e,
                dataset_id = %dataset_id,
                "Failed to dual-write dataset to KV"
            );
        } else {
            debug!(
                dataset_id = %dataset_id,
                tenant_id = %tenant_id,
                "Dataset dual-written to KV"
            );
        }

        Ok(())
    }

    /// Dual-write dataset version to KV after SQL write
    pub async fn dual_write_dataset_version_to_kv(&self, version_id: &str) -> Result<()> {
        if !self.storage_mode().write_to_kv() {
            return Ok(());
        }

        let Some(repo) = self.get_dataset_kv_repo() else {
            return Ok(());
        };

        // Fetch the version from SQL
        let version = self.get_training_dataset_version(version_id).await?;
        let Some(version) = version else {
            warn!(
                version_id = %version_id,
                "Dataset version not found in SQL for dual-write"
            );
            return Ok(());
        };

        // Convert and write to KV
        let kv_version = kv_version_from_record(&version);
        if let Err(e) = repo.create_version(kv_version).await {
            self.record_kv_write_fallback("datasets.version_dual_write");
            warn!(
                error = %e,
                version_id = %version_id,
                "Failed to dual-write dataset version to KV"
            );
        } else {
            debug!(
                version_id = %version_id,
                "Dataset version dual-written to KV"
            );
        }

        Ok(())
    }

    /// Delete dataset from KV (called after SQL delete)
    pub async fn delete_dataset_from_kv(&self, tenant_id: &str, dataset_id: &str) -> Result<()> {
        if !self.storage_mode().write_to_kv() {
            return Ok(());
        }

        let Some(repo) = self.get_dataset_kv_repo() else {
            return Ok(());
        };

        if let Err(e) = repo.delete_dataset(tenant_id, dataset_id).await {
            self.record_kv_write_fallback("datasets.delete");
            warn!(
                error = %e,
                dataset_id = %dataset_id,
                "Failed to delete dataset from KV"
            );
        } else {
            debug!(
                dataset_id = %dataset_id,
                tenant_id = %tenant_id,
                "Dataset deleted from KV"
            );
        }

        Ok(())
    }

    /// Get dataset version with storage mode routing
    pub async fn get_training_dataset_version_routed(
        &self,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<Option<TrainingDatasetVersion>> {
        // Try KV first if read_from_kv is enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_dataset_kv_repo() {
                match repo.get_version(tenant_id, version_id).await {
                    Ok(Some(kv_version)) => {
                        debug!(
                            version_id = %version_id,
                            tenant_id = %tenant_id,
                            mode = "kv-primary",
                            "Retrieved dataset version from KV"
                        );
                        return Ok(Some(version_record_from_kv(&kv_version)));
                    }
                    Ok(None) if !self.storage_mode().sql_fallback_enabled() => {
                        return Ok(None);
                    }
                    Ok(None) => {
                        self.record_kv_read_fallback("datasets.get_version.empty");
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("datasets.get_version.error");
                        warn!(
                            error = %e,
                            version_id = %version_id,
                            mode = "kv-fallback",
                            "KV read failed, falling back to SQL"
                        );
                    }
                    Err(e) => {
                        return Err(AosError::Database(format!(
                            "KV read failed for dataset version: {}",
                            e
                        )));
                    }
                }
            }
        }

        // SQL fallback or primary read
        if self.storage_mode().read_from_sql() {
            return self
                .get_training_dataset_version_for_tenant(version_id, tenant_id)
                .await;
        }

        Ok(None)
    }

    /// List dataset versions with storage mode routing
    pub async fn list_dataset_versions_routed(
        &self,
        tenant_id: &str,
        dataset_id: &str,
    ) -> Result<Vec<TrainingDatasetVersion>> {
        // Try KV first if read_from_kv is enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_dataset_kv_repo() {
                match repo.list_versions_by_dataset(tenant_id, dataset_id).await {
                    Ok(kv_versions) if !kv_versions.is_empty() => {
                        debug!(
                            dataset_id = %dataset_id,
                            count = kv_versions.len(),
                            mode = "kv-primary",
                            "Retrieved dataset versions from KV"
                        );
                        let mut versions: Vec<TrainingDatasetVersion> = kv_versions
                            .into_iter()
                            .map(|kv| version_record_from_kv(&kv))
                            .collect();
                        // Sort by version_number DESC
                        versions.sort_by(|a, b| b.version_number.cmp(&a.version_number));
                        return Ok(versions);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("datasets.list_versions.empty");
                    }
                    Ok(versions) => {
                        return Ok(versions
                            .into_iter()
                            .map(|kv| version_record_from_kv(&kv))
                            .collect());
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("datasets.list_versions.error");
                        warn!(
                            error = %e,
                            dataset_id = %dataset_id,
                            mode = "kv-fallback",
                            "KV read failed, falling back to SQL"
                        );
                    }
                    Err(e) => {
                        return Err(AosError::Database(format!(
                            "KV read failed for dataset versions: {}",
                            e
                        )));
                    }
                }
            }
        }

        // SQL fallback or primary read
        if self.storage_mode().read_from_sql() {
            let versions_with_trust = self.list_dataset_versions_for_dataset(dataset_id).await?;
            return Ok(versions_with_trust.into_iter().map(|(v, _)| v).collect());
        }

        Ok(vec![])
    }

    /// Update dataset in KV after SQL update
    pub async fn sync_dataset_to_kv(&self, tenant_id: &str, dataset_id: &str) -> Result<()> {
        if !self.storage_mode().write_to_kv() {
            return Ok(());
        }

        let Some(repo) = self.get_dataset_kv_repo() else {
            return Ok(());
        };

        // Fetch updated dataset from SQL
        let dataset = self.get_training_dataset(dataset_id).await?;
        let Some(dataset) = dataset else {
            warn!(
                dataset_id = %dataset_id,
                "Dataset not found in SQL for KV sync"
            );
            return Ok(());
        };

        // Convert and update in KV
        let kv_dataset = kv_dataset_from_record(&dataset, tenant_id);
        if let Err(e) = repo.update_dataset(kv_dataset).await {
            self.record_kv_write_fallback("datasets.sync");
            warn!(
                error = %e,
                dataset_id = %dataset_id,
                "Failed to sync dataset to KV"
            );
        }

        Ok(())
    }

    /// Update dataset version in KV after SQL update
    pub async fn sync_dataset_version_to_kv(&self, version_id: &str) -> Result<()> {
        if !self.storage_mode().write_to_kv() {
            return Ok(());
        }

        let Some(repo) = self.get_dataset_kv_repo() else {
            return Ok(());
        };

        // Fetch updated version from SQL
        let version = self.get_training_dataset_version(version_id).await?;
        let Some(version) = version else {
            warn!(
                version_id = %version_id,
                "Dataset version not found in SQL for KV sync"
            );
            return Ok(());
        };

        // Convert and update in KV
        let kv_version = kv_version_from_record(&version);
        if let Err(e) = repo.update_version(kv_version).await {
            self.record_kv_write_fallback("datasets.version_sync");
            warn!(
                error = %e,
                version_id = %version_id,
                "Failed to sync dataset version to KV"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dataset_roundtrip_conversion() {
        let sql_dataset = TrainingDataset {
            id: "ds-123".to_string(),
            name: "Test Dataset".to_string(),
            description: Some("A test dataset".to_string()),
            file_count: 5,
            total_size_bytes: 1024000,
            format: "jsonl".to_string(),
            hash_b3: "abc123".to_string(),
            dataset_hash_b3: "dataset-abc123".to_string(),
            storage_path: "/var/datasets/ds-123".to_string(),
            status: "ready".to_string(),
            validation_status: "valid".to_string(),
            validation_errors: None,
            metadata_json: Some(r#"{"key": "value"}"#.to_string()),
            created_by: Some("user-1".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            dataset_type: Some("training".to_string()),
            purpose: Some("fine-tuning".to_string()),
            source_location: Some("s3://bucket/data".to_string()),
            collection_method: Some("manual".to_string()),
            ownership: Some("team-a".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            workspace_id: Some("workspace-1".to_string()),
        };

        let kv_dataset = kv_dataset_from_record(&sql_dataset, "tenant-1");
        let roundtrip = dataset_record_from_kv(&kv_dataset);

        assert_eq!(sql_dataset.id, roundtrip.id);
        assert_eq!(sql_dataset.name, roundtrip.name);
        assert_eq!(sql_dataset.description, roundtrip.description);
        assert_eq!(sql_dataset.file_count, roundtrip.file_count);
        assert_eq!(sql_dataset.total_size_bytes, roundtrip.total_size_bytes);
        assert_eq!(sql_dataset.format, roundtrip.format);
        assert_eq!(sql_dataset.hash_b3, roundtrip.hash_b3);
        assert_eq!(sql_dataset.dataset_hash_b3, roundtrip.dataset_hash_b3);
        assert_eq!(sql_dataset.storage_path, roundtrip.storage_path);
        assert_eq!(sql_dataset.status, roundtrip.status);
        assert_eq!(sql_dataset.validation_status, roundtrip.validation_status);
        assert_eq!(sql_dataset.validation_errors, roundtrip.validation_errors);
        assert_eq!(sql_dataset.metadata_json, roundtrip.metadata_json);
        assert_eq!(sql_dataset.created_by, roundtrip.created_by);
        assert_eq!(sql_dataset.created_at, roundtrip.created_at);
        assert_eq!(sql_dataset.updated_at, roundtrip.updated_at);
        assert_eq!(sql_dataset.dataset_type, roundtrip.dataset_type);
        assert_eq!(sql_dataset.purpose, roundtrip.purpose);
        assert_eq!(sql_dataset.source_location, roundtrip.source_location);
        assert_eq!(sql_dataset.collection_method, roundtrip.collection_method);
        assert_eq!(sql_dataset.ownership, roundtrip.ownership);
        assert_eq!(sql_dataset.tenant_id, roundtrip.tenant_id);
        assert_eq!(sql_dataset.workspace_id, roundtrip.workspace_id);
    }

    #[test]
    fn test_version_roundtrip_conversion() {
        let sql_version = TrainingDatasetVersion {
            id: "ver-123".to_string(),
            dataset_id: "ds-123".to_string(),
            tenant_id: Some("tenant-1".to_string()),
            version_number: 1,
            version_label: Some("v1.0".to_string()),
            storage_path: "/var/datasets/ds-123/v1".to_string(),
            hash_b3: "def456".to_string(),
            manifest_path: Some("/manifests/ds-123-v1.json".to_string()),
            manifest_json: Some(r#"{"rows": 100}"#.to_string()),
            validation_status: "valid".to_string(),
            validation_errors_json: None,
            pii_status: "clean".to_string(),
            toxicity_status: "clean".to_string(),
            leak_status: "clean".to_string(),
            anomaly_status: "clean".to_string(),
            overall_safety_status: "clean".to_string(),
            trust_state: "allowed".to_string(),
            overall_trust_status: "allowed".to_string(),
            sensitivity: Some("low".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            created_by: Some("user-1".to_string()),
            locked_at: None,
            soft_deleted_at: None,
        };

        let kv_version = kv_version_from_record(&sql_version);
        let roundtrip = version_record_from_kv(&kv_version);

        assert_eq!(sql_version.id, roundtrip.id);
        assert_eq!(sql_version.dataset_id, roundtrip.dataset_id);
        assert_eq!(sql_version.tenant_id, roundtrip.tenant_id);
        assert_eq!(sql_version.version_number, roundtrip.version_number);
        assert_eq!(sql_version.version_label, roundtrip.version_label);
        assert_eq!(sql_version.storage_path, roundtrip.storage_path);
        assert_eq!(sql_version.hash_b3, roundtrip.hash_b3);
        assert_eq!(sql_version.manifest_path, roundtrip.manifest_path);
        assert_eq!(sql_version.manifest_json, roundtrip.manifest_json);
        assert_eq!(sql_version.validation_status, roundtrip.validation_status);
        assert_eq!(sql_version.pii_status, roundtrip.pii_status);
        assert_eq!(sql_version.toxicity_status, roundtrip.toxicity_status);
        assert_eq!(sql_version.leak_status, roundtrip.leak_status);
        assert_eq!(sql_version.anomaly_status, roundtrip.anomaly_status);
        assert_eq!(
            sql_version.overall_safety_status,
            roundtrip.overall_safety_status
        );
        assert_eq!(sql_version.trust_state, roundtrip.trust_state);
        assert_eq!(
            sql_version.overall_trust_status,
            roundtrip.overall_trust_status
        );
        assert_eq!(sql_version.sensitivity, roundtrip.sensitivity);
        assert_eq!(sql_version.created_at, roundtrip.created_at);
        assert_eq!(sql_version.created_by, roundtrip.created_by);
        assert_eq!(sql_version.locked_at, roundtrip.locked_at);
        assert_eq!(sql_version.soft_deleted_at, roundtrip.soft_deleted_at);
    }
}
