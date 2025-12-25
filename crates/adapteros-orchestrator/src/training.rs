//! # Training Job Orchestration and Management
//!
//! Handles scheduling, executing, and monitoring adapter training jobs.
//! Integrates with MLX backend for actual training operations.
//!
//! ## State Management Architecture (Triple State)
//!
//! Training jobs maintain state in **three separate locations**, which can
//! diverge under failure conditions. Understanding this is critical for
//! debugging and ensuring consistency.
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────────────────────┐
//! │                         TRIPLE STATE MANAGEMENT                               │
//! │                                                                               │
//! │  ┌─────────────────────────────────────────────────────────────────────────┐  │
//! │  │                    1. IN-MEMORY STATE (jobs HashMap)                    │  │
//! │  │                                                                         │  │
//! │  │  - Arc<RwLock<HashMap<String, TrainingJob>>>                            │  │
//! │  │  - Authoritative for progress_pct, current_epoch, status               │  │
//! │  │  - Lost on process restart                                             │  │
//! │  │  - Updated in real-time during training                                │  │
//! │  └─────────────────────────────────────────────────────────────────────────┘  │
//! │                                   │                                           │
//! │                                   │ persist (non-blocking)                    │
//! │                                   ▼                                           │
//! │  ┌─────────────────────────────────────────────────────────────────────────┐  │
//! │  │                    2. DATABASE STATE (SQLite)                           │  │
//! │  │                                                                         │  │
//! │  │  - training_jobs table                                                  │  │
//! │  │  - Updated periodically (every epoch or status change)                  │  │
//! │  │  - **DB writes are non-fatal**: failures logged but don't stop job     │  │
//! │  │  - May lag behind in-memory state                                       │  │
//! │  └─────────────────────────────────────────────────────────────────────────┘  │
//! │                                                                               │
//! │  ┌─────────────────────────────────────────────────────────────────────────┐  │
//! │  │                    3. CANCEL TOKENS (AtomicBool)                        │  │
//! │  │                                                                         │  │
//! │  │  - Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>                        │  │
//! │  │  - Cooperative cancellation (checked at epoch boundaries)               │  │
//! │  │  - Token removed after job completes (success or failure)               │  │
//! │  │  - No persistence - cancel requests lost on restart                     │  │
//! │  └─────────────────────────────────────────────────────────────────────────┘  │
//! └───────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Race Condition Scenarios
//!
//! | Scenario | Symptom | Cause | Mitigation |
//! |----------|---------|-------|------------|
//! | Process crash during training | DB shows "running" but no progress | In-memory state lost | Implement startup recovery scan |
//! | DB write failure | DB shows old progress_pct | Non-fatal write logged | Retry logic, monitoring |
//! | Cancel during epoch | Job completes current epoch | Token only checked at boundaries | Document expected behavior |
//! | Concurrent status updates | Inconsistent reads | RwLock allows concurrent reads | Use single-writer pattern |
//!
//! ## Job Lifecycle
//!
//! ```text
//!   ┌────────────┐     create_job()     ┌──────────────┐
//!   │   (none)   │ ─────────────────────▶│   pending    │
//!   └────────────┘                       └──────────────┘
//!                                               │
//!                                               │ run_training_job()
//!                                               ▼
//!   ┌────────────┐     cancel_job()     ┌──────────────┐
//!   │ cancelled  │ ◀─────────────────────│   running    │
//!   └────────────┘                       └──────────────┘
//!                                               │
//!                           ┌─────────────────┬┴───────────────┐
//!                           │ success         │ failure        │
//!                           ▼                 ▼                │
//!                    ┌──────────────┐  ┌──────────────┐        │
//!                    │  completed   │  │    failed    │        │
//!                    └──────────────┘  └──────────────┘        │
//! ```
//!
//! ## Critical Functions
//!
//! - [`TrainingService::create_job`]: Creates job in pending state
//! - [`TrainingService::run_training_job`]: Spawns deterministic task, manages tokens
//! - [`TrainingService::cancel_job`]: Sets cancel token (cooperative cancellation)
//! - [`TrainingService::update_job_progress`]: Updates in-memory + DB (non-fatal write)
//!
//! ## Known Limitations
//!
//! 1. **No startup recovery**: Jobs in "running" state after restart are orphaned
//! 2. **Non-transactional**: In-memory and DB updates are not atomic
//! 3. **Cancel latency**: Up to 1 epoch delay for cancellation to take effect
//! 4. **No distributed locking**: Single-node assumption for job management

use adapteros_config::CoreMLComputePreference;
use adapteros_core::{backend::BackendKind, AosError, B3Hash};
use adapteros_db::CreateCoremlFusionPairParams;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_lora_worker::training::trainer::EpochMetrics as WorkerEpochMetrics;
use adapteros_lora_worker::training::{
    MicroLoRATrainer as WorkerTrainer, TrainingBackend as WorkerTrainingBackend,
    TrainingConfig as WorkerTrainingConfig, TrainingExample as WorkerTrainingExample,
};
use adapteros_lora_worker::{ComputeUnits, CoreMLExportJob, CoreMLExportRecord};
use anyhow::Result;
use blake3;
use chrono::Utc;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

// Re-export canonical types from adapteros_types
pub use adapteros_types::training::{
    DataLineageMode, DatasetVersionSelection, DatasetVersionTrustSnapshot, LoraTier,
    TrainingBackendKind, TrainingBackendPolicy, TrainingConfig, TrainingJob, TrainingJobStatus,
    TrainingTemplate,
};

// Deterministic weighted round-robin merge for multi-dataset training.
fn weighted_round_robin_merge(
    datasets: Vec<(Vec<WorkerTrainingExample>, f32)>,
) -> Vec<WorkerTrainingExample> {
    let weights: Vec<f32> = datasets.iter().map(|(_, w)| *w).collect();
    let mut queues: Vec<VecDeque<WorkerTrainingExample>> = datasets
        .into_iter()
        .map(|(examples, _)| VecDeque::from(examples))
        .collect();

    let mut schedule: Vec<usize> = Vec::new();
    for (idx, queue) in queues.iter().enumerate() {
        let weight = (*weights.get(idx).unwrap_or(&1.0)).max(0.0);
        let slots = weight.round() as usize;
        let slots = if slots == 0 { 1 } else { slots };
        if !queue.is_empty() {
            for _ in 0..slots {
                schedule.push(idx);
            }
        }
    }

    if schedule.is_empty() {
        return Vec::new();
    }

    let mut merged = Vec::new();
    loop {
        let mut progressed = false;
        for &idx in schedule.iter() {
            if let Some(ex) =
                queues
                    .get_mut(idx)
                    .and_then(|q| if q.is_empty() { None } else { q.pop_front() })
            {
                merged.push(ex);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    merged
}

/// Versioning context for training output.
#[derive(Debug, Clone)]
pub struct TrainingVersioningContext {
    pub adapter_version_id: String,
    pub version_label: String,
    pub branch: String,
    pub repo_id: String,
    pub repo_name: String,
    pub parent_version_id: Option<String>,
    pub draft_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_json: Option<String>,
    pub data_spec_hash: Option<String>,
}

/// Deterministic combined data_spec_hash for multi-dataset jobs.
///
/// Input: (dataset_version_id, dataset_manifest_hash, weight)
/// - Sorted by dataset_version_id for stability.
/// - Weight hashed via IEEE-754 little-endian bytes to avoid formatting drift.
pub fn compute_combined_data_spec_hash(entries: &[(String, String, f32)]) -> String {
    let mut items = entries.to_vec();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = blake3::Hasher::new();
    for (id, hash, weight) in items {
        hasher.update(id.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b":");
        hasher.update(&weight.to_le_bytes());
        hasher.update(b";");
    }

    hasher.finalize().to_hex().to_string()
}

fn canonical_trust_state(raw: &str) -> String {
    const CANONICAL_TRUST_STATES: &[&str] = &[
        "allowed",
        "allowed_with_warning",
        "needs_approval",
        "blocked",
        "unknown",
    ];

    let normalized = match raw.trim().to_ascii_lowercase().as_str() {
        "allowed" => "allowed",
        "allowed_with_warning" | "warn" => "allowed_with_warning",
        "needs_approval" => "needs_approval",
        "blocked" | "blocked_regressed" => "blocked",
        "unknown" => "unknown",
        other => {
            warn!(state = %other, "Unknown trust_state; normalizing to unknown");
            "unknown"
        }
    };

    if !CANONICAL_TRUST_STATES.contains(&normalized) {
        warn!(state = %normalized, "Non-canonical trust_state emitted; forcing unknown");
        "unknown".to_string()
    } else {
        normalized.to_string()
    }
}

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
        lora_tier: Option<LoraTier>,
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
        // Default to tenant scope to satisfy adapter scope trigger.
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
        // Normalize to Some(false) so downstream snapshots capture caller intent.
        config.enable_coreml_export = Some(export_opt_in);
        let mut data_spec_hash = data_spec_hash;

        let dataset_versions_empty = dataset_version_ids
            .as_ref()
            .map(|v| v.is_empty())
            .unwrap_or(true);
        if synthetic_mode && !dataset_versions_empty {
            return Err(AosError::Validation(
                "synthetic_mode=true requires dataset_version_ids to be empty".to_string(),
            )
            .into());
        }
        if !synthetic_mode && dataset_versions_empty {
            return Err(AosError::Validation(
                "dataset_version_ids are required for non-synthetic training jobs".to_string(),
            )
            .into());
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
                    ))
                    .into());
                }
                if trust_state == "needs_approval" || trust_state == "unknown" {
                    return Err(AosError::Validation(format!(
                        "dataset version {} trust_state={} blocks training",
                        sel.dataset_version_id, trust_state
                    ))
                    .into());
                }

                // Workstream 9: Verify dataset integrity before training
                // Check that all dataset files match their stored BLAKE3 hashes
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
                            .take(5) // Limit to first 5 for error message
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
                        ))
                        .into());
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
                    return Err(AosError::Validation("DATA_SPEC_HASH_MISMATCH".to_string()).into());
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
            hidden_dim: 768, // Default hidden dimension
        };
        let config_hash = adapteros_db::training_jobs::compute_config_hash(&config_params).ok();

        // Get build ID from environment or use default
        let build_id = code_commit_sha
            .clone()
            .or_else(|| std::env::var("BUILD_ID").ok())
            .or_else(|| std::env::var("GIT_COMMIT").ok())
            .or_else(|| Some("dev".to_string()));

        let mut job = TrainingJob::new(job_id.clone(), adapter_name.clone(), config.clone());
        job.template_id = template_id;
        job.repo_id = repo_id.clone();
        job.target_branch = target_branch.clone();
        job.base_version_id = base_version_id.clone();
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
            // #region agent log
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/Users/mln-dev/Dev/adapter-os/.cursor/debug.log")
            {
                let _ = writeln!(
                    f,
                    r#"{{"sessionId":"debug-session","runId":"pre-fix","hypothesisId":"H3","location":"training.rs:trust_snapshots","message":"collected trust snapshots","data":{{"count":{},"dataset_version_ids":{:?}}},"timestamp":{}}}"#,
                    trust_snapshots.len(),
                    versions
                        .iter()
                        .map(|v| &v.dataset_version_id)
                        .collect::<Vec<_>>(),
                    Utc::now().timestamp_millis()
                );
            }
            // #endregion
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
        // Category metadata
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
        let base_model_id_for_det = base_model_id_for_run.clone();
        // Deterministic task clones (leave originals for fallback)
        let jobs_ref_det = jobs_ref.clone();
        let job_id_det = job_id_for_run.clone();
        let adapter_name_det = adapter_name_for_run.clone();
        let cfg_for_det = cfg_for_run.clone();
        let dataset_id_for_det = dataset_id_for_run.clone();
        let tenant_id_for_det = tenant_id_for_run.clone();
        let db_for_det = db_for_run.clone();
        let storage_for_det = storage_for_run.clone();
        let category_for_det = category_for_run.clone();
        let post_actions_for_det = post_actions_for_run.clone();
        // Clones reserved for fallback telemetry spawn to avoid move-after-use
        let dataset_id_for_fallback = dataset_id_for_run.clone();
        let tenant_id_for_fallback = tenant_id_for_run.clone();
        let db_for_fallback = db_for_run.clone();
        let storage_for_fallback = storage_for_run.clone();
        let category_for_fallback = category_for_run.clone();
        let post_actions_for_fallback = post_actions_for_run.clone();
        let base_model_id_for_fallback = base_model_id_for_run.clone();
        let jobs_ref_fallback = jobs_ref.clone();
        let job_id_for_fallback = job_id_for_run.clone();
        let adapter_name_for_fallback = adapter_name_for_run.clone();
        let cfg_for_fallback = cfg_for_run.clone();
        let synthetic_mode_for_run = synthetic_mode;
        let data_lineage_mode_for_run = data_lineage_mode;
        // Clone Arc handles for each spawned task to avoid move-after-use
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
                    category_for_det,
                    post_actions_for_det,
                    base_model_id_for_det,
                    cancel_token_for_run,
                )
                .await;

                // Clean up cancel token after job completes (success or failure)
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
                ))
                .into());
            }
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
            // No UDS client provided - rely on in-process token
            false
        };

        // Update status based on confirmation
        // Token set OR worker confirmed = cancellation initiated
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

            // Persist cancellation status to database
            if let Some(ref database) = self.db {
                if let Err(e) = database.update_training_status(job_id, "cancelled").await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist training cancellation status to DB (non-fatal)");
                }
            }

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

            // Persist progress to database
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

            // Persist completion status to database
            if let Some(ref database) = self.db {
                if let Err(e) = database.update_training_status(job_id, "completed").await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
                }
            }

            Ok(())
        } else {
            Err(AosError::NotFound(format!("Training job not found: {}", job_id)).into())
        }
    }

    /// Trigger a CoreML export for a completed training job.
    pub async fn export_coreml_for_job(&self, job_id: &str) -> Result<TrainingJob> {
        let snapshot = self.get_job(job_id).await?;
        if snapshot.status != TrainingJobStatus::Completed {
            return Err(AosError::Validation(
                "CoreML export requires a completed training job".to_string(),
            )
            .into());
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
    pub async fn fail_job(&self, job_id: &str, error: String) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrainingJobStatus::Failed;
            job.error_message = Some(error.clone());
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            let adapter_id = job.adapter_id.clone();
            error!("Training job failed: {}", job_id);

            // Persist failure status to database
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
#[derive(Debug, Clone, serde::Deserialize)]
struct PostActions {
    /// Package adapter after training (default: true)
    #[serde(default = "default_true")]
    package: bool,
    /// Register adapter in registry after packaging (default: true)
    #[serde(default = "default_true")]
    register: bool,
    /// Create a new stack with the adapter after registration (default: true).
    #[serde(default = "default_true")]
    create_stack: bool,
    /// Activate the stack after creation (default: false).
    /// If true, sets the created stack as the tenant's default stack.
    /// WARNING: This changes the tenant's active inference behavior immediately.
    #[serde(default = "default_false")]
    activate_stack: bool,
    /// Tier to assign: persistent, warm, ephemeral (default: warm)
    #[serde(default = "default_tier")]
    tier: String,
    /// Custom adapters root directory (optional)
    adapters_root: Option<String>,
}

impl Default for PostActions {
    fn default() -> Self {
        Self {
            package: true,
            register: true,
            create_stack: true,
            activate_stack: false,
            tier: default_tier(),
            adapters_root: None,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_tier() -> String {
    "warm".to_string()
}

fn to_core_backend(kind: TrainingBackendKind) -> BackendKind {
    match kind {
        TrainingBackendKind::Auto => BackendKind::Auto,
        TrainingBackendKind::CoreML => BackendKind::CoreML,
        TrainingBackendKind::Mlx => BackendKind::Mlx,
        TrainingBackendKind::Metal => BackendKind::Metal,
        TrainingBackendKind::Cpu => BackendKind::CPU,
    }
}

/// Preferred backend mapping for worker config (preserves CoreML intent + fallback)
#[derive(Debug, Clone, Copy, Default)]
struct PreferredBackendSelection {
    preferred: Option<WorkerTrainingBackend>,
    coreml_fallback: Option<WorkerTrainingBackend>,
}

/// Map API/DB preferred backend into worker enums (uses BackendKind for parsing)
fn map_preferred_backend(
    preferred: Option<TrainingBackendKind>,
    coreml_fallback: Option<TrainingBackendKind>,
) -> PreferredBackendSelection {
    let mut preferred_backend = None;
    let mut fallback_backend = None;

    if let Some(kind) = preferred {
        let core_kind = to_core_backend(kind);
        match WorkerTrainingBackend::try_from(core_kind) {
            Ok(mapped) => {
                preferred_backend = Some(mapped);

                // If the caller provided a CoreML fallback, keep it explicit; otherwise, do not
                // silently redirect. Fallbacks are handled downstream with explicit telemetry.
                if mapped == WorkerTrainingBackend::CoreML {
                    fallback_backend = coreml_fallback
                        .and_then(|fb| WorkerTrainingBackend::try_from(to_core_backend(fb)).ok());
                }
            }
            Err(err) => {
                warn!(
                    backend = %kind,
                    error = %err,
                    "Non-concrete preferred backend ignored; using auto-select"
                );
            }
        }
    }

    // Validate explicit fallback even if preferred backend isn't CoreML (defensive)
    if fallback_backend.is_none() {
        if let Some(fb) = coreml_fallback {
            match WorkerTrainingBackend::try_from(to_core_backend(fb)) {
                Ok(mapped) => fallback_backend = Some(mapped),
                Err(err) => warn!(
                    backend = %fb.as_str(),
                    error = %err,
                    "Invalid CoreML fallback backend ignored"
                ),
            }
        }
    }

    PreferredBackendSelection {
        preferred: preferred_backend,
        coreml_fallback: fallback_backend,
    }
}

/// Load plan/model bytes for GPU initialization.
///
/// - Uses `AOS_MODEL_PATH` (or legacy fallbacks) to find model assets.
/// - Returns path bytes for CoreML `.mlpackage` bundles.
/// - Returns safetensors bytes for Metal/CPU/MLX.
/// - When GPU is optional and assets are missing, returns an empty Vec so CPU can proceed.
fn load_plan_bytes_for_training(require_gpu: bool, job_id: &str) -> Result<Vec<u8>> {
    let model_path = match adapteros_config::model::get_model_path_with_fallback() {
        Ok(path) => path,
        Err(e) => {
            if require_gpu {
                return Err(anyhow::anyhow!(
                    "GPU initialization requested but model path is not configured: {}",
                    e
                ));
            }

            warn!(
                job_id = %job_id,
                error = %e,
                "No model path configured; GPU init will be skipped and CPU will be used"
            );
            return Ok(Vec::new());
        }
    };

    fn read_plan_bytes(model_path: &Path) -> Result<Vec<u8>> {
        let is_mlpackage = model_path
            .extension()
            .map(|ext| ext == "mlpackage" || ext == "mlmodel")
            .unwrap_or(false);

        if is_mlpackage {
            return Ok(model_path.to_string_lossy().into_owned().into_bytes());
        }

        if model_path.is_file() {
            return std::fs::read(model_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read model plan from '{}': {}",
                    model_path.display(),
                    e
                )
            });
        }

        if model_path.is_dir() {
            let safetensors_path = model_path.join("model.safetensors");
            if safetensors_path.exists() {
                return std::fs::read(&safetensors_path).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to read model.safetensors at '{}': {}",
                        safetensors_path.display(),
                        e
                    )
                });
            }

            if let Ok(entries) = std::fs::read_dir(model_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();

                    // Sharded safetensors first shard
                    if name.starts_with("model-00001-of-") && name.ends_with(".safetensors") {
                        return std::fs::read(&path).map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to read sharded model file '{}': {}",
                                path.display(),
                                e
                            )
                        });
                    }

                    // CoreML bundle inside the directory
                    if path
                        .extension()
                        .map(|ext| ext == "mlpackage")
                        .unwrap_or(false)
                    {
                        return Ok(path.to_string_lossy().into_owned().into_bytes());
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Model assets not found under '{}'. Provide model.safetensors or a .mlpackage path.",
            model_path.display()
        ))
    }

    match read_plan_bytes(&model_path) {
        Ok(bytes) => Ok(bytes),
        Err(e) => {
            if require_gpu {
                Err(e)
            } else {
                warn!(
                    job_id = %job_id,
                    path = %model_path.display(),
                    error = %e,
                    "Plan bytes unavailable; GPU init will be skipped and CPU will be used"
                );
                Ok(Vec::new())
            }
        }
    }
}

/// Background runner for a single training job. Converts orchestrator config into worker trainer
/// config, runs training with per-epoch callback, packages weights, registers adapter, and
/// updates the shared job map with artifact metadata.
///
/// The cancel_token is checked by the trainer at epoch boundaries - set it to true to
/// request graceful cancellation. Metrics are persisted to the database after each epoch
/// when db and job_id are provided to the trainer.
#[allow(clippy::too_many_arguments)]
async fn run_training_job(
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
    use adapteros_lora_worker::training::{
        AdapterPackager, LoRAQuantizer, TrainingConfig as WorkerTrainingConfigType,
    };

    #[derive(Clone, Debug)]
    struct VersioningSnapshot {
        adapter_version_id: Option<String>,
        version_label: Option<String>,
        target_branch: Option<String>,
        repo_name: Option<String>,
        repo_id: Option<String>,
        base_version_id: Option<String>,
        code_commit_sha: Option<String>,
        data_spec_hash: Option<String>,
        dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
    }

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
    let base_aos_path: Option<PathBuf> = match (
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

    if let Some(ref base_path) = base_aos_path {
        info!(
            job_id = %job_id,
            base_version = ?versioning_snapshot.as_ref().and_then(|v| v.base_version_id.clone()),
            base_aos_path = %base_path.display(),
            "Fetched base adapter artifact for training"
        );
    }

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
        // GPU init policy: honor preferred_backend/require_gpu, resolve plan bytes from AOS_MODEL_PATH,
        // call init_kernels() before entering the training loop, and fall back to CPU when GPU is
        // optional or unavailable (see docs/GPU_TRAINING_INTEGRATION.md).

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
            .or(storage_adapters_str.as_deref());
        // AdapterPaths::from_config() will respect ENV > Config > Default precedence
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
        hidden_dim: 768, // default; can be made configurable via orchestrator config later
        vocab_size: 32000, // default LLaMA/Mistral vocab size
        coreml_placement: orchestrator_cfg.coreml_placement.clone(),
        preferred_backend: preferred_backend.preferred,
        backend_policy: orchestrator_cfg.backend_policy,
        coreml_fallback_backend: preferred_backend.coreml_fallback,
        require_gpu: orchestrator_cfg.require_gpu,
        max_gpu_memory_mb: orchestrator_cfg.max_gpu_memory_mb.unwrap_or(0),
        max_tokens_per_batch: None,
        device_policy: None,
        checkpoint_interval: Some(5), // Save checkpoint every 5 epochs
        warmup_steps: orchestrator_cfg.warmup_steps,
        max_seq_length: orchestrator_cfg.max_seq_length,
        gradient_accumulation_steps: orchestrator_cfg.gradient_accumulation_steps,
        determinism: None,
        moe_config: None,
    };

    // If a CoreML placement is provided, align hidden_dim to the placement shapes for training.
    if let Some(placement) = orchestrator_cfg.coreml_placement.as_ref() {
        if let Some(first) = placement.bindings.first() {
            let placement_hidden = first.shape.output_dim as usize;
            if placement_hidden > 0
                && worker_cfg.hidden_dim != placement_hidden {
                    tracing::info!(
                        worker_hidden_dim = worker_cfg.hidden_dim,
                        placement_hidden_dim = placement_hidden,
                        "Adjusting worker hidden_dim to CoreML placement output_dim"
                    );
                    worker_cfg.hidden_dim = placement_hidden;
                }
        }
    }

    // Clone db for later use in packaging/registration
    let db_for_packaging = db.clone();

    let dataset_version_ids_for_training = versioning_snapshot
        .as_ref()
        .and_then(|v| v.dataset_version_ids.clone());
    let data_spec_hash_for_training = versioning_snapshot
        .as_ref()
        .and_then(|v| v.data_spec_hash.clone());

    // Load training examples from dataset versions (if provided) or dataset_id, otherwise synthetic
    let examples: Vec<WorkerTrainingExample> = match (
        dataset_version_ids_for_training.clone(),
        dataset_id.clone(),
        db.clone(),
        storage_root.clone(),
    ) {
        (Some(version_selections), _, Some(database), Some(storage)) => {
            use crate::training_dataset_integration::TrainingDatasetManager;
            let dataset_manager = TrainingDatasetManager::new(database, storage, None);

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
                let jobs_ref_inner = jobs_ref_clone.clone();
                let job_id_inner = job_id_clone.clone();
                // Deterministic progress update (part of training execution)
                let jobs_ref_for_det = jobs_ref_inner.clone();
                let job_id_for_det = job_id_inner.clone();
                let jobs_ref_for_fallback = jobs_ref_inner.clone();
                let job_id_for_fallback = job_id_inner.clone();
                if let Err(e) = spawn_deterministic(
                    format!(
                        "training-progress:{}:epoch-{}",
                        job_id_for_det, metrics.epoch
                    ),
                    async move {
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
                    },
                ) {
                    // Fallback: use tokio::spawn if deterministic executor not available
                    tracing::warn!("Failed to spawn deterministic progress update: {}", e);
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
                use adapteros_db::TrainingMetricRow;
                use uuid::Uuid;

                let timestamp = chrono::Utc::now().to_rfc3339();
                let step = training_result.examples_processed.unwrap_or(0) as i64;
                let epoch = training_result.stopped_at_epoch.map(|e| e as i64);
                let tokens_processed = training_result.tokens_processed.unwrap_or(0) as f64;
                let tokens_per_second = training_result.tokens_per_sec as f64;

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
                    TrainingMetricRow {
                        id: Uuid::now_v7().to_string(),
                        training_job_id: job_id.clone(),
                        step,
                        epoch,
                        metric_name: "tokens_processed".to_string(),
                        metric_value: tokens_processed,
                        metric_timestamp: Some(chrono::Utc::now().to_rfc3339()),
                    },
                    TrainingMetricRow {
                        id: Uuid::now_v7().to_string(),
                        training_job_id: job_id.clone(),
                        step,
                        epoch,
                        metric_name: "tokens_per_sec_final".to_string(),
                        metric_value: tokens_per_second,
                        metric_timestamp: Some(chrono::Utc::now().to_rfc3339()),
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
                drop(jobs); // Release lock before DB call

                // Persist cancellation status to database
                if let Some(database) = &db {
                    if let Err(e) = database.update_training_status(&job_id, "cancelled").await {
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
                // Mark as completed without packaging
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.status = TrainingJobStatus::Completed;
                    job.progress_pct = 100.0;
                    job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                }
                drop(jobs); // Release lock before DB call

                // Persist completion status to database
                if let Some(database) = &db {
                    if let Err(e) = database.update_training_status(&job_id, "completed").await {
                        warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
                    }
                }

                return Ok(());
            }

            // Step 1: Quantize weights to Q15 format
            let quantized_weights = LoRAQuantizer::quantize_to_q15(&training_result.weights);

            // Build packaging metadata for auditability
            let (scope_value, lora_tier_meta, backend_policy_meta) = {
                let jobs = jobs_ref.read().await;
                let scope_val = jobs
                    .get(&job_id)
                    .and_then(|j| j.scope.clone())
                    .unwrap_or_else(|| "project".to_string());
                let tier_val = jobs.get(&job_id).and_then(|j| j.lora_tier);
                let backend_policy = jobs.get(&job_id).and_then(|j| j.backend_policy.clone());
                (scope_val, tier_val, backend_policy)
            };

            let mut package_metadata = HashMap::new();
            package_metadata.insert("training_job_id".to_string(), job_id.clone());
            package_metadata.insert("adapter_name".to_string(), adapter_name.clone());
            if let Some(ref ds) = dataset_id {
                package_metadata.insert("dataset_id".to_string(), ds.clone());
            }
            if let Some(ref tid) = tenant_id {
                package_metadata.insert("tenant_id".to_string(), tid.clone());
            }
            package_metadata.insert("scope".to_string(), scope_value.clone());
            // Allow downstream consumers to treat lora_scope separately if needed
            package_metadata.insert("lora_scope".to_string(), scope_value.clone());
            package_metadata.insert(
                "data_lineage_mode".to_string(),
                data_lineage_mode.as_str().to_string(),
            );
            package_metadata.insert(
                "synthetic_mode".to_string(),
                synthetic_mode.to_string(),
            );
            if let Some(ref base_model) = base_model_id {
                package_metadata.insert("base_model_id".to_string(), base_model.clone());
            }
            if let Some(ref cat) = category {
                package_metadata.insert("category".to_string(), cat.clone());
            }
            if let Some(tier) = lora_tier_meta {
                let tier_label = match tier {
                    LoraTier::Micro => "micro",
                    LoraTier::Standard => "standard",
                    LoraTier::Max => "max",
                };
                package_metadata.insert("lora_tier".to_string(), tier_label.to_string());
            }
            let backend_label = trainer.backend_info().unwrap_or("CPU").to_ascii_lowercase();
            package_metadata.insert("training_backend".to_string(), backend_label);
            if let Some(reason) = trainer.backend_reason() {
                package_metadata
                    .insert("training_backend_reason".to_string(), reason.to_string());
            }
            if let Some(device) = training_result.backend_device.clone() {
                package_metadata.insert("training_backend_device".to_string(), device);
            }
            if let Some(ref policy) = backend_policy_meta {
                package_metadata.insert("backend_policy".to_string(), policy.clone());
            }
            package_metadata.insert(
                "determinism".to_string(),
                if cfg!(feature = "deterministic-only") {
                    "deterministic-only".to_string()
                } else {
                    "best-effort".to_string()
                },
            );
            package_metadata.insert("quantization".to_string(), "q15".to_string());
            package_metadata.insert(
                "gate_q15_denominator".to_string(),
                adapteros_lora_router::ROUTER_GATE_Q15_DENOM.to_string(),
            );
            if let Some(ref hash) = data_spec_hash_for_training {
                package_metadata.insert("data_spec_hash".to_string(), hash.clone());
            }
            package_metadata.insert(
                "synthetic_mode".to_string(),
                synthetic_mode.to_string(),
            );
            package_metadata.insert(
                "data_lineage_mode".to_string(),
                data_lineage_mode.as_str().to_string(),
            );
            if let Some(ref versions) = dataset_version_ids_for_training {
                if let Ok(json) = serde_json::to_string(versions) {
                    package_metadata.insert("dataset_version_ids".to_string(), json);
                }
            }

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
                coreml_placement: worker_cfg.coreml_placement.clone(),
                preferred_backend: worker_cfg.preferred_backend,
                backend_policy: worker_cfg.backend_policy,
                coreml_fallback_backend: worker_cfg.coreml_fallback_backend,
                require_gpu: worker_cfg.require_gpu,
                max_gpu_memory_mb: worker_cfg.max_gpu_memory_mb,
                max_tokens_per_batch: worker_cfg.max_tokens_per_batch,
                device_policy: worker_cfg.device_policy.clone(),
                checkpoint_interval: worker_cfg.checkpoint_interval,
                warmup_steps: worker_cfg.warmup_steps,
                max_seq_length: worker_cfg.max_seq_length,
                gradient_accumulation_steps: worker_cfg.gradient_accumulation_steps,
                determinism: None,
                moe_config: None,
            };

            // Generate unique adapter ID from job_id
            let adapter_id = format!("adapter-{}", job_id.trim_start_matches("train-"));

            let base_model_for_manifest = base_model_id.as_deref().unwrap_or("unknown-base-model");

            let artifact_metadata = serde_json::json!({
                "backend": training_result.backend,
                "backend_device": training_result.backend_device,
                "requested_backend": worker_cfg.preferred_backend.map(|b| b.tag().to_string()),
                "coreml_training_fallback": worker_cfg
                    .coreml_fallback_backend
                    .map(|b| b.tag().to_string()),
                "data_spec_hash": data_spec_hash_for_training,
                "dataset_version_ids": dataset_version_ids_for_training,
                "synthetic_mode": synthetic_mode,
                "data_lineage_mode": data_lineage_mode.as_str(),
            });

            let packaged = match packager
                .package_aos_with_metadata(
                    tenant,
                    &adapter_id,
                    &quantized_weights,
                    &packager_cfg,
                    base_model_for_manifest,
                    package_metadata,
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
                    drop(jobs); // Release lock before DB call

                    // Persist failure status to database
                    if let Some(database) = &db {
                        if let Err(db_err) =
                            database.update_training_status(&job_id, "failed").await
                        {
                            warn!(job_id = %job_id, error = %db_err, "Failed to persist training failure status to DB (non-fatal)");
                        }
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

            let (final_aos_path, final_aos_hash) = {
                let target = if let (Some(ref repo_name), Some(ref version_label)) = (
                    versioning_snapshot
                        .as_ref()
                        .and_then(|v| v.repo_name.clone()),
                    versioning_snapshot
                        .as_ref()
                        .and_then(|v| v.version_label.clone()),
                ) {
                    let repo_dir = adapters_root.join(tenant).join(repo_name);
                    if let Err(e) = tokio::fs::create_dir_all(&repo_dir).await {
                        warn!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to create repo directory for versioned artifact"
                        );
                    }
                    let dest = repo_dir.join(format!("{}.aos", version_label));
                    if dest != packaged.weights_path {
                        if let Err(e) = tokio::fs::copy(&packaged.weights_path, &dest).await {
                            warn!(
                                job_id = %job_id,
                                error = %e,
                                dest = %dest.display(),
                                "Failed to copy packaged artifact to versioned path"
                            );
                        }
                    }
                    dest
                } else {
                    packaged.weights_path.clone()
                };

                let hash = tokio::fs::read(&target)
                    .await
                    .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
                    .unwrap_or_else(|_| packaged.hash_b3.clone());

                (target, hash)
            };
            let final_aos_path_str = final_aos_path.to_string_lossy().to_string();

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
                        job.artifact_path = Some(final_aos_path_str.clone());
                        job.adapter_id = Some(packaged.adapter_id.clone());
                        job.weights_hash_b3 = Some(packaged.hash_b3.clone());
                        job.aos_path = Some(final_aos_path_str.clone());
                        job.package_hash_b3 = Some(final_aos_hash.clone());
                        job.manifest_rank = Some(packaged.manifest.rank as u32);
                        job.manifest_base_model = Some(packaged.manifest.base_model.clone());
                        job.manifest_per_layer_hashes =
                            Some(packaged.manifest.per_layer_hashes.is_some());
                        job.signature_status = Some("signed".to_string());
                    }
                    drop(jobs); // Release lock before DB call

                    // Persist completion status to database
                    if let Err(e) = database.update_training_status(&job_id, "completed").await {
                        warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
                    }

                    // Persist artifact metadata even when registration is disabled.
                    if let Err(e) = database
                        .update_training_job_artifact(
                            &job_id,
                            final_aos_path_str.as_str(),
                            &packaged.adapter_id,
                            &final_aos_hash,
                            Some(artifact_metadata.clone()),
                        )
                        .await
                    {
                        warn!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to persist training job artifact metadata (non-fatal)"
                        );
                    }

                    if let Some(version_id) =
                        versioning_snapshot.as_ref().and_then(|v| v.adapter_version_id.clone())
                    {
                            let backend_lower = training_result
                                .backend
                                .as_deref()
                                .map(|b| b.to_ascii_lowercase());
                            let coreml_used = training_result
                                .backend
                                .as_deref()
                                .map(|b| b.eq_ignore_ascii_case("coreml"));
                        let artifact_result = database
                            .update_adapter_version_artifact(
                                &version_id,
                                "ready",
                                Some(final_aos_path_str.as_str()),
                                Some(&final_aos_hash),
                                    data_spec_hash_for_training.as_deref(),
                                    backend_lower.as_deref(),
                                    coreml_used,
                                    training_result.backend_device.as_deref(),
                                None,
                                None,
                                Some("orchestrator"),
                                Some("training_complete"),
                                Some(&job_id),
                            )
                            .await;
                        if let Err(e) = artifact_result {
                            warn!(
                                version_id = %version_id,
                                error = %e,
                                "Failed to mark adapter version ready (non-fatal)"
                            );
                        } else if let Err(e) = database
                            .set_training_produced_version(&job_id, &version_id, None)
                            .await
                        {
                            warn!(
                                job_id = %job_id,
                                version_id = %version_id,
                                error = %e,
                                "Failed to record produced version for training job (non-fatal)"
                            );
                        }

                        if coreml_used.unwrap_or(false) {
                            if let Some(repo_id) =
                                versioning_snapshot.as_ref().and_then(|v| v.repo_id.clone())
                            {
                                let tenant_for_repo = tenant_id.as_deref().unwrap_or("default");
                                if let Ok(Some(policy)) = database
                                    .get_adapter_repository_policy(tenant_for_repo, &repo_id)
                                    .await
                                {
                                    if policy.autopromote_coreml {
                                        let _ = database
                                            .promote_adapter_version(
                                                tenant_for_repo,
                                                &repo_id,
                                                &version_id,
                                                Some("orchestrator"),
                                                Some("auto_coreml_promotion"),
                                            )
                                            .await;
                                    }
                                }
                            }
                        }

                        info!(
                            job_id = %job_id,
                            version_id = %version_id,
                            branch = ?versioning_snapshot.as_ref().and_then(|v| v.target_branch.clone()),
                            "history event: training_succeeded"
                        );
                    }

                    return Ok(());
                }

                use adapteros_db::AdapterRegistrationBuilder;

                // Use category from request or default to "trained"
                let adapter_category = category.as_deref().unwrap_or("code");
                let meta = &packaged.manifest.metadata;
                let domain = meta
                    .get("domain")
                    .cloned()
                    .unwrap_or_else(|| "unspecified".to_string());
                let group = meta
                    .get("group")
                    .cloned()
                    .unwrap_or_else(|| "unspecified".to_string());
                let scope_value = packaged.manifest.scope.clone();

                let reg_params = AdapterRegistrationBuilder::new()
                    .tenant_id(tenant_id.as_deref().unwrap_or("default"))
                    .adapter_id(&packaged.adapter_id)
                    .name(&adapter_name)
                    .hash_b3(&packaged.hash_b3)
                    .rank(orchestrator_cfg.rank as i32)
                    .tier(&post_actions.tier)
                    .alpha(orchestrator_cfg.alpha as f64)
                    .category(adapter_category)
                    .scope(&scope_value)
                    .domain(Some(domain))
                    .purpose(Some(group))
                    .base_model_id(base_model_id.as_deref())
                    .manifest_schema_version(Some(packaged.manifest.version.clone()))
                    .content_hash_b3(Some(packaged.hash_b3.clone()))
                    .aos_file_path(Some(
                        packaged.weights_path.to_string_lossy().to_string(),
                    ))
                    .aos_file_hash(Some(packaged.hash_b3.clone()))
                    .provenance_json(serde_json::to_string(&packaged.manifest.metadata).ok())
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
                                final_aos_path_str.as_str(),
                                &packaged.adapter_id,
                                &final_aos_hash,
                                Some(artifact_metadata.clone()),
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

                        if let Err(e) = database
                            .transition_adapter_lifecycle(
                                &packaged.adapter_id,
                                "ready",
                                "training_completed",
                                "system",
                            )
                            .await
                        {
                            warn!(
                                job_id = %job_id,
                                adapter_id = %packaged.adapter_id,
                                error = %e,
                                "Failed to mark adapter ready after training"
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
                                routing_determinism_mode: None,
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

                                    // Persist stack_id and adapter_id to database for chat_bootstrap endpoint
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

                                    // Step 5: Optionally activate the stack (set as tenant default)
                                    if post_actions.activate_stack {
                                        match database.set_default_stack(tenant_id, &stack_id).await
                                        {
                                            Ok(_) => {
                                                info!(
                                                    job_id = %job_id,
                                                    tenant_id = %tenant_id,
                                                    stack_id = %stack_id,
                                                    "Stack activated as tenant default"
                                                );

                                                // Ensure stack lifecycle_state is Active for control plane + KV
                                                if let Err(e) = database
                                                    .activate_stack(tenant_id, &stack_id)
                                                    .await
                                                {
                                                    warn!(
                                                        job_id = %job_id,
                                                        tenant_id = %tenant_id,
                                                        stack_id = %stack_id,
                                                        error = %e,
                                                        "Failed to mark stack active in DB after training (non-fatal)"
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                // Log but don't fail - stack is already created
                                                warn!(
                                                    job_id = %job_id,
                                                    tenant_id = %tenant_id,
                                                    stack_id = %stack_id,
                                                    error = %e,
                                                    "Failed to activate stack (non-fatal)"
                                                );
                                            }
                                        }
                                    } else {
                                        info!(
                                            job_id = %job_id,
                                            stack_id = %stack_id,
                                            "Stack created but NOT activated (activate_stack=false)"
                                        );
                                    }
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
                        drop(jobs); // Release lock before DB call

                        // Persist failure status to database
                        if let Err(db_err) =
                            database.update_training_status(&job_id, "failed").await
                        {
                            warn!(job_id = %job_id, error = %db_err, "Failed to persist training failure status to DB (non-fatal)");
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
                    job.artifact_path = Some(final_aos_path_str.clone());
                    job.adapter_id = Some(packaged.adapter_id.clone());
                    job.weights_hash_b3 = Some(packaged.hash_b3.clone());
                    job.aos_path = Some(final_aos_path_str.clone());
                    job.package_hash_b3 = Some(final_aos_hash.clone());
                    job.manifest_rank = Some(packaged.manifest.rank as u32);
                    job.manifest_base_model = Some(packaged.manifest.base_model.clone());
                    job.manifest_per_layer_hashes =
                        Some(packaged.manifest.per_layer_hashes.is_some());
                    job.signature_status = Some("signed".to_string());

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

            // Persist completion status to database
            if let Some(database) = &db_for_packaging {
                if let Err(e) = database.update_training_status(&job_id, "completed").await {
                    warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
                }

                if let Some(version_id) =
                    versioning_snapshot.as_ref().and_then(|v| v.adapter_version_id.clone())
                {
                    let backend_lower = training_result
                        .backend
                        .as_deref()
                        .map(|b| b.to_ascii_lowercase());
                    let coreml_used = training_result
                        .backend
                        .as_deref()
                        .map(|b| b.eq_ignore_ascii_case("coreml"));
                    let artifact_result = database
                        .update_adapter_version_artifact(
                            &version_id,
                            "ready",
                            Some(final_aos_path_str.as_str()),
                            Some(&final_aos_hash),
                            data_spec_hash_for_training.as_deref(),
                            backend_lower.as_deref(),
                            coreml_used,
                            training_result.backend_device.as_deref(),
                            None,
                            None,
                            Some("orchestrator"),
                            Some("training_complete"),
                            Some(&job_id),
                        )
                        .await;
                    if let Err(e) = artifact_result {
                        warn!(
                            version_id = %version_id,
                            error = %e,
                            "Failed to mark adapter version ready (non-fatal)"
                        );
                    } else if let Err(e) = database
                        .set_training_produced_version(&job_id, &version_id, None)
                        .await
                    {
                        warn!(
                            job_id = %job_id,
                            version_id = %version_id,
                            error = %e,
                            "Failed to record produced version for training job (non-fatal)"
                        );
                    }

                    if coreml_used.unwrap_or(false) {
                        if let Some(repo_id) =
                            versioning_snapshot.as_ref().and_then(|v| v.repo_id.clone())
                        {
                            let tenant_for_repo = tenant_id.as_deref().unwrap_or("default");
                            if let Ok(Some(policy)) = database
                                .get_adapter_repository_policy(tenant_for_repo, &repo_id)
                                .await
                            {
                                if policy.autopromote_coreml {
                                    let _ = database
                                        .promote_adapter_version(
                                            tenant_for_repo,
                                            &repo_id,
                                            &version_id,
                                            Some("orchestrator"),
                                            Some("auto_coreml_promotion"),
                                        )
                                        .await;
                                }
                            }
                        }
                    }

                    info!(
                        job_id = %job_id,
                        version_id = %version_id,
                        branch = ?versioning_snapshot.as_ref().and_then(|v| v.target_branch.clone()),
                        "history event: training_succeeded"
                    );
                }
            }

            // Optional CoreML export (post-training) - best-effort, does not change training status
            if orchestrator_cfg.enable_coreml_export.unwrap_or(false) {
                if let Err(e) = run_coreml_export_flow(
                    jobs_ref.clone(),
                    &job_id,
                    &packaged.adapter_id,
                    &final_aos_path,
                    &packaged.manifest.base_model,
                    &packaged.hash_b3,
                    adapters_root.as_path(),
                    tenant_id.as_deref(),
                    db_for_packaging.as_ref(),
                )
                .await
                {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "CoreML export failed (non-fatal)"
                    );
                }
            } else {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(&job_id) {
                    job.coreml_export_requested = Some(false);
                }
            }

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

            // Persist failure status and retryable flag to DB
            if let Some(database) = &db {
                // Update status to "failed"
                if let Err(e) = database.update_training_status(&job_id, "failed").await {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to persist training failure status to DB (non-fatal)"
                    );
                }

                // Update retryable flag
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

#[allow(clippy::too_many_arguments)]
async fn run_coreml_export_flow(
    jobs_ref: Arc<RwLock<HashMap<String, TrainingJob>>>,
    job_id: &str,
    adapter_id: &str,
    aos_path: &Path,
    base_model_id: &str,
    weights_hash_b3: &str,
    adapters_root: &Path,
    tenant_id: Option<&str>,
    db_for_packaging: Option<&adapteros_db::Db>,
) -> Result<()> {
    {
        let mut jobs = jobs_ref.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.coreml_export_requested = Some(true);
            job.coreml_export_status = Some("running".to_string());
            job.coreml_export_reason = None;
        }
    }

    let export_outcome = (|| -> Result<CoreMLExportRecord> {
        let base_package = adapteros_config::model::get_model_path_with_fallback()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let fused_root = adapters_root.join("coreml").join(adapter_id);
        let output_package = if base_package.is_dir() {
            fused_root
        } else {
            let filename = base_package
                .file_name()
                .unwrap_or_else(|| OsStr::new("fused.mlpackage"));
            fused_root.join(filename)
        };

        let export_job = CoreMLExportJob {
            base_package,
            adapter_aos: aos_path.to_path_buf(),
            output_package,
            compute_units: resolve_coreml_compute_units(),
            base_model_id: Some(base_model_id.to_string()),
            adapter_id: Some(adapter_id.to_string()),
        };

        perform_coreml_export(export_job)
    })();

    match export_outcome {
        Ok(record) => {
            let fused_hash = record.fused_manifest_hash.to_string();
            let base_hash = record.base_manifest_hash.to_string();
            let adapter_hash = record.adapter_hash.to_string();
            let metadata_path_str = record.metadata_path.to_string_lossy().to_string();
            let fused_path_str = record.fused_package.to_string_lossy().to_string();

            {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.coreml_export_status = Some("succeeded".to_string());
                    job.coreml_export_reason = None;
                    job.coreml_fused_package_hash = Some(fused_hash.clone());
                    job.coreml_package_path = Some(fused_path_str.clone());
                    job.coreml_metadata_path = Some(metadata_path_str.clone());
                    job.coreml_base_manifest_hash = Some(base_hash.clone());
                    job.coreml_adapter_hash_b3 = Some(adapter_hash.clone());
                }
            }

            if let Some(database) = db_for_packaging {
                let tenant_for_fusion = tenant_id.unwrap_or("default").to_string();
                let params = CreateCoremlFusionPairParams {
                    tenant_id: tenant_for_fusion,
                    base_model_id: base_model_id.to_string(),
                    adapter_id: adapter_id.to_string(),
                    fused_manifest_hash: fused_hash.clone(),
                    coreml_package_hash: fused_hash.clone(),
                    adapter_hash_b3: Some(adapter_hash.clone()),
                    base_model_hash_b3: Some(base_hash.clone()),
                    metadata_path: Some(metadata_path_str.clone()),
                };

                if let Err(e) = database.upsert_coreml_fusion_pair(params).await {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to upsert coreml_fusion_pairs record"
                    );
                }

                let export_meta = serde_json::json!({
                    "coreml_export": {
                        "requested": true,
                        "status": "succeeded",
                        "fused_manifest_hash": fused_hash,
                        "base_manifest_hash": base_hash,
                        "adapter_hash_b3": adapter_hash,
                        "package_path": fused_path_str,
                        "metadata_path": metadata_path_str
                    }
                });

                if let Err(e) = database
                    .update_training_job_artifact(
                        job_id,
                        aos_path.to_string_lossy().as_ref(),
                        adapter_id,
                        weights_hash_b3,
                        Some(export_meta),
                    )
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to persist CoreML export metadata (non-fatal)"
                    );
                }
            }
        }
        Err(e) => {
            let reason = e.to_string();
            {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.coreml_export_status = Some("failed".to_string());
                    job.coreml_export_reason = Some(reason.clone());
                }
            }

            if let Some(database) = db_for_packaging {
                let export_meta = serde_json::json!({
                    "coreml_export": {
                        "requested": true,
                        "status": "failed",
                        "reason": reason
                    }
                });
                if let Err(err) = database
                    .update_training_job_artifact(
                        job_id,
                        aos_path.to_string_lossy().as_ref(),
                        adapter_id,
                        weights_hash_b3,
                        Some(export_meta),
                    )
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        error = %err,
                        "Failed to persist failed CoreML export metadata (non-fatal)"
                    );
                }
            }

            return Err(e);
        }
    }

    Ok(())
}

fn resolve_coreml_compute_units() -> ComputeUnits {
    let pref = std::env::var("AOS_COREML_COMPUTE_UNITS")
        .ok()
        .and_then(|v| CoreMLComputePreference::from_str(&v).ok())
        .unwrap_or_default();
    match pref {
        CoreMLComputePreference::CpuOnly => ComputeUnits::CpuOnly,
        CoreMLComputePreference::CpuAndGpu => ComputeUnits::CpuAndGpu,
        CoreMLComputePreference::CpuAndNe => ComputeUnits::CpuAndNeuralEngine,
        CoreMLComputePreference::All => ComputeUnits::All,
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn perform_coreml_export(job: CoreMLExportJob) -> Result<CoreMLExportRecord> {
    run_coreml_export(job).map_err(|e| anyhow::anyhow!(e.to_string()))
}

#[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
fn perform_coreml_export(job: CoreMLExportJob) -> Result<CoreMLExportRecord> {
    if std::env::var("AOS_ALLOW_COREML_EXPORT_STUB").is_err() && !cfg!(test) {
        return Err(anyhow::anyhow!(
            "CoreML export not supported on this platform (enable AOS_ALLOW_COREML_EXPORT_STUB=1 to stub)"
        ));
    }

    if let Some(parent) = job.output_package.parent() {
        fs::create_dir_all(parent).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }
    if job.output_package.is_dir() || job.base_package.is_dir() {
        fs::create_dir_all(&job.output_package).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    let base_manifest_path = if job.base_package.is_dir() {
        job.base_package.join("Manifest.json")
    } else {
        job.base_package.clone()
    };
    let manifest_bytes = fs::read(&base_manifest_path).unwrap_or_default();
    let base_manifest_hash = B3Hash::hash(&manifest_bytes);
    let fused_manifest_hash = base_manifest_hash;

    let adapter_bytes = fs::read(&job.adapter_aos)
        .map_err(|e| anyhow::anyhow!(format!("Failed to read adapter bundle: {}", e)))?;
    let adapter_hash = B3Hash::hash(&adapter_bytes);

    let metadata_path = if job.output_package.is_dir() {
        job.output_package.join("adapteros_coreml_fusion.json")
    } else {
        job.output_package.with_extension("fusion.json")
    };
    let metadata = serde_json::json!({
        "base_manifest_hash": base_manifest_hash.to_string(),
        "fused_manifest_hash": fused_manifest_hash.to_string(),
        "adapter_hash": adapter_hash.to_string(),
        "base_package": job.base_package,
        "fused_package": job.output_package,
        "adapter_path": job.adapter_aos,
        "stub": true
    });
    fs::write(&metadata_path, serde_json::to_vec_pretty(&metadata)?)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(CoreMLExportRecord {
        fused_package: job.output_package.clone(),
        metadata_path,
        base_manifest_hash,
        fused_manifest_hash,
        adapter_hash,
        base_model_id: job.base_model_id,
        adapter_id: job.adapter_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_lora_worker::training::TrainingExample as WorkerTrainingExample;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    #[test]
    fn map_preferred_backend_coreml_does_not_inject_default_fallback() {
        let mapped = map_preferred_backend(Some(TrainingBackendKind::CoreML), None);
        assert_eq!(mapped.preferred, Some(WorkerTrainingBackend::CoreML));
        assert_eq!(mapped.coreml_fallback, None);
    }

    #[test]
    fn map_preferred_backend_coreml_respects_explicit_fallback() {
        let mapped = map_preferred_backend(
            Some(TrainingBackendKind::CoreML),
            Some(TrainingBackendKind::Metal),
        );
        assert_eq!(mapped.preferred, Some(WorkerTrainingBackend::CoreML));
        assert_eq!(mapped.coreml_fallback, Some(WorkerTrainingBackend::Metal));
    }

    #[test]
    fn weighted_round_robin_is_deterministic() {
        let ds1 = vec![
            WorkerTrainingExample {
                input: vec![1],
                target: vec![2],
                metadata: Default::default(),
                weight: 1.0,
            },
            WorkerTrainingExample {
                input: vec![3],
                target: vec![4],
                metadata: Default::default(),
                weight: 1.0,
            },
        ];
        let ds2 = vec![WorkerTrainingExample {
            input: vec![5],
            target: vec![6],
            metadata: Default::default(),
            weight: 1.0,
        }];

        let merged = weighted_round_robin_merge(vec![(ds1.clone(), 2.0), (ds2.clone(), 1.0)]);
        // Expect pattern: ds1, ds1, ds2 (since ds1 weight rounds to 2 slots)
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].input, vec![1]);
        assert_eq!(merged[1].input, vec![3]);
        assert_eq!(merged[2].input, vec![5]);

        let merged_again = weighted_round_robin_merge(vec![(ds1, 2.0), (ds2, 1.0)]);
        assert_eq!(
            merged.iter().map(|e| &e.input).collect::<Vec<_>>(),
            merged_again.iter().map(|e| &e.input).collect::<Vec<_>>()
        );
    }

    #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
    #[test]
    fn stub_coreml_export_path_is_invokable_when_allowed() {
        use std::fs;

        let tmp = new_test_tempdir();
        let base = tmp.path().join("base.json");
        let adapter = tmp.path().join("adapter.aos");
        fs::write(&base, b"base-bytes").unwrap();
        fs::write(&adapter, b"adapter-bytes").unwrap();

        std::env::set_var("AOS_ALLOW_COREML_EXPORT_STUB", "1");
        let record = perform_coreml_export(CoreMLExportJob {
            base_package: base.clone(),
            adapter_aos: adapter.clone(),
            output_package: tmp.path().join("fused"),
            compute_units: ComputeUnits::All,
            base_model_id: None,
            adapter_id: None,
        })
        .expect("stub export should be allowed when env enabled");
        std::env::remove_var("AOS_ALLOW_COREML_EXPORT_STUB");

        assert!(record.metadata_path.exists());
    }

    #[tokio::test]
    async fn start_training_records_coreml_intent_and_fallback() {
        let service = TrainingService::new();
        let mut config = TrainingConfig::default();
        config.preferred_backend = Some(TrainingBackendKind::CoreML);
        config.coreml_training_fallback = Some(TrainingBackendKind::Mlx);

        let job = service
            .start_training(
                "coreml-intent".to_string(),
                config,
                None, // template_id
                None, // repo_id
                None, // target_branch
                None, // base_version_id
                None, // dataset_id
                None, // dataset_version_ids
                true, // synthetic_mode
                DataLineageMode::Synthetic,
                None, // tenant_id
                None, // initiated_by
                None, // initiated_by_role
                None, // base_model_id
                None, // collection_id
                None, // scope
                None, // lora_tier
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
                None, // versioning
                None, // code_commit_sha
                None, // data_spec_json
                None, // data_spec_hash
            )
            .await
            .unwrap();

        assert_eq!(job.requested_backend.as_deref(), Some("coreml"));
        assert_eq!(job.coreml_training_fallback.as_deref(), Some("mlx"));
        assert!(job.backend.is_none(), "backend is recorded post-selection");
    }

    #[tokio::test]
    async fn start_training_rejects_missing_dataset_versions_when_non_synthetic() {
        let service = TrainingService::new();
        let config = TrainingConfig::default();

        let result = service
            .start_training(
                "missing-datasets".to_string(),
                config,
                None,  // template_id
                None,  // repo_id
                None,  // target_branch
                None,  // base_version_id
                None,  // dataset_id
                None,  // dataset_version_ids
                false, // synthetic_mode
                DataLineageMode::Synthetic,
                None, // tenant_id
                None, // initiated_by
                None, // initiated_by_role
                None, // base_model_id
                None, // collection_id
                None, // scope
                None, // lora_tier
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
                None, // versioning
                None, // code_commit_sha
                None, // data_spec_json
                None, // data_spec_hash
            )
            .await;

        assert!(
            result.is_err(),
            "non-synthetic training without datasets must fail"
        );
    }

    #[tokio::test]
    async fn test_create_and_list_jobs() {
        let service = TrainingService::new();

        let config = TrainingConfig::default();
        let job = service
            .start_training(
                "test-adapter".to_string(),
                config,
                None, // template_id
                None, // repo_id
                None, // target_branch
                None, // base_version_id
                None, // dataset_id
                None, // dataset_version_ids
                true, // synthetic_mode
                DataLineageMode::Synthetic,
                None, // tenant_id
                None, // initiated_by
                None, // initiated_by_role
                None, // base_model_id
                None, // collection_id
                None, // scope
                None, // lora_tier
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
                None, // versioning
                None, // code_commit_sha
                None, // data_spec_json
                None, // data_spec_hash
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
                None, // template_id
                None, // repo_id
                None, // target_branch
                None, // base_version_id
                None, // dataset_id
                None, // dataset_version_ids
                true, // synthetic_mode
                DataLineageMode::Synthetic,
                None, // tenant_id
                None, // initiated_by
                None, // initiated_by_role
                None, // base_model_id
                None, // collection_id
                None, // scope
                None, // lora_tier
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
                None, // versioning
                None, // code_commit_sha
                None, // data_spec_json
                None, // data_spec_hash
            )
            .await
            .unwrap();

        // Test without UDS client (will mark as Cancelled via token)
        service.cancel_job(&job.id, None, None).await.unwrap();

        let updated_job = service.get_job(&job.id).await.unwrap();
        // Without UDS client, job is marked as Cancelled
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
                None, // template_id
                None, // repo_id
                None, // target_branch
                None, // base_version_id
                None, // dataset_id
                None, // dataset_version_ids
                true, // synthetic_mode
                DataLineageMode::Synthetic,
                None, // tenant_id
                None, // initiated_by
                None, // initiated_by_role
                None, // base_model_id
                None, // collection_id
                None, // scope
                None, // lora_tier
                None, // category
                None, // description
                None, // language
                None, // framework_id
                None, // framework_version
                None, // post_actions_json
                None, // retry_of_job_id
                None, // versioning
                None, // code_commit_sha
                None, // data_spec_json
                None, // data_spec_hash
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

    fn cpu_only_config() -> TrainingConfig {
        let mut cfg = TrainingConfig::default();
        cfg.epochs = 1;
        cfg.batch_size = 1;
        cfg.learning_rate = 0.0001;
        cfg.preferred_backend = None;
        cfg.require_gpu = false;
        cfg.max_gpu_memory_mb = None;
        cfg
    }

    fn gpu_required_config() -> TrainingConfig {
        let mut cfg = cpu_only_config();
        cfg.require_gpu = true;
        cfg
    }

    fn no_package_actions() -> Option<String> {
        Some(
            serde_json::json!({
                "package": false,
                "register": false,
                "create_stack": false,
                "activate_stack": false
            })
            .to_string(),
        )
    }

    #[tokio::test]
    async fn coreml_export_flow_updates_job_and_registry() {
        std::env::set_var("AOS_ALLOW_COREML_EXPORT_STUB", "1");
        let temp = new_test_tempdir();
        let base_dir = temp.path().join("base");
        std::fs::create_dir_all(&base_dir).unwrap();
        std::fs::write(base_dir.join("Manifest.json"), "{}").unwrap();
        std::env::set_var("AOS_MODEL_PATH", base_dir.to_string_lossy().to_string());
        let aos_path = temp.path().join("adapter.aos");
        std::fs::write(&aos_path, b"adapter-bytes").unwrap();

        let mut db = adapteros_db::factory::DbFactory::create_in_memory()
            .await
            .expect("db");
        db.migrate().await.expect("migrate");

        let service = TrainingService::with_db(db.clone(), temp.path().to_path_buf());
        let mut job = TrainingJob::new(
            "job-export".into(),
            "adapter-export".into(),
            TrainingConfig::default(),
        );
        job.status = TrainingJobStatus::Completed;
        job.adapter_id = Some("adapter-export".into());
        job.aos_path = Some(aos_path.to_string_lossy().to_string());
        job.manifest_base_model = Some("base-model-x".into());
        job.package_hash_b3 = Some("hash123".into());
        job.tenant_id = Some("tenant-test".into());
        {
            let mut jobs = service.jobs.write().await;
            jobs.insert(job.id.clone(), job);
        }

        let updated = service
            .export_coreml_for_job("job-export")
            .await
            .expect("export");

        assert_eq!(updated.coreml_export_status.as_deref(), Some("succeeded"));
        assert!(updated.coreml_fused_package_hash.is_some());

        let pair = db
            .get_coreml_fusion_pair("tenant-test", "base-model-x", "adapter-export")
            .await
            .expect("pair lookup");
        assert!(pair.is_some(), "fusion pair should be recorded");

        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("AOS_ALLOW_COREML_EXPORT_STUB");
    }

    #[tokio::test]
    async fn cpu_training_succeeds_without_gpu_init() {
        std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
        let jobs = Arc::new(RwLock::new(HashMap::new()));
        let job_id = "cpu-job".to_string();
        let config = cpu_only_config();
        let job = TrainingJob::new(job_id.clone(), "adapter-cpu".to_string(), config.clone());
        jobs.write().await.insert(job_id.clone(), job);

        let result = run_training_job(
            jobs.clone(),
            job_id.clone(),
            "adapter-cpu".to_string(),
            config,
            None,
            false,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            no_package_actions(),
            None,
            Arc::new(AtomicBool::new(false)),
        )
        .await;

        assert!(result.is_ok(), "CPU training should succeed");
        let jobs_guard = jobs.read().await;
        let finished = jobs_guard.get(&job_id).unwrap();
        assert_eq!(finished.status, TrainingJobStatus::Completed);
        assert_eq!(finished.require_gpu, Some(false));
        assert_eq!(
            finished
                .backend
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase(),
            "cpu"
        );
        std::env::remove_var("AOS_FORCE_GPU_BACKEND");
    }

    #[tokio::test]
    async fn coreml_preference_records_fallback_reason() {
        std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
        let jobs = Arc::new(RwLock::new(HashMap::new()));
        let job_id = "coreml-pref-job".to_string();
        let mut config = cpu_only_config();
        config.preferred_backend = Some(TrainingBackendKind::CoreML);
        config.coreml_training_fallback = Some(TrainingBackendKind::Mlx);
        let job = TrainingJob::new(
            job_id.clone(),
            "adapter-coreml-pref".to_string(),
            config.clone(),
        );
        jobs.write().await.insert(job_id.clone(), job);

        let result = run_training_job(
            jobs.clone(),
            job_id.clone(),
            "adapter-coreml-pref".to_string(),
            config,
            None,
            false,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            no_package_actions(),
            None,
            Arc::new(AtomicBool::new(false)),
        )
        .await;

        assert!(
            result.is_ok(),
            "CoreML request should fall back deterministically"
        );
        let jobs_guard = jobs.read().await;
        let finished = jobs_guard.get(&job_id).unwrap();
        let reason = finished.backend_reason.clone().unwrap_or_default();
        assert!(
            reason.contains("coreml_training_not_supported"),
            "expected backend_reason to mention CoreML fallback, got: {reason}"
        );
        assert_eq!(
            finished
                .backend
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase(),
            "cpu"
        );
        std::env::remove_var("AOS_FORCE_GPU_BACKEND");
    }

    #[tokio::test]
    async fn gpu_optional_falls_back_when_init_fails() {
        std::env::set_var("AOS_FORCE_GPU_BACKEND", "metal");
        let temp_model = new_test_tempdir();
        let model_path = temp_model.path().join("model.safetensors");
        std::fs::write(&model_path, b"not-a-real-model").unwrap();
        std::env::set_var("AOS_MODEL_PATH", temp_model.path());

        let jobs = Arc::new(RwLock::new(HashMap::new()));
        let job_id = "gpu-fallback-job".to_string();
        let mut config = cpu_only_config();
        config.preferred_backend = Some(TrainingBackendKind::Metal);
        let job = TrainingJob::new(
            job_id.clone(),
            "adapter-gpu-fallback".to_string(),
            config.clone(),
        );
        jobs.write().await.insert(job_id.clone(), job);

        let result = run_training_job(
            jobs.clone(),
            job_id.clone(),
            "adapter-gpu-fallback".to_string(),
            config,
            None,
            false,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            no_package_actions(),
            None,
            Arc::new(AtomicBool::new(false)),
        )
        .await;

        assert!(
            result.is_ok(),
            "Optional GPU init should fall back to CPU even if GPU init fails"
        );
        let jobs_guard = jobs.read().await;
        let finished = jobs_guard.get(&job_id).unwrap();
        assert_eq!(finished.status, TrainingJobStatus::Completed);
        assert_eq!(
            finished
                .backend
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase(),
            "cpu"
        );

        std::env::remove_var("AOS_FORCE_GPU_BACKEND");
        std::env::remove_var("AOS_MODEL_PATH");
    }

    #[tokio::test]
    async fn gpu_required_errors_when_unavailable() {
        std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
        let jobs = Arc::new(RwLock::new(HashMap::new()));
        let job_id = "gpu-required-job".to_string();
        let mut config = gpu_required_config();
        config.epochs = 1;
        let job = TrainingJob::new(
            job_id.clone(),
            "adapter-gpu-required".to_string(),
            config.clone(),
        );
        jobs.write().await.insert(job_id.clone(), job);

        let result = run_training_job(
            jobs.clone(),
            job_id.clone(),
            "adapter-gpu-required".to_string(),
            config,
            None,
            false,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            no_package_actions(),
            None,
            Arc::new(AtomicBool::new(false)),
        )
        .await;

        assert!(result.is_err(), "GPU-required job should error without GPU");
        let jobs_guard = jobs.read().await;
        let failed = jobs_guard.get(&job_id).unwrap();
        assert_eq!(failed.status, TrainingJobStatus::Failed);
        assert_eq!(failed.require_gpu, Some(true));
        std::env::remove_var("AOS_FORCE_GPU_BACKEND");
    }
}
