//! Training job execution - the main training flow.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use adapteros_core::{B3Hash, GuardLogLevel, SeedMode, SeedScopeGuard};
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_lora_worker::training::trainer::EpochMetrics as WorkerEpochMetrics;
use adapteros_lora_worker::training::{
    MicroLoRATrainer as WorkerTrainer, TrainingConfig as WorkerTrainingConfig,
    TrainingExample as WorkerTrainingExample,
};
use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::training::config::{map_preferred_backend, PostActions};
use crate::training::dataset::weighted_round_robin_merge;
use crate::training::job::{DataLineageMode, TrainingConfig, TrainingJob, TrainingJobStatus};
use crate::training::metrics::persist_final_metrics;
use crate::training::packaging::{load_plan_bytes_for_training, package_and_register_adapter};
use crate::training::versioning::VersioningSnapshot;

/// Background runner for a single training job. Converts orchestrator config into worker trainer
/// config, runs training with per-epoch callback, packages weights, registers adapter, and
/// updates the shared job map with artifact metadata.
///
/// The cancel_token is checked by the trainer at epoch boundaries - set it to true to
/// request graceful cancellation. Metrics are persisted to the database after each epoch
/// when db and job_id are provided to the trainer.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_training_job(
    jobs_ref: Arc<RwLock<HashMap<String, TrainingJob>>>,
    job_id: String,
    adapter_name: String,
    orchestrator_cfg: TrainingConfig,
    dataset_id: Option<String>,
    synthetic_mode: bool,
    data_lineage_mode: DataLineageMode,
    tenant_id: Option<String>,
    db: Option<adapteros_db::Db>,
    storage_root: Option<PathBuf>,
    category: Option<String>,
    post_actions_json: Option<String>,
    base_model_id: Option<String>,
    cancel_token: Arc<AtomicBool>,
) -> Result<()> {
    // Ensure seed registry is scoped to this training job to prevent cross-job seed reuse errors.
    // This mirrors the pattern used in inference_core.rs for determinism consistency.
    let _seed_scope = SeedScopeGuard::for_training(GuardLogLevel::Warn);

    let versioning_snapshot = {
        let jobs = jobs_ref.read().await;
        jobs.get(&job_id).map(|job| VersioningSnapshot {
            adapter_version_id: job.adapter_version_id.clone(),
            version_label: job.version_label.clone(),
            target_branch: job.target_branch.clone(),
            repo_name: job.repo_name.clone(),
            repo_id: job.repo_id.clone(),
            base_version_id: job.base_version_id.clone(),
            code_commit_sha: job.code_commit_sha.clone(),
            data_spec_hash: job.data_spec_hash.clone(),
            dataset_version_ids: job.dataset_version_ids.clone(),
        })
    };

    // Mark version as training when applicable
    if let (Some(database), Some(version_id)) = (
        db.clone(),
        versioning_snapshot
            .as_ref()
            .and_then(|v| v.adapter_version_id.clone()),
    ) {
        if let Err(e) = database
            .set_adapter_version_state_with_metadata(
                &version_id,
                "training",
                None,
                Some("orchestrator"),
                Some("training_start"),
                Some(&job_id),
            )
            .await
        {
            warn!(
                version_id = %version_id,
                error = %e,
                "Failed to mark adapter version as training (non-fatal)"
            );
        }
    }

    // Fetch base adapter artifact if provided
    let _base_aos_path: Option<PathBuf> = match (
        versioning_snapshot
            .as_ref()
            .and_then(|v| v.base_version_id.clone()),
        db.clone(),
    ) {
        (Some(base_version_id), Some(database)) => {
            let tenant_lookup = tenant_id.as_deref().unwrap_or("default");
            match database
                .get_adapter_version(tenant_lookup, &base_version_id)
                .await
            {
                Ok(Some(version)) => {
                    if let Some(path) = version.aos_path {
                        Some(PathBuf::from(path))
                    } else {
                        return Err(anyhow::anyhow!(format!(
                            "Base adapter version {} missing aos_path",
                            base_version_id
                        )));
                    }
                }
                Ok(None) => {
                    return Err(anyhow::anyhow!(format!(
                        "Base adapter version {} not found",
                        base_version_id
                    )));
                }
                Err(e) => return Err(anyhow::anyhow!(e)),
            }
        }
        _ => None,
    };

    if let Some(ref ver) = versioning_snapshot {
        if let Some(ref sha) = ver.code_commit_sha {
            info!(job_id = %job_id, commit = %sha, "Resolved training code commit");
        }
        if let Some(ref spec) = ver.data_spec_hash {
            info!(job_id = %job_id, data_spec_hash = %spec, "Resolved training data spec hash");
        }
    }

    let version_id_for_state = versioning_snapshot
        .as_ref()
        .and_then(|v| v.adapter_version_id.clone());
    let db_for_state = db.clone();
    let job_id_for_run = job_id.clone();

    let outcome: Result<()> = async move {
        let job_id = job_id_for_run;

        // Parse post-actions configuration (defaults if not provided or invalid)
        let post_actions: PostActions = post_actions_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();

        // Determine adapters root using centralized path resolution (ENV > Config > Default)
        use adapteros_core::paths::AdapterPaths;
        let adapters_root = {
            let storage_adapters_str = storage_root
                .as_ref()
                .map(|s| s.join("adapters").to_string_lossy().to_string());
            let config_value = post_actions
                .adapters_root
                .as_deref()
                .or(storage_adapters_str.as_deref());
            AdapterPaths::from_config(config_value).root().to_path_buf()
        };
        let tenant = tenant_id.as_deref().unwrap_or("default");

        // Transition to running
        {
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = TrainingJobStatus::Running;
                job.started_at = Some(chrono::Utc::now().to_rfc3339());
            }
        }

        // Map orchestrator config to worker trainer config
        let preferred_backend = map_preferred_backend(
            orchestrator_cfg.preferred_backend,
            orchestrator_cfg.coreml_training_fallback,
        );
        let mut worker_cfg = WorkerTrainingConfig {
            rank: orchestrator_cfg.rank as usize,
            alpha: orchestrator_cfg.alpha as f32,
            learning_rate: orchestrator_cfg.learning_rate,
            batch_size: orchestrator_cfg.batch_size as usize,
            epochs: orchestrator_cfg.epochs as usize,
            hidden_dim: 768,
            vocab_size: 32000,
            coreml_placement: orchestrator_cfg.coreml_placement.clone(),
            preferred_backend: preferred_backend.preferred,
            backend_policy: orchestrator_cfg.backend_policy,
            coreml_fallback_backend: preferred_backend.coreml_fallback,
            require_gpu: orchestrator_cfg.require_gpu,
            max_gpu_memory_mb: orchestrator_cfg.max_gpu_memory_mb.unwrap_or(0),
            max_tokens_per_batch: None,
            device_policy: None,
            checkpoint_interval: Some(5),
            warmup_steps: orchestrator_cfg.warmup_steps,
            max_seq_length: orchestrator_cfg.max_seq_length,
            gradient_accumulation_steps: orchestrator_cfg.gradient_accumulation_steps,
            early_stopping: orchestrator_cfg.early_stopping,
            patience: orchestrator_cfg.patience,
            min_delta: orchestrator_cfg.min_delta,
            determinism: None,
            moe_config: None,
            use_gpu_backward: true,
            optimizer_config: Default::default(),
            base_model_path: orchestrator_cfg.base_model_path.clone(),
            hidden_state_layer: orchestrator_cfg.hidden_state_layer.clone(),
            validation_split: orchestrator_cfg.validation_split.unwrap_or(0.0),
        };

        // If a CoreML placement is provided, align hidden_dim to the placement shapes for training.
        if let Some(placement) = orchestrator_cfg.coreml_placement.as_ref() {
            if let Some(first) = placement.bindings.first() {
                let placement_hidden = first.shape.output_dim as usize;
                if placement_hidden > 0 && worker_cfg.hidden_dim != placement_hidden {
                    tracing::info!(
                        worker_hidden_dim = worker_cfg.hidden_dim,
                        placement_hidden_dim = placement_hidden,
                        "Adjusting worker hidden_dim to CoreML placement output_dim"
                    );
                    worker_cfg.hidden_dim = placement_hidden;
                }
            }
        }

        let db_for_packaging = db.clone();

        let dataset_version_ids_for_training = versioning_snapshot
            .as_ref()
            .and_then(|v| v.dataset_version_ids.clone());
        let data_spec_hash_for_training = versioning_snapshot
            .as_ref()
            .and_then(|v| v.data_spec_hash.clone());
        let tokenizer_path = worker_cfg
            .base_model_path
            .as_ref()
            .map(|path| path.join("tokenizer.json"))
            .filter(|path| path.exists());

        // Load training examples from dataset versions (if provided) or dataset_id, otherwise synthetic
        let examples: Vec<WorkerTrainingExample> = match (
            dataset_version_ids_for_training.clone(),
            dataset_id.clone(),
            db.clone(),
            storage_root.clone(),
        ) {
            (Some(version_selections), _, Some(database), Some(storage)) => {
                use crate::training_dataset_integration::TrainingDatasetManager;
                let dataset_manager =
                    TrainingDatasetManager::new(database, storage, tokenizer_path.clone());

                if version_selections.is_empty() {
                    return Err(anyhow::anyhow!(
                        "dataset_version_ids provided but empty for job {}",
                        job_id
                    ));
                }

                let mut per_version: Vec<(Vec<WorkerTrainingExample>, f32)> = Vec::new();
                for sel in version_selections.iter() {
                    let (examples, hash_b3, _dataset_id_for_ver) = dataset_manager
                        .load_dataset_version_examples(&sel.dataset_version_id)
                        .await
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to load dataset version {}: {}",
                                sel.dataset_version_id,
                                e
                            )
                        })?;

                    if let Some(ref expected_hash) = data_spec_hash_for_training {
                        if expected_hash != &hash_b3 {
                            return Err(anyhow::anyhow!(format!(
                                "Dataset version {} hash mismatch vs data_spec_hash (expected {}, got {})",
                                sel.dataset_version_id, expected_hash, hash_b3
                            )));
                        }
                    }

                    let weight = if sel.weight <= 0.0 { 1.0 } else { sel.weight };
                    per_version.push((examples, weight));
                }

                tracing::info!(
                    job_id = %job_id,
                    versions = ?version_selections,
                    "Loaded dataset versions for training"
                );

                weighted_round_robin_merge(per_version)
            }
            (_, Some(ds_id), Some(database), Some(storage)) => {
                use crate::training_dataset_integration::TrainingDatasetManager;
                let dataset_manager =
                    TrainingDatasetManager::new(database, storage, tokenizer_path.clone());
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

        // Record determinism/backends expectations on job snapshot
        {
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.determinism_mode = Some("hkdf_seeded".to_string());
                job.training_seed = Some(trainer.training_seed());
                job.require_gpu = Some(worker_cfg.require_gpu);
                job.max_gpu_memory_mb = Some(worker_cfg.max_gpu_memory_mb);
            }
        }

        let determinism_seed_override = worker_cfg
            .determinism
            .as_ref()
            .and_then(|d| d.seed)
            .filter(|seed| *seed != 0);
        let global_seed = determinism_seed_override
            .map(|seed| B3Hash::hash(&seed.to_le_bytes()))
            .unwrap_or_else(|| B3Hash::hash(b"training"));
        let global_seed_hex = global_seed.to_hex();
        let seed_mode = SeedMode::default().as_str();
        let determinism_config_json = serde_json::to_string(&serde_json::json!({
            "seed_mode": seed_mode,
            "training_seed": trainer.training_seed(),
            "seed_override": determinism_seed_override,
            "determinism": worker_cfg.determinism.clone(),
            "dataset_version_ids": dataset_version_ids_for_training.as_ref(),
            "data_spec_hash": data_spec_hash_for_training.as_deref(),
            "base_model_id": base_model_id.as_deref(),
            "job_id": job_id.as_str(),
        }))
        .ok();
        let is_deterministic_run =
            worker_cfg.determinism.is_some() || cfg!(feature = "deterministic-only");
        if let Some(database) = &db {
            if let Err(e) = database
                .update_training_job_determinism(
                    &job_id,
                    is_deterministic_run,
                    Some(global_seed_hex.as_str()),
                    determinism_config_json.as_deref(),
                    Some(seed_mode),
                )
                .await
            {
                warn!(
                    job_id = %job_id,
                    error = %e,
                    "Failed to persist training determinism metadata (non-fatal)"
                );
            }
        }

        // Wire job_id and DB for metrics persistence (before GPU init so telemetry carries job_id)
        trainer.set_job_id(job_id.clone());
        trainer.set_cancel_token(cancel_token);
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

            if let Err(e) = std::fs::create_dir_all(&checkpoint_dir) {
                tracing::warn!(
                    job_id = %job_id,
                    error = %e,
                    "Failed to create checkpoint directory, checkpoints disabled"
                );
            } else {
                trainer.enable_checkpointing(&checkpoint_dir, &job_id, 3);
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
        let require_gpu = worker_cfg.require_gpu;
        let result = async {
            let plan_bytes = load_plan_bytes_for_training(require_gpu, &job_id)?;
            if plan_bytes.is_empty() && !require_gpu {
                info!(
                    job_id = %job_id,
                    "Proceeding without plan bytes; GPU init will be skipped and CPU will be used"
                );
            }

            trainer.init_kernels(&plan_bytes)?;

            trainer
                .train_with_resume(&examples, move |metrics: WorkerEpochMetrics| {
                    // Emit per-epoch timing telemetry
                    let duration_ms = metrics.duration_us / 1000;
                    tracing::event!(
                        tracing::Level::INFO,
                        name = "epoch_completed",
                        job_id = %job_id_clone,
                        epoch = metrics.epoch,
                        duration_ms = duration_ms,
                        loss = metrics.loss,
                        tokens_per_sec = metrics.tokens_per_sec,
                        examples_per_sec = metrics.examples_per_sec,
                        tokens_in_epoch = metrics.tokens_in_epoch,
                        examples_in_epoch = metrics.examples_in_epoch,
                        total_tokens_processed = metrics.total_tokens_processed,
                        total_examples_processed = metrics.total_examples_processed,
                        "Training epoch completed"
                    );

                    let jobs_ref_inner = jobs_ref_clone.clone();
                    let job_id_inner = job_id_clone.clone();
                    let jobs_ref_for_det = jobs_ref_inner.clone();
                    let job_id_for_det = job_id_inner.clone();
                    let jobs_ref_for_fallback = jobs_ref_inner.clone();
                    let job_id_for_fallback = job_id_inner.clone();

                    // Add atomic flag to prevent dual execution of progress updates
                    use std::sync::atomic::Ordering;
                    let executed = Arc::new(AtomicBool::new(false));
                    let executed_clone = executed.clone();

                    if let Err(e) = spawn_deterministic(
                        format!(
                            "training-progress:{}:epoch-{}",
                            job_id_for_det, metrics.epoch
                        ),
                        async move {
                            // Only execute if not already executed by fallback
                            if executed_clone
                                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok()
                            {
                                let mut jobs = jobs_ref_for_det.write().await;
                                if let Some(job) = jobs.get_mut(&job_id_for_det) {
                                    job.current_epoch = metrics.epoch;
                                    job.current_loss = metrics.loss;
                                    job.tokens_per_second = metrics.tokens_per_sec;
                                    job.examples_processed = Some(metrics.total_examples_processed);
                                    job.tokens_processed = Some(metrics.total_tokens_processed);
                                    job.throughput_examples_per_sec = Some(metrics.examples_per_sec);
                                    if job.total_epochs > 0 {
                                        job.progress_pct =
                                            (metrics.epoch as f32 / job.total_epochs as f32) * 100.0;
                                    }
                                }
                            }
                        },
                    ) {
                        tracing::warn!("Failed to spawn deterministic progress update: {}", e);
                        // Only run fallback if deterministic didn't execute
                        if executed
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            tokio::spawn(async move {
                                let mut jobs = jobs_ref_for_fallback.write().await;
                                if let Some(job) = jobs.get_mut(&job_id_for_fallback) {
                                    job.current_epoch = metrics.epoch;
                                    job.current_loss = metrics.loss;
                                    job.tokens_per_second = metrics.tokens_per_sec;
                                    job.examples_processed = Some(metrics.total_examples_processed);
                                    job.tokens_processed = Some(metrics.total_tokens_processed);
                                    job.throughput_examples_per_sec = Some(metrics.examples_per_sec);
                                    if job.total_epochs > 0 {
                                        job.progress_pct =
                                            (metrics.epoch as f32 / job.total_epochs as f32) * 100.0;
                                    }
                                }
                            });
                        }
                    }
                })
                .await
        }
        .await;

        match result {
            Ok(training_result) => {
                // Capture backend selection and performance metrics after training
                let backend_selected = trainer.backend_info().map(|b| b.to_string());
                let perf = trainer.get_performance_metrics();
                let training_time_ms = training_result.training_time_ms();
                let examples_processed = training_result.examples_processed.unwrap_or(0);
                let tokens_processed = training_result.tokens_processed.unwrap_or(0);
                let tokens_per_second = training_result.tokens_per_sec;
                let examples_per_sec = training_result.examples_per_sec;

                // Observability: log CoreML forward metrics if available
                if let Some(samples) = perf.coreml_forward_samples.checked_sub(0) {
                    tracing::info!(
                        job_id = %job_id,
                        backend = ?backend_selected,
                        coreml_forward_samples = samples,
                        coreml_forward_mean_us = ?perf.coreml_forward_mean_us,
                        coreml_forward_p95_us = ?perf.coreml_forward_p95_us,
                        total_tokens = tokens_processed,
                        "CoreML forward metrics recorded"
                    );
                }

                {
                    let mut jobs = jobs_ref.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.backend = backend_selected.clone();
                        job.backend_device = training_result.backend_device.clone();
                        job.backend_reason = trainer.backend_reason().map(|s| s.to_string());
                        job.determinism_mode = job
                            .determinism_mode
                            .clone()
                            .or(Some("hkdf_seeded".to_string()));
                        job.training_seed = job.training_seed.or(Some(trainer.training_seed()));
                        job.examples_processed = Some(examples_processed);
                        job.tokens_processed = Some(tokens_processed);
                        job.training_time_ms = Some(training_time_ms);
                        job.throughput_examples_per_sec = Some(if examples_per_sec > 0.0 {
                            examples_per_sec
                        } else {
                            perf.throughput_examples_per_sec
                        });
                        job.tokens_per_second = tokens_per_second;
                        job.gpu_utilization_pct = Some(perf.avg_gpu_utilization);
                        job.peak_gpu_memory_mb = Some(perf.peak_gpu_memory_mb);
                        job.require_gpu = job.require_gpu.or(Some(worker_cfg.require_gpu));
                        job.max_gpu_memory_mb =
                            job.max_gpu_memory_mb.or(Some(worker_cfg.max_gpu_memory_mb));
                    }
                }

                // Persist final summary metrics to database
                if let Some(database) = &db {
                    let _ = persist_final_metrics(database, &job_id, &training_result).await;
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
                    drop(jobs);

                    if let Some(database) = &db {
                        if let Err(e) = database.update_training_status(&job_id, "cancelled").await
                        {
                            warn!(job_id = %job_id, error = %e, "Failed to persist training cancellation status to DB (non-fatal)");
                        }
                    }

                    if let (Some(database), Some(version_id)) = (
                        db.clone(),
                        versioning_snapshot
                            .as_ref()
                            .and_then(|v| v.adapter_version_id.clone()),
                    ) {
                        if let Err(e) = database
                            .set_adapter_version_state_with_metadata(
                                &version_id,
                                "failed",
                                Some("cancelled"),
                                Some("orchestrator"),
                                Some("training_cancelled"),
                                Some(&job_id),
                            )
                            .await
                        {
                            warn!(
                                version_id = %version_id,
                                error = %e,
                                "Failed to mark adapter version cancelled (non-fatal)"
                            );
                        }
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
                    let mut jobs = jobs_ref.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = TrainingJobStatus::Completed;
                        job.progress_pct = 100.0;
                        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                    }
                    drop(jobs);

                    if let Some(database) = &db {
                        if let Err(e) =
                            database.update_training_status(&job_id, "completed").await
                        {
                            warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
                        }
                    }

                    return Ok(());
                }

                // Package and register adapter
                package_and_register_adapter(
                    jobs_ref.clone(),
                    &job_id,
                    &adapter_name,
                    &training_result,
                    &worker_cfg,
                    &orchestrator_cfg,
                    &post_actions,
                    &adapters_root,
                    tenant,
                    tenant_id.as_deref(),
                    dataset_id.as_deref(),
                    synthetic_mode,
                    data_lineage_mode,
                    base_model_id.as_deref(),
                    category.as_deref(),
                    versioning_snapshot.as_ref(),
                    dataset_version_ids_for_training.as_ref(),
                    data_spec_hash_for_training.as_deref(),
                    trainer.training_seed(),
                    db_for_packaging.as_ref(),
                )
                .await?;

                Ok(())
            }
            Err(e) => {
                let error_str = e.to_string();

                // Determine if error is retryable based on error type
                let is_retryable = {
                    let err_lower = error_str.to_lowercase();
                    err_lower.contains("out of memory")
                        || err_lower.contains("oom")
                        || err_lower.contains("timeout")
                        || err_lower.contains("timed out")
                        || err_lower.contains("connection")
                        || err_lower.contains("network")
                        || err_lower.contains("resource")
                        || err_lower.contains("busy")
                        || !(err_lower.contains("config")
                            || err_lower.contains("validation")
                            || err_lower.contains("invalid")
                            || err_lower.contains("not found")
                            || err_lower.contains("permission")
                            || err_lower.contains("unauthorized"))
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

                // Persist failure status and retryable flag to DB
                if let Some(database) = &db {
                    if let Err(e) = database.update_training_status(&job_id, "failed").await {
                        warn!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to persist training failure status to DB (non-fatal)"
                        );
                    }

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
                            "Training job failed, status and retryable flag persisted"
                        );
                    }
                }

                Err(e.into())
            }
        }
    }
    .await;

    if outcome.is_err() {
        if let (Some(database), Some(version_id)) = (db_for_state, version_id_for_state) {
            let reason = outcome.as_ref().err().map(|e| e.to_string());
            let _ = database
                .set_adapter_version_state_with_metadata(
                    &version_id,
                    "failed",
                    reason.as_deref(),
                    Some("orchestrator"),
                    Some("training_failed"),
                    Some(&job_id),
                )
                .await;
            warn!(
                job_id = %job_id,
                version_id = %version_id,
                reason = ?reason,
                "history event: training_failed"
            );
        }
    }

    outcome
}
