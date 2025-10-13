//! Training job orchestration and management
//!
//! Handles scheduling, executing, and monitoring adapter training jobs.
//! Integrates with MLX backend for actual training operations.

use anyhow::{Context, Result};
use adapteros_core::AosError;
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
            config,
        };

        let mut jobs = self.jobs.write().await;
        jobs.insert(job_id.clone(), job.clone());

        // TODO: Actually start the training job with MLX backend
        // For now, just transition to Running state
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_list_jobs() {
        let service = TrainingService::new();

        let config = TrainingConfig::default();
        let job = service
            .start_training("test-adapter".to_string(), config, None, None)
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
            .start_training("test-adapter".to_string(), config, None, None)
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
            .start_training("test-adapter".to_string(), config, None, None)
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
