//! Training job database operations
//!
//! Implements CRUD operations for repository training jobs.
//! Evidence: migrations/0013_git_repository_integration.sql:25-40
//! Pattern: Database schema for training jobs

use crate::Db;
use adapteros_core::{AosError, Result};
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
        .execute(self.pool())
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
                    created_at, metadata_json
             FROM repository_training_jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(self.pool())
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
        .execute(self.pool())
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
        .execute(self.pool())
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
                    created_at, metadata_json
             FROM repository_training_jobs
             WHERE repo_id = ?
             ORDER BY started_at DESC",
        )
        .bind(repo_id)
        .fetch_all(self.pool())
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
                    created_at, metadata_json
             FROM repository_training_jobs
             WHERE status = ?
             ORDER BY started_at DESC",
        )
        .bind(status)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(jobs)
    }

    /// Delete a training job
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn delete_training_job(&self, job_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM repository_training_jobs WHERE id = ?")
            .bind(job_id)
            .execute(self.pool())
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
        .execute(self.pool())
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
        .execute(self.pool())
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
                    created_at, metadata_json
             FROM repository_training_jobs
             WHERE metadata_json LIKE ?",
        )
        .bind(format!("%\"adapter_id\":\"{}\"%", adapter_id))
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(job)
    }
}
