//! Training job execution - the main training flow.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use adapteros_config::ModelConfig;
use adapteros_core::{AosError, B3Hash, GuardLogLevel, Result, SeedMode, SeedScopeGuard};
use adapteros_db::training_jobs::TrainingProgress;
use adapteros_db::ProtectedDb;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_lora_worker::training::trainer::{EpochMetrics as WorkerEpochMetrics, OptimizerType};
use adapteros_lora_worker::training::{
    preprocessing::preprocess_examples, split_examples_for_validation,
    MicroLoRATrainer as WorkerTrainer, PreprocessCompression as WorkerPreprocessCompression,
    PreprocessingConfig as WorkerPreprocessingConfig, TrainingBackend as WorkerTrainingBackend,
    TrainingConfig as WorkerTrainingConfig, TrainingExample as WorkerTrainingExample,
};
use adapteros_types::training::{
    ExampleMetadataV1, OptimizerConfigSummary, PreprocessCompression as ApiPreprocessCompression,
    PreprocessingConfig as ApiPreprocessingConfig, TrainingDataContractConfig,
};
use blake3::Hasher;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::training::config::{map_preferred_backend, PostActions};
use crate::training::dataset::weighted_round_robin_merge;
use crate::training::job::{DataLineageMode, TrainingConfig, TrainingJob, TrainingJobStatus};
use crate::training::metrics::persist_final_metrics;
use crate::training::packaging::{load_plan_bytes_for_training, package_and_register_adapter};
use crate::training::pipeline::{
    PhaseStatus, PipelineConfigSnapshot, PipelinePhase, TrainingPipeline,
};
use crate::training::report::write_training_report;
use crate::training::versioning::VersioningSnapshot;
use crate::training_dataset_integration::TrainingFramingPolicy;

const DEV_FAILFAST_ENV: &str = "ADAPTEROS_DEV_TRAINING_FAILFAST";
const DEV_FAILFAST_REASON_ENV: &str = "ADAPTEROS_DEV_TRAINING_FAILFAST_REASON";

fn parse_dev_failfast_enabled(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn deterministic_dev_failfast_reason() -> Option<String> {
    if !cfg!(debug_assertions) {
        return None;
    }

    let enabled = std::env::var(DEV_FAILFAST_ENV)
        .map(|raw| parse_dev_failfast_enabled(&raw))
        .unwrap_or(false);
    if !enabled {
        return None;
    }

    Some(
        std::env::var(DEV_FAILFAST_REASON_ENV)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "Deterministic dev fail-fast triggered via {}",
                    DEV_FAILFAST_ENV
                )
            }),
    )
}

/// Background runner for a single training job. Converts orchestrator config into worker trainer
/// config, runs training with per-epoch callback, packages weights, registers adapter, and
/// updates the shared job map with artifact metadata.
///
/// The cancel_token is checked by the trainer at epoch boundaries - set it to true to
/// request graceful cancellation. The pause_token is checked between epochs by the scheduler
/// to temporarily pause training when inference is active.
///
/// Metrics are persisted to the database after each epoch when db and job_id are provided
/// to the trainer.
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
    artifacts_root: Option<PathBuf>,
    category: Option<String>,
    post_actions_json: Option<String>,
    base_model_id: Option<String>,
    base_model_tenant_or_workspace_id: Option<String>,
    cancel_token: Arc<AtomicBool>,
    pause_token: Arc<AtomicBool>,
    scheduler: Arc<crate::training::scheduler::TrainingScheduler>,
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
                        return Err(AosError::Validation(format!(
                            "Base adapter version {} missing aos_path",
                            base_version_id
                        )));
                    }
                }
                Ok(None) => {
                    return Err(AosError::NotFound(format!(
                        "Base adapter version {} not found",
                        base_version_id
                    )));
                }
                Err(e) => return Err(AosError::Database(e.to_string())),
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
    let jobs_ref_for_state = jobs_ref.clone();
    let job_id_for_run = job_id.clone();

    let outcome: Result<()> = async move {
        let job_id = job_id_for_run;

        // Deterministic dev fail-fast probe for operational verification.
        // This is intentionally debug-only and explicitly opt-in via env flag.
        if let Some(reason) = deterministic_dev_failfast_reason() {
            return Err(AosError::Validation(reason));
        }

        // Parse post-actions configuration (defaults if not provided or invalid)
        let post_actions: PostActions = match post_actions_json.as_ref() {
            Some(json) => match serde_json::from_str(json) {
                Ok(actions) => actions,
                Err(e) => {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to parse post_actions_json, using defaults"
                    );
                    PostActions::default()
                }
            },
            None => PostActions::default(),
        };

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

        // Normalize orchestrator config for deterministic hashing before mapping
        let mut orchestrator_cfg = orchestrator_cfg;
        orchestrator_cfg.normalize();

        // Map orchestrator config to worker trainer config
        let preferred_backend = map_preferred_backend(
            orchestrator_cfg.preferred_backend,
            orchestrator_cfg.coreml_training_fallback,
        );
        let runtime_caps = adapteros_lora_worker::backend_factory::detect_capabilities();
        // GPU backward currently requires MLX. Fall back to CPU-proxy training when
        // MLX is unavailable so jobs can still execute on CoreML/Metal/CPU hosts.
        let use_gpu_backward = runtime_caps.has_mlx
            || matches!(preferred_backend.preferred, Some(WorkerTrainingBackend::Mlx));
        if !use_gpu_backward {
            warn!(
                adapter = %adapter_name,
                "MLX backend unavailable; using CPU-proxy training path (use_gpu_backward=false)"
            );
        }
        let mut worker_cfg = WorkerTrainingConfig {
            rank: orchestrator_cfg.rank as usize,
            alpha: orchestrator_cfg.alpha as f32,
            learning_rate: orchestrator_cfg.learning_rate,
            batch_size: orchestrator_cfg.batch_size as usize,
            epochs: orchestrator_cfg.epochs as usize,
            hidden_dim: 768,
            vocab_size: 32000,
            training_contract_version: orchestrator_cfg.training_contract_version.clone(),
            pad_token_id: orchestrator_cfg.pad_token_id,
            ignore_index: orchestrator_cfg.ignore_index,
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
            use_gpu_backward,
            optimizer_config: Default::default(),
            base_model_path: orchestrator_cfg.base_model_path.clone(),
            hidden_state_layer: orchestrator_cfg.hidden_state_layer.clone(),
            validation_split: orchestrator_cfg.validation_split.unwrap_or(0.0),
            preprocessing: map_preprocessing_config_opt(orchestrator_cfg.preprocessing.clone()),
            targets: orchestrator_cfg.targets.clone(),
            multi_module_training: orchestrator_cfg.multi_module_training,
            lora_layer_indices: orchestrator_cfg.lora_layer_indices.clone(),
            mlx_version: None, // Will be populated during trainer initialization
        };

        if let Some(base_model_path) = resolve_base_model_path(
            worker_cfg.base_model_path.clone(),
            db.as_ref(),
            tenant_id.as_deref(),
            base_model_id.as_deref(),
        )
        .await
        {
            worker_cfg.base_model_path = Some(base_model_path.clone());
            match ModelConfig::from_config_json(&base_model_path) {
                Ok(model_cfg) => {
                    if worker_cfg.hidden_dim != model_cfg.hidden_size {
                        tracing::info!(
                            worker_hidden_dim = worker_cfg.hidden_dim,
                            model_hidden_dim = model_cfg.hidden_size,
                            "Aligning training hidden_dim with base model config"
                        );
                        worker_cfg.hidden_dim = model_cfg.hidden_size;
                    }
                    if worker_cfg.vocab_size != model_cfg.vocab_size {
                        tracing::info!(
                            worker_vocab_size = worker_cfg.vocab_size,
                            model_vocab_size = model_cfg.vocab_size,
                            "Aligning training vocab_size with base model config"
                        );
                        worker_cfg.vocab_size = model_cfg.vocab_size;
                    }
                }
                Err(e) => {
                    warn!(
                        path = %base_model_path.display(),
                        error = %e,
                        "Failed to load base model config.json; keeping default hidden_dim/vocab_size"
                    );
                }
            }
        }

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

        let pipeline_training_config_hash = compute_pipeline_training_config_hash(&worker_cfg)?;
        let base_model_hash = compute_pipeline_base_model_hash(worker_cfg.base_model_path.as_ref())?;

        let db_for_packaging = db.clone();

        let dataset_version_ids_for_training = versioning_snapshot
            .as_ref()
            .and_then(|v| v.dataset_version_ids.clone());
        let data_spec_hash_for_training = versioning_snapshot
            .as_ref()
            .and_then(|v| v.data_spec_hash.clone());
        let config_snapshot = PipelineConfigSnapshot {
            training_config: orchestrator_cfg.clone(),
            dataset_id: dataset_id.clone(),
            dataset_version_ids: dataset_version_ids_for_training.clone(),
            data_spec_hash: data_spec_hash_for_training.clone(),
            synthetic_mode,
            data_lineage_mode,
            base_model_id: base_model_id.clone(),
        };
        let mut pipeline =
            TrainingPipeline::load_or_init(&job_id, config_snapshot, storage_root.as_deref())
                .await?;
        pipeline
            .seed_receipt(
                &pipeline_training_config_hash,
                &base_model_hash,
                dataset_id.as_deref(),
                orchestrator_cfg.training_contract_version.as_str(),
            )
            .await?;

        if pipeline.is_complete() {
            info!(job_id = %job_id, "Training pipeline already complete; skipping execution");
            let mut mark_completed = false;
            {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    if matches!(job.status, TrainingJobStatus::Pending | TrainingJobStatus::Running)
                    {
                        job.status = TrainingJobStatus::Completed;
                        job.progress_pct = 100.0;
                        if job.completed_at.is_none() {
                            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                        }
                        mark_completed = true;
                    }
                }
            }
            if mark_completed {
                if let Some(database) = &db {
                    if let Err(e) = database.update_training_status(&job_id, "completed").await {
                        warn!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to persist training completion status to DB (non-fatal)"
                        );
                    }
                }
            }
            return Ok(());
        }

        // Transition to running
        {
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = TrainingJobStatus::Running;
                job.started_at = Some(chrono::Utc::now().to_rfc3339());
            }
        }
        let (tokenizer_path, tokenizer_hash_b3) = resolve_tokenizer_info(
            worker_cfg.base_model_path.as_ref(),
            db.as_ref(),
            tenant_id.as_deref(),
            base_model_id.as_deref(),
        )
        .await?;

        // Load training examples from dataset versions (if provided) or dataset_id, otherwise synthetic
        let dataset_phase_active = pipeline.current_phase() == PipelinePhase::DatasetBuild;
        if dataset_phase_active {
            pipeline.enter_phase(PipelinePhase::DatasetBuild).await?;
        }
        let mut dataset_source = "synthetic";
        let mut dataset_ids_for_receipt: Vec<String> = Vec::new();
        let mut dataset_version_hashes: Vec<String> = Vec::new();
        let mut dataset_framing_policy: Option<TrainingFramingPolicy> = None;
        let mut dataset_file_hash_b3: Option<String> = None;
        let examples: Vec<WorkerTrainingExample> = match (
            dataset_version_ids_for_training.clone(),
            dataset_id.clone(),
            db.clone(),
            storage_root.clone(),
        ) {
            (Some(version_selections), _, Some(database), Some(storage)) => {
                use crate::training_dataset_integration::TrainingDatasetManager;
                let dataset_manager = TrainingDatasetManager::new(
                    ProtectedDb::new(database),
                    storage,
                    Some(tokenizer_path.clone()),
                )
                .with_pad_token_id(worker_cfg.pad_token_id);
                dataset_source = "dataset_versions";

                if version_selections.is_empty() {
                    return Err(AosError::Validation(format!(
                        "dataset_version_ids provided but empty for job {}",
                        job_id
                    )));
                }

                let mut per_version: Vec<(Vec<WorkerTrainingExample>, f32)> = Vec::new();
                for sel in version_selections.iter() {
                    let loaded = dataset_manager
                        .load_dataset_version_examples(&sel.dataset_version_id)
                        .await
                        .map_err(|e| {
                            AosError::Internal(format!(
                                "Failed to load dataset version {}: {}",
                                sel.dataset_version_id,
                                e
                            ))
                        })?;
                    if let Some(active) = dataset_framing_policy {
                        if active != loaded.framing_policy {
                            warn!(
                                job_id = %job_id,
                                dataset_version_id = %sel.dataset_version_id,
                                active_policy = active.as_str(),
                                incoming_policy = loaded.framing_policy.as_str(),
                                "Mixed dataset framing policies detected; normalizing training framing policy to supervised"
                            );
                            dataset_framing_policy = Some(TrainingFramingPolicy::Supervised);
                        }
                    } else {
                        dataset_framing_policy = Some(loaded.framing_policy);
                    }

                    dataset_version_hashes.push(loaded.dataset_hash_b3.clone());
                    dataset_ids_for_receipt.push(loaded.dataset_id.clone());

                    let weight = if sel.weight <= 0.0 { 1.0 } else { sel.weight };
                    per_version.push((loaded.examples, weight));
                }

                // Validate data_spec_hash using the combined hash (matching how it
                // was computed at job creation time), not per-version hashes.
                if let Some(ref expected_hash) = data_spec_hash_for_training {
                    use crate::training::versioning::compute_combined_data_spec_hash;
                    let entries: Vec<(String, String, f32)> = version_selections
                        .iter()
                        .zip(dataset_version_hashes.iter())
                        .map(|(sel, hash)| {
                            let w = if sel.weight <= 0.0 { 1.0 } else { sel.weight };
                            (sel.dataset_version_id.clone(), hash.clone(), w)
                        })
                        .collect();
                    let actual_combined = compute_combined_data_spec_hash(&entries);
                    if expected_hash != &actual_combined {
                        return Err(AosError::Validation(format!(
                            "Combined data_spec_hash mismatch (expected {}, got {})",
                            expected_hash, actual_combined
                        )));
                    }
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
                let dataset_manager = TrainingDatasetManager::new(
                    ProtectedDb::new(database),
                    storage,
                    Some(tokenizer_path.clone()),
                )
                .with_pad_token_id(worker_cfg.pad_token_id);
                dataset_source = "dataset_id";
                let loaded = dataset_manager
                    .load_dataset_examples(&ds_id)
                    .await
                    .map_err(|e| AosError::Internal(format!("Failed to load dataset: {}", e)))?;
                dataset_ids_for_receipt.push(loaded.dataset_id.clone());
                dataset_framing_policy = Some(loaded.framing_policy);
                dataset_file_hash_b3 = Some(loaded.dataset_hash_b3.clone());
                loaded.examples
            }
            _ => {
                if !synthetic_mode {
                    return Err(AosError::Validation(format!(
                        "Training job {} missing dataset_id or dataset_version_ids (synthetic_mode=false)",
                        job_id
                    )));
                }
                // Explicit synthetic mode only.
                tracing::warn!(
                    "Synthetic training data requested for job {} (synthetic_mode=true)",
                    job_id
                );
                dataset_framing_policy = Some(TrainingFramingPolicy::Supervised);
                vec![
                    WorkerTrainingExample::new(
                        vec![1, 2, 3],
                        vec![4, 5, 6],
                        vec![1, 1, 1],
                        ExampleMetadataV1::new(
                            "synthetic",
                            0,
                            B3Hash::hash(b"synthetic-0").to_hex(),
                            "{}",
                            0,
                        ),
                    ),
                    WorkerTrainingExample::new(
                        vec![7, 8, 9],
                        vec![10, 11, 12],
                        vec![1, 1, 1],
                        ExampleMetadataV1::new(
                            "synthetic",
                            1,
                            B3Hash::hash(b"synthetic-1").to_hex(),
                            "{}",
                            0,
                        ),
                    ),
                ]
            }
        };
        let dataset_hash_b3 = hash_examples_for_receipt(&examples);
        let framing_policy = dataset_framing_policy.ok_or_else(|| {
            AosError::Internal("Dataset framing policy missing after load".to_string())
        })?;
        let dataset_ids_receipt = if dataset_ids_for_receipt.is_empty() {
            None
        } else {
            Some(dataset_ids_for_receipt.clone())
        };
        let dataset_version_hashes_receipt = if dataset_version_hashes.is_empty() {
            None
        } else {
            Some(dataset_version_hashes)
        };
        let dataset_id_label =
            resolve_dataset_id_for_report(dataset_id.as_deref(), &dataset_ids_for_receipt);
        if dataset_phase_active {
            let mut inputs = HashMap::new();
            inputs.insert("dataset_id".to_string(), dataset_id_label.clone());
            inputs.insert("dataset_source".to_string(), dataset_source.to_string());
            if let Some(ref data_spec_hash) = data_spec_hash_for_training {
                inputs.insert("data_spec_hash".to_string(), data_spec_hash.clone());
            }
            if let Some(ref selections) = dataset_version_ids_for_training {
                let joined = selections
                    .iter()
                    .map(|sel| sel.dataset_version_id.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                inputs.insert("dataset_version_ids".to_string(), joined);
            }

            let mut outputs = HashMap::new();
            outputs.insert("dataset_content_hash".to_string(), dataset_hash_b3.clone());
            outputs.insert("examples".to_string(), examples.len().to_string());
            outputs.insert(
                "framing_policy".to_string(),
                framing_policy.as_str().to_string(),
            );
            outputs.insert("tokenizer_hash_b3".to_string(), tokenizer_hash_b3.clone());
            if let Some(ref file_hash) = dataset_file_hash_b3 {
                outputs.insert("dataset_file_hash_b3".to_string(), file_hash.clone());
            }

            pipeline
                .complete_phase(
                    PipelinePhase::DatasetBuild,
                    PhaseStatus::Completed,
                    inputs,
                    outputs,
                    serde_json::json!({
                        "source": dataset_source,
                        "dataset_id": dataset_id.clone(),
                        "dataset_ids": dataset_ids_receipt,
                        "dataset_version_selections": dataset_version_ids_for_training.clone(),
                        "dataset_version_hashes_b3": dataset_version_hashes_receipt,
                        "data_spec_hash": data_spec_hash_for_training.clone(),
                        "examples": examples.len(),
                        "dataset_hash_b3": dataset_hash_b3,
                        "dataset_file_hash_b3": dataset_file_hash_b3,
                        "framing_policy": framing_policy.as_str(),
                        "tokenizer_hash_b3": tokenizer_hash_b3.clone(),
                        "tokenizer_path": tokenizer_path.display().to_string(),
                    }),
                )
                .await?;
        } else {
            verify_phase_hash(
                &pipeline,
                PipelinePhase::DatasetBuild,
                "dataset_content_hash",
                &dataset_hash_b3,
            )?;
        }

        let mut trainer = WorkerTrainer::new(worker_cfg.clone())?;
        let correlation_id = {
            let jobs = jobs_ref.read().await;
            jobs.get(&job_id).and_then(|job| job.correlation_id.clone())
        };
        trainer.set_correlation_id(correlation_id);
        trainer.set_force_resume(orchestrator_cfg.force_resume);
        let mut preprocessed_ready = false;

        if pipeline.current_phase() == PipelinePhase::Preprocess {
            pipeline.enter_phase(PipelinePhase::Preprocess).await?;
            let mut inputs = HashMap::new();
            inputs.insert("dataset_content_hash".to_string(), dataset_hash_b3.clone());
            inputs.insert("examples".to_string(), examples.len().to_string());
            let mut outputs = HashMap::new();

            let (status, metadata) = match worker_cfg.preprocessing.as_ref() {
                Some(cfg) if cfg.enabled => {
                    let base_model_path = worker_cfg.base_model_path.as_ref().ok_or_else(|| {
                        AosError::Config("preprocessing requires base_model_path".to_string())
                    })?;
                    let config_hash = hash_preprocess_config(cfg)?;
                    inputs.insert("preprocess_config_hash".to_string(), config_hash);
                    let contract = TrainingDataContractConfig {
                        contract_version: worker_cfg.training_contract_version.clone(),
                        pad_token_id: worker_cfg.pad_token_id,
                        ignore_index: worker_cfg.ignore_index,
                    };

                    let result = preprocess_examples(
                        &examples,
                        &contract,
                        cfg,
                        worker_cfg.hidden_dim,
                        worker_cfg.vocab_size,
                        base_model_path,
                        Some(dataset_id_label.as_str()),
                        artifacts_root.as_deref(),
                        None,
                        trainer.training_seed(),
                    )
                    .inspect_err(|err| {
                        pipeline
                            .event_context()
                            .emit_phase_error(PipelinePhase::Preprocess, &err.to_string());
                    })?;
                    let adapteros_lora_worker::training::preprocessing::PreprocessResult {
                        examples: preprocessed_examples,
                        stats,
                    } = result;
                    trainer
                        .set_preprocessed_examples(preprocessed_examples)
                        .inspect_err(|err| {
                            pipeline
                                .event_context()
                                .emit_phase_error(PipelinePhase::Preprocess, &err.to_string());
                        })?;
                    preprocessed_ready = true;
                    outputs.insert("preprocess_hash".to_string(), stats.cache_key.clone());

                    let compression = cfg.compression.map(|value| value.as_str());
                    let coreml_model_path = cfg
                        .coreml_model_path
                        .as_ref()
                        .map(|path| path.display().to_string());
                    let coreml_model_id = cfg.coreml_model_id.as_deref();
                    let output_feature = cfg.output_feature.as_str();
                    let layer_key = cfg.layer_key.as_deref();
                    (
                        PhaseStatus::Completed,
                        serde_json::json!({
                            "strategy": "worker_preprocess",
                            "enabled": true,
                            "examples_in": examples.len(),
                            "examples_out": examples.len(),
                            "dataset_hash_b3": dataset_hash_b3,
                            "output_feature": output_feature,
                            "layer_key": layer_key,
                            "max_seq_len": cfg.max_seq_len,
                            "batch_size": cfg.batch_size,
                            "compression": compression,
                            "coreml_model_id": coreml_model_id,
                            "coreml_model_path": coreml_model_path,
                            "cache_dir": stats.cache_dir,
                            "cache_hit": stats.cache_hit,
                            "cached_examples": stats.cached_examples,
                            "processed_examples": stats.processed_examples,
                            "elapsed_ms": stats.elapsed_ms,
                            "preprocess_id": stats.preprocess_id,
                            "cache_key": stats.cache_key,
                            "coreml_model_hash": stats.coreml_model_hash,
                            "produced_at_unix_ms": stats.produced_at_unix_ms,
                            "seed": cfg.seed,
                            "changed": false
                        }),
                    )
                }
                Some(cfg) => {
                    let config_hash = hash_preprocess_config(cfg)?;
                    inputs.insert("preprocess_config_hash".to_string(), config_hash);
                    (
                        PhaseStatus::Skipped,
                        serde_json::json!({
                            "strategy": "noop",
                            "reason": "preprocessing_disabled",
                            "enabled": false,
                            "examples_in": examples.len(),
                            "examples_out": examples.len(),
                            "dataset_hash_b3": dataset_hash_b3,
                            "changed": false
                        }),
                    )
                }
                None => (
                    PhaseStatus::Skipped,
                    serde_json::json!({
                        "strategy": "noop",
                        "reason": "no_preprocessing_configured",
                        "enabled": false,
                        "examples_in": examples.len(),
                        "examples_out": examples.len(),
                        "dataset_hash_b3": dataset_hash_b3,
                        "changed": false
                    }),
                ),
            };
            pipeline
                .complete_phase(
                    PipelinePhase::Preprocess,
                    status,
                    inputs,
                    outputs,
                    metadata,
                )
                .await?;
        }

        if !preprocessed_ready {
            if let Some(cfg) = worker_cfg.preprocessing.as_ref().filter(|cfg| cfg.enabled) {
                let base_model_path = worker_cfg.base_model_path.as_ref().ok_or_else(|| {
                    AosError::Config("preprocessing requires base_model_path".to_string())
                })?;
                let contract = TrainingDataContractConfig {
                    contract_version: worker_cfg.training_contract_version.clone(),
                    pad_token_id: worker_cfg.pad_token_id,
                    ignore_index: worker_cfg.ignore_index,
                };
                let result = preprocess_examples(
                    &examples,
                    &contract,
                    cfg,
                    worker_cfg.hidden_dim,
                    worker_cfg.vocab_size,
                    base_model_path,
                    Some(dataset_id_label.as_str()),
                    artifacts_root.as_deref(),
                    None,
                    trainer.training_seed(),
                )
                .inspect_err(|err| {
                    pipeline
                        .event_context()
                        .emit_phase_error(PipelinePhase::Preprocess, &err.to_string());
                })?;
                trainer
                    .set_preprocessed_examples(result.examples)
                    .inspect_err(|err| {
                        pipeline
                            .event_context()
                            .emit_phase_error(PipelinePhase::Preprocess, &err.to_string());
                    })?;

                if let Some(receipt) = pipeline.receipt(PipelinePhase::Preprocess) {
                    if matches!(receipt.status, PhaseStatus::Completed) {
                        verify_phase_hash(
                            &pipeline,
                            PipelinePhase::Preprocess,
                            "preprocess_hash",
                            &result.stats.cache_key,
                        )?;
                    }
                }
            }
        }

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
        let determinism_config_json = match serde_json::to_string(&serde_json::json!({
            "seed_mode": seed_mode,
            "training_seed": trainer.training_seed(),
            "seed_override": determinism_seed_override,
            "determinism": worker_cfg.determinism.clone(),
            "dataset_version_ids": dataset_version_ids_for_training.as_ref(),
            "data_spec_hash": data_spec_hash_for_training.as_deref(),
            "base_model_id": base_model_id.as_deref(),
            "job_id": job_id.as_str(),
        })) {
            Ok(json) => Some(json),
            Err(e) => {
                warn!(
                    job_id = %job_id,
                    error = %e,
                    "Failed to serialize determinism config JSON (non-fatal)"
                );
                None
            }
        };
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
                .unwrap_or_else(|| adapteros_core::rebase_var_path("var/checkpoints").join(&job_id));

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

        // Emit structured reproducibility receipt before training begins.
        // All reproducibility-critical fields are logged in a single event for
        // easy extraction from log aggregators.
        {
            let normalized_cfg_hash =
                crate::training::config::normalized_config_hash_b3(&orchestrator_cfg);
            info!(
                job_id = %job_id,
                adapter_name = %adapter_name,
                base_model_id = ?base_model_id,
                dataset_hash_b3 = %dataset_hash_b3,
                normalized_config_hash_b3 = %normalized_cfg_hash,
                training_seed = trainer.training_seed(),
                determinism_mode = "hkdf_seeded",
                backend = ?preferred_backend.preferred,
                epochs = worker_cfg.epochs,
                rank = worker_cfg.rank,
                alpha = %worker_cfg.alpha,
                learning_rate = worker_cfg.learning_rate,
                batch_size = worker_cfg.batch_size,
                dataset_source = %dataset_source,
                tokenizer_hash_b3 = %tokenizer_hash_b3,
                examples = examples.len(),
                "TRAINING_JOB_REPRODUCIBILITY_RECEIPT"
            );
        }

        let (train_examples, validation_examples, split_summary) = split_examples_for_validation(
            &examples,
            worker_cfg.validation_split,
            trainer.training_seed(),
        );
        let mut split_inputs = HashMap::new();
        split_inputs.insert("dataset_content_hash".to_string(), dataset_hash_b3.clone());
        split_inputs.insert(
            "validation_split".to_string(),
            split_summary.split_ratio.to_string(),
        );
        split_inputs.insert(
            "training_seed".to_string(),
            trainer.training_seed().to_string(),
        );
        let mut split_outputs = HashMap::new();
        split_outputs.insert("split_hash".to_string(), split_summary.split_hash_b3.clone());
        split_outputs.insert("train_count".to_string(), split_summary.train_count.to_string());
        split_outputs.insert(
            "validation_count".to_string(),
            split_summary.validation_count.to_string(),
        );
        if pipeline.current_phase() == PipelinePhase::Split {
            pipeline.enter_phase(PipelinePhase::Split).await?;
            // Check resume compatibility
            pipeline
                .assert_resume_compatible(
                    &dataset_hash_b3,
                    &split_summary.split_hash_b3,
                    &base_model_hash,
                    &pipeline_training_config_hash,
                    &orchestrator_cfg.training_contract_version,
                    orchestrator_cfg.force_resume,
                )
                .inspect_err(|err| {
                    pipeline
                        .event_context()
                        .emit_phase_error(PipelinePhase::Split, &err.to_string());
                })?;
            pipeline
                .complete_phase(
                    PipelinePhase::Split,
                    PhaseStatus::Completed,
                    split_inputs,
                    split_outputs,
                    serde_json::json!({
                        "split_ratio": split_summary.split_ratio,
                        "total_examples": split_summary.total_examples,
                        "train_count": split_summary.train_count,
                        "validation_count": split_summary.validation_count,
                        "split_hash_b3": split_summary.split_hash_b3,
                        "training_seed": trainer.training_seed(),
                    }),
                )
                .await?;
        } else {
            verify_phase_hash(
                &pipeline,
                PipelinePhase::Split,
                "split_hash",
                &split_summary.split_hash_b3,
            )?;
        }
        drop(examples);

        let mut training_loop_executed = false;
        let mut training_result_hash: Option<String> = None;
        let mut resume_epoch: Option<u32> = None;
        let target_epochs = trainer.target_epochs();

        let result = match pipeline.current_phase() {
            PipelinePhase::TrainingLoop => {
                training_loop_executed = true;
                pipeline.enter_phase(PipelinePhase::TrainingLoop).await?;
                let resume_state = trainer.try_resume_from_checkpoint().await?;
                resume_epoch = resume_state.as_ref().map(|state| state.epoch);
                let pipeline_events = pipeline.event_context();

                // Run with per-epoch callback to update progress (with checkpoint resume support)
                let job_id_clone = job_id.clone();
                let jobs_ref_clone = jobs_ref.clone();
                let pause_token_clone = pause_token.clone();
                let scheduler_clone = scheduler.clone();
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
                        .train_with_resume_split_state(
                            &train_examples,
                            &validation_examples,
                            move |metrics: WorkerEpochMetrics| {
                            // Check pause token at epoch boundary — block until resumed.
                            // This runs synchronously inside the training loop, so blocking
                            // here pauses training without burning GPU/memory resources.
                            if pause_token_clone.load(std::sync::atomic::Ordering::SeqCst) {
                                let pt = pause_token_clone.clone();
                                let sc = scheduler_clone.clone();
                                let jid = job_id_clone.clone();
                                // Use tokio's block_in_place to run async wait without
                                // starving the runtime (we're already in a spawned task)
                                tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current().block_on(async {
                                        sc.wait_if_paused(&pt, &jid).await;
                                    });
                                });
                            }

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
                            let pipeline_events_base = pipeline_events.clone();
                            let pipeline_events_for_det = pipeline_events_base.clone();
                            let pipeline_events_for_fallback = pipeline_events_base.clone();
                            let target_epochs = target_epochs;

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
                                        if target_epochs > 0 {
                                            let progress_pct =
                                                (metrics.epoch as f32 / target_epochs as f32) * 100.0;
                                            pipeline_events_for_det.emit_phase_progress(
                                                PipelinePhase::TrainingLoop,
                                                progress_pct,
                                                Some(serde_json::json!({
                                                    "epoch": metrics.epoch,
                                                    "target_epochs": target_epochs
                                                })),
                                            );
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
                                        if target_epochs > 0 {
                                            let progress_pct =
                                                (metrics.epoch as f32 / target_epochs as f32) * 100.0;
                                            pipeline_events_for_fallback.emit_phase_progress(
                                                PipelinePhase::TrainingLoop,
                                                progress_pct,
                                                Some(serde_json::json!({
                                                    "epoch": metrics.epoch,
                                                    "target_epochs": target_epochs,
                                                    "fallback": true
                                                })),
                                            );
                                        }
                                    });
                                }
                            }
                        },
                            resume_state,
                        )
                        .await
                }
                .await;

                match result {
                    Ok(training_result) => {
                        let hash = pipeline
                            .persist_training_result(&training_result)
                            .await
                            .inspect_err(|err| {
                                pipeline
                                    .event_context()
                                    .emit_phase_error(PipelinePhase::TrainingLoop, &err.to_string());
                            })?;
                        training_result_hash = Some(hash);
                        Ok(training_result)
                    }
                    Err(e) => Err(e),
                }
            }
            PipelinePhase::ValidationEarlyStopping
            | PipelinePhase::Packaging
            | PipelinePhase::Complete => {
                let training_result = pipeline
                    .load_training_result()
                    .await
                    .inspect_err(|err| {
                        pipeline
                            .event_context()
                            .emit_phase_error(pipeline.current_phase(), &err.to_string());
                    })?
                    .ok_or_else(|| {
                        let err = AosError::Internal(format!(
                            "Missing training result; cannot resume from {}",
                            pipeline.current_phase().as_str()
                        ));
                        pipeline
                            .event_context()
                            .emit_phase_error(pipeline.current_phase(), &err.to_string());
                        err
                    })?;
                let hash = hash_training_result_for_receipt(&training_result)?;
                training_result_hash = Some(hash.clone());
                if pipeline
                    .receipt(PipelinePhase::TrainingLoop)
                    .and_then(|receipt| receipt.outputs.get("training_result_hash"))
                    .is_some()
                {
                    verify_phase_hash(
                        &pipeline,
                        PipelinePhase::TrainingLoop,
                        "training_result_hash",
                        &hash,
                    )?;
                }
                Ok(training_result)
            }
            _ => {
                let err_msg = format!(
                    "Cannot resume training: pipeline phase is {}, expected training_loop or later",
                    pipeline.current_phase().as_str()
                );
                pipeline
                    .event_context()
                    .emit_phase_error(pipeline.current_phase(), &err_msg);
                Err(AosError::Internal(err_msg))
            }
        };

        match result {
            Ok(training_result) => {
                let training_time_ms = training_result.training_time_ms();
                if training_loop_executed {
                    let training_result_hash = training_result_hash.clone().ok_or_else(|| {
                        AosError::Internal("Missing training result hash after training loop".to_string())
                    })?;
                    let mut training_inputs = HashMap::new();
                    training_inputs.insert("split_hash".to_string(), split_summary.split_hash_b3.clone());
                    training_inputs.insert("target_epochs".to_string(), target_epochs.to_string());
                    training_inputs.insert(
                        "resume_epoch".to_string(),
                        resume_epoch.unwrap_or_default().to_string(),
                    );

                    let mut training_outputs = HashMap::new();
                    training_outputs.insert(
                        "final_loss".to_string(),
                        training_result.final_loss.to_string(),
                    );
                    training_outputs.insert(
                        "stopped_at_epoch".to_string(),
                        training_result.stopped_at_epoch.unwrap_or_default().to_string(),
                    );
                    training_outputs.insert(
                        "cancelled".to_string(),
                        training_result.cancelled.to_string(),
                    );
                    training_outputs.insert(
                        "training_result_hash".to_string(),
                        training_result_hash,
                    );
                    if let Some(ref backend) = training_result.backend {
                        training_outputs.insert("backend".to_string(), backend.clone());
                    }
                    pipeline
                        .complete_phase(
                            PipelinePhase::TrainingLoop,
                            PhaseStatus::Completed,
                            training_inputs,
                            training_outputs,
                            serde_json::json!({
                                "resumed": resume_epoch.is_some(),
                                "resume_epoch": resume_epoch,
                                "target_epochs": target_epochs,
                                "stopped_at_epoch": training_result.stopped_at_epoch,
                                "final_loss": training_result.final_loss,
                                "training_time_ms": training_time_ms,
                                "examples_processed": training_result.examples_processed,
                                "tokens_processed": training_result.tokens_processed,
                                "cancelled": training_result.cancelled,
                                "backend": training_result.backend.clone(),
                                "backend_device": training_result.backend_device.clone()
                            }),
                        )
                        .await?;
                }

                if pipeline.current_phase() == PipelinePhase::ValidationEarlyStopping {
                    pipeline.enter_phase(PipelinePhase::ValidationEarlyStopping).await?;
                    if training_result.cancelled {
                        let mut inputs = HashMap::new();
                        inputs.insert("split_hash".to_string(), split_summary.split_hash_b3.clone());
                        inputs.insert("validation_enabled".to_string(), "true".to_string());
                        let mut outputs = HashMap::new();
                        outputs.insert("skipped".to_string(), "true".to_string());
                        pipeline
                            .complete_phase(
                                PipelinePhase::ValidationEarlyStopping,
                                PhaseStatus::Skipped,
                                inputs,
                                outputs,
                                serde_json::json!({
                                    "validation_enabled": !validation_examples.is_empty(),
                                    "reason": "training_cancelled",
                                }),
                            )
                            .await?;
                    } else if validation_examples.is_empty() {
                        let mut inputs = HashMap::new();
                        inputs.insert("split_hash".to_string(), split_summary.split_hash_b3.clone());
                        inputs.insert("validation_enabled".to_string(), "false".to_string());
                        let mut outputs = HashMap::new();
                        outputs.insert("skipped".to_string(), "true".to_string());
                        pipeline
                            .complete_phase(
                                PipelinePhase::ValidationEarlyStopping,
                                PhaseStatus::Skipped,
                                inputs,
                                outputs,
                                serde_json::json!({
                                    "validation_enabled": false,
                                    "reason": "validation_split_disabled"
                                }),
                            )
                            .await?;
                    } else {
                        let mut inputs = HashMap::new();
                        inputs.insert("split_hash".to_string(), split_summary.split_hash_b3.clone());
                        inputs.insert("validation_enabled".to_string(), "true".to_string());
                        let mut outputs = HashMap::new();
                        outputs.insert(
                            "final_validation_loss".to_string(),
                            training_result.final_validation_loss.unwrap_or_default().to_string(),
                        );
                        if let Some(best) = training_result.best_validation {
                            outputs.insert("best_validation_epoch".to_string(), best.1.to_string());
                            outputs.insert("best_validation_loss".to_string(), best.0.to_string());
                        }
                        pipeline
                            .complete_phase(
                                PipelinePhase::ValidationEarlyStopping,
                                PhaseStatus::Completed,
                                inputs,
                                outputs,
                                serde_json::json!({
                                    "validation_enabled": true,
                                    "validation_loss_curve_len": training_result.validation_loss_curve.len(),
                                    "validation_perplexity_curve_len": training_result.validation_perplexity_curve.len(),
                                    "final_validation_loss": training_result.final_validation_loss,
                                    "best_validation": training_result.best_validation,
                                    "early_stopping_enabled": worker_cfg.early_stopping.unwrap_or(false),
                                    "patience": worker_cfg.patience,
                                    "min_delta": worker_cfg.min_delta
                                }),
                            )
                            .await?;
                    }
                }

                // Capture backend selection and performance metrics after training
                let backend_selected = trainer.backend_info().map(|b| b.to_string());
                let perf = trainer.get_performance_metrics();
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

                    if pipeline.current_phase() == PipelinePhase::Packaging {
                        pipeline.enter_phase(PipelinePhase::Packaging).await?;
                        let mut inputs = HashMap::new();
                        inputs.insert("package_enabled".to_string(), "true".to_string());
                        let mut outputs = HashMap::new();
                        outputs.insert("skipped".to_string(), "true".to_string());
                        pipeline
                            .complete_phase(
                                PipelinePhase::Packaging,
                                PhaseStatus::Skipped,
                                inputs,
                                outputs,
                                serde_json::json!({
                                    "reason": "training_cancelled"
                                }),
                            )
                            .await?;
                    }

                    return Ok(());
                }

                let report_dataset_id = resolve_dataset_id_for_report(
                    dataset_id.as_deref(),
                    &dataset_ids_for_receipt,
                );
                let base_model_id_for_report = base_model_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let base_model_hash_for_report = resolve_base_model_hash(
                    db.as_ref(),
                    tenant_id.as_deref(),
                    base_model_id.as_deref(),
                )
                .await;
                let training_config_hash =
                    resolve_training_config_hash(&jobs_ref, &job_id, &orchestrator_cfg).await;
                let optimizer_summary = OptimizerConfigSummary {
                    optimizer_type: match worker_cfg.optimizer_config.optimizer_type {
                        OptimizerType::Sgd => "sgd",
                        OptimizerType::Adam => "adam",
                        OptimizerType::AdamW => "adamw",
                    }
                    .to_string(),
                    beta1: worker_cfg.optimizer_config.beta1,
                    beta2: worker_cfg.optimizer_config.beta2,
                    epsilon: worker_cfg.optimizer_config.epsilon,
                    weight_decay: worker_cfg.optimizer_config.weight_decay,
                    momentum: worker_cfg.optimizer_config.momentum,
                };

                let report_root =
                    artifacts_root.unwrap_or_else(|| adapteros_core::rebase_var_path("var/artifacts"));
                let pipeline_id_for_report = pipeline
                    .pipeline_id()
                    .unwrap_or_else(|| {
                        warn!(
                            job_id = %job_id,
                            "Training pipeline id missing; falling back to job id for report"
                        );
                        &job_id
                    })
                    .to_string();
                match write_training_report(
                    &report_root,
                    &pipeline_id_for_report,
                    &report_dataset_id,
                    &dataset_hash_b3,
                    &split_summary.split_hash_b3,
                    &base_model_id_for_report,
                    &base_model_hash_for_report,
                    optimizer_summary,
                    &training_config_hash,
                    orchestrator_cfg.epochs,
                    adapteros_core::time::unix_timestamp_millis(),
                    &training_result,
                ) {
                    Ok(report_path) => {
                        info!(
                            job_id = %job_id,
                            report_path = %report_path.display(),
                            "Training report generated"
                        );
                    }
                    Err(e) => {
                        warn!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to generate training report (non-fatal)"
                        );
                    }
                }

                // Emit structured completion receipt with outcome metrics.
                info!(
                    job_id = %job_id,
                    adapter_name = %adapter_name,
                    base_model_id = ?base_model_id,
                    final_loss = training_result.final_loss,
                    training_time_ms = training_time_ms,
                    backend = ?backend_selected,
                    examples_processed = examples_processed,
                    tokens_processed = tokens_processed,
                    dataset_hash_b3 = %dataset_hash_b3,
                    training_seed = trainer.training_seed(),
                    "TRAINING_JOB_COMPLETION_RECEIPT"
                );

                info!(
                    job_id = %job_id,
                    adapter_name = %adapter_name,
                    final_loss = training_result.final_loss,
                    "Training completed, packaging adapter"
                );

                if pipeline.current_phase() == PipelinePhase::Packaging {
                    pipeline.enter_phase(PipelinePhase::Packaging).await?;
                }

                // Check if packaging is disabled
                if !post_actions.package {
                    if pipeline.current_phase() == PipelinePhase::Packaging {
                        let mut inputs = HashMap::new();
                        inputs.insert("package_enabled".to_string(), "false".to_string());
                        inputs.insert("adapter_name".to_string(), adapter_name.clone());
                        let mut outputs = HashMap::new();
                        outputs.insert("skipped".to_string(), "true".to_string());
                        pipeline
                            .complete_phase(
                                PipelinePhase::Packaging,
                                PhaseStatus::Skipped,
                                inputs,
                                outputs,
                                serde_json::json!({
                                    "reason": "packaging_disabled"
                                }),
                            )
                            .await?;
                    }

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

                // Package and register adapter.
                let training_receipt_schema_version =
                    pipeline.receipt_v1().canonical_receipt_schema_version;
                let training_receipt_digest_b3 = pipeline
                    .receipt_v1()
                    .canonical_receipt_digest_b3
                    .as_deref();
                if let Err(err) = package_and_register_adapter(
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
                    Some(framing_policy.as_str()),
                    synthetic_mode,
                    data_lineage_mode,
                    base_model_id.as_deref(),
                    base_model_tenant_or_workspace_id.as_deref(),
                    category.as_deref(),
                    versioning_snapshot.as_ref(),
                    dataset_version_ids_for_training.as_ref(),
                    data_spec_hash_for_training.as_deref(),
                    Some(tokenizer_hash_b3.as_str()),
                    Some(pipeline_training_config_hash.as_str()),
                    training_receipt_schema_version,
                    training_receipt_digest_b3,
                    trainer.training_seed(),
                    db_for_packaging.as_ref(),
                )
                .await
                {
                    pipeline
                        .event_context()
                        .emit_phase_error(PipelinePhase::Packaging, &err.to_string());
                    return Err(err);
                }

                if pipeline.current_phase() == PipelinePhase::Packaging {
                    let mut inputs = HashMap::new();
                    inputs.insert("package_enabled".to_string(), "true".to_string());
                    inputs.insert("adapter_name".to_string(), adapter_name.clone());
                    inputs.insert("adapter_id".to_string(), training_result.adapter_id.clone());
                    let mut outputs = HashMap::new();
                    outputs.insert("packaged".to_string(), "true".to_string());
                    pipeline
                        .complete_phase(
                            PipelinePhase::Packaging,
                            PhaseStatus::Completed,
                            inputs,
                            outputs,
                            serde_json::json!({
                                "registered": post_actions.register,
                                "create_stack": post_actions.create_stack,
                                "activate_stack": post_actions.activate_stack,
                                "tier": post_actions.tier,
                            }),
                        )
                        .await?;
                }

                Ok(())
            }
            Err(e) => {
                let error_str = e.to_string();
                pipeline
                    .event_context()
                    .emit_phase_error(pipeline.current_phase(), &error_str);

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
                    if let Err(db_err) = database
                        .update_training_progress(
                            &job_id,
                            &TrainingProgress {
                                progress_pct: 0.0,
                                current_epoch: 0,
                                total_epochs: 0,
                                current_loss: 0.0,
                                learning_rate: 0.0,
                                tokens_per_second: 0.0,
                                error_message: Some(error_str.clone()),
                            },
                        )
                        .await
                    {
                        warn!(
                            job_id = %job_id,
                            error = %db_err,
                            "Failed to persist training failure progress payload to DB (non-fatal)"
                        );
                    }

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

                Err(e)
            }
        }
    }
    .await;

    if outcome.is_err() {
        let reason = outcome.as_ref().err().map(|e| e.to_string());
        if let Some(ref error_message) = reason {
            let mut jobs = jobs_ref_for_state.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                if matches!(
                    job.status,
                    TrainingJobStatus::Pending | TrainingJobStatus::Running
                ) {
                    job.status = TrainingJobStatus::Failed;
                    job.error_message = Some(error_message.clone());
                    if job.completed_at.is_none() {
                        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                    }
                }
            }
        }

        if let Some(database) = db_for_state.as_ref() {
            if let Some(ref error_message) = reason {
                if let Err(db_err) = database
                    .update_training_progress(
                        &job_id,
                        &TrainingProgress {
                            progress_pct: 0.0,
                            current_epoch: 0,
                            total_epochs: 0,
                            current_loss: 0.0,
                            learning_rate: 0.0,
                            tokens_per_second: 0.0,
                            error_message: Some(error_message.clone()),
                        },
                    )
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        error = %db_err,
                        "Failed to persist terminal failure progress payload to DB (non-fatal)"
                    );
                }
            }

            if let Err(e) = database.update_training_status(&job_id, "failed").await {
                warn!(
                    job_id = %job_id,
                    error = %e,
                    "Failed to persist training failure status to DB (non-fatal)"
                );
            }
        }

        if let (Some(database), Some(version_id)) = (db_for_state, version_id_for_state) {
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

fn hash_examples_for_receipt(examples: &[WorkerTrainingExample]) -> String {
    let mut hasher = Hasher::new();
    for example in examples {
        for token in &example.input_tokens {
            hasher.update(&token.to_le_bytes());
        }
        for token in &example.target_tokens {
            hasher.update(&token.to_le_bytes());
        }
        hasher.update(&example.attention_mask);
    }
    hasher.finalize().to_hex().to_string()
}

fn hash_training_result_for_receipt(
    training_result: &adapteros_lora_worker::training::trainer::TrainingResult,
) -> Result<String> {
    let bytes = serde_json::to_vec(training_result).map_err(AosError::Serialization)?;
    Ok(B3Hash::hash(&bytes).to_hex().to_string())
}

fn resolve_dataset_id_for_report(dataset_id: Option<&str>, dataset_ids: &[String]) -> String {
    if let Some(id) = dataset_id {
        return id.to_string();
    }
    let mut ids = dataset_ids.to_vec();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return "synthetic".to_string();
    }
    if ids.len() == 1 {
        return ids[0].clone();
    }
    let mut hasher = Hasher::new();
    for id in &ids {
        hasher.update(id.as_bytes());
    }
    format!("multi:{}", hasher.finalize().to_hex())
}

async fn resolve_base_model_hash(
    db: Option<&adapteros_db::Db>,
    tenant_id: Option<&str>,
    base_model_id: Option<&str>,
) -> String {
    let (Some(database), Some(tenant), Some(model_id)) = (db, tenant_id, base_model_id) else {
        return "unknown".to_string();
    };

    match database.get_model_for_tenant(tenant, model_id).await {
        Ok(Some(model)) => model.hash_b3,
        Ok(None) => {
            warn!(
                tenant_id = %tenant,
                model_id = %model_id,
                "Base model not found while generating training report"
            );
            "unknown".to_string()
        }
        Err(e) => {
            warn!(
                tenant_id = %tenant,
                model_id = %model_id,
                error = %e,
                "Failed to resolve base model hash while generating training report"
            );
            "unknown".to_string()
        }
    }
}

async fn resolve_tokenizer_info(
    base_model_path: Option<&PathBuf>,
    db: Option<&adapteros_db::Db>,
    tenant_id: Option<&str>,
    base_model_id: Option<&str>,
) -> Result<(PathBuf, String)> {
    let base_model_path = base_model_path
        .ok_or_else(|| AosError::Config("base_model_path is required for training".to_string()))?;
    let tokenizer_path = base_model_path.join("tokenizer.json");
    if !tokenizer_path.exists() {
        return Err(AosError::Validation(format!(
            "Tokenizer not found at {}",
            tokenizer_path.display()
        )));
    }
    let tokenizer_hash_b3 = B3Hash::hash_file(&tokenizer_path)
        .map_err(|e| {
            AosError::Validation(format!(
                "Failed to hash tokenizer at {}: {}",
                tokenizer_path.display(),
                e
            ))
        })?
        .to_hex();

    if let (Some(database), Some(tenant), Some(model_id)) = (db, tenant_id, base_model_id) {
        if let Ok(Some(model)) = database.get_model_for_tenant(tenant, model_id).await {
            if model.tokenizer_hash_b3 != tokenizer_hash_b3 {
                return Err(AosError::Validation(format!(
                    "Tokenizer hash mismatch for base model {}: expected {}, got {}",
                    model_id, model.tokenizer_hash_b3, tokenizer_hash_b3
                )));
            }
        }
    }

    Ok((tokenizer_path, tokenizer_hash_b3))
}

async fn resolve_training_config_hash(
    jobs_ref: &Arc<RwLock<HashMap<String, TrainingJob>>>,
    job_id: &str,
    orchestrator_cfg: &TrainingConfig,
) -> String {
    let from_job = {
        let jobs = jobs_ref.read().await;
        jobs.get(job_id).and_then(|job| job.config_hash_b3.clone())
    };
    if let Some(hash) = from_job {
        return hash;
    }

    // Use normalized config hash which covers all fields
    crate::training::config::normalized_config_hash_b3(orchestrator_cfg)
}

fn compute_pipeline_training_config_hash(worker_cfg: &WorkerTrainingConfig) -> Result<String> {
    let mut snapshot = worker_cfg.clone();
    snapshot.base_model_path = None;
    let bytes = serde_json::to_vec(&snapshot).map_err(AosError::Serialization)?;
    Ok(B3Hash::hash(&bytes).to_hex().to_string())
}

fn compute_pipeline_base_model_hash(base_model_path: Option<&PathBuf>) -> Result<String> {
    let model_path = base_model_path
        .ok_or_else(|| AosError::Config("base_model_path is required for training".to_string()))?;
    let config_path = model_path.join("config.json");
    let hash = if config_path.exists() {
        B3Hash::hash_file(&config_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to hash base model config {}: {}",
                config_path.display(),
                e
            ))
        })?
    } else {
        B3Hash::hash(model_path.to_string_lossy().as_bytes())
    };
    Ok(hash.to_hex().to_string())
}

async fn resolve_base_model_path(
    base_model_path: Option<PathBuf>,
    db: Option<&adapteros_db::Db>,
    tenant_id: Option<&str>,
    base_model_id: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = base_model_path {
        return Some(path);
    }

    let (Some(database), Some(tenant), Some(model_id)) = (db, tenant_id, base_model_id) else {
        return None;
    };

    match database.get_model_for_tenant(tenant, model_id).await {
        Ok(Some(model)) => model.model_path.map(PathBuf::from),
        Ok(None) => {
            warn!(
                tenant_id = %tenant,
                model_id = %model_id,
                "Base model not found while resolving model path"
            );
            None
        }
        Err(e) => {
            warn!(
                tenant_id = %tenant,
                model_id = %model_id,
                error = %e,
                "Failed to resolve base model while resolving model path"
            );
            None
        }
    }
}

fn hash_preprocess_config(config: &WorkerPreprocessingConfig) -> Result<String> {
    let bytes = serde_json::to_vec(config).map_err(AosError::Serialization)?;
    Ok(B3Hash::hash(&bytes).to_hex().to_string())
}

fn verify_phase_hash(
    pipeline: &TrainingPipeline,
    phase: PipelinePhase,
    key: &str,
    expected: &str,
) -> Result<()> {
    let receipt = pipeline.receipt(phase).ok_or_else(|| {
        AosError::Internal(format!("Missing pipeline receipt for {}", phase.as_str()))
    })?;
    let actual = receipt
        .outputs
        .get(key)
        .map(|value| value.as_str())
        .or_else(|| receipt.metadata.get(key).and_then(|value| value.as_str()))
        .ok_or_else(|| {
            AosError::Internal(format!(
                "Missing {} in pipeline receipt for {}",
                key,
                phase.as_str()
            ))
        })?;
    if actual != expected {
        return Err(AosError::Internal(format!(
            "Pipeline receipt mismatch for {} ({}): expected {}, got {}",
            phase.as_str(),
            key,
            expected,
            actual
        )));
    }
    Ok(())
}

fn map_preprocess_compression(
    compression: ApiPreprocessCompression,
) -> WorkerPreprocessCompression {
    match compression {
        ApiPreprocessCompression::None => WorkerPreprocessCompression::None,
        ApiPreprocessCompression::Q15 => WorkerPreprocessCompression::Q15,
    }
}

fn map_preprocessing_config(config: ApiPreprocessingConfig) -> WorkerPreprocessingConfig {
    use adapteros_lora_worker::training::PreprocessOutputFeature as WorkerOutputFeature;
    let output_feature = match config.output_feature {
        adapteros_types::training::PreprocessOutputFeature::Embedding => {
            WorkerOutputFeature::Embedding
        }
        adapteros_types::training::PreprocessOutputFeature::HiddenStateLast => {
            WorkerOutputFeature::HiddenStateLast
        }
        adapteros_types::training::PreprocessOutputFeature::Pooled => WorkerOutputFeature::Pooled,
    };
    WorkerPreprocessingConfig {
        enabled: config.enabled,
        coreml_model_id: config.coreml_model_id,
        coreml_model_path: config.coreml_model_path,
        output_feature,
        layer_key: config.layer_key,
        max_seq_len: config.max_seq_len,
        batch_size: config.batch_size,
        compression: config.compression.map(map_preprocess_compression),
        cache_dir: config.cache_dir,
        seed: config.seed,
    }
}

fn map_preprocessing_config_opt(
    config: Option<ApiPreprocessingConfig>,
) -> Option<WorkerPreprocessingConfig> {
    config.map(map_preprocessing_config)
}
