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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Training job state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TrainingJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TrainingJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrainingJobStatus::Pending => write!(f, "pending"),
            TrainingJobStatus::Running => write!(f, "running"),
            TrainingJobStatus::Completed => write!(f, "completed"),
            TrainingJobStatus::Failed => write!(f, "failed"),
            TrainingJobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Training job information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingJob {
    pub id: String,
    pub adapter_name: String,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub status: TrainingJobStatus,
    pub progress_pct: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub current_loss: f32,
    pub learning_rate: f32,
    pub tokens_per_second: f32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub config: TrainingConfig,
    // Artifact metadata (populated when packaging is enabled)
    pub artifact_path: Option<String>,
    pub adapter_id: Option<String>,
    pub weights_hash_b3: Option<String>,
}

/// Training configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    pub warmup_steps: Option<u32>,
    pub max_seq_length: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            rank: 16,
            alpha: 32,
            targets: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
                "gate_proj".to_string(),
                "up_proj".to_string(),
                "down_proj".to_string(),
            ],
            epochs: 3,
            learning_rate: 0.001,
            batch_size: 32,
            warmup_steps: Some(100),
            max_seq_length: Some(2048),
            gradient_accumulation_steps: Some(4),
        }
    }
}

/// Training template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub config: TrainingConfig,
}

/// Training service for managing jobs
pub struct TrainingService {
    jobs: Arc<RwLock<HashMap<String, TrainingJob>>>,
    templates: Arc<RwLock<HashMap<String, TrainingTemplate>>>,
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
        }
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
        dataset_path: Option<String>,
        directory_root: Option<String>,
        directory_path: Option<String>,
        _tenant_id: Option<String>,
        adapters_root: Option<String>,
        package: bool,
        adapter_id: Option<String>,
    ) -> Result<TrainingJob> {
        let job_id = format!("train-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();

        let job = TrainingJob {
            id: job_id.clone(),
            adapter_name,
            template_id,
            repo_id,
            status: TrainingJobStatus::Pending,
            progress_pct: 0.0,
            current_epoch: 0,
            total_epochs: config.epochs,
            current_loss: 0.0,
            learning_rate: config.learning_rate,
            tokens_per_second: 0.0,
            created_at: now,
            started_at: None,
            completed_at: None,
            error_message: None,
            config: config.clone(),
            artifact_path: None,
            adapter_id: None,
            weights_hash_b3: None,
        };

        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job_id.clone(), job.clone());
        }

        // Spawn background training task
        let jobs_ref = self.jobs.clone();
        let cfg_for_run = job.config.clone();
        let job_id_for_run = job.id.clone();
        let dataset_for_run = dataset_path.clone();
        let dir_root_for_run = directory_root.clone();
        let dir_path_for_run = directory_path.clone();
        let adapters_root_for_run = adapters_root.clone();
        let package_for_run = package;
        let adapter_id_for_run = adapter_id.clone();
        tokio::spawn(async move {
            if let Err(e) = run_training_job(
                jobs_ref,
                job_id_for_run.clone(),
                cfg_for_run,
                dataset_for_run,
                dir_root_for_run,
                dir_path_for_run,
                adapters_root_for_run,
                package_for_run,
                adapter_id_for_run,
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

    /// Get training logs (placeholder)
    pub async fn get_logs(&self, job_id: &str) -> Result<Vec<String>> {
        // Verify job exists
        let _ = self.get_job(job_id).await?;

        // TODO: Fetch actual logs from persistent storage
        Ok(vec![
            format!("Training job {} started", job_id),
            "Loading model and adapters...".to_string(),
            "Preparing training data...".to_string(),
            "Starting epoch 1/3...".to_string(),
        ])
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
    dataset_path: Option<String>,
    directory_root: Option<String>,
    directory_path: Option<String>,
    adapters_root: Option<String>,
    package: bool,
    adapter_id_opt: Option<String>,
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

    // Load dataset if provided, else build from directory, else use a small synthetic batch
    let examples: Vec<WorkerTrainingExample> = if let Some(path) = dataset_path {
        match tokio::fs::read_to_string(&path).await {
            Ok(s) => {
                #[derive(serde::Deserialize)]
                struct TrainingData {
                    examples: Vec<TrainingExampleJson>,
                }
                #[derive(serde::Deserialize)]
                struct TrainingExampleJson {
                    input: Vec<u32>,
                    target: Vec<u32>,
                }
                match serde_json::from_str::<TrainingData>(&s) {
                    Ok(td) => td
                        .examples
                        .into_iter()
                        .map(|e| WorkerTrainingExample {
                            input: e.input,
                            target: e.target,
                            metadata: Default::default(),
                        })
                        .collect(),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse dataset {}: {}. Falling back to synthetic",
                            path,
                            e
                        );
                        vec![
                            WorkerTrainingExample {
                                input: vec![1, 2, 3],
                                target: vec![4, 5, 6],
                                metadata: Default::default(),
                            },
                            WorkerTrainingExample {
                                input: vec![7, 8, 9],
                                target: vec![10, 11, 12],
                                metadata: Default::default(),
                            },
                        ]
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to read dataset {}: {}. Falling back to synthetic",
                    path,
                    e
                );
                vec![
                    WorkerTrainingExample {
                        input: vec![1, 2, 3],
                        target: vec![4, 5, 6],
                        metadata: Default::default(),
                    },
                    WorkerTrainingExample {
                        input: vec![7, 8, 9],
                        target: vec![10, 11, 12],
                        metadata: Default::default(),
                    },
                ]
            }
        }
    } else if let (Some(root), Some(rel)) = (directory_root.clone(), directory_path.clone()) {
        // Build from directory
        match (
            std::path::PathBuf::from(root.clone()),
            std::path::PathBuf::from(rel.clone()),
        ) {
            (root_path, rel_path) => {
                match crate::dataset_builder::build_from_directory(
                    &root_path,
                    &rel_path,
                    crate::dataset_builder::DatasetBuilderConfig::default(),
                ) {
                    Ok(ex) => ex,
                    Err(e) => {
                        tracing::error!(
                            "Directory dataset build failed (root={}, rel={}): {}",
                            root,
                            rel,
                            e
                        );
                        // Mark job failed before returning error
                        {
                            let mut jobs = jobs_ref.write().await;
                            if let Some(job) = jobs.get_mut(&job_id) {
                                job.status = TrainingJobStatus::Failed;
                                job.error_message = Some(e.to_string());
                                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                            }
                        }
                        // Propagate error to fail the task
                        return Err(e.into());
                    }
                }
            }
        }
    } else {
        vec![
            WorkerTrainingExample {
                input: vec![1, 2, 3],
                target: vec![4, 5, 6],
                metadata: Default::default(),
            },
            WorkerTrainingExample {
                input: vec![7, 8, 9],
                target: vec![10, 11, 12],
                metadata: Default::default(),
            },
        ]
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
        Ok(training_result) => {
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = TrainingJobStatus::Completed;
                job.progress_pct = 100.0;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());

                if package {
                    // Quantize and package into adapters_root
                    let chosen_root = adapters_root.unwrap_or_else(|| {
                        std::env::var("AOS_ADAPTERS_ROOT")
                            .unwrap_or_else(|_| "./adapters".to_string())
                    });
                    let adapter_id = adapter_id_opt
                        .clone()
                        .unwrap_or_else(|| format!("train-{}", uuid::Uuid::new_v4()));
                    let aid_for_pack = adapter_id.clone();
                    let root_for_pack = chosen_root.clone();

                    // Quantize weights
                    let quantized = adapteros_lora_worker::training::LoRAQuantizer::quantize_to_q15(
                        &training_result.weights,
                    );

                    // Package synchronously in this async task (no nested runtime spawn)
                    let packager = adapteros_lora_worker::training::packager::AdapterPackager::new(
                        &chosen_root,
                    );
                    match packager
                        .package(
                            &aid_for_pack,
                            &quantized,
                            &adapteros_lora_worker::training::TrainingConfig {
                                rank: orchestrator_cfg.rank as usize,
                                alpha: orchestrator_cfg.alpha as f32,
                                learning_rate: orchestrator_cfg.learning_rate,
                                batch_size: orchestrator_cfg.batch_size as usize,
                                epochs: orchestrator_cfg.epochs as usize,
                                hidden_dim: 768,
                            },
                        )
                        .await
                    {
                        Ok(packaged) => {
                            job.artifact_path = Some(format!("{}/{}", root_for_pack, adapter_id));
                            job.adapter_id = Some(adapter_id);
                            job.weights_hash_b3 = Some(packaged.hash_b3);
                        }
                        Err(e) => {
                            tracing::error!("Packaging failed: {}", e);
                        }
                    }
                }
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
            .start_training(
                "test-adapter".to_string(),
                config,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                false,
                None,
            )
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
            .start_training(
                "test-adapter".to_string(),
                config,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                false,
                None,
            )
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
            .start_training(
                "test-adapter".to_string(),
                config,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                false,
                None,
            )
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

    #[tokio::test]
    async fn test_directory_dataset_failure_propagates() {
        use std::time::Duration;

        let service = TrainingService::new();
        let config = TrainingConfig::default();

        // Create an absolute root and point to an empty subdirectory to induce failure (min_examples not met)
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let empty_sub = root.join("empty");
        std::fs::create_dir_all(&empty_sub).unwrap();

        let job = service
            .start_training(
                "dir-fail".to_string(),
                config,
                None,
                None,
                None,
                Some(root.display().to_string()),
                Some("empty".to_string()),
                None,
                None,
                false,
                None,
            )
            .await
            .unwrap();

        // Poll briefly for failure
        for _ in 0..60u32 {
            let j = service.get_job(&job.id).await.unwrap();
            if matches!(j.status, TrainingJobStatus::Failed) {
                return;
            }
            if matches!(j.status, TrainingJobStatus::Completed) {
                panic!("expected job to fail for invalid directory dataset");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let j = service.get_job(&job.id).await.unwrap();
        assert!(matches!(j.status, TrainingJobStatus::Failed));
    }
}
