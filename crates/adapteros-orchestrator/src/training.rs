//! Training job orchestration and management
//!
//! Handles scheduling, executing, and monitoring adapter training jobs.
//! Integrates with MLX backend for actual training operations.

use adapteros_core::AosError;
use adapteros_lora_worker::training::{
    MicroLoRATrainer as WorkerTrainer, TrainingConfig as WorkerTrainingConfig,
    TrainingExample as WorkerTrainingExample,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

// Re-export canonical types from adapteros_types
pub use adapteros_types::training::{TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate};

/// Training service for managing jobs
pub struct TrainingService {
    jobs: Arc<RwLock<HashMap<String, TrainingJob>>>,
    templates: Arc<RwLock<HashMap<String, TrainingTemplate>>>,
    /// Database connection for dataset loading
    db: Option<adapteros_db::Db>,
    /// Storage root for dataset files
    storage_root: Option<PathBuf>,
}

impl TrainingService {
    /// Create a new training service
    pub fn new() -> Self {
        let mut templates = HashMap::new();

        // Add default templates
        templates.insert(
            "general-code".to_string(),
            TrainingTemplate {
                id: "general-code".to_string(),
                name: "General Code Adapter".to_string(),
                description: "Train a general-purpose coding adapter for multiple languages"
                    .to_string(),
                category: "code".to_string(),
                config: TrainingConfig {
                    rank: 16,
                    alpha: 32,
                    ..Default::default()
                },
            },
        );

        templates.insert(
            "framework-specific".to_string(),
            TrainingTemplate {
                id: "framework-specific".to_string(),
                name: "Framework Specific".to_string(),
                description: "Train adapter for specific frameworks (Django, React, FastAPI, etc.)"
                    .to_string(),
                category: "framework".to_string(),
                config: TrainingConfig {
                    rank: 12,
                    alpha: 24,
                    ..Default::default()
                },
            },
        );

        templates.insert(
            "codebase-specific".to_string(),
            TrainingTemplate {
                id: "codebase-specific".to_string(),
                name: "Codebase Specific".to_string(),
                description: "Train adapter for a specific codebase with internal APIs".to_string(),
                category: "codebase".to_string(),
                config: TrainingConfig {
                    rank: 24,
                    alpha: 48,
                    epochs: 4,
                    ..Default::default()
                },
            },
        );

        templates.insert(
            "ephemeral-quick".to_string(),
            TrainingTemplate {
                id: "ephemeral-quick".to_string(),
                name: "Ephemeral Quick Fix".to_string(),
                description: "Quick ephemeral adapter for temporary fixes".to_string(),
                category: "ephemeral".to_string(),
                config: TrainingConfig {
                    rank: 8,
                    alpha: 16,
                    epochs: 1,
                    learning_rate: 0.002,
                    batch_size: 16,
                    targets: vec![
                        "q_proj".to_string(),
                        "k_proj".to_string(),
                        "v_proj".to_string(),
                        "o_proj".to_string(),
                    ],
                    ..Default::default()
                },
            },
        );

        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(templates)),
            db: None,
            storage_root: None,
        }
    }

    /// Create a new training service with database and storage configuration
    pub fn with_db(db: adapteros_db::Db, storage_root: PathBuf) -> Self {
        let mut service = Self::new();
        service.db = Some(db);
        service.storage_root = Some(storage_root);
        service
    }

    /// Set database connection
    pub fn set_db(&mut self, db: adapteros_db::Db) {
        self.db = Some(db);
    }

    /// Set storage root
    pub fn set_storage_root(&mut self, path: PathBuf) {
        self.storage_root = Some(path);
    }

    /// List all training jobs
    pub async fn list_jobs(&self) -> Result<Vec<TrainingJob>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.values().cloned().collect())
    }

    /// Get a specific training job
    pub async fn get_job(&self, job_id: &str) -> Result<TrainingJob> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id)
            .cloned()
            .ok_or_else(|| AosError::NotFound(format!("Training job not found: {}", job_id)).into())
    }

    /// Start a new training job
    pub async fn start_training(
        &self,
        adapter_name: String,
        config: TrainingConfig,
        template_id: Option<String>,
        repo_id: Option<String>,
        dataset_id: Option<String>,
    ) -> Result<TrainingJob> {
        let job_id = format!("train-{}", uuid::Uuid::new_v4());

        let mut job = TrainingJob::new(job_id.clone(), adapter_name, config.clone());
        job.template_id = template_id;
        job.repo_id = repo_id;
        job.dataset_id = dataset_id;

        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job_id.clone(), job.clone());
        }

        // Spawn background training task
        let jobs_ref = self.jobs.clone();
        let cfg_for_run = job.config.clone();
        let job_id_for_run = job.id.clone();
        let dataset_id_for_run = job.dataset_id.clone();
        let db_for_run = self.db.clone();
        let storage_for_run = self.storage_root.clone();
        tokio::spawn(async move {
            if let Err(e) = run_training_job(
                jobs_ref,
                job_id_for_run.clone(),
                cfg_for_run,
                dataset_id_for_run,
                db_for_run,
                storage_for_run,
            )
            .await
            {
                tracing::error!("Training job {} failed: {}", job_id_for_run, e);
            }
        });

        tracing::info!("Training job created: {}", job_id);

        Ok(job)
    }

    /// Cancel a training job
    pub async fn cancel_job(&self, job_id: &str) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            if job.status == TrainingJobStatus::Running || job.status == TrainingJobStatus::Pending
            {
                job.status = TrainingJobStatus::Cancelled;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                info!("Training job cancelled: {}", job_id);
                Ok(())
            } else {
                Err(
                    AosError::Internal(format!("Cannot cancel job in state: {:?}", job.status))
                        .into(),
                )
            }
        } else {
            Err(AosError::Internal(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Update job progress (called by training worker)
    pub async fn update_progress(
        &self,
        job_id: &str,
        epoch: u32,
        loss: f32,
        tokens_per_second: f32,
    ) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.current_epoch = epoch;
            job.current_loss = loss;
            job.tokens_per_second = tokens_per_second;
            job.progress_pct = (epoch as f32 / job.total_epochs as f32) * 100.0;

            if job.status == TrainingJobStatus::Pending {
                job.status = TrainingJobStatus::Running;
                job.started_at = Some(chrono::Utc::now().to_rfc3339());
            }

            Ok(())
        } else {
            Err(AosError::NotFound(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Mark job as completed
    pub async fn complete_job(&self, job_id: &str) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrainingJobStatus::Completed;
            job.progress_pct = 100.0;
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            tracing::info!("Training job completed: {}", job_id);
            Ok(())
        } else {
            Err(AosError::NotFound(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Mark job as failed
    pub async fn fail_job(&self, job_id: &str, error: String) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrainingJobStatus::Failed;
            job.error_message = Some(error);
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            error!("Training job failed: {}", job_id);
            Ok(())
        } else {
            Err(AosError::Internal(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Get training logs
    ///
    /// Returns logs for the specified training job. Currently returns
    /// in-memory progress logs. For production use, integrate with
    /// a persistent log store by adding a database connection to TrainingService.
    pub async fn get_logs(&self, job_id: &str) -> Result<Vec<String>> {
        // Verify job exists and get current state
        let job = self.get_job(job_id).await?;

        // Build logs from job state
        let mut logs = vec![format!("Training job {} created", job_id)];

        if let Some(started) = &job.started_at {
            logs.push(format!("Training started at {}", started));
            logs.push(format!("Configuration: rank={}, alpha={}, epochs={}",
                job.config.rank, job.config.alpha, job.total_epochs));
        }

        if job.current_epoch > 0 {
            for epoch in 1..=job.current_epoch {
                logs.push(format!("Epoch {}/{} completed", epoch, job.total_epochs));
            }
            logs.push(format!("Current loss: {:.4}", job.current_loss));
            if job.tokens_per_second > 0.0 {
                logs.push(format!("Throughput: {:.1} tokens/sec", job.tokens_per_second));
            }
        }

        match job.status {
            TrainingJobStatus::Completed => {
                if let Some(completed) = &job.completed_at {
                    logs.push(format!("Training completed at {}", completed));
                }
            }
            TrainingJobStatus::Failed => {
                if let Some(error) = &job.error_message {
                    logs.push(format!("Training failed: {}", error));
                }
            }
            TrainingJobStatus::Cancelled => {
                logs.push("Training was cancelled".to_string());
            }
            _ => {}
        }

        Ok(logs)
    }

    /// List all training templates
    pub async fn list_templates(&self) -> Result<Vec<TrainingTemplate>> {
        let templates = self.templates.read().await;
        Ok(templates.values().cloned().collect())
    }

    /// Get a specific training template
    pub async fn get_template(&self, template_id: &str) -> Result<TrainingTemplate> {
        let templates = self.templates.read().await;
        templates.get(template_id).cloned().ok_or_else(|| {
            AosError::NotFound(format!("Template not found: {}", template_id)).into()
        })
    }
}

impl Default for TrainingService {
    fn default() -> Self {
        Self::new()
    }
}

/// Background runner for a single training job. Converts orchestrator config into worker trainer
/// config, runs training with per-epoch callback, and updates the shared job map.
async fn run_training_job(
    jobs_ref: Arc<RwLock<HashMap<String, TrainingJob>>>,
    job_id: String,
    orchestrator_cfg: TrainingConfig,
    dataset_id: Option<String>,
    db: Option<adapteros_db::Db>,
    storage_root: Option<PathBuf>,
) -> Result<()> {
    // Transition to running
    {
        let mut jobs = jobs_ref.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = TrainingJobStatus::Running;
            job.started_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    // Map orchestrator config to worker trainer config
    let worker_cfg = WorkerTrainingConfig {
        rank: orchestrator_cfg.rank as usize,
        alpha: orchestrator_cfg.alpha as f32,
        learning_rate: orchestrator_cfg.learning_rate,
        batch_size: orchestrator_cfg.batch_size as usize,
        epochs: orchestrator_cfg.epochs as usize,
        hidden_dim: 768, // default; can be made configurable via orchestrator config later
    };

    // Load training examples from dataset if available, otherwise use synthetic fallback
    let examples: Vec<WorkerTrainingExample> = match (dataset_id, db, storage_root) {
        (Some(ds_id), Some(database), Some(storage)) => {
            use crate::training_dataset_integration::TrainingDatasetManager;
            let dataset_manager = TrainingDatasetManager::new(database, storage, None);
            dataset_manager
                .load_dataset_examples(&ds_id)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to load dataset: {}", e))?
        }
        _ => {
            // Fallback: tiny synthetic batch for testing
            tracing::warn!(
                "No dataset configured for job {}, using synthetic training data",
                job_id
            );
            vec![
                WorkerTrainingExample {
                    input: vec![1, 2, 3],
                    target: vec![4, 5, 6],
                    metadata: Default::default(),
                    weight: 1.0,
                },
                WorkerTrainingExample {
                    input: vec![7, 8, 9],
                    target: vec![10, 11, 12],
                    metadata: Default::default(),
                    weight: 1.0,
                },
            ]
        }
    };

    let mut trainer = WorkerTrainer::new(worker_cfg)?;

    // Run with per-epoch callback to update progress
    let job_id_clone = job_id.clone();
    let jobs_ref_clone = jobs_ref.clone();
    let result = trainer
        .train_with_callback(&examples, move |epoch, loss| {
            let jobs_ref_inner = jobs_ref_clone.clone();
            let job_id_inner = job_id_clone.clone();
            // Fire-and-forget async update
            tokio::spawn(async move {
                let mut jobs = jobs_ref_inner.write().await;
                if let Some(job) = jobs.get_mut(&job_id_inner) {
                    job.current_epoch = epoch as u32;
                    job.current_loss = loss;
                    if job.total_epochs > 0 {
                        job.progress_pct = (epoch as f32 / job.total_epochs as f32) * 100.0;
                    }
                }
            });
        })
        .await;

    match result {
        Ok(_training_result) => {
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = TrainingJobStatus::Completed;
                job.progress_pct = 100.0;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
            Ok(())
        }
        Err(e) => {
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = TrainingJobStatus::Failed;
                job.error_message = Some(e.to_string());
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_list_jobs() {
        let service = TrainingService::new();

        let config = TrainingConfig::default();
        let job = service
            .start_training("test-adapter".to_string(), config, None, None, None)
            .await
            .unwrap();

        assert_eq!(job.status, TrainingJobStatus::Pending);
        assert_eq!(job.adapter_name, "test-adapter");

        let jobs = service.list_jobs().await.unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let service = TrainingService::new();

        let config = TrainingConfig::default();
        let job = service
            .start_training("test-adapter".to_string(), config, None, None, None)
            .await
            .unwrap();

        service.cancel_job(&job.id).await.unwrap();

        let updated_job = service.get_job(&job.id).await.unwrap();
        assert_eq!(updated_job.status, TrainingJobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_update_progress() {
        let service = TrainingService::new();

        let config = TrainingConfig::default();
        let job = service
            .start_training("test-adapter".to_string(), config, None, None, None)
            .await
            .unwrap();

        service
            .update_progress(&job.id, 1, 0.5, 1000.0)
            .await
            .unwrap();

        let updated_job = service.get_job(&job.id).await.unwrap();
        assert_eq!(updated_job.status, TrainingJobStatus::Running);
        assert_eq!(updated_job.current_epoch, 1);
        assert!((updated_job.current_loss - 0.5).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_list_templates() {
        let service = TrainingService::new();
        let templates = service.list_templates().await.unwrap();

        assert!(templates.len() >= 4);
        assert!(templates.iter().any(|t| t.id == "general-code"));
        assert!(templates.iter().any(|t| t.id == "framework-specific"));
    }
}
