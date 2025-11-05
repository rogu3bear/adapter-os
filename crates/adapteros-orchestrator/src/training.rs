//! Training job orchestration and management
//!
//! Handles scheduling, executing, and monitoring adapter training jobs.
//! Integrates with MLX backend for actual training operations.

use adapteros_core::AosError;
use adapteros_core::{TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate};
use adapteros_db::training_jobs::{TrainingJobRecord, TrainingProgress};
use adapteros_db::Db;
use adapteros_lora_worker::training::{
    MicroLoRATrainer as WorkerTrainer, TrainingConfig as WorkerTrainingConfig,
    TrainingExample as WorkerTrainingExample,
};
use anyhow::{anyhow, Result};
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};

/// Builder for creating training job parameters
#[derive(Debug, Default)]
pub struct TrainingJobBuilder {
    adapter_name: Option<String>,
    config: Option<TrainingConfig>,
    template_id: Option<String>,
    repo_id: Option<String>,
    dataset_path: Option<String>,
    directory_root: Option<String>,
    directory_path: Option<String>,
    tenant_id: Option<String>,
    adapters_root: Option<String>,
    package: Option<bool>,
    adapter_id: Option<String>,
}

/// Parameters for training job creation
#[derive(Debug)]
pub struct TrainingJobParams {
    pub adapter_name: String,
    pub config: TrainingConfig,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub dataset_path: Option<String>,
    pub directory_root: Option<String>,
    pub directory_path: Option<String>,
    pub tenant_id: Option<String>,
    pub adapters_root: Option<String>,
    pub package: bool,
    pub adapter_id: Option<String>,
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

    /// Set the training configuration (required)
    pub fn config(mut self, config: TrainingConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the template ID (optional)
    pub fn template_id(mut self, template_id: Option<impl Into<String>>) -> Self {
        self.template_id = template_id.map(|s| s.into());
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

    /// Set the tenant ID (optional)
    pub fn tenant_id(mut self, tenant_id: Option<impl Into<String>>) -> Self {
        self.tenant_id = tenant_id.map(|s| s.into());
        self
    }

    /// Set the adapters root directory (optional)
    pub fn adapters_root(mut self, adapters_root: Option<impl Into<String>>) -> Self {
        self.adapters_root = adapters_root.map(|s| s.into());
        self
    }

    /// Set whether to package the adapter (optional, defaults to false)
    pub fn package(mut self, package: bool) -> Self {
        self.package = Some(package);
        self
    }

    /// Set the adapter ID (optional)
    pub fn adapter_id(mut self, adapter_id: Option<impl Into<String>>) -> Self {
        self.adapter_id = adapter_id.map(|s| s.into());
        self
    }

    /// Build the training job parameters
    pub fn build(self) -> Result<TrainingJobParams> {
        Ok(TrainingJobParams {
            adapter_name: self
                .adapter_name
                .ok_or_else(|| anyhow!("adapter_name is required"))?,
            config: self.config.ok_or_else(|| anyhow!("config is required"))?,
            template_id: self.template_id,
            repo_id: self.repo_id,
            dataset_path: self.dataset_path,
            directory_root: self.directory_root,
            directory_path: self.directory_path,
            tenant_id: self.tenant_id,
            adapters_root: self.adapters_root,
            package: self.package.unwrap_or(false),
            adapter_id: self.adapter_id,
        })
    }
}

/// Per-job control handle for pause/resume/cancel
pub struct JobControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
    resume_notify: tokio::sync::Notify,
}

impl JobControl {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            resume_notify: tokio::sync::Notify::new(),
        })
    }
}

/// Training service for managing jobs
pub struct TrainingService {
    db: Option<Arc<Db>>,
    /// In-memory cache for active jobs (for fast access)
    jobs_cache: Arc<RwLock<HashMap<String, TrainingJob>>>,
    templates: Arc<RwLock<HashMap<String, TrainingTemplate>>>,
    base_model: Arc<String>,
    controls: Arc<RwLock<HashMap<String, Arc<JobControl>>>>,
    /// Log directory for training job logs
    log_dir: PathBuf,
    /// Broadcast channel for training events (SSE streaming)
    event_tx: broadcast::Sender<serde_json::Value>,
}

const DEFAULT_BASE_MODEL: &str = "qwen2.5-7b";

impl TrainingService {
    /// Create a new training service (in-memory only, for backward compatibility)
    pub fn new() -> Self {
        Self::new_with_base_model(DEFAULT_BASE_MODEL)
    }

    /// Create a new training service with a specific base model identifier
    pub fn new_with_base_model<S: Into<String>>(base_model: S) -> Self {
        let base_model = Arc::new(base_model.into());
        let templates = Self::default_templates();
        let log_dir = std::env::var("AOS_TRAINING_LOGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/training_logs"));
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            db: None,
            jobs_cache: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(templates)),
            base_model,
            controls: Arc::new(RwLock::new(HashMap::new())),
            log_dir,
            event_tx,
        }
    }

    /// Create a new training service with database persistence
    pub fn new_with_db<S: Into<String>>(db: Arc<Db>, base_model: S) -> Self {
        let base_model = Arc::new(base_model.into());
        let templates = Self::default_templates();
        let log_dir = std::env::var("AOS_TRAINING_LOGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/training_logs"));
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            db: Some(db),
            jobs_cache: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(templates)),
            base_model,
            controls: Arc::new(RwLock::new(HashMap::new())),
            log_dir,
            event_tx,
        }
    }

    /// Create a new training service with database and event broadcaster
    pub fn new_with_db_and_events<S: Into<String>>(
        db: Arc<Db>,
        base_model: S,
        event_tx: broadcast::Sender<serde_json::Value>,
    ) -> Self {
        let base_model = Arc::new(base_model.into());
        let templates = Self::default_templates();
        let log_dir = std::env::var("AOS_TRAINING_LOGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/training_logs"));

        Self {
            db: Some(db),
            jobs_cache: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(templates)),
            base_model,
            controls: Arc::new(RwLock::new(HashMap::new())),
            log_dir,
            event_tx,
        }
    }

    /// Get a receiver for training events (for SSE streaming)
    pub fn subscribe_events(&self) -> broadcast::Receiver<serde_json::Value> {
        self.event_tx.subscribe()
    }

    /// Emit a training event
    fn emit_event(&self, event_type: &str, job_id: &str, payload: serde_json::Value) {
        let event = json!({
            "event_type": event_type,
            "job_id": job_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "payload": payload,
        });
        if let Err(e) = self.event_tx.send(event) {
            tracing::warn!(
                job_id = %job_id,
                event_type = %event_type,
                error = %e,
                "Failed to emit training event"
            );
        }
    }

    /// Get log file path for a job
    fn log_file_path(&self, job_id: &str) -> PathBuf {
        self.log_dir.join(format!("{}.log", job_id))
    }

    /// Append a log entry to the job's log file
    fn append_log(&self, job_id: &str, message: &str) -> Result<()> {
        // Ensure log directory exists
        if let Some(parent) = self.log_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AosError::Io(format!(
                    "Failed to ensure parent log directory {} for job {}: {}",
                    parent.display(),
                    job_id,
                    e
                ))
            })?;
        }
        std::fs::create_dir_all(&self.log_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to ensure log directory {} for job {}: {}",
                self.log_dir.display(),
                job_id,
                e
            ))
        })?;

        let log_path = self.log_file_path(job_id);
        let timestamp = chrono::Utc::now().to_rfc3339();
        let log_line = format!("[{}] {}\n", timestamp, message);

        // Append to log file
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| {
                AosError::Io(format!(
                    "Failed to open log file for job {} ({}): {}",
                    job_id,
                    log_path.display(),
                    e
                ))
            })?;

        file.write_all(log_line.as_bytes()).map_err(|e| {
            AosError::Io(format!(
                "Failed to write to log file for job {} ({}): {}",
                job_id,
                log_path.display(),
                e
            ))
        })?;
        file.flush().map_err(|e| {
            AosError::Io(format!(
                "Failed to flush log file for job {} ({}): {}",
                job_id,
                log_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    /// Convert TrainingJobRecord to TrainingJob
    fn record_to_job(record: TrainingJobRecord) -> Result<TrainingJob> {
        let config: TrainingConfig =
            serde_json::from_str(&record.training_config_json).map_err(|e| {
                AosError::Internal(format!(
                    "Failed to parse training config for job {}: {}",
                    record.id, e
                ))
            })?;

        let progress: TrainingProgress =
            serde_json::from_str(&record.progress_json).map_err(|e| {
                AosError::Internal(format!(
                    "Failed to parse training progress for job {}: {}",
                    record.id, e
                ))
            })?;

        let status = match record.status.as_str() {
            "pending" => TrainingJobStatus::Pending,
            "running" => TrainingJobStatus::Running,
            "paused" => TrainingJobStatus::Paused,
            "completed" => TrainingJobStatus::Completed,
            "failed" => TrainingJobStatus::Failed,
            "cancelled" => TrainingJobStatus::Cancelled,
            _ => TrainingJobStatus::Pending,
        };

        let metadata: Option<HashMap<String, serde_json::Value>> = record
            .metadata_json
            .as_ref()
            .and_then(|m| serde_json::from_str(m).ok());

        Ok(TrainingJob {
            id: record.id,
            adapter_name: record.adapter_name.unwrap_or_default(),
            template_id: record.template_id,
            repo_id: Some(record.repo_id),
            status,
            progress_pct: progress.progress_pct,
            current_epoch: progress.current_epoch,
            total_epochs: progress.total_epochs,
            current_loss: progress.current_loss,
            learning_rate: progress.learning_rate,
            tokens_per_second: progress.tokens_per_second,
            created_at: record
                .created_at
                .unwrap_or_else(|| record.started_at.clone()),
            started_at: Some(record.started_at),
            completed_at: record.completed_at,
            error_message: progress.error_message,
            config,
            artifact_path: metadata
                .as_ref()
                .and_then(|m| m.get("artifact_path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            adapter_id: metadata
                .as_ref()
                .and_then(|m| m.get("adapter_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            weights_hash_b3: metadata
                .as_ref()
                .and_then(|m| m.get("weights_hash_b3"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// Convert TrainingJob to database operations
    fn job_to_db_data(job: &TrainingJob) -> Result<(String, String, Option<String>)> {
        let config_json = serde_json::to_string(&job.config).map_err(|e| {
            AosError::Internal(format!(
                "Failed to serialize training config for job {}: {}",
                job.id, e
            ))
        })?;

        let progress = TrainingProgress {
            progress_pct: job.progress_pct,
            current_epoch: job.current_epoch,
            total_epochs: job.total_epochs,
            current_loss: job.current_loss,
            learning_rate: job.learning_rate,
            tokens_per_second: job.tokens_per_second,
            error_message: job.error_message.clone(),
        };
        let progress_json = serde_json::to_string(&progress).map_err(|e| {
            AosError::Internal(format!(
                "Failed to serialize training progress for job {}: {}",
                job.id, e
            ))
        })?;

        let metadata = json!({
            "artifact_path": job.artifact_path,
            "adapter_id": job.adapter_id,
            "weights_hash_b3": job.weights_hash_b3,
        });
        let metadata_json = serde_json::to_string(&metadata).map_err(|e| {
            AosError::Internal(format!(
                "Failed to serialize training metadata for job {}: {}",
                job.id, e
            ))
        })?;

        Ok((config_json, progress_json, Some(metadata_json)))
    }

    fn default_templates() -> HashMap<String, TrainingTemplate> {
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

        templates
    }

    /// List all training jobs
    pub async fn list_jobs(&self) -> Result<Vec<TrainingJob>> {
        if let Some(ref db) = self.db {
            // Load from database
            let records = db.list_all_training_jobs().await?;
            let mut jobs = Vec::new();
            for record in records {
                match Self::record_to_job(record) {
                    Ok(job) => jobs.push(job),
                    Err(e) => {
                        tracing::warn!("Failed to convert job record: {}", e);
                    }
                }
            }
            Ok(jobs)
        } else {
            // Fallback to in-memory cache
            let jobs = self.jobs_cache.read().await;
            Ok(jobs.values().cloned().collect())
        }
    }

    /// Get a specific training job
    pub async fn get_job(&self, job_id: &str) -> Result<TrainingJob> {
        // Check cache first for fast access
        {
            let cache = self.jobs_cache.read().await;
            if let Some(job) = cache.get(job_id) {
                return Ok(job.clone());
            }
        }

        // Load from database if available
        if let Some(ref db) = self.db {
            if let Some(record) = db.get_training_job(job_id).await? {
                let job = Self::record_to_job(record)?;
                // Update cache
                {
                    let mut cache = self.jobs_cache.write().await;
                    cache.insert(job_id.to_string(), job.clone());
                }
                return Ok(job);
            }
        }

        Err(AosError::NotFound(format!("Training job not found: {}", job_id)).into())
    }

    /// Start a new training job
    ///
    /// Use `TrainingJobBuilder` to construct complex parameter sets:
    /// ```rust
    /// let config = TrainingConfig::default();
    /// let params = TrainingJobBuilder::new()
    ///     .adapter_name("my-adapter")
    ///     .config(config)
    ///     .repo_id(Some("github.com/org/repo"))
    ///     .dataset_path(Some("/path/to/data"))
    ///     .package(true)
    ///     .build()?;
    /// let job = orchestrator.start_training(params).await?;
    /// ```
    pub async fn start_training(&self, params: TrainingJobParams) -> Result<TrainingJob> {
        let job_id = format!("train-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();

        let job = TrainingJob {
            id: job_id.clone(),
            adapter_name: params.adapter_name.clone(),
            template_id: params.template_id.clone(),
            repo_id: params.repo_id.clone(),
            status: TrainingJobStatus::Pending,
            progress_pct: 0.0,
            current_epoch: 0,
            total_epochs: params.config.epochs,
            current_loss: 0.0,
            learning_rate: params.config.learning_rate,
            tokens_per_second: 0.0,
            created_at: now,
            started_at: None,
            completed_at: None,
            error_message: None,
            config: params.config.clone(),
            artifact_path: None,
            adapter_id: params.adapter_id.clone(),
            weights_hash_b3: None,
        };

        // Persist to database if available
        if let Some(ref db) = self.db {
            let (config_json, _progress_json, metadata_json) = Self::job_to_db_data(&job)?;
            let repo_id = params.repo_id.as_deref().unwrap_or("default-repo");
            let created_by = params.tenant_id.as_deref().unwrap_or("system");

            db.create_training_job_with_metadata(
                repo_id,
                &config_json,
                created_by,
                Some(&job.adapter_name),
                job.template_id.as_deref(),
                metadata_json.as_deref(),
            )
            .await?;
        }

        // Update cache
        {
            let mut cache = self.jobs_cache.write().await;
            cache.insert(job_id.clone(), job.clone());
        }

        // Initialize job control entry
        {
            let mut controls = self.controls.write().await;
            controls.insert(job_id.clone(), JobControl::new());
        }

        // Spawn background training task
        let jobs_cache_ref = self.jobs_cache.clone();
        let db_ref = self.db.clone();
        let cfg_for_run = job.config.clone();
        let job_id_for_run = job.id.clone();
        let dataset_for_run = params.dataset_path.clone();
        let dir_root_for_run = params.directory_root.clone();
        let dir_path_for_run = params.directory_path.clone();
        let adapters_root_for_run = params.adapters_root.clone();
        let package_for_run = params.package;
        let adapter_id_for_run = params.adapter_id.clone();
        let controls_ref = self.controls.clone();
        let base_model_for_run = Arc::clone(&self.base_model);
        let log_dir_for_run = self.log_dir.clone();
        tokio::spawn(async move {
            if let Err(e) = run_training_job(
                jobs_cache_ref,
                db_ref,
                controls_ref,
                job_id_for_run.clone(),
                cfg_for_run,
                dataset_for_run,
                dir_root_for_run,
                dir_path_for_run,
                adapters_root_for_run,
                package_for_run,
                adapter_id_for_run,
                base_model_for_run,
                log_dir_for_run,
            )
            .await
            {
                tracing::error!("Training job {} failed: {}", job_id_for_run, e);
            }
        });

        tracing::info!("Training job created: {}", job_id);

        // Log job creation
        if let Err(e) = self.append_log(&job_id, &format!("Training job {} created", job_id)) {
            tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
        }
        if let Err(e) = self.append_log(&job_id, &format!("Adapter name: {}", job.adapter_name)) {
            tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
        }
        if let Some(ref template_id) = job.template_id {
            if let Err(e) = self.append_log(&job_id, &format!("Template: {}", template_id)) {
                tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
            }
        }
        if let Err(e) = self.append_log(
            &job_id,
            &format!(
                "Config: rank={}, epochs={}, lr={}",
                job.config.rank, job.config.epochs, job.config.learning_rate
            ),
        ) {
            tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
        }

        // Emit event
        self.emit_event(
            "job_started",
            &job_id,
            json!({
                "adapter_name": job.adapter_name,
                "template_id": job.template_id,
                "repo_id": job.repo_id,
                "status": "pending",
                "config": job.config,
            }),
        );

        Ok(job)
    }

    /// Cancel a training job
    pub async fn cancel_job(&self, job_id: &str) -> Result<()> {
        // Set cancelled flag and wake any paused waiters
        if let Some(control) = self.controls.read().await.get(job_id).cloned() {
            control.cancelled.store(true, Ordering::SeqCst);
            control.resume_notify.notify_waiters();
        }

        let mut cache = self.jobs_cache.write().await;
        if let Some(job) = cache.get_mut(job_id) {
            if matches!(
                job.status,
                TrainingJobStatus::Running | TrainingJobStatus::Pending | TrainingJobStatus::Paused
            ) {
                job.status = TrainingJobStatus::Cancelled;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());

                // Update database
                if let Some(ref db) = self.db {
                    db.update_training_status(job_id, "cancelled").await?;
                }

                // Emit event
                self.emit_event(
                    "job_cancelled",
                    job_id,
                    json!({
                        "status": "cancelled",
                    }),
                );

                info!("Training job cancelled: {}", job_id);
                Ok(())
            } else {
                Err(AosError::Internal(format!(
                    "Cannot cancel job {} in state: {:?}",
                    job_id, job.status
                ))
                .into())
            }
        } else {
            Err(AosError::Internal(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Pause a training job (idempotent)
    pub async fn pause_job(&self, job_id: &str) -> Result<()> {
        // Update job status with validation
        {
            let mut cache = self.jobs_cache.write().await;
            let job = cache
                .get_mut(job_id)
                .ok_or_else(|| AosError::NotFound(format!("Training job not found: {}", job_id)))?;

            match job.status {
                TrainingJobStatus::Completed
                | TrainingJobStatus::Failed
                | TrainingJobStatus::Cancelled => {
                    return Err(AosError::Internal(format!(
                        "Cannot pause terminal job {} in state: {:?}",
                        job_id, job.status
                    ))
                    .into())
                }
                TrainingJobStatus::Paused => {
                    // Idempotent
                    return Ok(());
                }
                _ => {
                    job.status = TrainingJobStatus::Paused;
                    // Update database
                    if let Some(ref db) = self.db {
                        db.update_training_status(job_id, "paused").await?;
                    }
                    // Emit event
                    self.emit_event(
                        "job_paused",
                        job_id,
                        json!({
                            "status": "paused",
                        }),
                    );
                }
            }
        }

        // Set paused flag in control map
        let control = {
            let controls = self.controls.read().await;
            controls
                .get(job_id)
                .cloned()
                .ok_or_else(|| AosError::NotFound(format!("Training job not found: {}", job_id)))?
        };
        control.paused.store(true, Ordering::SeqCst);
        info!("Training job paused: {}", job_id);
        Ok(())
    }

    /// Resume a training job (idempotent)
    pub async fn resume_job(&self, job_id: &str) -> Result<()> {
        #[allow(unused_assignments)]
        let mut should_notify = false;

        // Update job status with validation
        {
            let mut cache = self.jobs_cache.write().await;
            let job = cache
                .get_mut(job_id)
                .ok_or_else(|| AosError::NotFound(format!("Training job not found: {}", job_id)))?;

            match job.status {
                TrainingJobStatus::Completed
                | TrainingJobStatus::Failed
                | TrainingJobStatus::Cancelled => {
                    return Err(AosError::Internal(format!(
                        "Cannot resume terminal job {} in state: {:?}",
                        job_id, job.status
                    ))
                    .into())
                }
                TrainingJobStatus::Running | TrainingJobStatus::Pending => {
                    // Idempotent
                    return Ok(());
                }
                TrainingJobStatus::Paused => {
                    job.status = TrainingJobStatus::Running;
                    should_notify = true;
                    // Update database
                    if let Some(ref db) = self.db {
                        db.update_training_status(job_id, "running").await?;
                    }
                    // Emit event
                    self.emit_event(
                        "job_resumed",
                        job_id,
                        json!({
                            "status": "running",
                        }),
                    );
                }
            }
        }

        // Update controls and wake waiters
        if should_notify {
            if let Some(control) = self.controls.read().await.get(job_id).cloned() {
                control.paused.store(false, Ordering::SeqCst);
                control.resume_notify.notify_waiters();
            }
        }

        info!("Training job resumed: {}", job_id);
        Ok(())
    }

    /// Update job progress (called by training worker)
    pub async fn update_progress(
        &self,
        job_id: &str,
        epoch: u32,
        loss: f32,
        tokens_per_second: f32,
    ) -> Result<()> {
        let mut cache = self.jobs_cache.write().await;
        if let Some(job) = cache.get_mut(job_id) {
            job.current_epoch = epoch;
            job.current_loss = loss;
            job.tokens_per_second = tokens_per_second;
            job.progress_pct = (epoch as f32 / job.total_epochs as f32) * 100.0;

            if job.status == TrainingJobStatus::Pending {
                job.status = TrainingJobStatus::Running;
                job.started_at = Some(chrono::Utc::now().to_rfc3339());
            }

            // Update database
            if let Some(ref db) = self.db {
                let progress = TrainingProgress {
                    progress_pct: job.progress_pct,
                    current_epoch: job.current_epoch,
                    total_epochs: job.total_epochs,
                    current_loss: job.current_loss,
                    learning_rate: job.learning_rate,
                    tokens_per_second: job.tokens_per_second,
                    error_message: job.error_message.clone(),
                };
                db.update_training_progress(job_id, &progress).await?;
                if job.status == TrainingJobStatus::Running && job.started_at.is_some() {
                    db.update_training_status(job_id, "running").await?;
                }
            }

            // Emit progress event
            self.emit_event(
                "progress_updated",
                job_id,
                json!({
                    "epoch": job.current_epoch,
                    "total_epochs": job.total_epochs,
                    "loss": job.current_loss,
                    "progress_pct": job.progress_pct,
                    "tokens_per_second": job.tokens_per_second,
                }),
            );

            Ok(())
        } else {
            Err(AosError::NotFound(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Mark job as completed
    pub async fn complete_job(&self, job_id: &str) -> Result<()> {
        let mut cache = self.jobs_cache.write().await;
        if let Some(job) = cache.get_mut(job_id) {
            job.status = TrainingJobStatus::Completed;
            job.progress_pct = 100.0;
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());

            // Update database
            if let Some(ref db) = self.db {
                let progress = TrainingProgress {
                    progress_pct: job.progress_pct,
                    current_epoch: job.current_epoch,
                    total_epochs: job.total_epochs,
                    current_loss: job.current_loss,
                    learning_rate: job.learning_rate,
                    tokens_per_second: job.tokens_per_second,
                    error_message: job.error_message.clone(),
                };
                db.update_training_progress(job_id, &progress).await?;
                db.update_training_status(job_id, "completed").await?;

                // Update metadata if artifacts exist
                if job.artifact_path.is_some()
                    || job.adapter_id.is_some()
                    || job.weights_hash_b3.is_some()
                {
                    let (_, _, metadata_json) = Self::job_to_db_data(job)?;
                    if let Some(metadata) = metadata_json {
                        db.update_training_job_metadata(job_id, &metadata).await?;
                    }
                }
            }

            // Emit completion event
            self.emit_event(
                "job_completed",
                job_id,
                json!({
                    "status": "completed",
                    "final_loss": job.current_loss,
                    "artifact_path": job.artifact_path,
                    "adapter_id": job.adapter_id,
                    "weights_hash_b3": job.weights_hash_b3,
                }),
            );

            tracing::info!("Training job completed: {}", job_id);
            Ok(())
        } else {
            Err(AosError::NotFound(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Mark job as failed
    pub async fn fail_job(&self, job_id: &str, error: String) -> Result<()> {
        let mut cache = self.jobs_cache.write().await;
        if let Some(job) = cache.get_mut(job_id) {
            job.status = TrainingJobStatus::Failed;
            job.error_message = Some(error.clone());
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());

            // Update database
            if let Some(ref db) = self.db {
                let progress = TrainingProgress {
                    progress_pct: job.progress_pct,
                    current_epoch: job.current_epoch,
                    total_epochs: job.total_epochs,
                    current_loss: job.current_loss,
                    learning_rate: job.learning_rate,
                    tokens_per_second: job.tokens_per_second,
                    error_message: Some(error.clone()),
                };
                db.update_training_progress(job_id, &progress).await?;
                db.update_training_status(job_id, "failed").await?;
            }

            // Emit failure event
            self.emit_event(
                "job_failed",
                job_id,
                json!({
                    "status": "failed",
                    "error": error,
                }),
            );

            error!("Training job failed: {}", job_id);
            Ok(())
        } else {
            Err(AosError::Internal(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Get training logs from persistent storage
    pub async fn get_logs(&self, job_id: &str) -> Result<Vec<String>> {
        // Verify job exists
        let _ = self.get_job(job_id).await?;

        let log_path = self.log_file_path(job_id);

        if !log_path.exists() {
            // Return empty logs if file doesn't exist yet
            return Ok(vec![]);
        }

        // Read log file
        let content = tokio::fs::read_to_string(&log_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to read log file for job {} ({}): {}",
                job_id,
                log_path.display(),
                e
            ))
        })?;

        // Split into lines and return
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        Ok(lines)
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

    /// List training jobs with artifacts
    pub async fn list_jobs_with_artifacts(&self) -> Result<Vec<TrainingJob>> {
        if let Some(ref db) = self.db {
            let records = db.list_training_jobs_with_artifacts().await?;
            let mut jobs = Vec::new();
            for record in records {
                // Filter jobs that actually have artifacts in metadata
                if let Some(ref metadata_json) = record.metadata_json {
                    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_json) {
                        if metadata.get("artifact_path").is_some()
                            || metadata.get("adapter_id").is_some()
                            || metadata.get("weights_hash_b3").is_some()
                        {
                            match Self::record_to_job(record) {
                                Ok(job) => jobs.push(job),
                                Err(e) => {
                                    tracing::warn!("Failed to convert job record: {}", e);
                                }
                            }
                        }
                    }
                }
            }
            Ok(jobs)
        } else {
            // Fallback to cache
            let cache = self.jobs_cache.read().await;
            Ok(cache
                .values()
                .filter(|job| {
                    job.artifact_path.is_some()
                        || job.adapter_id.is_some()
                        || job.weights_hash_b3.is_some()
                })
                .cloned()
                .collect())
        }
    }

    /// Clean up old training artifacts (removes files older than specified days)
    ///
    /// This removes artifact files from disk but keeps database records.
    /// Use with caution - this is a destructive operation.
    pub async fn cleanup_old_artifacts(&self, days: i64) -> Result<usize> {
        if let Some(ref db) = self.db {
            let old_jobs = db.get_old_training_jobs(days).await?;
            let mut cleaned_count = 0;

            for record in old_jobs {
                if let Some(ref metadata_json) = record.metadata_json {
                    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_json) {
                        if let Some(artifact_path) =
                            metadata.get("artifact_path").and_then(|v| v.as_str())
                        {
                            let path = std::path::PathBuf::from(artifact_path);
                            if path.exists() {
                                // Remove artifact directory
                                if let Err(e) = std::fs::remove_dir_all(&path) {
                                    tracing::warn!(
                                        "Failed to remove artifact directory {}: {}",
                                        path.display(),
                                        e
                                    );
                                } else {
                                    cleaned_count += 1;
                                    tracing::info!(
                                        "Cleaned up artifact directory: {}",
                                        path.display()
                                    );
                                }
                            }
                        }
                    }
                }
            }

            Ok(cleaned_count)
        } else {
            // In-memory mode: no cleanup needed
            Ok(0)
        }
    }

    /// Warm up the in-memory cache by loading all jobs from the database
    ///
    /// This should be called on server startup to ensure cache is populated.
    /// Without this, jobs may appear missing until first accessed.
    pub async fn warmup_cache(&self) -> Result<usize> {
        if let Some(ref db) = self.db {
            let records = db.list_all_training_jobs().await?;
            let mut cache = self.jobs_cache.write().await;
            let mut loaded = 0;

            for record in records {
                match Self::record_to_job(record) {
                    Ok(job) => {
                        cache.insert(job.id.clone(), job);
                        loaded += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load job from DB during cache warmup: {}", e);
                    }
                }
            }

            tracing::info!("Cache warmup complete: loaded {} jobs", loaded);
            Ok(loaded)
        } else {
            // In-memory mode: no warmup needed
            Ok(0)
        }
    }

    /// Clean up old log files (removes log files older than specified days)
    ///
    /// This removes log files from disk but keeps database records.
    /// Use with caution - this is a destructive operation.
    pub async fn cleanup_old_logs(&self, days: i64) -> Result<usize> {
        if !self.log_dir.exists() {
            return Ok(0);
        }

        let cutoff_time = chrono::Utc::now() - chrono::Duration::days(days);
        let mut cleaned_count = 0;

        // Read log directory
        let entries = std::fs::read_dir(&self.log_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to read log directory {}: {}",
                self.log_dir.display(),
                e
            ))
        })?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AosError::Io(format!("Failed to read log directory entry: {}", e)))?;
            let path = entry.path();

            // Only process .log files
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                // Check file modification time
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let modified_time = chrono::DateTime::<chrono::Utc>::from(modified);
                        if modified_time < cutoff_time {
                            // Remove old log file
                            if let Err(e) = std::fs::remove_file(&path) {
                                tracing::warn!(
                                    "Failed to remove old log file {}: {}",
                                    path.display(),
                                    e
                                );
                            } else {
                                cleaned_count += 1;
                                tracing::debug!("Cleaned up old log file: {}", path.display());
                            }
                        }
                    }
                }
            }
        }

        if cleaned_count > 0 {
            tracing::info!("Cleaned up {} old log files", cleaned_count);
        }

        Ok(cleaned_count)
    }

    /// Reconcile stuck training jobs (jobs in 'running' state that are likely stuck)
    ///
    /// This detects jobs that have been in 'running' state for longer than expected
    /// and marks them as failed. Should be called on server startup to handle crashes.
    ///
    /// # Arguments
    /// * `max_age_hours` - Maximum age in hours for a running job before considering it stuck
    pub async fn reconcile_stuck_jobs(&self, max_age_hours: i64) -> Result<usize> {
        if let Some(ref db) = self.db {
            // Get all running jobs
            let running_jobs = db.list_training_jobs_by_status("running").await?;
            let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(max_age_hours);
            let mut reconciled = 0;

            for record in running_jobs {
                // Parse started_at to check age
                if let Ok(started_at) = chrono::DateTime::parse_from_rfc3339(&record.started_at) {
                    let started_at_utc = started_at.with_timezone(&chrono::Utc);
                    if started_at_utc < cutoff_time {
                        // Job is stuck - mark as failed
                        let error_msg = format!(
                            "Job stuck in running state for more than {} hours (started: {})",
                            max_age_hours, record.started_at
                        );

                        tracing::warn!("Reconciling stuck job {}: {}", record.id, error_msg);

                        // Update database
                        db.update_training_status(&record.id, "failed").await?;

                        // Update cache if present
                        {
                            let mut cache = self.jobs_cache.write().await;
                            if let Some(job) = cache.get_mut(&record.id) {
                                job.status = TrainingJobStatus::Failed;
                                job.error_message = Some(error_msg.clone());
                                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                            }
                        }

                        // Emit event
                        self.emit_event(
                            "job_failed",
                            &record.id,
                            json!({
                                "status": "failed",
                                "reason": "stuck_job_reconciliation",
                                "error_message": error_msg,
                            }),
                        );

                        reconciled += 1;
                    }
                }
            }

            if reconciled > 0 {
                tracing::info!("Reconciled {} stuck training jobs", reconciled);
            }

            Ok(reconciled)
        } else {
            // In-memory mode: check cache
            let cache = self.jobs_cache.read().await;
            let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(max_age_hours);
            let mut reconciled = 0;

            for (_job_id, job) in cache.iter() {
                if matches!(job.status, TrainingJobStatus::Running) {
                    if let Some(ref started_at_str) = job.started_at {
                        if let Ok(started_at) = chrono::DateTime::parse_from_rfc3339(started_at_str)
                        {
                            let started_at_utc = started_at.with_timezone(&chrono::Utc);
                            if started_at_utc < cutoff_time {
                                reconciled += 1;
                            }
                        }
                    }
                }
            }

            Ok(reconciled)
        }
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
    jobs_cache_ref: Arc<RwLock<HashMap<String, TrainingJob>>>,
    db_ref: Option<Arc<Db>>,
    controls_ref: Arc<RwLock<HashMap<String, Arc<JobControl>>>>,
    job_id: String,
    orchestrator_cfg: TrainingConfig,
    dataset_path: Option<String>,
    directory_root: Option<String>,
    directory_path: Option<String>,
    adapters_root: Option<String>,
    package: bool,
    adapter_id_opt: Option<String>,
    base_model: Arc<String>,
    log_dir: PathBuf,
) -> Result<()> {
    // Helper to append log
    let append_log = |message: &str| -> Result<()> {
        let log_path = log_dir.join(format!("{}.log", job_id));
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow!(
                    "Failed to ensure training log directory {} for job {}: {}",
                    parent.display(),
                    job_id,
                    e
                )
            })?;
        }

        let timestamp = chrono::Utc::now().to_rfc3339();
        let log_line = format!("[{}] {}\n", timestamp, message);

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| {
                anyhow!(
                    "Failed to open training log file for job {} ({}): {}",
                    job_id,
                    log_path.display(),
                    e
                )
            })?;

        file.write_all(log_line.as_bytes()).map_err(|e| {
            anyhow!(
                "Failed to write training log entry for job {} to {}: {}",
                job_id,
                log_path.display(),
                e
            )
        })?;

        file.flush().map_err(|e| {
            anyhow!(
                "Failed to flush training log entry for job {} to {}: {}",
                job_id,
                log_path.display(),
                e
            )
        })?;

        Ok(())
    };

    // Transition to running
    {
        let mut cache = jobs_cache_ref.write().await;
        if let Some(job) = cache.get_mut(&job_id) {
            job.status = TrainingJobStatus::Running;
            job.started_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    if let Err(e) = append_log("Training job started") {
        tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
    }
    if let Err(e) = append_log(&format!(
        "Loading dataset from: {:?}",
        dataset_path.as_ref().or(directory_path.as_ref())
    )) {
        tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
    }

    // Update database
    if let Some(ref db) = db_ref {
        db.update_training_status(&job_id, "running").await?;
    }

    // Map orchestrator config to worker trainer config
    let worker_cfg = WorkerTrainingConfig {
        rank: orchestrator_cfg.rank as usize,
        alpha: orchestrator_cfg.alpha as f32,
        learning_rate: orchestrator_cfg.learning_rate,
        batch_size: orchestrator_cfg.batch_size as usize,
        epochs: orchestrator_cfg.epochs as usize,
        hidden_dim: 768, // default; can be made configurable via orchestrator config later
        weight_group_config: Default::default(),
    };

    if let Err(e) = append_log(&format!(
        "Training config: rank={}, alpha={}, epochs={}, batch_size={}, lr={}",
        worker_cfg.rank,
        worker_cfg.alpha,
        worker_cfg.epochs,
        worker_cfg.batch_size,
        worker_cfg.learning_rate
    )) {
        tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
    }

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
                            weight: 1.0,
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
                        let error_msg = e.to_string();
                        if let Err(e) = append_log(&format!(
                            "ERROR: Directory dataset build failed: {}",
                            error_msg
                        )) {
                            tracing::warn!(
                                job_id = %job_id,
                                error = %e,
                                "Failed to append training log entry"
                            );
                        }
                        {
                            let mut cache = jobs_cache_ref.write().await;
                            if let Some(job) = cache.get_mut(&job_id) {
                                job.status = TrainingJobStatus::Failed;
                                job.error_message = Some(error_msg.clone());
                                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                            }
                        }
                        // Update database
                        if let Some(ref db) = db_ref {
                            let progress = TrainingProgress {
                                progress_pct: 0.0,
                                current_epoch: 0,
                                total_epochs: orchestrator_cfg.epochs,
                                current_loss: 0.0,
                                learning_rate: orchestrator_cfg.learning_rate,
                                tokens_per_second: 0.0,
                                error_message: Some(error_msg.clone()),
                            };
                            if let Err(db_err) =
                                db.update_training_progress(&job_id, &progress).await
                            {
                                tracing::error!(
                                    job_id = %job_id,
                                    error = %db_err,
                                    "Failed to persist training progress after dataset build failure"
                                );
                            }
                            if let Err(db_err) = db.update_training_status(&job_id, "failed").await
                            {
                                tracing::error!(
                                    job_id = %job_id,
                                    error = %db_err,
                                    "Failed to mark training job failed after dataset build failure"
                                );
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
                weight: 1.0,
            },
            WorkerTrainingExample {
                input: vec![7, 8, 9],
                target: vec![10, 11, 12],
                metadata: Default::default(),
                weight: 1.0,
            },
        ]
    };

    if let Err(e) = append_log(&format!("Loaded {} training examples", examples.len())) {
        tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
    }
    let mut trainer = WorkerTrainer::new(worker_cfg)?;
    if let Err(e) = append_log("Initialized trainer") {
        tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
    }

    // Respect pause/cancel before starting
    let control = {
        let controls = controls_ref.read().await;
        controls.get(&job_id).cloned().ok_or_else(|| {
            AosError::Internal(format!("Job control not found for job {}", job_id))
        })?
    };
    if control.cancelled.load(Ordering::SeqCst) {
        if let Err(e) = append_log("Training job cancelled before start") {
            tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
        }
        let mut cache = jobs_cache_ref.write().await;
        if let Some(job) = cache.get_mut(&job_id) {
            job.status = TrainingJobStatus::Cancelled;
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
        // Update database
        if let Some(ref db) = db_ref {
            if let Err(db_err) = db.update_training_status(&job_id, "cancelled").await {
                tracing::error!(
                    job_id = %job_id,
                    error = %db_err,
                    "Failed to mark training job cancelled"
                );
            }
        }
        return Ok(());
    }

    if let Err(e) = append_log("Starting training loop") {
        tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
    }

    // Run with per-epoch callback to update progress
    let job_id_clone = job_id.clone();
    let jobs_cache_ref_clone = jobs_cache_ref.clone();
    let db_ref_clone = db_ref.clone();
    let log_dir_clone = log_dir.clone();
    // Get event tx from cache if available (we'll need to pass it through)
    // For now, we'll emit events through the cache update callback
    let result = trainer
        .train_with_callback(&examples, move |epoch, loss| {
            // Log epoch start
            let log_path = log_dir_clone.join(format!("{}.log", job_id_clone));
            if let Some(parent) = log_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!(
                        job_id = %job_id_clone,
                        dir = %parent.display(),
                        error = %e,
                        "Failed to ensure training log directory"
                    );
                }
            }
            let timestamp = chrono::Utc::now().to_rfc3339();
            let log_line = format!(
                "[{}] Epoch {}/{} completed, loss: {:.6}\n",
                timestamp, epoch, orchestrator_cfg.epochs, loss
            );
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(log_line.as_bytes()) {
                        tracing::warn!(
                            job_id = %job_id_clone,
                            path = %log_path.display(),
                            error = %e,
                            "Failed to write training log entry"
                        );
                    } else if let Err(e) = file.flush() {
                        tracing::warn!(
                            job_id = %job_id_clone,
                            path = %log_path.display(),
                            error = %e,
                            "Failed to flush training log entry"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        job_id = %job_id_clone,
                        path = %log_path.display(),
                        error = %e,
                        "Failed to open training log file"
                    );
                }
            }

            // Emit epoch completed event (via cache update - we'll handle this in update_progress)

            // If paused, busy-wait with small sleep until resumed or cancelled
            while control.paused.load(Ordering::SeqCst) {
                if control.cancelled.load(Ordering::SeqCst) {
                    break;
                }
                // Best-effort status update to Paused while we wait
                let cache_ref = jobs_cache_ref_clone.clone();
                let job_id_inner = job_id_clone.clone();
                tokio::spawn(async move {
                    let mut cache = cache_ref.write().await;
                    if let Some(job) = cache.get_mut(&job_id_inner) {
                        if job.status == TrainingJobStatus::Running {
                            job.status = TrainingJobStatus::Paused;
                        }
                    }
                });
                let sleep_duration = std::time::Duration::from_millis(50);
                match tokio::runtime::Handle::try_current() {
                    Ok(handle) => {
                        tokio::task::block_in_place(move || {
                            handle.block_on(async {
                                tokio::time::sleep(sleep_duration).await;
                            });
                        });
                    }
                    Err(_) => {
                        std::thread::sleep(sleep_duration);
                    }
                }
            }
            let cache_ref = jobs_cache_ref_clone.clone();
            let db_ref_inner = db_ref_clone.clone();
            let job_id_inner = job_id_clone.clone();
            // Fire-and-forget async update
            tokio::spawn(async move {
                let mut cache = cache_ref.write().await;
                if let Some(job) = cache.get_mut(&job_id_inner) {
                    job.current_epoch = epoch as u32;
                    job.current_loss = loss;
                    if job.total_epochs > 0 {
                        job.progress_pct = (epoch as f32 / job.total_epochs as f32) * 100.0;
                    }
                    if job.status == TrainingJobStatus::Paused {
                        job.status = TrainingJobStatus::Running;
                    }

                    // Update database
                    if let Some(ref db) = db_ref_inner {
                        let progress = TrainingProgress {
                            progress_pct: job.progress_pct,
                            current_epoch: job.current_epoch,
                            total_epochs: job.total_epochs,
                            current_loss: job.current_loss,
                            learning_rate: job.learning_rate,
                            tokens_per_second: job.tokens_per_second,
                            error_message: job.error_message.clone(),
                        };
                        if let Err(db_err) =
                            db.update_training_progress(&job_id_inner, &progress).await
                        {
                            tracing::error!(
                                job_id = %job_id_inner,
                                error = %db_err,
                                "Failed to persist training progress update"
                            );
                        }
                    }
                }
            });
        })
        .await;

    match result {
        Ok(training_result) => {
            if let Err(e) = append_log(&format!(
                "Training completed successfully. Final loss: {:.6}",
                training_result.final_loss
            )) {
                tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
            }
            let mut cache = jobs_cache_ref.write().await;
            if let Some(job) = cache.get_mut(&job_id) {
                job.status = TrainingJobStatus::Completed;
                job.progress_pct = 100.0;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());

                if package {
                    if let Err(e) = append_log("Starting adapter packaging") {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to append training log entry"
                        );
                    }
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

                    if let Err(e) = append_log(&format!(
                        "Packaging adapter: {} to {}",
                        adapter_id, chosen_root
                    )) {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to append training log entry"
                        );
                    }

                    // Quantize weights
                    let quantized = adapteros_lora_worker::training::LoRAQuantizer::quantize_to_q15(
                        &training_result.weights,
                    );
                    if let Err(e) = append_log("Weights quantized to Q15") {
                        tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
                    }

                    // Package synchronously in this async task (no nested runtime spawn)
                    let packager = adapteros_lora_worker::training::packager::AdapterPackager::new(
                        &chosen_root,
                    );
                    let base_model = (*base_model).clone();
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
                                weight_group_config: Default::default(),
                            },
                            base_model.as_ref(),
                        )
                        .await
                    {
                        Ok(packaged) => {
                            job.artifact_path = Some(format!("{}/{}", root_for_pack, adapter_id));
                            job.adapter_id = Some(adapter_id.clone());
                            job.weights_hash_b3 = Some(packaged.hash_b3.clone());
                            if let Err(e) = append_log(&format!(
                                "Adapter packaged successfully: {} (hash: {})",
                                adapter_id, packaged.hash_b3
                            )) {
                                tracing::warn!(
                                    job_id = %job_id,
                                    error = %e,
                                    "Failed to append training log entry"
                                );
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Packaging failed: {}", e);
                            if let Err(e) = append_log(&format!("ERROR: {}", error_msg)) {
                                tracing::warn!(
                                    job_id = %job_id,
                                    error = %e,
                                    "Failed to append training log entry"
                                );
                            }
                            tracing::error!("Packaging failed: {}", e);
                        }
                    }
                }
            }

            // Update database with completion
            {
                let cache = jobs_cache_ref.read().await;
                if let Some(job) = cache.get(&job_id) {
                    if let Some(ref db) = db_ref {
                        let progress = TrainingProgress {
                            progress_pct: 100.0,
                            current_epoch: job.current_epoch,
                            total_epochs: job.total_epochs,
                            current_loss: job.current_loss,
                            learning_rate: job.learning_rate,
                            tokens_per_second: job.tokens_per_second,
                            error_message: job.error_message.clone(),
                        };
                        if let Err(db_err) = db.update_training_progress(&job_id, &progress).await {
                            tracing::error!(
                                job_id = %job_id,
                                error = %db_err,
                                "Failed to persist training progress on completion"
                            );
                        }
                        if let Err(db_err) = db.update_training_status(&job_id, "completed").await {
                            tracing::error!(
                                job_id = %job_id,
                                error = %db_err,
                                "Failed to mark training job completed"
                            );
                        }

                        // Update metadata if artifacts exist
                        if job.artifact_path.is_some()
                            || job.adapter_id.is_some()
                            || job.weights_hash_b3.is_some()
                        {
                            let metadata = json!({
                                "artifact_path": job.artifact_path,
                                "adapter_id": job.adapter_id,
                                "weights_hash_b3": job.weights_hash_b3,
                            });
                            if let Ok(metadata_json) = serde_json::to_string(&metadata) {
                                if let Err(db_err) = db
                                    .update_training_job_metadata(&job_id, &metadata_json)
                                    .await
                                {
                                    tracing::error!(
                                        job_id = %job_id,
                                        error = %db_err,
                                        "Failed to update training job metadata after packaging"
                                    );
                                }
                            }
                        }
                    }
                }
            }

            if let Err(e) = append_log("Training job completed successfully") {
                tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
            }
            Ok(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            if let Err(e) = append_log(&format!("ERROR: Training failed: {}", error_msg)) {
                tracing::warn!(job_id = %job_id, error = %e, "Failed to append training log entry");
            }
            let mut cache = jobs_cache_ref.write().await;
            if let Some(job) = cache.get_mut(&job_id) {
                job.status = TrainingJobStatus::Failed;
                job.error_message = Some(error_msg.clone());
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }

            // Update database with failure
            if let Some(ref db) = db_ref {
                let progress = TrainingProgress {
                    progress_pct: 0.0,
                    current_epoch: 0,
                    total_epochs: orchestrator_cfg.epochs,
                    current_loss: 0.0,
                    learning_rate: orchestrator_cfg.learning_rate,
                    tokens_per_second: 0.0,
                    error_message: Some(error_msg.clone()),
                };
                if let Err(db_err) = db.update_training_progress(&job_id, &progress).await {
                    tracing::error!(
                        job_id = %job_id,
                        error = %db_err,
                        "Failed to persist training progress after job failure"
                    );
                }
                if let Err(db_err) = db.update_training_status(&job_id, "failed").await {
                    tracing::error!(
                        job_id = %job_id,
                        error = %db_err,
                        "Failed to mark training job failed"
                    );
                }
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
        let params = TrainingJobBuilder::new()
            .adapter_name("test-adapter")
            .config(config)
            .build()
            .unwrap();
        let job = service.start_training(params).await.unwrap();

        assert_eq!(job.status, TrainingJobStatus::Pending);
        assert_eq!(job.adapter_name, "test-adapter");

        let jobs = service.list_jobs().await.unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let service = TrainingService::new();

        let config = TrainingConfig::default();
        let params = TrainingJobBuilder::new()
            .adapter_name("test-adapter")
            .config(config)
            .build()
            .unwrap();
        let job = service.start_training(params).await.unwrap();

        service.cancel_job(&job.id).await.unwrap();

        let updated_job = service.get_job(&job.id).await.unwrap();
        assert_eq!(updated_job.status, TrainingJobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_update_progress() {
        let service = TrainingService::new();

        let config = TrainingConfig::default();
        let params = TrainingJobBuilder::new()
            .adapter_name("test-adapter")
            .config(config)
            .build()
            .unwrap();
        let job = service.start_training(params).await.unwrap();

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

        let params = TrainingJobBuilder::new()
            .adapter_name("dir-fail")
            .config(config)
            .directory_root(Some(root.display().to_string()))
            .directory_path(Some("empty".to_string()))
            .build()
            .unwrap();
        let job = service.start_training(params).await.unwrap();

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
