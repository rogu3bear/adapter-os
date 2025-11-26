//! Training dataset database operations

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    // PRD-DATA-01: Dataset Lab extensions (migration 0084)
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

/// Evidence entry for datasets and adapters (PRD-DATA-01)
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

/// Dataset-to-adapter link (PRD-DATA-01)
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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create training dataset: {}", e)))?;
        Ok(id)
    }

    /// Get training dataset by ID
    pub async fn get_training_dataset(&self, dataset_id: &str) -> Result<Option<TrainingDataset>> {
        let dataset = sqlx::query_as::<_, TrainingDataset>(
            "SELECT id, name, description, file_count, total_size_bytes, format, hash_b3,
                    storage_path, validation_status, validation_errors, metadata_json,
                    created_by, created_at, updated_at, dataset_type, purpose,
                    source_location, collection_method, ownership, tenant_id
             FROM training_datasets
             WHERE id = ?",
        )
        .bind(dataset_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get training dataset: {}", e)))?;
        Ok(dataset)
    }

    /// List all training datasets
    pub async fn list_training_datasets(&self, limit: i64) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(
            "SELECT id, name, description, file_count, total_size_bytes, format, hash_b3,
                    storage_path, validation_status, validation_errors, metadata_json,
                    created_by, created_at, updated_at, dataset_type, purpose,
                    source_location, collection_method, ownership, tenant_id
             FROM training_datasets
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list training datasets: {}", e)))?;
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
        let datasets = sqlx::query_as::<_, TrainingDataset>(
            "SELECT id, name, description, file_count, total_size_bytes, format, hash_b3,
                    storage_path, validation_status, validation_errors, metadata_json,
                    created_by, created_at, updated_at, dataset_type, purpose,
                    source_location, collection_method, ownership, tenant_id
             FROM training_datasets
             WHERE tenant_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list training datasets for tenant: {}",
                e
            ))
        })?;
        Ok(datasets)
    }

    /// Delete training dataset
    pub async fn delete_training_dataset(&self, dataset_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM training_datasets WHERE id = ?")
            .bind(dataset_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete training dataset: {}", e)))?;
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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to add dataset file: {}", e)))?;

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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update dataset file count: {}", e)))?;

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
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get dataset files: {}", e)))?;
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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update dataset validation: {}", e)))?;
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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to store dataset statistics: {}", e)))?;
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
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get dataset statistics: {}", e)))?;
        Ok(stats)
    }

    // ============================================================================
    // Evidence Entries Operations (PRD-DATA-01)
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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create evidence entry: {}", e)))?;
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
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list evidence entries: {}", e)))?;
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
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get evidence entry: {}", e)))?;
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
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get dataset evidence: {}", e)))?;
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
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get adapter evidence: {}", e)))?;
        Ok(entries)
    }

    /// Count evidence entries for a dataset
    pub async fn count_dataset_evidence(&self, dataset_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM evidence_entries WHERE dataset_id = ?")
                .bind(dataset_id)
                .fetch_one(&*self.pool())
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
                .fetch_one(&*self.pool())
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
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete evidence entry: {}", e)))?;
        Ok(())
    }

    // ============================================================================
    // Dataset-Adapter Links Operations (PRD-DATA-01)
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
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create dataset-adapter link: {}", e)))?;
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
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get dataset adapters: {}", e)))?;
        Ok(links)
    }

    /// Alias for get_dataset_adapters (PRD naming)
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
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get adapter datasets: {}", e)))?;
        Ok(links)
    }

    /// Alias for get_adapter_datasets (PRD naming)
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
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count dataset usage: {}", e)))?;
        Ok(count.0)
    }

    /// Delete dataset-adapter link
    pub async fn delete_dataset_adapter_link(&self, link_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM dataset_adapter_links WHERE id = ?")
            .bind(link_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to delete dataset-adapter link: {}", e))
            })?;
        Ok(())
    }

    /// Update dataset extended fields (PRD-DATA-01)
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
        .execute(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update dataset extended fields: {}", e))
        })?;
        Ok(())
    }
}
