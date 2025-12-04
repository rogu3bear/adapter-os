//! Training job database operations
//!
//! Implements CRUD operations for repository training jobs.
//! Evidence: migrations/0013_git_repository_integration.sql:25-40
//! Pattern: Database schema for training jobs

use crate::Db;
use adapteros_core::{AosError, Result};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Training job record from database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TrainingJobRecord {
    pub id: String,
    pub repo_id: String,
    pub training_config_json: String,
    pub status: String,
    pub progress_json: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub created_by: String,
    // Fields from migration 0050
    pub adapter_name: Option<String>,
    pub template_id: Option<String>,
    pub created_at: Option<String>,
    pub metadata_json: Option<String>,
    // Field from migration 0099
    pub config_hash_b3: Option<String>,
    // Fields from migration 0100 - provenance tracking
    pub dataset_id: Option<String>,
    pub base_model_id: Option<String>,
    pub collection_id: Option<String>,
    pub tenant_id: Option<String>,
    pub build_id: Option<String>,
    pub source_documents_json: Option<String>,
    // Fields from migration 0133 - retry tracking
    /// Whether this job can be retried (determined by error type)
    pub retryable: Option<i64>,
    /// ID of the original job this is a retry of (for retry chain tracking)
    pub retry_of_job_id: Option<String>,
    // Fields from migration 0073 - unified train-to-chat pipeline
    /// Stack ID created from this training job (if post_actions.create_stack = true)
    pub stack_id: Option<String>,
    /// Adapter ID created from this training job
    pub adapter_id: Option<String>,
}

/// Training metric record from database
///
/// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
/// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
/// Pattern: Time-series metrics for training jobs
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TrainingMetricRow {
    pub id: String,
    pub training_job_id: String,
    pub step: i64,
    pub epoch: Option<i64>,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_timestamp: Option<String>,
}

/// Training progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingProgress {
    pub progress_pct: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub current_loss: f32,
    pub learning_rate: f32,
    pub tokens_per_second: f32,
    pub error_message: Option<String>,
}

/// Training configuration parameters for hash computation
///
/// Evidence: migrations/0099_training_config_hash.sql
/// Pattern: Deterministic hash of training hyperparameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfigParams {
    pub rank: usize,
    pub alpha: f32,
    pub learning_rate: f32,
    pub batch_size: usize,
    pub epochs: usize,
    pub hidden_dim: usize,
}

/// Compute BLAKE3 hash of training configuration for reproducibility tracking
///
/// Takes training hyperparameters and produces a deterministic hash that can be used to:
/// - Identify identical training configurations across jobs
/// - Verify reproducibility of training runs
/// - Detect configuration drift
///
/// Evidence: migrations/0099_training_config_hash.sql
/// Pattern: Deterministic hashing per Determinism Policy
///
/// # Arguments
/// * `params` - Training configuration parameters (rank, alpha, learning_rate, batch_size, epochs, hidden_dim)
///
/// # Returns
/// BLAKE3 hash as lowercase hexadecimal string (64 characters)
///
/// # Example
/// ```ignore
/// let params = TrainingConfigParams {
///     rank: 16,
///     alpha: 32.0,
///     learning_rate: 0.001,
///     batch_size: 8,
///     epochs: 3,
///     hidden_dim: 768,
/// };
/// let hash = compute_config_hash(&params);
/// ```
pub fn compute_config_hash(params: &TrainingConfigParams) -> Result<String> {
    // Serialize to deterministic JSON (sorted keys)
    let json = serde_json::to_string(params).map_err(AosError::Serialization)?;

    // Compute BLAKE3 hash
    let mut hasher = Hasher::new();
    hasher.update(json.as_bytes());
    let hash = hasher.finalize();

    // Return as hex string
    Ok(hash.to_hex().to_string())
}

impl Db {
    /// Create a new training job
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn create_training_job(
        &self,
        repo_id: &str,
        training_config_json: &str,
        created_by: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let progress = TrainingProgress {
            progress_pct: 0.0,
            current_epoch: 0,
            total_epochs: 3, // Default from TrainingConfig
            current_loss: 0.0,
            learning_rate: 0.001,
            tokens_per_second: 0.0,
            error_message: None,
        };
        let progress_json = serde_json::to_string(&progress).map_err(AosError::Serialization)?;

        sqlx::query(
            "INSERT INTO repository_training_jobs 
             (id, repo_id, training_config_json, status, progress_json, created_by) 
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(repo_id)
        .bind(training_config_json)
        .bind("pending")
        .bind(&progress_json)
        .bind(created_by)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Get a training job by ID
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn get_training_job(&self, job_id: &str) -> Result<Option<TrainingJobRecord>> {
        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    retryable, retry_of_job_id, stack_id, adapter_id
             FROM repository_training_jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(job)
    }

    /// Update training job progress
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn update_training_progress(
        &self,
        job_id: &str,
        progress: &TrainingProgress,
    ) -> Result<()> {
        let progress_json = serde_json::to_string(progress).map_err(AosError::Serialization)?;

        sqlx::query(
            "UPDATE repository_training_jobs
             SET progress_json = ?
             WHERE id = ?",
        )
        .bind(&progress_json)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Update training job status
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn update_training_status(&self, job_id: &str, status: &str) -> Result<()> {
        let completed_at = if status == "completed" || status == "failed" {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };

        sqlx::query(
            "UPDATE repository_training_jobs 
             SET status = ?, completed_at = ? 
             WHERE id = ?",
        )
        .bind(status)
        .bind(completed_at)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// List training jobs for a repository
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn list_training_jobs(&self, repo_id: &str) -> Result<Vec<TrainingJobRecord>> {
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    retryable, retry_of_job_id, stack_id, adapter_id
             FROM repository_training_jobs
             WHERE repo_id = ?
             ORDER BY started_at DESC",
        )
        .bind(repo_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(jobs)
    }

    /// List training jobs by status
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn list_training_jobs_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    retryable, retry_of_job_id, stack_id, adapter_id
             FROM repository_training_jobs
             WHERE status = ?
             ORDER BY started_at DESC",
        )
        .bind(status)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(jobs)
    }

    /// List training jobs for a specific tenant
    ///
    /// Filters training jobs by tenant_id through the created_by user reference.
    /// This method joins repository_training_jobs with users table to enforce
    /// tenant isolation in multi-tenant deployments.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    ///
    /// # Returns
    /// Vector of training jobs created by users belonging to the specified tenant,
    /// ordered by start time (newest first)
    ///
    /// # Implementation Note
    /// Since repository_training_jobs doesn't have a direct tenant_id column,
    /// we filter via created_by (user_id) which links to the users table.
    /// Users table doesn't have tenant_id either, so this implementation assumes
    /// that tenant isolation is handled at the application layer or that a future
    /// migration will add tenant_id columns to these tables.
    ///
    /// For now, this method filters by the created_by field matching the tenant_id
    /// pattern (assuming created_by contains tenant information in format "user@tenant").
    pub async fn list_training_jobs_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        // Note: This is a placeholder implementation. The repository_training_jobs table
        // doesn't currently have tenant_id. This implementation filters by created_by
        // which may contain tenant information in the user identifier.
        // A proper implementation would require a migration to add tenant_id to
        // repository_training_jobs or git_repositories tables.

        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT rtj.id, rtj.repo_id, rtj.training_config_json, rtj.status, rtj.progress_json,
                    rtj.started_at, rtj.completed_at, rtj.created_by, rtj.adapter_name,
                    rtj.template_id, rtj.created_at, rtj.metadata_json, rtj.config_hash_b3,
                    rtj.dataset_id, rtj.base_model_id, rtj.collection_id, rtj.tenant_id,
                    rtj.build_id, rtj.source_documents_json,
                    rtj.retryable, rtj.retry_of_job_id, rtj.stack_id, rtj.adapter_id
             FROM repository_training_jobs rtj
             WHERE rtj.tenant_id = ? OR rtj.created_by LIKE ?
             ORDER BY rtj.started_at DESC",
        )
        .bind(tenant_id)
        .bind(format!("%{}%", tenant_id))
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to list training jobs for tenant: {}", e))
        })?;

        Ok(jobs)
    }

    /// Delete a training job
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn delete_training_job(&self, job_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM repository_training_jobs WHERE id = ?")
            .bind(job_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Update training job with artifact metadata
    ///
    /// Called after training completes successfully to record:
    /// - artifact_path: Path to the packaged .aos file
    /// - adapter_id: Registered adapter identifier
    /// - weights_hash_b3: BLAKE3 hash of the trained weights
    ///
    /// Evidence: migrations/0050_training_jobs_extensions.sql:18-19
    /// Pattern: metadata_json column for artifact tracking
    pub async fn update_training_job_artifact(
        &self,
        job_id: &str,
        artifact_path: &str,
        adapter_id: &str,
        weights_hash_b3: &str,
    ) -> Result<()> {
        let metadata = serde_json::json!({
            "artifact_path": artifact_path,
            "adapter_id": adapter_id,
            "weights_hash_b3": weights_hash_b3
        });
        let metadata_json = serde_json::to_string(&metadata).map_err(AosError::Serialization)?;

        sqlx::query(
            "UPDATE repository_training_jobs
             SET metadata_json = ?
             WHERE id = ?",
        )
        .bind(&metadata_json)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Update training job with adapter name
    ///
    /// Records the adapter_name field (from migration 0050)
    pub async fn update_training_job_adapter_name(
        &self,
        job_id: &str,
        adapter_name: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE repository_training_jobs
             SET adapter_name = ?
             WHERE id = ?",
        )
        .bind(adapter_name)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get training job by adapter ID
    ///
    /// Looks up the training job that produced a given adapter by searching
    /// the metadata_json field for the adapter_id.
    ///
    /// Evidence: migrations/0050_training_jobs_extensions.sql:18-19
    /// Pattern: metadata_json column for artifact tracking
    pub async fn get_training_job_by_adapter_id(
        &self,
        adapter_id: &str,
    ) -> Result<Option<TrainingJobRecord>> {
        // Search in metadata_json for adapter_id
        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    retryable, retry_of_job_id, stack_id, adapter_id
             FROM repository_training_jobs
             WHERE metadata_json LIKE ?",
        )
        .bind(format!("%\"adapter_id\":\"{}\"%", adapter_id))
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(job)
    }

    /// Update training job with config hash
    ///
    /// Evidence: migrations/0099_training_config_hash.sql
    /// Pattern: Training reproducibility tracking
    pub async fn update_training_job_config_hash(
        &self,
        job_id: &str,
        config_hash_b3: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE repository_training_jobs
             SET config_hash_b3 = ?
             WHERE id = ?",
        )
        .bind(config_hash_b3)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Find training jobs with the same configuration hash
    ///
    /// Evidence: migrations/0099_training_config_hash.sql
    /// Pattern: Reproducibility verification
    ///
    /// # Returns
    /// List of training jobs that used identical training configuration
    pub async fn find_jobs_by_config_hash(
        &self,
        config_hash_b3: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    retryable, retry_of_job_id, stack_id, adapter_id
             FROM repository_training_jobs
             WHERE config_hash_b3 = ?
             ORDER BY started_at DESC",
        )
        .bind(config_hash_b3)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(jobs)
    }

    /// Create training job with full provenance tracking
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Evidence: migrations/0133_training_retryable.sql (retry_of_job_id column)
    /// Pattern: Complete provenance chain from dataset to adapter
    ///
    /// # Arguments
    /// * `job_id` - Optional job ID (if None, generates UUID v7)
    /// * `repo_id` - Repository ID for training context
    /// * `training_config_json` - JSON-serialized training configuration
    /// * `created_by` - User who initiated the training
    /// * `dataset_id` - Optional dataset ID for provenance tracking
    /// * `base_model_id` - Optional base model ID
    /// * `collection_id` - Optional document collection ID
    /// * `tenant_id` - Tenant isolation identifier
    /// * `build_id` - Build/commit identifier for reproducibility
    /// * `source_documents_json` - JSON list of source document IDs
    /// * `retry_of_job_id` - Optional ID of original job this is a retry of (for retry chain tracking)
    pub async fn create_training_job_with_provenance(
        &self,
        job_id: Option<&str>,
        repo_id: &str,
        training_config_json: &str,
        created_by: &str,
        dataset_id: Option<&str>,
        base_model_id: Option<&str>,
        collection_id: Option<&str>,
        tenant_id: Option<&str>,
        build_id: Option<&str>,
        source_documents_json: Option<&str>,
        retry_of_job_id: Option<&str>,
    ) -> Result<String> {
        let id = job_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        let progress = TrainingProgress {
            progress_pct: 0.0,
            current_epoch: 0,
            total_epochs: 3,
            current_loss: 0.0,
            learning_rate: 0.001,
            tokens_per_second: 0.0,
            error_message: None,
        };
        let progress_json = serde_json::to_string(&progress).map_err(AosError::Serialization)?;

        sqlx::query(
            "INSERT INTO repository_training_jobs
             (id, repo_id, training_config_json, status, progress_json, created_by,
              dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
              retry_of_job_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(repo_id)
        .bind(training_config_json)
        .bind("pending")
        .bind(&progress_json)
        .bind(created_by)
        .bind(dataset_id)
        .bind(base_model_id)
        .bind(collection_id)
        .bind(tenant_id)
        .bind(build_id)
        .bind(source_documents_json)
        .bind(retry_of_job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Get adapters trained on a specific dataset
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Pattern: Reverse lookup for provenance queries
    pub async fn get_adapters_trained_on_dataset(&self, dataset_id: &str) -> Result<Vec<String>> {
        let adapter_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT a.id
             FROM adapters a
             JOIN repository_training_jobs tj ON a.training_job_id = tj.id
             WHERE tj.dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(adapter_ids.into_iter().map(|(id,)| id).collect())
    }

    /// Update adapter's training_job_id link
    ///
    /// Evidence: migrations/0101_adapter_training_job_link.sql
    /// Pattern: Link adapter back to training job after training completes
    pub async fn update_adapter_training_job_id(
        &self,
        adapter_id: &str,
        training_job_id: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE adapters SET training_job_id = ? WHERE id = ?")
            .bind(training_job_id)
            .bind(adapter_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Insert a single training metric
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
    /// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
    /// Pattern: Record fine-grained training metrics for monitoring and debugging
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `step` - Training step number (e.g., 1200)
    /// * `epoch` - Optional epoch number (e.g., 3)
    /// * `metric_name` - Name of the metric (e.g., "loss", "learning_rate", "tokens_per_second")
    /// * `value` - Metric value
    ///
    /// # Returns
    /// ID of the inserted metric record
    pub async fn insert_training_metric(
        &self,
        job_id: &str,
        step: i64,
        epoch: Option<i64>,
        metric_name: &str,
        value: f64,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        sqlx::query(
            "INSERT INTO repository_training_metrics
             (id, training_job_id, step, epoch, metric_name, metric_value)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(job_id)
        .bind(step)
        .bind(epoch)
        .bind(metric_name)
        .bind(value)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert training metric: {}", e)))?;

        Ok(id)
    }

    /// Insert multiple training metrics in a batch (for efficiency)
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
    /// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
    /// Pattern: Batch insertion for efficient metric recording
    ///
    /// # Arguments
    /// * `metrics` - Vector of training metric records to insert
    ///
    /// # Implementation Note
    /// This uses a transaction to ensure atomicity and improve performance
    /// when inserting multiple metrics at once (e.g., all metrics from a training step).
    pub async fn insert_training_metrics_batch(&self, metrics: &[TrainingMetricRow]) -> Result<()> {
        if metrics.is_empty() {
            return Ok(());
        }

        // Use a transaction for batch insertion
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("Failed to begin transaction: {}", e)))?;

        for metric in metrics {
            sqlx::query(
                "INSERT INTO repository_training_metrics
                 (id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&metric.id)
            .bind(&metric.training_job_id)
            .bind(metric.step)
            .bind(metric.epoch)
            .bind(&metric.metric_name)
            .bind(metric.metric_value)
            .bind(&metric.metric_timestamp)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to insert training metric in batch: {}", e))
            })?;
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit metrics batch: {}", e)))?;

        Ok(())
    }

    /// Get training metrics for a job, optionally filtered by metric name
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
    /// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
    /// Pattern: Retrieve time-series metrics for visualization and analysis
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `metric_name` - Optional filter for specific metric (e.g., "loss", "learning_rate")
    /// * `limit` - Optional limit on number of records returned
    ///
    /// # Returns
    /// Vector of training metrics ordered by step (oldest first)
    ///
    /// # Examples
    /// ```ignore
    /// // Get all metrics for a job
    /// let metrics = db.get_training_metrics("job-123", None, None).await?;
    ///
    /// // Get only loss metrics, limited to last 100 steps
    /// let loss = db.get_training_metrics("job-123", Some("loss"), Some(100)).await?;
    /// ```
    pub async fn get_training_metrics(
        &self,
        job_id: &str,
        metric_name: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<TrainingMetricRow>> {
        let query = if let Some(name) = metric_name {
            if let Some(lim) = limit {
                sqlx::query_as::<_, TrainingMetricRow>(
                    "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                     FROM repository_training_metrics
                     WHERE training_job_id = ? AND metric_name = ?
                     ORDER BY step ASC
                     LIMIT ?",
                )
                .bind(job_id)
                .bind(name)
                .bind(lim)
            } else {
                sqlx::query_as::<_, TrainingMetricRow>(
                    "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                     FROM repository_training_metrics
                     WHERE training_job_id = ? AND metric_name = ?
                     ORDER BY step ASC",
                )
                .bind(job_id)
                .bind(name)
            }
        } else {
            if let Some(lim) = limit {
                sqlx::query_as::<_, TrainingMetricRow>(
                    "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                     FROM repository_training_metrics
                     WHERE training_job_id = ?
                     ORDER BY step ASC
                     LIMIT ?",
                )
                .bind(job_id)
                .bind(lim)
            } else {
                sqlx::query_as::<_, TrainingMetricRow>(
                    "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                     FROM repository_training_metrics
                     WHERE training_job_id = ?
                     ORDER BY step ASC",
                )
                .bind(job_id)
            }
        };

        let metrics = query
            .fetch_all(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch training metrics: {}", e)))?;

        Ok(metrics)
    }

    /// Update the retryable flag on a training job
    ///
    /// Evidence: migrations/0124_training_retryable.sql
    /// Pattern: Mark failed jobs as retryable or non-retryable based on error type
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `retryable` - Whether the job can be retried (true for OOM/timeout, false for invalid config)
    ///
    /// # Implementation Note
    /// The retryable flag is used to filter failed jobs that can be safely retried
    /// from those that will fail again due to configuration issues.
    /// SQLite stores booleans as INTEGER (0 = false, 1 = true).
    pub async fn update_training_job_retryable(&self, job_id: &str, retryable: bool) -> Result<()> {
        let retryable_int = if retryable { 1 } else { 0 };

        sqlx::query(
            "UPDATE repository_training_jobs
             SET retryable = ?
             WHERE id = ?",
        )
        .bind(retryable_int)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to update training job retryable flag: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Get retry chain for a training job
    ///
    /// Returns all jobs that are retries of the given job ID (direct or transitive).
    /// Useful for displaying retry history in the UI.
    ///
    /// Evidence: migrations/0133_training_retryable.sql (retry_of_job_id column)
    /// Pattern: Traverse retry chain for audit and visualization
    ///
    /// # Arguments
    /// * `original_job_id` - The original job ID to find retries of
    ///
    /// # Returns
    /// Vector of training jobs that are retries of the original, ordered by creation time (newest first)
    pub async fn get_retry_chain(&self, original_job_id: &str) -> Result<Vec<TrainingJobRecord>> {
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    retryable, retry_of_job_id, stack_id, adapter_id
             FROM repository_training_jobs
             WHERE retry_of_job_id = ?
             ORDER BY started_at DESC",
        )
        .bind(original_job_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get retry chain for job {}: {}", original_job_id, e))
        })?;

        Ok(jobs)
    }

    /// Update retry_of_job_id for a training job
    ///
    /// Evidence: migrations/0133_training_retryable.sql (retry_of_job_id column)
    /// Pattern: Link retry job to its original for audit trail
    ///
    /// # Arguments
    /// * `job_id` - The retry job ID
    /// * `original_job_id` - The original job this is a retry of
    pub async fn update_training_job_retry_of(
        &self,
        job_id: &str,
        original_job_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE repository_training_jobs
             SET retry_of_job_id = ?
             WHERE id = ?",
        )
        .bind(original_job_id)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update retry_of_job_id: {}", e)))?;

        Ok(())
    }

    /// Update training job with stack_id and adapter_id after successful training
    ///
    /// Called by orchestrator after stack creation to persist the result IDs
    /// for the chat_bootstrap endpoint.
    ///
    /// Evidence: unified train-to-chat pipeline
    /// Evidence: migrations/0073_training_job_stack_adapter_ids.sql
    /// Pattern: Link training job to its output artifacts (adapter + stack)
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `stack_id` - Optional stack ID created from this training job
    /// * `adapter_id` - Optional adapter ID created from this training job
    ///
    /// # Notes
    /// This method is called when `post_actions.create_stack = true` to record
    /// the full provenance chain from training job to ready-to-use stack.
    pub async fn update_training_job_result_ids(
        &self,
        job_id: &str,
        stack_id: Option<&str>,
        adapter_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE repository_training_jobs
             SET stack_id = ?, adapter_id = ?
             WHERE id = ?",
        )
        .bind(stack_id)
        .bind(adapter_id)
        .bind(job_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update training job result IDs: {}", e))
        })?;

        Ok(())
    }
}
