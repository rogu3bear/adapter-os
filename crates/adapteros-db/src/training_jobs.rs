//! Training job database operations
//!
//! Implements CRUD operations for repository training jobs.
//! Evidence: migrations/0013_git_repository_integration.sql:25-40
//! Pattern: Database schema for training jobs

use crate::Db;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// Builder for creating training job parameters
#[derive(Debug, Default)]
pub struct TrainingJobBuilder {
    adapter_name: Option<String>,
    config: Option<Value>,
    repo_id: Option<String>,
    dataset_path: Option<String>,
    tenant_id: Option<String>,
    template_id: Option<String>,
    directory_root: Option<String>,
    directory_path: Option<String>,
    adapters_root: Option<String>,
    package: Option<bool>,
    register: Option<bool>,
    adapter_id: Option<String>,
    tier: Option<i32>,
}

/// Parameters for training job creation
#[derive(Debug)]
pub struct TrainingJobParams {
    pub adapter_name: String,
    pub config: Value,
    pub repo_id: Option<String>,
    pub dataset_path: Option<String>,
    pub tenant_id: Option<String>,
    pub template_id: Option<String>,
    pub directory_root: Option<String>,
    pub directory_path: Option<String>,
    pub adapters_root: Option<String>,
    pub package: Option<bool>,
    pub register: Option<bool>,
    pub adapter_id: Option<String>,
    pub tier: Option<i32>,
}

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

impl TrainingJobBuilder {
    /// Create a new training job builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the adapter name (required)
    pub fn adapter_name(mut self, adapter_name: impl Into<String>) -> Self {
        self.adapter_name = Some(adapter_name.into());
        self
    }

    /// Set the training configuration as JSON (required)
    pub fn config(mut self, config: Value) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the repository ID (optional)
    pub fn repo_id(mut self, repo_id: Option<impl Into<String>>) -> Self {
        self.repo_id = repo_id.map(|s| s.into());
        self
    }

    /// Set the dataset path (optional)
    pub fn dataset_path(mut self, dataset_path: Option<impl Into<String>>) -> Self {
        self.dataset_path = dataset_path.map(|s| s.into());
        self
    }

    /// Set the tenant ID (optional)
    pub fn tenant_id(mut self, tenant_id: Option<impl Into<String>>) -> Self {
        self.tenant_id = tenant_id.map(|s| s.into());
        self
    }

    /// Set the template ID (optional)
    pub fn template_id(mut self, template_id: Option<impl Into<String>>) -> Self {
        self.template_id = template_id.map(|s| s.into());
        self
    }

    /// Set the directory root (optional)
    pub fn directory_root(mut self, directory_root: Option<impl Into<String>>) -> Self {
        self.directory_root = directory_root.map(|s| s.into());
        self
    }

    /// Set the directory path (optional)
    pub fn directory_path(mut self, directory_path: Option<impl Into<String>>) -> Self {
        self.directory_path = directory_path.map(|s| s.into());
        self
    }

    /// Set the adapters root (optional)
    pub fn adapters_root(mut self, adapters_root: Option<impl Into<String>>) -> Self {
        self.adapters_root = adapters_root.map(|s| s.into());
        self
    }

    /// Set the package flag (optional)
    pub fn package(mut self, package: Option<bool>) -> Self {
        self.package = package;
        self
    }

    /// Set the register flag (optional)
    pub fn register(mut self, register: Option<bool>) -> Self {
        self.register = register;
        self
    }

    /// Set the adapter ID (optional)
    pub fn adapter_id(mut self, adapter_id: Option<impl Into<String>>) -> Self {
        self.adapter_id = adapter_id.map(|s| s.into());
        self
    }

    /// Set the tier (optional)
    pub fn tier(mut self, tier: Option<i32>) -> Self {
        self.tier = tier;
        self
    }

    /// Build the training job parameters
    pub fn build(self) -> Result<TrainingJobParams> {
        Ok(TrainingJobParams {
            adapter_name: self
                .adapter_name
                .ok_or_else(|| anyhow!("adapter_name is required"))?,
            config: self.config.ok_or_else(|| anyhow!("config is required"))?,
            repo_id: self.repo_id,
            dataset_path: self.dataset_path,
            tenant_id: self.tenant_id,
            template_id: self.template_id,
            directory_root: self.directory_root,
            directory_path: self.directory_path,
            adapters_root: self.adapters_root,
            package: self.package,
            register: self.register,
            adapter_id: self.adapter_id,
            tier: self.tier,
        })
    }
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
        let progress_json = serde_json::to_string(&progress)?;

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
        .await?;

        Ok(id)
    }

    /// Get a training job by ID
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn get_training_job(&self, job_id: &str) -> Result<Option<TrainingJobRecord>> {
        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json, 
                    started_at, completed_at, created_by 
             FROM repository_training_jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(self.pool())
        .await?;

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
        let progress_json = serde_json::to_string(progress)?;

        sqlx::query(
            "UPDATE repository_training_jobs 
             SET progress_json = ? 
             WHERE id = ?",
        )
        .bind(&progress_json)
        .bind(job_id)
        .execute(self.pool())
        .await?;

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
        .await?;

        Ok(())
    }

    /// List training jobs for a repository
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn list_training_jobs(&self, repo_id: &str) -> Result<Vec<TrainingJobRecord>> {
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json, 
                    started_at, completed_at, created_by 
             FROM repository_training_jobs 
             WHERE repo_id = ? 
             ORDER BY started_at DESC",
        )
        .bind(repo_id)
        .fetch_all(self.pool())
        .await?;

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
                    started_at, completed_at, created_by 
             FROM repository_training_jobs 
             WHERE status = ? 
             ORDER BY started_at DESC",
        )
        .bind(status)
        .fetch_all(self.pool())
        .await?;

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
            .await?;

        Ok(())
    }

    /// Start a training session with complex parameters
    ///
    /// Use [`TrainingJobBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::training_jobs::TrainingJobBuilder;
    /// use serde_json::json;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let config = json!({
    ///     "rank": 16,
    ///     "alpha": 32,
    ///     "targets": ["q_proj"],
    ///     "epochs": 3,
    ///     "learning_rate": 0.001,
    ///     "batch_size": 4,
    ///     "warmup_steps": 100,
    ///     "max_seq_length": 512,
    ///     "gradient_accumulation_steps": 2
    /// });
    ///
    /// let params = TrainingJobBuilder::new()
    ///     .adapter_name("my_adapter")
    ///     .config(config)
    ///     .repo_id(Some("github.com/org/repo"))
    ///     .dataset_path(Some("/path/to/dataset"))
    ///     .tenant_id(Some("tenant-123"))
    ///     .template_id(Some("template-456"))
    ///     .directory_root(Some("/repo/root"))
    ///     .directory_path(Some("src/"))
    ///     .adapters_root(Some("/adapters/"))
    ///     .package(Some(true))
    ///     .register(Some(true))
    ///     .adapter_id(Some("adapter-789"))
    ///     .tier(Some(1))
    ///     .build()
    ///     .expect("required fields");
    ///
    /// db.start_training_session(params).await.expect("training session started");
    /// # }
    /// ```
    pub async fn start_training_session(&self, params: TrainingJobParams) -> Result<String> {
        let training_config_json = serde_json::to_string(&params.config)?;
        let created_by = params
            .tenant_id
            .clone()
            .unwrap_or_else(|| "system".to_string());

        // Use repo_id from params if available, otherwise use a default
        let repo_id = params.repo_id.unwrap_or_else(|| "default-repo".to_string());

        self.create_training_job(&repo_id, &training_config_json, &created_by)
            .await
    }
}
