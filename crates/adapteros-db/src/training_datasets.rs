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
        .map_err(|e| AosError::Database(format!("Failed to create training dataset: {}", e)))?;
        Ok(id)
    }

    /// Get training dataset by ID
    pub async fn get_training_dataset(&self, dataset_id: &str) -> Result<Option<TrainingDataset>> {
        let dataset = sqlx::query_as::<_, TrainingDataset>(
            "SELECT id, name, description, file_count, total_size_bytes, format, hash_b3,
                    storage_path, validation_status, validation_errors, metadata_json,
                    created_by, created_at, updated_at
             FROM training_datasets
             WHERE id = ?",
        )
        .bind(dataset_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get training dataset: {}", e)))?;
        Ok(dataset)
    }

    /// List all training datasets
    pub async fn list_training_datasets(&self, limit: i64) -> Result<Vec<TrainingDataset>> {
        let datasets = sqlx::query_as::<_, TrainingDataset>(
            "SELECT id, name, description, file_count, total_size_bytes, format, hash_b3,
                    storage_path, validation_status, validation_errors, metadata_json,
                    created_by, created_at, updated_at
             FROM training_datasets
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list training datasets: {}", e)))?;
        Ok(datasets)
    }

    /// Delete training dataset
    pub async fn delete_training_dataset(&self, dataset_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM training_datasets WHERE id = ?")
            .bind(dataset_id)
            .execute(self.pool())
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
        .execute(self.pool())
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
        .execute(self.pool())
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
        .fetch_all(self.pool())
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
        .execute(self.pool())
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
        .execute(self.pool())
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
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get dataset statistics: {}", e)))?;
        Ok(stats)
    }
}
