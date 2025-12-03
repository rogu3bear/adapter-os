//! Training job orchestration and management
//!
//! Handles scheduling, executing, and monitoring adapter training jobs.
//! Integrates with MLX backend for actual training operations.

use adapteros_core::AosError;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_lora_worker::training::{
    MicroLoRATrainer as WorkerTrainer, TrainingConfig as WorkerTrainingConfig,
    TrainingExample as WorkerTrainingExample,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

// Re-export canonical types from adapteros_types
pub use adapteros_types::training::{
    TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate,
};

/// Training service for managing jobs
pub struct TrainingService {
    jobs: Arc<RwLock<HashMap<String, TrainingJob>>>,
    templates: Arc<RwLock<HashMap<String, TrainingTemplate>>>,
    /// Database connection for dataset loading
    db: Option<adapteros_db::Db>,
    /// Storage root for dataset files
    storage_root: Option<PathBuf>,
    /// Cancel tokens for active training jobs (job_id -> token)
    /// Set token to true to request cancellation; trainer checks at epoch boundaries
    cancel_tokens: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
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
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new training service with database and storage configuration
    pub fn with_db(db: adapteros_db::Db, storage_root: PathBuf) -> Self {
        let mut service = Self::new();
        service.db = Some(db);
        service.storage_root = Some(storage_root);
        // cancel_tokens already initialized by new()
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
        tenant_id: Option<String>,
        initiated_by: Option<String>,
        initiated_by_role: Option<String>,
        base_model_id: Option<String>,
        collection_id: Option<String>,
        // Category metadata
        category: Option<String>,
        description: Option<String>,
        language: Option<String>,
        framework_id: Option<String>,
        framework_version: Option<String>,
        // Post-training actions (JSON serialized)
        post_actions_json: Option<String>,
        // Retry tracking: ID of the original job this is a retry of
        retry_of_job_id: Option<String>,
    ) -> Result<TrainingJob> {
        let job_id = format!("train-{}", uuid::Uuid::new_v4());

        // Compute config hash for reproducibility tracking
        let config_params = adapteros_db::training_jobs::TrainingConfigParams {
            rank: config.rank as usize,
            alpha: config.alpha as f32,
            learning_rate: config.learning_rate,
            batch_size: config.batch_size as usize,
            epochs: config.epochs as usize,
            hidden_dim: 768, // Default hidden dimension
        };
        let config_hash = adapteros_db::training_jobs::compute_config_hash(&config_params).ok();

        // Get build ID from environment or use default
        let build_id = std::env::var("BUILD_ID")
            .or_else(|_| std::env::var("GIT_COMMIT"))
            .ok()
            .or_else(|| Some("dev".to_string()));

        let mut job = TrainingJob::new(job_id.clone(), adapter_name.clone(), config.clone());
        job.template_id = template_id;
        job.repo_id = repo_id.clone();
        job.dataset_id = dataset_id.clone();
        job.tenant_id = tenant_id.clone();
        job.initiated_by = initiated_by.clone();
        job.initiated_by_role = initiated_by_role;
        job.base_model_id = base_model_id.clone();
        job.collection_id = collection_id.clone();
        job.build_id = build_id.clone();
        job.config_hash_b3 = config_hash.clone();
        // Category metadata
        job.category = category.clone();
        job.description = description;
        job.language = language;
        job.framework_id = framework_id;
        job.framework_version = framework_version;
        job.post_actions_json = post_actions_json.clone();
        job.retry_of_job_id = retry_of_job_id.clone();

        // Persist job to database for durability and retry chain tracking
        // This ensures retry_of_job_id, retryable flag, and metrics are properly persisted
        if let Some(ref db) = self.db {
            let config_json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());

            // Use repo_id or a sentinel value for dataset-based training
            let db_repo_id = repo_id.as_deref().unwrap_or("direct-training");
            let created_by = initiated_by.as_deref().unwrap_or("system");

            // Pass our job_id so DB and in-memory IDs match
            match db
                .create_training_job_with_provenance(
                    Some(&job_id), // Use our generated job_id
                    db_repo_id,
                    &config_json,
                    created_by,
                    dataset_id.as_deref(),
                    base_model_id.as_deref(),
                    collection_id.as_deref(),
                    tenant_id.as_deref(),
                    build_id.as_deref(),
                    None, // source_documents_json - not tracked at job level
                    retry_of_job_id.as_deref(),
                )
                .await
            {
                Ok(_) => {
                    info!(
                        job_id = %job_id,
                        retry_of = ?retry_of_job_id,
                        "Training job persisted to database"
                    );

                    // Update adapter_name in DB (not included in create_training_job_with_provenance)
                    if let Err(e) = db
                        .update_training_job_adapter_name(&job_id, &adapter_name)
                        .await
                    {
                        warn!(job_id = %job_id, error = %e, "Failed to update adapter name in DB (non-fatal)");
                    }

                    // Store config hash if available
                    if let Some(ref hash) = config_hash {
                        if let Err(e) = db.update_training_job_config_hash(&job_id, hash).await {
                            warn!(job_id = %job_id, error = %e, "Failed to update config hash in DB (non-fatal)");
                        }
                    }
                }
                Err(e) => {
                    // Log but don't fail - job can still run in memory
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to persist training job to database (job will run but metrics/retry may not persist)"
                    );
                }
            }
        }

        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job_id.clone(), job.clone());
        }

        // Create and register cancel token for this job
        let cancel_token = Arc::new(AtomicBool::new(false));
        {
            let mut tokens = self.cancel_tokens.write().await;
            tokens.insert(job_id.clone(), cancel_token.clone());
        }

        // Spawn deterministic training task (training must be reproducible)
        let jobs_ref = self.jobs.clone();
        let cancel_tokens_ref = self.cancel_tokens.clone();
        let cfg_for_run = job.config.clone();
        let job_id_for_run = job.id.clone();
        let adapter_name_for_run = job.adapter_name.clone();
        let dataset_id_for_run = job.dataset_id.clone();
        let tenant_id_for_run = tenant_id;
        let db_for_run = self.db.clone();
        let storage_for_run = self.storage_root.clone();
        let category_for_run = category;
        let post_actions_for_run = post_actions_json;
        let base_model_id_for_run = job.base_model_id.clone();
        let cancel_token_for_run = cancel_token.clone();
        if let Err(e) =
            spawn_deterministic(format!("training-job:{}", job_id_for_run), async move {
                let result = run_training_job(
                    jobs_ref.clone(),
                    job_id_for_run.clone(),
                    adapter_name_for_run,
                    cfg_for_run,
                    dataset_id_for_run,
                    tenant_id_for_run,
                    db_for_run,
                    storage_for_run,
                    category_for_run,
                    post_actions_for_run,
                    base_model_id_for_run,
                    cancel_token_for_run,
                )
                .await;

                // Clean up cancel token after job completes (success or failure)
                {
                    let mut tokens = cancel_tokens_ref.write().await;
                    tokens.remove(&job_id_for_run);
                }

                if let Err(err) = result {
                    tracing::error!("Training job {} failed: {}", job_id_for_run, err);
                }
            })
        {
            tracing::error!("Failed to spawn deterministic training task: {}", e);
            // Training operations require deterministic execution - fail rather than fallback
            return Err(adapteros_core::AosError::DeterminismViolation(format!(
                "Training job {} requires deterministic executor: {}",
                job_id, e
            ))
            .into());
        }

        tracing::info!("Training job created: {}", job_id);

        Ok(job)
    }

    /// Cancel a training job
    ///
    /// Sets the in-process cancel token (if the job is running in this orchestrator),
    /// then optionally sends a cancellation request to the worker via UDS.
    /// The trainer checks the cancel token at epoch boundaries and stops gracefully.
    pub async fn cancel_job(
        &self,
        job_id: &str,
        uds_client: Option<&adapteros_client::UdsClient>,
        socket_path: Option<&str>,
    ) -> Result<()> {
        // Verify job exists and is in a cancellable state
        {
            let jobs = self.jobs.read().await;
            if let Some(job) = jobs.get(job_id) {
                if job.status != TrainingJobStatus::Running
                    && job.status != TrainingJobStatus::Pending
                {
                    return Err(AosError::Internal(format!(
                        "Cannot cancel job in state: {:?}",
                        job.status
                    ))
                    .into());
                }
            } else {
                return Err(
                    AosError::Internal(format!("Training job not found: {}", job_id)).into(),
                );
            }
        }

        // Set the cancel token directly - this is the primary cancellation mechanism
        // The trainer checks this token at epoch boundaries and stops gracefully
        let token_set = {
            let tokens = self.cancel_tokens.read().await;
            if let Some(token) = tokens.get(job_id) {
                token.store(true, Ordering::SeqCst);
                info!(job_id = %job_id, "Cancel token set for training job");
                true
            } else {
                warn!(job_id = %job_id, "No cancel token found for job (may have already completed)");
                false
            }
        };

        // Also send cancel to worker via UDS with 5s timeout (for jobs running in separate workers)
        let worker_confirmed = if let Some(client) = uds_client {
            let socket = socket_path.unwrap_or("/var/run/adapteros.sock");
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                client.cancel_training_job(Path::new(socket), job_id, None),
            )
            .await
            {
                Ok(Ok(resp)) => {
                    info!(
                        job_id = %job_id,
                        status = %resp.status,
                        "Worker confirmed cancellation"
                    );
                    true
                }
                Ok(Err(e)) => {
                    warn!(job_id = %job_id, error = %e, "Worker cancel failed");
                    false
                }
                Err(_) => {
                    warn!(
                        job_id = %job_id,
                        "Worker cancel timeout (5s) - relying on in-process token"
                    );
                    false
                }
            }
        } else {
            // No UDS client provided - rely on in-process token
            false
        };

        // Update status based on confirmation
        // Token set OR worker confirmed = cancellation initiated
        let cancellation_initiated = token_set || worker_confirmed;

        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            if cancellation_initiated {
                // Mark as CancelPending - will become Cancelled when trainer actually stops
                job.status = TrainingJobStatus::CancelPending;
                info!(job_id = %job_id, token_set = token_set, worker_confirmed = worker_confirmed, "Training job cancellation initiated");
            } else {
                job.status = TrainingJobStatus::CancelPending;
                warn!(job_id = %job_id, "Training job cancel requested but no confirmation - marking cancel_pending");
            }
            // Don't set completed_at here - let the trainer set it when it actually stops
            Ok(())
        } else {
            // Job disappeared between checks - should not happen but handle gracefully
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
            logs.push(format!(
                "Configuration: rank={}, alpha={}, epochs={}",
                job.config.rank, job.config.alpha, job.total_epochs
            ));
        }

        if job.current_epoch > 0 {
            for epoch in 1..=job.current_epoch {
                logs.push(format!("Epoch {}/{} completed", epoch, job.total_epochs));
            }
            logs.push(format!("Current loss: {:.4}", job.current_loss));
            if job.tokens_per_second > 0.0 {
                logs.push(format!(
                    "Throughput: {:.1} tokens/sec",
                    job.tokens_per_second
                ));
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

/// Post-actions configuration parsed from JSON
#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PostActions {
    /// Package adapter after training (default: true)
    #[serde(default = "default_true")]
    package: bool,
    /// Register adapter in registry after packaging (default: true)
    #[serde(default = "default_true")]
    register: bool,
    /// Create a new stack with the adapter after registration (default: true).
    /// Note: The stack will NOT be set as the tenant's default.
    #[serde(default = "default_true")]
    create_stack: bool,
    /// Tier to assign: persistent, warm, ephemeral (default: warm)
    #[serde(default = "default_tier")]
    tier: String,
    /// Custom adapters root directory (optional)
    adapters_root: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_tier() -> String {
    "warm".to_string()
}

/// Background runner for a single training job. Converts orchestrator config into worker trainer
/// config, runs training with per-epoch callback, packages weights, registers adapter, and
/// updates the shared job map with artifact metadata.
///
/// The cancel_token is checked by the trainer at epoch boundaries - set it to true to
/// request graceful cancellation. Metrics are persisted to the database after each epoch
/// when db and job_id are provided to the trainer.
async fn run_training_job(
    jobs_ref: Arc<RwLock<HashMap<String, TrainingJob>>>,
    job_id: String,
    adapter_name: String,
    orchestrator_cfg: TrainingConfig,
    dataset_id: Option<String>,
    tenant_id: Option<String>,
    db: Option<adapteros_db::Db>,
    storage_root: Option<PathBuf>,
    category: Option<String>,
    post_actions_json: Option<String>,
    base_model_id: Option<String>,
    cancel_token: Arc<AtomicBool>,
) -> Result<()> {
    use adapteros_lora_worker::training::{
        AdapterPackager, LoRAQuantizer, TrainingConfig as WorkerTrainingConfigType,
    };

    // Parse post-actions configuration (defaults if not provided or invalid)
    let post_actions: PostActions = post_actions_json
        .as_ref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();

    // Determine adapters root using centralized path resolution (ENV > Config > Default)
    use adapteros_core::paths::AdapterPaths;
    let adapters_root = {
        // Use post_actions.adapters_root as config value, or derive from storage_root
        // Convert storage_root PathBuf to String if needed (stored in variable to avoid lifetime issues)
        let storage_adapters_str = storage_root
            .as_ref()
            .map(|s| s.join("adapters").to_string_lossy().to_string());
        let config_value = post_actions
            .adapters_root
            .as_deref()
            .or_else(|| storage_adapters_str.as_deref());
        // AdapterPaths::from_config() will respect ENV > Config > Default precedence
        AdapterPaths::from_config(config_value).root().to_path_buf()
    };

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
        vocab_size: 32000, // default LLaMA/Mistral vocab size
        preferred_backend: None, // auto-select
        require_gpu: false,
        max_gpu_memory_mb: 0,         // unlimited
        checkpoint_interval: Some(5), // Save checkpoint every 5 epochs
    };

    // Clone db for later use in packaging/registration
    let db_for_packaging = db.clone();

    // Load training examples from dataset if available, otherwise use synthetic fallback
    let examples: Vec<WorkerTrainingExample> = match (dataset_id, db.clone(), storage_root.clone())
    {
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

    let mut trainer = WorkerTrainer::new(worker_cfg.clone())?;

    // Wire cancel token into trainer - checked at epoch boundaries
    trainer.set_cancel_token(cancel_token);

    // Wire job_id and DB for metrics persistence
    trainer.set_job_id(job_id.clone());
    if let Some(database) = db.clone() {
        trainer.set_db(database);
        info!(job_id = %job_id, "Trainer configured with DB for metrics persistence");
    }

    // Enable checkpointing if checkpoint_interval is configured
    if worker_cfg.checkpoint_interval.is_some() {
        let checkpoint_dir = storage_root
            .clone()
            .map(|s| s.join("checkpoints").join(&job_id))
            .unwrap_or_else(|| PathBuf::from("var/checkpoints").join(&job_id));

        // Create checkpoint directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&checkpoint_dir) {
            tracing::warn!(
                job_id = %job_id,
                error = %e,
                "Failed to create checkpoint directory, checkpoints disabled"
            );
        } else {
            trainer.enable_checkpointing(
                &checkpoint_dir,
                &job_id,
                3, // Keep last 3 checkpoints
            );
            tracing::info!(
                job_id = %job_id,
                checkpoint_dir = %checkpoint_dir.display(),
                "Checkpointing enabled"
            );
        }
    }

    // Run with per-epoch callback to update progress (with checkpoint resume support)
    let job_id_clone = job_id.clone();
    let jobs_ref_clone = jobs_ref.clone();
    let result = trainer
        .train_with_resume(&examples, move |epoch, loss| {
            let jobs_ref_inner = jobs_ref_clone.clone();
            let job_id_inner = job_id_clone.clone();
            // Deterministic progress update (part of training execution)
            let jobs_ref_for_det = jobs_ref_inner.clone();
            let job_id_for_det = job_id_inner.clone();
            let jobs_ref_for_fallback = jobs_ref_inner.clone();
            let job_id_for_fallback = job_id_inner.clone();
            if let Err(e) = spawn_deterministic(
                format!("training-progress:{}:epoch-{}", job_id_for_det, epoch),
                async move {
                    let mut jobs = jobs_ref_for_det.write().await;
                    if let Some(job) = jobs.get_mut(&job_id_for_det) {
                        job.current_epoch = epoch as u32;
                        job.current_loss = loss;
                        if job.total_epochs > 0 {
                            job.progress_pct = (epoch as f32 / job.total_epochs as f32) * 100.0;
                        }
                    }
                },
            ) {
                // Fallback: use tokio::spawn if deterministic executor not available
                tracing::warn!("Failed to spawn deterministic progress update: {}", e);
                tokio::spawn(async move {
                    let mut jobs = jobs_ref_for_fallback.write().await;
                    if let Some(job) = jobs.get_mut(&job_id_for_fallback) {
                        job.current_epoch = epoch as u32;
                        job.current_loss = loss;
                        if job.total_epochs > 0 {
                            job.progress_pct = (epoch as f32 / job.total_epochs as f32) * 100.0;
                        }
                    }
                });
            }
        })
        .await;

    match result {
        Ok(training_result) => {
            // Persist final summary metrics to database
            if let Some(database) = &db {
                use adapteros_db::TrainingMetricRow;
                use uuid::Uuid;

                let timestamp = chrono::Utc::now().to_rfc3339();
                let step = training_result.examples_processed.unwrap_or(0) as i64;
                let epoch = training_result.stopped_at_epoch.map(|e| e as i64);

                let final_metrics = vec![
                    TrainingMetricRow {
                        id: Uuid::now_v7().to_string(),
                        training_job_id: job_id.clone(),
                        step,
                        epoch,
                        metric_name: "final_loss".to_string(),
                        metric_value: training_result.final_loss as f64,
                        metric_timestamp: Some(timestamp.clone()),
                    },
                    TrainingMetricRow {
                        id: Uuid::now_v7().to_string(),
                        training_job_id: job_id.clone(),
                        step,
                        epoch,
                        metric_name: "cancelled".to_string(),
                        metric_value: if training_result.cancelled { 1.0 } else { 0.0 },
                        metric_timestamp: Some(timestamp.clone()),
                    },
                    TrainingMetricRow {
                        id: Uuid::now_v7().to_string(),
                        training_job_id: job_id.clone(),
                        step,
                        epoch,
                        metric_name: "examples_processed".to_string(),
                        metric_value: training_result.examples_processed.unwrap_or(0) as f64,
                        metric_timestamp: Some(timestamp),
                    },
                ];

                if let Err(e) = database.insert_training_metrics_batch(&final_metrics).await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist final training metrics (non-fatal)");
                } else {
                    info!(job_id = %job_id, cancelled = training_result.cancelled, "Final training metrics persisted");
                }
            }

            // If training was cancelled, mark job as cancelled and return early
            if training_result.cancelled {
                info!(
                    job_id = %job_id,
                    adapter_name = %adapter_name,
                    final_loss = training_result.final_loss,
                    stopped_at_epoch = ?training_result.stopped_at_epoch,
                    examples_processed = ?training_result.examples_processed,
                    "Training cancelled gracefully"
                );

                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.status = TrainingJobStatus::Cancelled;
                    job.current_loss = training_result.final_loss;
                    if let Some(epoch) = training_result.stopped_at_epoch {
                        job.current_epoch = epoch;
                    }
                    job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                }
                return Ok(());
            }

            info!(
                job_id = %job_id,
                adapter_name = %adapter_name,
                final_loss = training_result.final_loss,
                "Training completed, packaging adapter"
            );

            // Check if packaging is disabled
            if !post_actions.package {
                info!(
                    job_id = %job_id,
                    adapter_name = %adapter_name,
                    final_loss = training_result.final_loss,
                    "Training completed, packaging skipped per post_actions"
                );
                // Mark as completed without packaging
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.status = TrainingJobStatus::Completed;
                    job.progress_pct = 100.0;
                    job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                }
                return Ok(());
            }

            // Step 1: Quantize weights to Q15 format
            let quantized_weights = LoRAQuantizer::quantize_to_q15(&training_result.weights);

            // Step 2: Package the adapter
            // Use adapters_root (already resolved with ENV > Config > Default precedence)
            let packager = AdapterPackager::new(adapters_root.clone());

            // Create worker training config for packaging
            let packager_cfg = WorkerTrainingConfigType {
                rank: worker_cfg.rank,
                alpha: worker_cfg.alpha,
                learning_rate: worker_cfg.learning_rate,
                batch_size: worker_cfg.batch_size,
                epochs: worker_cfg.epochs,
                hidden_dim: worker_cfg.hidden_dim,
                vocab_size: worker_cfg.vocab_size,
                preferred_backend: None,
                require_gpu: false,
                max_gpu_memory_mb: 0,
                checkpoint_interval: worker_cfg.checkpoint_interval,
            };

            // Generate unique adapter ID from job_id
            let adapter_id = format!("adapter-{}", job_id.trim_start_matches("train-"));

            let packaged = match packager
                .package(
                    &adapter_id,
                    &quantized_weights,
                    &packager_cfg,
                    "base-model", // TODO: Make configurable via orchestrator config
                )
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    error!(job_id = %job_id, error = %e, "Failed to package adapter");
                    let mut jobs = jobs_ref.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = TrainingJobStatus::Failed;
                        job.error_message = Some(format!("Packaging failed: {}", e));
                        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                    }
                    return Err(e.into());
                }
            };

            info!(
                job_id = %job_id,
                adapter_id = %packaged.adapter_id,
                weights_path = %packaged.weights_path.display(),
                hash_b3 = %packaged.hash_b3,
                "Adapter packaged successfully"
            );

            // Step 3: Register adapter in database (if db available and register is enabled)
            if let Some(database) = &db_for_packaging {
                if !post_actions.register {
                    info!(
                        job_id = %job_id,
                        adapter_id = %packaged.adapter_id,
                        "Adapter packaged but registration skipped per post_actions"
                    );
                    // Update job status to completed with artifact info but no registration
                    let mut jobs = jobs_ref.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = TrainingJobStatus::Completed;
                        job.progress_pct = 100.0;
                        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                        job.artifact_path =
                            Some(packaged.weights_path.to_string_lossy().to_string());
                        job.adapter_id = Some(packaged.adapter_id.clone());
                        job.weights_hash_b3 = Some(packaged.hash_b3.clone());
                    }
                    return Ok(());
                }

                use adapteros_db::AdapterRegistrationBuilder;

                // Use category from request or default to "trained"
                let adapter_category = category.as_deref().unwrap_or("trained");

                let reg_params = AdapterRegistrationBuilder::new()
                    .adapter_id(&packaged.adapter_id)
                    .name(&adapter_name)
                    .hash_b3(&packaged.hash_b3)
                    .rank(orchestrator_cfg.rank as i32)
                    .tier(&post_actions.tier)
                    .alpha(orchestrator_cfg.alpha as f64)
                    .category(adapter_category)
                    .scope("global")
                    .base_model_id(base_model_id.as_deref())
                    .build()
                    .map_err(|e| anyhow::anyhow!("Failed to build registration params: {}", e))?;

                match database.register_adapter(reg_params).await {
                    Ok(db_id) => {
                        info!(
                            job_id = %job_id,
                            adapter_id = %packaged.adapter_id,
                            db_id = %db_id,
                            "Adapter registered in database"
                        );

                        // Update training job with artifact metadata
                        if let Err(e) = database
                            .update_training_job_artifact(
                                &job_id,
                                packaged.weights_path.to_string_lossy().as_ref(),
                                &packaged.adapter_id,
                                &packaged.hash_b3,
                            )
                            .await
                        {
                            // Log but don't fail - adapter is already registered
                            tracing::warn!(
                                job_id = %job_id,
                                error = %e,
                                "Failed to update job artifact metadata (non-fatal)"
                            );
                        }

                        // Link adapter back to training job for provenance
                        if let Err(e) = database
                            .update_adapter_training_job_id(&packaged.adapter_id, &job_id)
                            .await
                        {
                            // Log but don't fail - adapter is already registered
                            tracing::warn!(
                                job_id = %job_id,
                                adapter_id = %packaged.adapter_id,
                                error = %e,
                                "Failed to link adapter to training job (non-fatal)"
                            );
                        }

                        // Step 4: Optionally create stack with adapter (NOT set as default)
                        if post_actions.create_stack {
                            let tenant_id = tenant_id.as_deref().unwrap_or("default");
                            let stack_name = format!("stack.{}.{}", tenant_id, adapter_name);

                            use adapteros_db::traits::CreateStackRequest;
                            let stack_request = CreateStackRequest {
                                tenant_id: tenant_id.to_string(),
                                name: stack_name.clone(),
                                description: Some(format!(
                                    "Auto-created stack for adapter {}",
                                    adapter_name
                                )),
                                adapter_ids: vec![packaged.adapter_id.clone()],
                                workflow_type: Some("Sequential".to_string()),
                                determinism_mode: None, // Use global default
                            };

                            match database.insert_stack(&stack_request).await {
                                Ok(stack_id) => {
                                    info!(
                                        job_id = %job_id,
                                        adapter_id = %packaged.adapter_id,
                                        stack_id = %stack_id,
                                        "Stack created automatically (not set as default)"
                                    );

                                    // Update training job with stack_id
                                    {
                                        let mut jobs = jobs_ref.write().await;
                                        if let Some(job) = jobs.get_mut(&job_id) {
                                            job.stack_id = Some(stack_id.clone());
                                        }
                                    }

                                    // PRD-CORE-03: Persist stack_id and adapter_id to database for chat_bootstrap endpoint
                                    if let Err(e) = database
                                        .update_training_job_result_ids(
                                            &job_id,
                                            Some(&stack_id),
                                            Some(&packaged.adapter_id),
                                        )
                                        .await
                                    {
                                        warn!(job_id = %job_id, error = %e, "Failed to persist training job result IDs to database");
                                        // Continue - the in-memory values are still set
                                    }

                                    // IMPORTANT: We intentionally do NOT call set_default_stack() here.
                                    // Per PRD decision 5B: "Training finishes → adapter registered →
                                    // stack auto-created, but NOT set as default."
                                    // Users must explicitly set their default stack via the API:
                                    // PUT /v1/tenants/{tenant_id}/default-stack
                                }
                                Err(e) => {
                                    // Log but don't fail - adapter is already registered
                                    tracing::warn!(
                                        job_id = %job_id,
                                        adapter_id = %packaged.adapter_id,
                                        error = %e,
                                        "Failed to create stack (non-fatal)"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(job_id = %job_id, error = %e, "Failed to register adapter in database");
                        let mut jobs = jobs_ref.write().await;
                        if let Some(job) = jobs.get_mut(&job_id) {
                            job.status = TrainingJobStatus::Failed;
                            job.error_message = Some(format!("Registration failed: {}", e));
                            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                        }
                        return Err(e.into());
                    }
                }
            } else {
                tracing::warn!(
                    job_id = %job_id,
                    "No database connection available, skipping adapter registration"
                );
            }

            // Step 5: Update job status to completed with artifact info
            let (initiated_by, initiated_by_role, tenant_id_for_audit) = {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.status = TrainingJobStatus::Completed;
                    job.progress_pct = 100.0;
                    job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                    job.artifact_path = Some(packaged.weights_path.to_string_lossy().to_string());
                    job.adapter_id = Some(packaged.adapter_id.clone());
                    job.weights_hash_b3 = Some(packaged.hash_b3.clone());

                    // Extract audit context for logging
                    (
                        job.initiated_by.clone(),
                        job.initiated_by_role.clone(),
                        job.tenant_id.clone(),
                    )
                } else {
                    (None, None, None)
                }
            };

            // Audit log: training completion (if we have user context and database)
            if let (Some(database), Some(user_id), Some(user_role)) =
                (&db, initiated_by, initiated_by_role)
            {
                // Create a minimal Claims-like structure for audit logging
                let tenant_id_str = tenant_id_for_audit.unwrap_or_else(|| "system".to_string());

                if let Err(e) = database
                    .log_audit(
                        &user_id,
                        &user_role,
                        &tenant_id_str,
                        "training.complete",
                        "training_job",
                        Some(&job_id),
                        "success",
                        None,
                        None,
                        None,
                    )
                    .await
                {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to log training completion audit event"
                    );
                }
            }

            info!(
                job_id = %job_id,
                adapter_id = %packaged.adapter_id,
                "Training job completed successfully"
            );

            Ok(())
        }
        Err(e) => {
            let error_str = e.to_string();

            // Determine if error is retryable based on error type
            // OOM, timeout, network issues = retryable
            // Config errors, validation errors = not retryable
            let is_retryable = {
                let err_lower = error_str.to_lowercase();
                err_lower.contains("out of memory") ||
                err_lower.contains("oom") ||
                err_lower.contains("timeout") ||
                err_lower.contains("timed out") ||
                err_lower.contains("connection") ||
                err_lower.contains("network") ||
                err_lower.contains("resource") ||
                err_lower.contains("busy") ||
                // NOT retryable patterns
                !(err_lower.contains("config") ||
                  err_lower.contains("validation") ||
                  err_lower.contains("invalid") ||
                  err_lower.contains("not found") ||
                  err_lower.contains("permission") ||
                  err_lower.contains("unauthorized"))
            };

            // Update job state
            {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.status = TrainingJobStatus::Failed;
                    job.error_message = Some(error_str.clone());
                    job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                    job.retryable = Some(is_retryable);
                }
            }

            // Persist retryable flag to DB
            if let Some(database) = &db {
                if let Err(db_err) = database
                    .update_training_job_retryable(&job_id, is_retryable)
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        error = %db_err,
                        "Failed to update retryable flag in DB (non-fatal)"
                    );
                } else {
                    info!(
                        job_id = %job_id,
                        retryable = is_retryable,
                        "Training job failed, retryable flag set"
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
                None, // base_model_id
                None, // collection_id
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
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
                None, // base_model_id
                None, // collection_id
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
            )
            .await
            .unwrap();

        // Test without UDS client (will mark as CancelPending)
        service.cancel_job(&job.id, None, None).await.unwrap();

        let updated_job = service.get_job(&job.id).await.unwrap();
        // Without UDS client, job is marked as CancelPending
        assert_eq!(updated_job.status, TrainingJobStatus::CancelPending);
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
                None, // base_model_id
                None, // collection_id
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
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
}
