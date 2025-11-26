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
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json
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
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json
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
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json
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
                    rtj.build_id, rtj.source_documents_json
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
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json
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
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json
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
    /// Pattern: Complete provenance chain from dataset to adapter
    pub async fn create_training_job_with_provenance(
        &self,
        repo_id: &str,
        training_config_json: &str,
        created_by: &str,
        dataset_id: Option<&str>,
        base_model_id: Option<&str>,
        collection_id: Option<&str>,
        tenant_id: Option<&str>,
        build_id: Option<&str>,
        source_documents_json: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
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
              dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
}
