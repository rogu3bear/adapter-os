//! Training dataset database operations

use crate::Db;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Builder for creating dataset statistics parameters
#[derive(Debug, Default)]
pub struct DatasetStatisticsBuilder {
    dataset_id: Option<String>,
    num_examples: Option<i32>,
    avg_input_length: Option<f64>,
    avg_target_length: Option<f64>,
    language_distribution: Option<String>,
    file_type_distribution: Option<String>,
    total_tokens: Option<i64>,
}

/// Parameters for dataset statistics storage
#[derive(Debug)]
pub struct DatasetStatisticsParams {
    pub dataset_id: String,
    pub num_examples: i32,
    pub avg_input_length: f64,
    pub avg_target_length: f64,
    pub language_distribution: Option<String>,
    pub file_type_distribution: Option<String>,
    pub total_tokens: i64,
}

impl DatasetStatisticsBuilder {
    /// Create a new dataset statistics builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the dataset ID (required)
    pub fn dataset_id(mut self, dataset_id: impl Into<String>) -> Self {
        self.dataset_id = Some(dataset_id.into());
        self
    }

    /// Set the number of examples (required)
    pub fn num_examples(mut self, num_examples: i32) -> Self {
        self.num_examples = Some(num_examples);
        self
    }

    /// Set the average input length (required)
    pub fn avg_input_length(mut self, avg_input_length: f64) -> Self {
        self.avg_input_length = Some(avg_input_length);
        self
    }

    /// Set the average target length (required)
    pub fn avg_target_length(mut self, avg_target_length: f64) -> Self {
        self.avg_target_length = Some(avg_target_length);
        self
    }

    /// Set the language distribution JSON (optional)
    pub fn language_distribution(
        mut self,
        language_distribution: Option<impl Into<String>>,
    ) -> Self {
        self.language_distribution = language_distribution.map(|s| s.into());
        self
    }

    /// Set the file type distribution JSON (optional)
    pub fn file_type_distribution(
        mut self,
        file_type_distribution: Option<impl Into<String>>,
    ) -> Self {
        self.file_type_distribution = file_type_distribution.map(|s| s.into());
        self
    }

    /// Set the total tokens count (required)
    pub fn total_tokens(mut self, total_tokens: i64) -> Self {
        self.total_tokens = Some(total_tokens);
        self
    }

    /// Build the dataset statistics parameters
    pub fn build(self) -> Result<DatasetStatisticsParams> {
        Ok(DatasetStatisticsParams {
            dataset_id: self
                .dataset_id
                .ok_or_else(|| anyhow!("dataset_id is required"))?,
            num_examples: self
                .num_examples
                .ok_or_else(|| anyhow!("num_examples is required"))?,
            avg_input_length: self
                .avg_input_length
                .ok_or_else(|| anyhow!("avg_input_length is required"))?,
            avg_target_length: self
                .avg_target_length
                .ok_or_else(|| anyhow!("avg_target_length is required"))?,
            language_distribution: self.language_distribution,
            file_type_distribution: self.file_type_distribution,
            total_tokens: self
                .total_tokens
                .ok_or_else(|| anyhow!("total_tokens is required"))?,
        })
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
        .await?;
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
        .await?;
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
        .await?;
        Ok(datasets)
    }

    /// Delete training dataset
    pub async fn delete_training_dataset(&self, dataset_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM training_datasets WHERE id = ?")
            .bind(dataset_id)
            .execute(self.pool())
            .await?;
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
        .await?;

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
        .await?;

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
        .await?;
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
        .await?;
        Ok(())
    }

    /// Store dataset statistics
    ///
    /// Use [`DatasetStatisticsBuilder`] to construct dataset statistics:
    /// ```no_run
    /// use adapteros_db::training_datasets::DatasetStatisticsBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = DatasetStatisticsBuilder::new()
    ///     .dataset_id("dataset-123")
    ///     .num_examples(1000)
    ///     .avg_input_length(512.5)
    ///     .avg_target_length(128.3)
    ///     .language_distribution(Some(r#"{"python": 0.6, "rust": 0.4}"#))
    ///     .file_type_distribution(Some(r#"{"py": 0.6, "rs": 0.4}"#))
    ///     .total_tokens(256000)
    ///     .build()
    ///     .expect("required fields");
    /// db.store_dataset_statistics(params)
    ///     .await
    ///     .expect("stats stored");
    /// # }
    /// ```
    pub async fn store_dataset_statistics(&self, params: DatasetStatisticsParams) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO dataset_statistics (
                dataset_id, num_examples, avg_input_length, avg_target_length,
                language_distribution, file_type_distribution, total_tokens, computed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&params.dataset_id)
        .bind(params.num_examples)
        .bind(params.avg_input_length)
        .bind(params.avg_target_length)
        .bind(&params.language_distribution)
        .bind(&params.file_type_distribution)
        .bind(params.total_tokens)
        .execute(self.pool())
        .await?;
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
        .await?;
        Ok(stats)
    }
}
