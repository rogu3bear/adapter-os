//! TrainingService - manages training jobs, templates, and job lifecycle.
//!
//! # Algorithm Version Compatibility
//!
//! When training with existing datasets, algorithm versions should be checked
//! to ensure deterministic replay is possible. Use
//! [`adapteros_core::AlgorithmVersionBundle::check_runtime_compatibility`]
//! to validate dataset hash inputs before training.
//!
//! TODO: Integrate version compatibility checking when dataset_version_ids
//! are provided. Query `dataset_hash_inputs` table for each version,
//! construct `AlgorithmVersionBundle`, and call `check_runtime_compatibility()`.
//! Log warnings for minor mismatches, fail for breaking mismatches.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::spawn_deterministic;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::training::coreml::run_coreml_export_flow;
use crate::training::execution::run_training_job;
use crate::training::job::{
    DataLineageMode, DatasetVersionSelection, DatasetVersionTrustSnapshot, TrainingBackendKind,
    TrainingBackendPolicy, TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate,
};
use crate::training::versioning::{
    canonical_trust_state, compute_combined_data_spec_hash, TrainingVersioningContext,
};

/// Report from orphaned job recovery at startup
#[derive(Debug, Clone)]
pub struct OrphanedJobRecoveryReport {
    /// Number of jobs successfully recovered
    pub recovered_count: usize,
    /// IDs of recovered jobs
    pub recovered_job_ids: Vec<String>,
}

/// Training service for managing jobs
pub struct TrainingService {
    jobs: Arc<RwLock<HashMap<String, TrainingJob>>>,
    templates: Arc<RwLock<HashMap<String, TrainingTemplate>>>,
    /// Database connection for dataset loading
    db: Option<adapteros_db::Db>,
    /// Storage root for dataset files
    storage_root: Option<PathBuf>,
    /// Artifacts root for training reports and outputs
    artifacts_root: Option<PathBuf>,
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
                    preferred_backend: None,
                    require_gpu: false,
                    max_gpu_memory_mb: None,
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
                    preferred_backend: None,
                    require_gpu: false,
                    max_gpu_memory_mb: None,
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
                    preferred_backend: None,
                    require_gpu: false,
                    max_gpu_memory_mb: None,
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
                    preferred_backend: None,
                    require_gpu: false,
                    max_gpu_memory_mb: None,
                    ..Default::default()
                },
            },
        );

        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(templates)),
            db: None,
            storage_root: None,
            artifacts_root: None,
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Accessor for the configured training storage root (dataset files).
    pub fn storage_root(&self) -> Option<PathBuf> {
        self.storage_root.clone()
    }

    /// Accessor for the configured training artifacts root.
    pub fn artifacts_root(&self) -> Option<PathBuf> {
        self.artifacts_root.clone()
    }

    #[cfg(test)]
    pub async fn insert_job_for_test(&self, job: TrainingJob) {
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id.clone(), job);
    }

    /// Recover orphaned training jobs at startup
    ///
    /// Finds jobs that were marked as "running" but haven't had any activity
    /// within the staleness threshold. These jobs are transitioned to "interrupted"
    /// status, which allows them to be retried via the existing retry mechanism.
    ///
    /// This should be called during service initialization to clean up jobs
    /// that were left in an inconsistent state due to crashes or restarts.
    ///
    /// # Arguments
    /// * `staleness_threshold` - Duration after which a running job without activity is considered orphaned
    ///
    /// # Returns
    /// * `RecoveryReport` with count of recovered jobs
    pub async fn recover_orphaned_jobs(
        &self,
        staleness_threshold: std::time::Duration,
    ) -> Result<OrphanedJobRecoveryReport> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| AosError::config("Database not configured for training service"))?;

        let orphaned: Vec<adapteros_db::training_jobs::TrainingJobRecord> =
            db.find_orphaned_training_jobs(staleness_threshold).await?;

        if orphaned.is_empty() {
            info!("No orphaned training jobs found during startup recovery");
            return Ok(OrphanedJobRecoveryReport {
                recovered_count: 0,
                recovered_job_ids: vec![],
            });
        }

        info!(
            count = orphaned.len(),
            "Found orphaned training jobs, initiating recovery"
        );

        let mut recovered_ids = Vec::with_capacity(orphaned.len());

        for job in &orphaned {
            if let Err(e) = db
                .mark_training_job_interrupted(&job.id, "orphaned_recovery_at_startup")
                .await
            {
                error!(
                    job_id = %job.id,
                    error = %e,
                    "Failed to mark orphaned job as interrupted"
                );
                continue;
            }

            info!(
                job_id = %job.id,
                started_at = %job.started_at,
                adapter_name = ?job.adapter_name,
                "Recovered orphaned training job"
            );

            recovered_ids.push(job.id.clone());
        }

        Ok(OrphanedJobRecoveryReport {
            recovered_count: recovered_ids.len(),
            recovered_job_ids: recovered_ids,
        })
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

    /// Set artifacts root
    pub fn set_artifacts_root(&mut self, path: PathBuf) {
        self.artifacts_root = Some(path);
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
            .ok_or_else(|| AosError::NotFound(format!("Training job not found: {}", job_id)))
    }

    /// Start a new training job
    #[allow(clippy::too_many_arguments)]
    pub async fn start_training(
        &self,
        adapter_name: String,
        config: TrainingConfig,
        template_id: Option<String>,
        repo_id: Option<String>,
        target_branch: Option<String>,
        base_version_id: Option<String>,
        dataset_id: Option<String>,
        dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
        synthetic_mode: bool,
        data_lineage_mode: DataLineageMode,
        tenant_id: Option<String>,
        initiated_by: Option<String>,
        initiated_by_role: Option<String>,
        base_model_id: Option<String>,
        collection_id: Option<String>,
        scope: Option<String>,
        lora_tier: Option<crate::training::job::LoraTier>,
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
        // Versioning context (adapter_versions table)
        versioning: Option<TrainingVersioningContext>,
        // Source control + data provenance
        code_commit_sha: Option<String>,
        data_spec_json: Option<String>,
        data_spec_hash: Option<String>,
    ) -> Result<TrainingJob> {
        let job_id = format!("train-{}", uuid::Uuid::new_v4());
        let scope_value = scope.clone().unwrap_or_else(|| "tenant".to_string());

        // Preserve caller intent and record deterministic CoreML fallback for auditability
        let mut config = config;
        if let Some(policy) = config.backend_policy {
            match policy {
                TrainingBackendPolicy::CoremlOnly => {
                    config.preferred_backend = Some(TrainingBackendKind::CoreML);
                    config.coreml_training_fallback = None;
                    config.require_gpu = true;
                }
                TrainingBackendPolicy::CoremlElseFallback => {
                    config.preferred_backend = Some(TrainingBackendKind::CoreML);
                    if config.coreml_training_fallback.is_none() {
                        config.coreml_training_fallback = Some(TrainingBackendKind::Mlx);
                    }
                }
                TrainingBackendPolicy::Auto => {}
            }
        }
        if config.preferred_backend == Some(TrainingBackendKind::CoreML)
            && config.coreml_training_fallback.is_none()
        {
            config.coreml_training_fallback = Some(TrainingBackendKind::Mlx);
        }
        let export_opt_in = config.enable_coreml_export.unwrap_or(false);
        config.enable_coreml_export = Some(export_opt_in);
        let mut data_spec_hash = data_spec_hash;

        let dataset_versions_empty = dataset_version_ids
            .as_ref()
            .map(|v| v.is_empty())
            .unwrap_or(true);
        if synthetic_mode && !dataset_versions_empty {
            return Err(AosError::Validation(
                "synthetic_mode=true requires dataset_version_ids to be empty".to_string(),
            ));
        }
        if !synthetic_mode && dataset_versions_empty {
            return Err(AosError::Validation(
                "dataset_version_ids are required for non-synthetic training jobs".to_string(),
            ));
        }

        let mut combined_inputs: Vec<(String, String, f32)> = Vec::new();
        if let (Some(ref db), Some(versions)) = (&self.db, dataset_version_ids.as_ref()) {
            for sel in versions.iter() {
                let ds_version = db
                    .get_training_dataset_version(&sel.dataset_version_id)
                    .await
                    .map_err(|e| AosError::Database(e.to_string()))?
                    .ok_or_else(|| {
                        AosError::Validation(format!(
                            "dataset version {} not found",
                            sel.dataset_version_id
                        ))
                    })?;

                let trust_state = canonical_trust_state(&ds_version.trust_state);
                if trust_state == "blocked" {
                    return Err(AosError::Validation(format!(
                        "dataset version {} trust_state={} blocks training",
                        sel.dataset_version_id, trust_state
                    )));
                }
                if trust_state == "needs_approval" || trust_state == "unknown" {
                    return Err(AosError::Validation(format!(
                        "dataset version {} trust_state={} blocks training",
                        sel.dataset_version_id, trust_state
                    )));
                }

                // Verify dataset integrity before training
                {
                    let dataset_id_val = &ds_version.dataset_id;
                    let integrity_result = db
                        .verify_dataset_integrity(dataset_id_val)
                        .await
                        .map_err(|e| {
                            AosError::Database(format!("Dataset integrity check failed: {}", e))
                        })?;

                    if !integrity_result.is_valid {
                        let mismatch_summary: Vec<String> = integrity_result
                            .mismatches
                            .iter()
                            .take(5)
                            .map(|m| {
                                format!(
                                    "{} (expected: {}, actual: {})",
                                    m.file_name, m.expected_hash, m.actual_hash
                                )
                            })
                            .collect();

                        return Err(AosError::Validation(format!(
                            "Dataset {} failed integrity check: {}/{} files corrupted. Mismatches: {}",
                            dataset_id_val,
                            integrity_result.mismatches.len(),
                            integrity_result.total_files,
                            mismatch_summary.join(", ")
                        )));
                    }

                    info!(
                        dataset_id = %dataset_id_val,
                        verified_files = integrity_result.verified_files,
                        total_files = integrity_result.total_files,
                        "Dataset integrity verified before training"
                    );
                }

                let weight = if sel.weight <= 0.0 { 1.0 } else { sel.weight };
                combined_inputs.push((
                    sel.dataset_version_id.clone(),
                    ds_version.hash_b3.clone(),
                    weight,
                ));
            }
        }
        if !combined_inputs.is_empty() {
            let combined_hash = if combined_inputs.len() == 1 && data_spec_hash.is_none() {
                combined_inputs[0].1.clone()
            } else {
                compute_combined_data_spec_hash(&combined_inputs)
            };
            if let Some(ref provided) = data_spec_hash {
                if provided != &combined_hash {
                    return Err(AosError::Validation("DATA_SPEC_HASH_MISMATCH".to_string()));
                }
            }
            data_spec_hash = Some(combined_hash);
        }

        // Compute config hash for reproducibility tracking
        let config_params = adapteros_db::training_jobs::TrainingConfigParams {
            rank: config.rank as usize,
            alpha: config.alpha as f32,
            learning_rate: config.learning_rate,
            batch_size: config.batch_size as usize,
            epochs: config.epochs as usize,
            hidden_dim: 768,
        };
        let config_hash = adapteros_db::training_jobs::compute_config_hash(&config_params).ok();

        // Get build ID from environment or use default
        let build_id = code_commit_sha
            .clone()
            .or_else(|| std::env::var("BUILD_ID").ok())
            .or_else(|| std::env::var("GIT_COMMIT").ok())
            .or_else(|| Some("dev".to_string()));

        let correlation_id = if let Some(db) = self.db.as_ref() {
            if let Some(dataset_id) = dataset_id.as_deref() {
                db.get_dataset_correlation_id(dataset_id)
                    .await
                    .ok()
                    .flatten()
            } else if let Some(sel) = dataset_version_ids
                .as_ref()
                .and_then(|versions| versions.first())
            {
                db.get_dataset_correlation_id_from_version(&sel.dataset_version_id)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            }
        } else {
            None
        }
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let mut job = TrainingJob::new(job_id.clone(), adapter_name.clone(), config.clone());
        job.template_id = template_id;
        job.repo_id = repo_id.clone();
        job.target_branch = target_branch.clone();
        job.base_version_id = base_version_id.clone();
        job.correlation_id = Some(correlation_id.clone());
        if let Some(ref ver) = versioning {
            job.repo_name = Some(ver.repo_name.clone());
            job.target_branch = Some(ver.branch.clone());
            job.base_version_id = ver.parent_version_id.clone();
            job.adapter_version_id = Some(ver.adapter_version_id.clone());
            job.version_label = Some(ver.version_label.clone());
            job.draft_version_id = ver.draft_version_id.clone();
            job.code_commit_sha = ver.code_commit_sha.clone();
            job.data_spec_json = ver.data_spec_json.clone();
            job.data_spec_hash = ver.data_spec_hash.clone();
        }
        if job.code_commit_sha.is_none() {
            job.code_commit_sha = code_commit_sha.clone();
        }
        if job.data_spec_json.is_none() {
            job.data_spec_json = data_spec_json.clone();
        }
        if job.data_spec_hash.is_none() {
            job.data_spec_hash = data_spec_hash.clone().or_else(|| {
                job.data_spec_json
                    .as_ref()
                    .map(|spec| blake3::hash(spec.as_bytes()).to_hex().to_string())
            });
        }
        job.dataset_id = dataset_id.clone();
        job.dataset_version_ids = dataset_version_ids.clone();
        if let (Some(ref db), Some(versions)) = (&self.db, dataset_version_ids.as_ref()) {
            let mut trust_snapshots = Vec::new();
            for sel in versions.iter() {
                let trust_state = db
                    .get_effective_trust_state(&sel.dataset_version_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|t| canonical_trust_state(&t));
                trust_snapshots.push(DatasetVersionTrustSnapshot {
                    dataset_version_id: sel.dataset_version_id.clone(),
                    trust_at_training_time: trust_state,
                });
            }
            if !trust_snapshots.is_empty() {
                job.dataset_version_trust = Some(trust_snapshots);
            }
        }
        job.synthetic_mode = synthetic_mode;
        job.data_lineage_mode = Some(data_lineage_mode);
        job.tenant_id = tenant_id.clone();
        job.initiated_by = initiated_by.clone();
        job.initiated_by_role = initiated_by_role;
        job.base_model_id = base_model_id.clone();
        job.collection_id = collection_id.clone();
        job.build_id = build_id.clone();
        job.config_hash_b3 = config_hash.clone();
        job.requested_backend = config.preferred_backend.map(|b| b.as_str().to_string());
        job.backend_policy = config.backend_policy.map(|p| p.as_str().to_string());
        job.coreml_training_fallback = config
            .coreml_training_fallback
            .map(|b| b.as_str().to_string());
        job.coreml_export_requested = Some(export_opt_in);
        job.category = category.clone();
        job.description = description;
        job.language = language;
        job.framework_id = framework_id;
        job.framework_version = framework_version;
        job.lora_tier = lora_tier;
        job.scope = Some(scope_value.clone());
        job.post_actions_json = post_actions_json.clone();
        job.retry_of_job_id = retry_of_job_id.clone();

        // Persist job to database for durability and retry chain tracking
        if let Some(ref db) = self.db {
            let config_json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());

            let db_repo_id = repo_id.as_deref().unwrap_or("direct-training");
            let created_by = initiated_by.as_deref().unwrap_or("system");

            // Extract first dataset_version_id for provenance tracking
            let dataset_version_id = job
                .dataset_version_ids
                .as_ref()
                .and_then(|ids| ids.first())
                .map(|sel| sel.dataset_version_id.clone());

            match db
                .create_training_job_with_provenance(
                    Some(&job_id),
                    db_repo_id,
                    &config_json,
                    created_by,
                    dataset_id.as_deref(),
                    Some(correlation_id.as_str()),
                    dataset_version_id.as_deref(),
                    job.dataset_version_ids.as_deref(),
                    base_model_id.as_deref(),
                    collection_id.as_deref(),
                    tenant_id.as_deref(),
                    build_id.as_deref(),
                    None,
                    retry_of_job_id.as_deref(),
                    job.target_branch.as_deref(),
                    job.base_version_id.as_deref(),
                    job.draft_version_id.as_deref(),
                    job.code_commit_sha.as_deref(),
                    job.data_spec_json.as_deref(),
                    synthetic_mode,
                    job.data_lineage_mode.as_ref().map(|m| m.as_str()),
                )
                .await
            {
                Ok(_) => {
                    info!(
                        job_id = %job_id,
                        retry_of = ?retry_of_job_id,
                        "Training job persisted to database"
                    );

                    if let Err(e) = db
                        .update_training_job_adapter_name(&job_id, &adapter_name)
                        .await
                    {
                        warn!(job_id = %job_id, error = %e, "Failed to update adapter name in DB (non-fatal)");
                    }

                    if let Some(ref hash) = config_hash {
                        if let Err(e) = db.update_training_job_config_hash(&job_id, hash).await {
                            warn!(job_id = %job_id, error = %e, "Failed to update config hash in DB (non-fatal)");
                        }
                    }
                }
                Err(e) => {
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

        // Spawn deterministic training task
        let jobs_ref = self.jobs.clone();
        let cancel_tokens_ref = self.cancel_tokens.clone();
        let cfg_for_run = job.config.clone();
        let job_id_for_run = job.id.clone();
        let adapter_name_for_run = job.adapter_name.clone();
        let dataset_id_for_run = job.dataset_id.clone();
        let tenant_id_for_run = tenant_id;
        let db_for_run = self.db.clone();
        let storage_for_run = self.storage_root.clone();
        let artifacts_for_run = self.artifacts_root.clone();
        let category_for_run = category;
        let post_actions_for_run = post_actions_json;
        let base_model_id_for_run = job.base_model_id.clone();
        let base_model_id_for_det = base_model_id_for_run.clone();
        let jobs_ref_det = jobs_ref.clone();
        let job_id_det = job_id_for_run.clone();
        let adapter_name_det = adapter_name_for_run.clone();
        let cfg_for_det = cfg_for_run.clone();
        let dataset_id_for_det = dataset_id_for_run.clone();
        let tenant_id_for_det = tenant_id_for_run.clone();
        let db_for_det = db_for_run.clone();
        let storage_for_det = storage_for_run.clone();
        let artifacts_for_det = artifacts_for_run.clone();
        let category_for_det = category_for_run.clone();
        let post_actions_for_det = post_actions_for_run.clone();
        let dataset_id_for_fallback = dataset_id_for_run.clone();
        let tenant_id_for_fallback = tenant_id_for_run.clone();
        let db_for_fallback = db_for_run.clone();
        let storage_for_fallback = storage_for_run.clone();
        let artifacts_for_fallback = artifacts_for_run.clone();
        let category_for_fallback = category_for_run.clone();
        let post_actions_for_fallback = post_actions_for_run.clone();
        let base_model_id_for_fallback = base_model_id_for_run.clone();
        let jobs_ref_fallback = jobs_ref.clone();
        let job_id_for_fallback = job_id_for_run.clone();
        let adapter_name_for_fallback = adapter_name_for_run.clone();
        let cfg_for_fallback = cfg_for_run.clone();
        let synthetic_mode_for_run = synthetic_mode;
        let data_lineage_mode_for_run = data_lineage_mode;
        let cancel_token_for_run = cancel_token.clone();
        let cancel_tokens_for_det = cancel_tokens_ref.clone();
        if let Err(e) =
            spawn_deterministic(format!("training-job:{}", job_id_for_run), async move {
                let result = run_training_job(
                    jobs_ref_det.clone(),
                    job_id_det.clone(),
                    adapter_name_det,
                    cfg_for_det,
                    dataset_id_for_det,
                    synthetic_mode_for_run,
                    data_lineage_mode_for_run,
                    tenant_id_for_det,
                    db_for_det,
                    storage_for_det,
                    artifacts_for_det,
                    category_for_det,
                    post_actions_for_det,
                    base_model_id_for_det,
                    cancel_token_for_run,
                )
                .await;

                {
                    let mut tokens = cancel_tokens_for_det.write().await;
                    tokens.remove(&job_id_for_run);
                }

                if let Err(err) = result {
                    tracing::error!("Training job {} failed: {}", job_id_for_run, err);
                }
            })
        {
            // Allow explicit non-deterministic fallback for tests/sandboxes
            if cfg!(test) || std::env::var("AOS_ALLOW_NONDET_TRAINING").is_ok() {
                tracing::warn!(
                    "Deterministic executor unavailable, falling back to tokio::spawn for job {}",
                    job_id
                );
                let cancel_tokens_for_fallback = cancel_tokens_ref.clone();
                let cancel_token_for_fallback = cancel_token.clone();
                tokio::spawn(async move {
                    let result = run_training_job(
                        jobs_ref_fallback.clone(),
                        job_id_for_fallback.clone(),
                        adapter_name_for_fallback.clone(),
                        cfg_for_fallback.clone(),
                        dataset_id_for_fallback,
                        synthetic_mode_for_run,
                        data_lineage_mode_for_run,
                        tenant_id_for_fallback,
                        db_for_fallback,
                        storage_for_fallback,
                        artifacts_for_fallback,
                        category_for_fallback,
                        post_actions_for_fallback,
                        base_model_id_for_fallback,
                        cancel_token_for_fallback,
                    )
                    .await;
                    let mut tokens = cancel_tokens_for_fallback.write().await;
                    tokens.remove(&job_id_for_fallback);
                    if let Err(err) = result {
                        tracing::error!(
                            "Training job {} failed (nondet fallback): {}",
                            job_id_for_fallback,
                            err
                        );
                    }
                });
            } else {
                tracing::error!("Failed to spawn deterministic training task: {}", e);
                return Err(adapteros_core::AosError::DeterminismViolation(format!(
                    "Training job {} requires deterministic executor: {}",
                    job_id, e
                )));
            }
        }

        tracing::info!("Training job created: {}", job_id);

        Ok(job)
    }

    /// Cancel a training job
    ///
    /// Sets the in-process cancel token (if the job is running in this orchestrator),
    /// then optionally sends a cancellation request to the worker via UDS.
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
                    )));
                }
            } else {
                return Err(AosError::Internal(format!(
                    "Training job not found: {}",
                    job_id
                )));
            }
        }

        // Set the cancel token directly
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

        // Also send cancel to worker via UDS with 5s timeout
        let worker_confirmed = if let Some(client) = uds_client {
            let socket_buf = if let Some(socket) = socket_path {
                info!(
                    job_id = %job_id,
                    socket_path = socket,
                    "Using provided worker socket for cancel"
                );
                std::path::PathBuf::from(socket)
            } else {
                let resolved = adapteros_config::resolve_worker_socket_for_cp()?;
                info!(
                    job_id = %job_id,
                    socket_path = %resolved.path.display(),
                    socket_source = %resolved.source,
                    "Resolved worker socket for cancel"
                );
                resolved.path
            };
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                client.cancel_training_job(socket_buf.as_path(), job_id, None),
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
            false
        };

        let cancellation_initiated = token_set || worker_confirmed;

        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            if cancellation_initiated {
                job.cancel();
                info!(job_id = %job_id, token_set = token_set, worker_confirmed = worker_confirmed, "Training job cancellation initiated");
            } else {
                job.cancel();
                warn!(job_id = %job_id, "Training job cancel requested but no confirmation - marking cancelled via token");
            }

            if let Some(ref database) = self.db {
                if let Err(e) = database.update_training_status(job_id, "cancelled").await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist training cancellation status to DB (non-fatal)");
                }
            }

            Ok(())
        } else {
            Err(AosError::Internal(format!("Training job not found: {}", job_id)))
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

            if let Some(ref database) = self.db {
                let progress = adapteros_db::training_jobs::TrainingProgress {
                    progress_pct: job.progress_pct,
                    current_epoch: job.current_epoch,
                    total_epochs: job.total_epochs,
                    current_loss: job.current_loss,
                    learning_rate: job.config.learning_rate,
                    tokens_per_second: job.tokens_per_second,
                    error_message: job.error_message.clone(),
                };
                if let Err(e) = database.update_training_progress(job_id, &progress).await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist training progress to DB (non-fatal)");
                }
            }

            Ok(())
        } else {
            Err(AosError::NotFound(format!(
                "Training job not found: {}",
                job_id
            )))
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

            if let Some(ref database) = self.db {
                if let Err(e) = database.update_training_status(job_id, "completed").await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
                }
            }

            Ok(())
        } else {
            Err(AosError::NotFound(format!(
                "Training job not found: {}",
                job_id
            )))
        }
    }

    /// Trigger a CoreML export for a completed training job.
    pub async fn export_coreml_for_job(&self, job_id: &str) -> Result<TrainingJob> {
        let snapshot = self.get_job(job_id).await?;
        if snapshot.status != TrainingJobStatus::Completed {
            return Err(AosError::Validation(
                "CoreML export requires a completed training job".to_string(),
            ));
        }

        let adapter_id = snapshot.adapter_id.clone().ok_or_else(|| {
            AosError::Validation("Adapter ID missing; cannot export CoreML package".to_string())
        })?;
        let aos_path = snapshot
            .aos_path
            .clone()
            .or(snapshot.artifact_path.clone())
            .ok_or_else(|| {
                AosError::Validation(
                    "Adapter artifact path missing; cannot export CoreML".to_string(),
                )
            })?;
        let base_model_id = snapshot
            .manifest_base_model
            .clone()
            .or(snapshot.base_model_id.clone())
            .ok_or_else(|| {
                AosError::Validation("Base model id missing for CoreML export".to_string())
            })?;
        let weights_hash = snapshot
            .package_hash_b3
            .clone()
            .or(snapshot.weights_hash_b3.clone())
            .ok_or_else(|| {
                AosError::Validation("Weights hash missing; cannot export CoreML".to_string())
            })?;

        let adapters_root = adapteros_core::paths::AdapterPaths::from_config(None)
            .root()
            .to_path_buf();
        let tenant = snapshot
            .tenant_id
            .clone()
            .unwrap_or_else(|| "default".to_string());

        run_coreml_export_flow(
            self.jobs.clone(),
            job_id,
            &adapter_id,
            Path::new(&aos_path),
            &base_model_id,
            &weights_hash,
            &adapters_root,
            Some(tenant.as_str()),
            self.db.as_ref(),
        )
        .await?;

        self.get_job(job_id).await
    }

    /// Mark job as failed
    pub async fn fail_job(&self, job_id: &str, err: String) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrainingJobStatus::Failed;
            job.error_message = Some(err.clone());
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            let adapter_id = job.adapter_id.clone();
            error!("Training job failed: {}", job_id);

            if let Some(ref database) = self.db {
                if let Err(e) = database.update_training_status(job_id, "failed").await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist training failure status to DB (non-fatal)");
                }

                if let Some(adapter_id) = adapter_id {
                    if let Err(e) = database
                        .transition_adapter_lifecycle(
                            &adapter_id,
                            "failed",
                            "training_failed",
                            "system",
                        )
                        .await
                    {
                        warn!(
                            job_id = %job_id,
                            adapter_id = %adapter_id,
                            error = %e,
                            "Failed to mark adapter failed after training error"
                        );
                    }
                }
            }

            Ok(())
        } else {
            Err(AosError::Internal(format!("Training job not found: {}", job_id)))
        }
    }

    /// Get training logs
    pub async fn get_logs(&self, job_id: &str) -> Result<Vec<String>> {
        let job = self.get_job(job_id).await?;

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
        templates
            .get(template_id)
            .cloned()
            .ok_or_else(|| AosError::NotFound(format!("Template not found: {}", template_id)))
    }
}

impl Default for TrainingService {
    fn default() -> Self {
        Self::new()
    }
}
