//! Training job database operations
//!
//! Implements CRUD operations for repository training jobs.
//! Evidence: migrations/0013_git_repository_integration.sql:25-40
//! Pattern: Database schema for training jobs

use crate::training_jobs_kv::{TrainingJobKv, TrainingJobKvRepository, TrainingMetricKv};
use crate::{Db, KvBackend};
use adapteros_core::{AosError, Result};
use adapteros_types::training::DatasetVersionSelection;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{info, warn};
use crate::new_id;
use adapteros_id::IdPrefix;

const DEFAULT_SEED_MODE: &str = "best_effort";
const DATASET_LINK_CONFLICT_MARKER: &str = "already linked to job";

fn is_dataset_link_conflict(err: &AosError) -> bool {
    matches!(err, AosError::Validation(msg) if msg.contains(DATASET_LINK_CONFLICT_MARKER))
}

/// Training job record from database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TrainingJobRecord {
    pub id: String,
    pub repo_id: String,
    #[sqlx(default)]
    pub target_branch: Option<String>,
    #[sqlx(default)]
    pub base_version_id: Option<String>,
    #[sqlx(default)]
    pub draft_version_id: Option<String>,
    #[sqlx(default)]
    pub code_commit_sha: Option<String>,
    pub training_config_json: String,
    pub status: String,
    pub progress_json: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub created_by: String,
    // Fields from migration 0050
    pub adapter_name: Option<String>,
    pub template_id: Option<String>,
    pub created_at: Option<String>,
    pub metadata_json: Option<String>,
    // Field from migration 0099
    pub config_hash_b3: Option<String>,
    // Fields from migration 0100 - provenance tracking
    pub dataset_id: Option<String>,
    /// Correlation ID for tracing across dataset -> training -> inference
    #[sqlx(default)]
    pub correlation_id: Option<String>,
    /// Dataset version ID for provenance tracking and trust gating
    /// Links to training_dataset_versions.id for version-specific lineage
    /// Evidence: migrations/0177_dataset_trust_gates.sql:67
    #[sqlx(default)]
    pub dataset_version_id: Option<String>,
    pub base_model_id: Option<String>,
    pub collection_id: Option<String>,
    pub tenant_id: Option<String>,
    pub build_id: Option<String>,
    pub source_documents_json: Option<String>,
    #[sqlx(default)]
    pub synthetic_mode: Option<i64>,
    #[sqlx(default)]
    pub data_lineage_mode: Option<String>,
    // Fields from migration 0133 - retry tracking
    /// Whether this job can be retried (determined by error type)
    pub retryable: Option<i64>,
    /// ID of the original job this is a retry of (for retry chain tracking)
    pub retry_of_job_id: Option<String>,
    // Fields from migration 0073 - unified train-to-chat pipeline
    /// Stack ID created from this training job (if post_actions.create_stack = true)
    pub stack_id: Option<String>,
    /// Adapter ID created from this training job
    pub adapter_id: Option<String>,
    /// BLAKE3 hash of the packaged adapter weights (.aos)
    #[sqlx(default)]
    pub weights_hash_b3: Option<String>,
    /// Filesystem path to the packaged .aos artifact
    #[sqlx(default)]
    pub artifact_path: Option<String>,
    #[sqlx(default)]
    pub produced_version_id: Option<String>,
    #[sqlx(default)]
    pub hyperparameters_json: Option<String>,
    #[sqlx(default)]
    pub data_spec_json: Option<String>,
    #[sqlx(default)]
    pub metrics_snapshot_id: Option<String>,
    // Fields from migration 0247 - deterministic run tracking
    /// Whether this job explicitly requested deterministic execution
    #[sqlx(default)]
    pub is_deterministic_run: Option<i64>,
    /// BLAKE3 hash of the global seed used for all RNG sources
    #[sqlx(default)]
    pub global_seed_hex: Option<String>,
    /// Full determinism configuration snapshot (seed sources, overrides, etc.)
    #[sqlx(default)]
    pub determinism_config_json: Option<String>,
    /// Seed derivation strategy: best_effort, strict, disabled
    #[sqlx(default)]
    pub seed_mode: Option<String>,
    // Fields from migration 0253 - API contract alignment
    /// Adapter category (code, framework, codebase, docs, domain)
    #[sqlx(default)]
    pub category: Option<String>,
    /// Human-readable description
    #[sqlx(default)]
    pub description: Option<String>,
    /// Programming language for code adapters
    #[sqlx(default)]
    pub language: Option<String>,
    /// Symbol targets JSON array for code adapters
    #[sqlx(default)]
    pub symbol_targets_json: Option<String>,
    /// Framework ID for framework adapters
    #[sqlx(default)]
    pub framework_id: Option<String>,
    /// Framework version
    #[sqlx(default)]
    pub framework_version: Option<String>,
    /// Marketing/operational tier (micro/standard/max)
    #[sqlx(default)]
    pub lora_tier: Option<String>,
    /// LoRA strength multiplier
    #[sqlx(default)]
    pub lora_strength: Option<f64>,
    /// Adapter scope (project, tenant)
    #[sqlx(default)]
    pub scope: Option<String>,
    /// API patterns JSON array for framework adapters
    #[sqlx(default)]
    pub api_patterns_json: Option<String>,
    /// Repository scope for codebase adapters
    #[sqlx(default)]
    pub repo_scope: Option<String>,
    /// File patterns to include JSON array
    #[sqlx(default)]
    pub file_patterns_json: Option<String>,
    /// File patterns to exclude JSON array
    #[sqlx(default)]
    pub exclude_patterns_json: Option<String>,
    /// Actual backend used (coreml, metal, mlx, cpu)
    #[sqlx(default)]
    pub backend: Option<String>,
    /// Reason for backend selection
    #[sqlx(default)]
    pub backend_reason: Option<String>,
    /// Backend device identifier
    #[sqlx(default)]
    pub backend_device: Option<String>,
    /// BLAKE3 hash of combined dataset manifests
    #[sqlx(default)]
    pub dataset_hash_b3: Option<String>,
}

/// Training metric record from database
///
/// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
/// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
/// Pattern: Time-series metrics for training jobs
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TrainingMetricRow {
    pub id: String,
    pub training_job_id: String,
    pub step: i64,
    pub epoch: Option<i64>,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_timestamp: Option<String>,
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

/// Training configuration parameters for hash computation
///
/// Evidence: migrations/0099_training_config_hash.sql
/// Pattern: Deterministic hash of training hyperparameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfigParams {
    pub rank: usize,
    pub alpha: f32,
    pub learning_rate: f32,
    pub batch_size: usize,
    pub epochs: usize,
    pub hidden_dim: usize,
}

/// Training job dataset link record from database
///
/// Represents a many-to-many relationship between training jobs and datasets,
/// allowing a single training job to use multiple datasets with different roles.
///
/// Evidence: migrations/0241_training_job_datasets.sql
/// Pattern: Junction table for training job to dataset linking
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TrainingJobDatasetLink {
    pub id: String,
    pub training_job_id: String,
    pub dataset_id: String,
    pub dataset_version_id: Option<String>,
    /// Role of this dataset in the training job (e.g., 'primary', 'validation', 'supplementary')
    pub role: String,
    /// Ordering for datasets when order matters (e.g., curriculum learning)
    pub ordinal: i32,
    /// Weight for this dataset in the training mix
    pub weight: Option<f64>,
    /// Snapshot of dataset hash at link time for reproducibility
    pub hash_b3_at_link: Option<String>,
    pub tenant_id: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
    pub metadata_json: Option<String>,
}

/// Parameters for linking a dataset to a training job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkDatasetParams {
    /// Dataset ID to link
    pub dataset_id: String,
    /// Optional specific version ID (if not provided, uses latest)
    pub dataset_version_id: Option<String>,
    /// Role of this dataset (default: "primary")
    pub role: Option<String>,
    /// Ordering for this dataset (default: 0)
    pub ordinal: Option<i32>,
    /// Weight for this dataset in the mix (default: 1.0)
    pub weight: Option<f64>,
    /// Who is creating this link
    pub created_by: Option<String>,
    /// Additional metadata as JSON
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DataSpecDatasetVersion {
    dataset_version_id: String,
    #[serde(default)]
    weight: Option<f64>,
}

fn parse_dataset_version_selections(data_spec_json: Option<&str>) -> Vec<DataSpecDatasetVersion> {
    let raw = match data_spec_json {
        Some(raw) if !raw.trim().is_empty() => raw,
        _ => return Vec::new(),
    };

    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut selections = Vec::new();
    let mut seen = HashSet::new();

    let mut push_selection = |id: &str, weight: Option<f64>| {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return;
        }
        if seen.insert(trimmed.to_string()) {
            selections.push(DataSpecDatasetVersion {
                dataset_version_id: trimmed.to_string(),
                weight,
            });
        }
    };

    match &value {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::Array(items)) = map.get("dataset_version_ids") {
                for item in items {
                    if let Ok(selection) =
                        serde_json::from_value::<DataSpecDatasetVersion>(item.clone())
                    {
                        push_selection(&selection.dataset_version_id, selection.weight);
                        continue;
                    }
                    if let Some(id) = item.as_str() {
                        push_selection(id, None);
                        continue;
                    }
                    if let Some(obj) = item.as_object() {
                        if let Some(id) = obj.get("dataset_version_id").and_then(|v| v.as_str()) {
                            let weight = obj.get("weight").and_then(|v| v.as_f64());
                            push_selection(id, weight);
                        }
                    }
                }
            } else if let Some(id) = map.get("dataset_version_id").and_then(|v| v.as_str()) {
                push_selection(id, None);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Ok(selection) =
                    serde_json::from_value::<DataSpecDatasetVersion>(item.clone())
                {
                    push_selection(&selection.dataset_version_id, selection.weight);
                    continue;
                }
                if let Some(id) = item.as_str() {
                    push_selection(id, None);
                    continue;
                }
                if let Some(obj) = item.as_object() {
                    if let Some(id) = obj.get("dataset_version_id").and_then(|v| v.as_str()) {
                        let weight = obj.get("weight").and_then(|v| v.as_f64());
                        push_selection(id, weight);
                    }
                }
            }
        }
        _ => {}
    }

    selections
}

/// Compute BLAKE3 hash of training configuration for reproducibility tracking
///
/// Takes training hyperparameters and produces a deterministic hash that can be used to:
/// - Identify identical training configurations across jobs
/// - Verify reproducibility of training runs
/// - Detect configuration drift
///
/// Evidence: migrations/0099_training_config_hash.sql
/// Pattern: Deterministic hashing per Determinism Policy
///
/// # Arguments
/// * `params` - Training configuration parameters (rank, alpha, learning_rate, batch_size, epochs, hidden_dim)
///
/// # Returns
/// BLAKE3 hash as lowercase hexadecimal string (64 characters)
///
/// # Example
/// ```ignore
/// let params = TrainingConfigParams {
///     rank: 16,
///     alpha: 32.0,
///     learning_rate: 0.001,
///     batch_size: 8,
///     epochs: 3,
///     hidden_dim: 768,
/// };
/// let hash = compute_config_hash(&params);
/// ```
pub fn compute_config_hash(params: &TrainingConfigParams) -> Result<String> {
    // Serialize to deterministic JSON (sorted keys)
    let json = serde_json::to_string(params).map_err(AosError::Serialization)?;

    // Compute BLAKE3 hash
    let mut hasher = Hasher::new();
    hasher.update(json.as_bytes());
    let hash = hasher.finalize();

    // Return as hex string
    Ok(hash.to_hex().to_string())
}

impl Db {
    fn get_training_job_kv_repo(&self) -> Option<TrainingJobKvRepository> {
        if (self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv())
            && self.has_kv_backend()
        {
            self.kv_backend()
                .map(|kv| TrainingJobKvRepository::new(kv.backend().clone()))
        } else {
            None
        }
    }

    #[allow(dead_code)]
    fn record_to_kv(record: &TrainingJobRecord) -> TrainingJobKv {
        TrainingJobKv {
            id: record.id.clone(),
            repo_id: record.repo_id.clone(),
            target_branch: record.target_branch.clone(),
            base_version_id: record.base_version_id.clone(),
            draft_version_id: record.draft_version_id.clone(),
            code_commit_sha: record.code_commit_sha.clone(),
            training_config_json: record.training_config_json.clone(),
            status: record.status.clone(),
            progress_json: record.progress_json.clone(),
            started_at: record.started_at.clone(),
            completed_at: record.completed_at.clone(),
            created_by: record.created_by.clone(),
            adapter_name: record.adapter_name.clone(),
            template_id: record.template_id.clone(),
            created_at: record.created_at.clone(),
            metadata_json: record.metadata_json.clone(),
            config_hash_b3: record.config_hash_b3.clone(),
            dataset_id: record.dataset_id.clone(),
            correlation_id: record.correlation_id.clone(),
            dataset_version_id: record.dataset_version_id.clone(),
            base_model_id: record.base_model_id.clone(),
            collection_id: record.collection_id.clone(),
            tenant_id: record.tenant_id.clone(),
            build_id: record.build_id.clone(),
            source_documents_json: record.source_documents_json.clone(),
            synthetic_mode: record.synthetic_mode.map(|v| v != 0),
            data_lineage_mode: record.data_lineage_mode.clone(),
            retryable: record.retryable,
            retry_of_job_id: record.retry_of_job_id.clone(),
            stack_id: record.stack_id.clone(),
            adapter_id: record.adapter_id.clone(),
            weights_hash_b3: record.weights_hash_b3.clone(),
            artifact_path: record.artifact_path.clone(),
            produced_version_id: record.produced_version_id.clone(),
            hyperparameters_json: record.hyperparameters_json.clone(),
            data_spec_json: record.data_spec_json.clone(),
            metrics_snapshot_id: record.metrics_snapshot_id.clone(),
            is_deterministic_run: record.is_deterministic_run.map(|v| v != 0),
            global_seed_hex: record.global_seed_hex.clone(),
            determinism_config_json: record.determinism_config_json.clone(),
            seed_mode: record.seed_mode.clone(),
            // Fields from migration 0253
            category: record.category.clone(),
            description: record.description.clone(),
            language: record.language.clone(),
            symbol_targets_json: record.symbol_targets_json.clone(),
            framework_id: record.framework_id.clone(),
            framework_version: record.framework_version.clone(),
            lora_tier: record.lora_tier.clone(),
            lora_strength: record.lora_strength,
            scope: record.scope.clone(),
            api_patterns_json: record.api_patterns_json.clone(),
            repo_scope: record.repo_scope.clone(),
            file_patterns_json: record.file_patterns_json.clone(),
            exclude_patterns_json: record.exclude_patterns_json.clone(),
            backend: record.backend.clone(),
            backend_reason: record.backend_reason.clone(),
            backend_device: record.backend_device.clone(),
            dataset_hash_b3: record.dataset_hash_b3.clone(),
        }
    }

    fn kv_to_record(kv: &TrainingJobKv) -> TrainingJobRecord {
        TrainingJobRecord {
            id: kv.id.clone(),
            repo_id: kv.repo_id.clone(),
            target_branch: kv.target_branch.clone(),
            base_version_id: kv.base_version_id.clone(),
            draft_version_id: kv.draft_version_id.clone(),
            code_commit_sha: kv.code_commit_sha.clone(),
            training_config_json: kv.training_config_json.clone(),
            status: kv.status.clone(),
            progress_json: kv.progress_json.clone(),
            started_at: kv.started_at.clone(),
            completed_at: kv.completed_at.clone(),
            created_by: kv.created_by.clone(),
            adapter_name: kv.adapter_name.clone(),
            template_id: kv.template_id.clone(),
            created_at: kv.created_at.clone(),
            metadata_json: kv.metadata_json.clone(),
            config_hash_b3: kv.config_hash_b3.clone(),
            dataset_id: kv.dataset_id.clone(),
            correlation_id: kv.correlation_id.clone(),
            dataset_version_id: kv.dataset_version_id.clone(),
            base_model_id: kv.base_model_id.clone(),
            collection_id: kv.collection_id.clone(),
            tenant_id: kv.tenant_id.clone(),
            build_id: kv.build_id.clone(),
            source_documents_json: kv.source_documents_json.clone(),
            synthetic_mode: kv.synthetic_mode.map(|v| if v { 1 } else { 0 }),
            data_lineage_mode: kv.data_lineage_mode.clone(),
            retryable: kv.retryable,
            retry_of_job_id: kv.retry_of_job_id.clone(),
            stack_id: kv.stack_id.clone(),
            adapter_id: kv.adapter_id.clone(),
            weights_hash_b3: kv.weights_hash_b3.clone(),
            artifact_path: kv.artifact_path.clone(),
            produced_version_id: kv.produced_version_id.clone(),
            hyperparameters_json: kv.hyperparameters_json.clone(),
            data_spec_json: kv.data_spec_json.clone(),
            metrics_snapshot_id: kv.metrics_snapshot_id.clone(),
            is_deterministic_run: kv.is_deterministic_run.map(|v| if v { 1 } else { 0 }),
            global_seed_hex: kv.global_seed_hex.clone(),
            determinism_config_json: kv.determinism_config_json.clone(),
            seed_mode: kv.seed_mode.clone(),
            // Fields from migration 0253
            category: kv.category.clone(),
            description: kv.description.clone(),
            language: kv.language.clone(),
            symbol_targets_json: kv.symbol_targets_json.clone(),
            framework_id: kv.framework_id.clone(),
            framework_version: kv.framework_version.clone(),
            lora_tier: kv.lora_tier.clone(),
            lora_strength: kv.lora_strength,
            scope: kv.scope.clone(),
            api_patterns_json: kv.api_patterns_json.clone(),
            repo_scope: kv.repo_scope.clone(),
            file_patterns_json: kv.file_patterns_json.clone(),
            exclude_patterns_json: kv.exclude_patterns_json.clone(),
            backend: kv.backend.clone(),
            backend_reason: kv.backend_reason.clone(),
            backend_device: kv.backend_device.clone(),
            dataset_hash_b3: kv.dataset_hash_b3.clone(),
        }
    }

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
        let id = new_id(IdPrefix::Job);
        let progress = TrainingProgress {
            progress_pct: 0.0,
            current_epoch: 0,
            total_epochs: 3, // Default from TrainingConfig
            current_loss: 0.0,
            learning_rate: 0.001,
            tokens_per_second: 0.0,
            error_message: None,
        };
        let progress_json = serde_json::to_string(&progress).map_err(AosError::Serialization)?;

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "INSERT INTO repository_training_jobs 
             (id, repo_id, training_config_json, status, progress_json, created_by,
              is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(repo_id)
            .bind(training_config_json)
            .bind("pending")
            .bind(&progress_json)
            .bind(created_by)
            .bind(0)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(DEFAULT_SEED_MODE)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for create_training_job".to_string(),
            ));
        }

        if let Some(repo) = self.get_training_job_kv_repo() {
            let job = TrainingJobKv {
                id: id.clone(),
                repo_id: repo_id.to_string(),
                target_branch: None,
                base_version_id: None,
                draft_version_id: None,
                code_commit_sha: None,
                training_config_json: training_config_json.to_string(),
                status: "pending".to_string(),
                progress_json: progress_json.clone(),
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: None,
                created_by: created_by.to_string(),
                adapter_name: None,
                template_id: None,
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                metadata_json: None,
                config_hash_b3: None,
                dataset_id: None,
                correlation_id: None,
                dataset_version_id: None,
                base_model_id: None,
                collection_id: None,
                tenant_id: None,
                build_id: None,
                source_documents_json: None,
                synthetic_mode: Some(false),
                data_lineage_mode: None,
                retryable: None,
                retry_of_job_id: None,
                stack_id: None,
                adapter_id: None,
                weights_hash_b3: None,
                artifact_path: None,
                produced_version_id: None,
                hyperparameters_json: None,
                data_spec_json: None,
                metrics_snapshot_id: None,
                is_deterministic_run: Some(false),
                global_seed_hex: None,
                determinism_config_json: None,
                seed_mode: Some(DEFAULT_SEED_MODE.to_string()),
                category: None,
                description: None,
                language: None,
                symbol_targets_json: None,
                framework_id: None,
                framework_version: None,
                lora_tier: None,
                lora_strength: None,
                scope: None,
                api_patterns_json: None,
                repo_scope: None,
                file_patterns_json: None,
                exclude_patterns_json: None,
                backend: None,
                backend_reason: None,
                backend_device: None,
                dataset_hash_b3: None,
            };
            if let Err(e) = repo.put_job(&job).await {
                self.record_kv_write_fallback("training_jobs.create");
                warn!(error = %e, job_id = %id, "KV write failed for training job");
            }
        }

        info!(
            target: "audit.training",
            job_id = %id,
            repo_id = %repo_id,
            created_by = %created_by,
            "Training job created"
        );
        Ok(id)
    }

    /// Get a training job by ID
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn get_training_job(&self, job_id: &str) -> Result<Option<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let job = repo
                    .get_job(job_id)
                    .await?
                    .map(|kv| Self::kv_to_record(&kv));
                if !self.storage_mode().sql_fallback_enabled() || job.is_some() {
                    return Ok(job);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(None);
        }

        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(job)
    }

    /// Get a training job by adapter ID (tenant-scoped)
    pub async fn get_training_job_by_adapter(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<Option<TrainingJobRecord>> {
        if !self.storage_mode().read_from_sql() {
            return Ok(None);
        }

        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE adapter_id = ? AND tenant_id = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

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
        let progress_json = serde_json::to_string(progress).map_err(AosError::Serialization)?;

        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| job.progress_json = progress_json.clone())
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_progress");
                warn!(error = %e, job_id = %job_id, "KV update progress failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
             SET progress_json = ?
             WHERE id = ?",
            )
            .bind(&progress_json)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_progress".to_string(),
            ));
        }

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

        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.status = status.to_string();
                    job.completed_at = completed_at.clone();
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_status");
                warn!(error = %e, job_id = %job_id, "KV update status failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs 
             SET status = ?, completed_at = ? 
             WHERE id = ?",
            )
            .bind(status)
            .bind(completed_at)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_status".to_string(),
            ));
        }

        let is_terminal = status == "completed" || status == "failed";
        info!(
            target: "audit.training",
            job_id = %job_id,
            new_status = %status,
            is_terminal = %is_terminal,
            "Training job status updated"
        );
        Ok(())
    }

    /// Link a training job to the produced adapter version (and optional metrics snapshot).
    pub async fn set_training_produced_version(
        &self,
        job_id: &str,
        version_id: &str,
        metrics_snapshot_id: Option<&str>,
    ) -> Result<()> {
        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.produced_version_id = Some(version_id.to_string());
                    if let Some(metrics) = metrics_snapshot_id {
                        job.metrics_snapshot_id = Some(metrics.to_string());
                    }
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.set_produced_version");
                warn!(error = %e, job_id = %job_id, "KV update produced_version failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                r#"
                UPDATE repository_training_jobs
                SET produced_version_id = ?, metrics_snapshot_id = COALESCE(?, metrics_snapshot_id)
                WHERE id = ?
                "#,
            )
            .bind(version_id)
            .bind(metrics_snapshot_id)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for set_training_produced_version".to_string(),
            ));
        }

        Ok(())
    }

    /// List training jobs for a repository
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn list_training_jobs(&self, repo_id: &str) -> Result<Vec<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let mut jobs = repo
                    .list_jobs_for_repo(repo_id, usize::MAX)
                    .await?
                    .into_iter()
                    .map(|kv| Self::kv_to_record(&kv))
                    .collect::<Vec<_>>();
                jobs.sort_by(|a, b| {
                    b.started_at
                        .cmp(&a.started_at)
                        .then_with(|| a.id.cmp(&b.id))
                });
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(jobs);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE repo_id = ?
             ORDER BY started_at DESC",
        )
        .bind(repo_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(jobs)
    }

    /// List training jobs by status
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    ///
    /// # Performance Warning
    /// This is a cross-tenant query that scans repository_training_jobs by status.
    /// It should only be used for admin dashboards or background workers.
    /// For tenant-scoped lists, use `list_training_jobs_for_tenant`.
    pub async fn list_training_jobs_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let mut jobs = repo
                    .list_jobs_by_status(status, usize::MAX)
                    .await?
                    .into_iter()
                    .map(|kv| Self::kv_to_record(&kv))
                    .collect::<Vec<_>>();
                jobs.sort_by(|a, b| {
                    b.started_at
                        .cmp(&a.started_at)
                        .then_with(|| a.id.cmp(&b.id))
                });
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(jobs);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE status = ?
             ORDER BY started_at DESC",
        )
        .bind(status)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(jobs)
    }

    /// List training jobs for a specific tenant
    ///
    /// Filters training jobs by tenant_id.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    ///
    /// # Returns
    /// * Vector of training jobs belonging to the specified tenant,
    /// * ordered by start time (newest first)
    pub async fn list_training_jobs_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let mut jobs = repo
                    .list_jobs_for_tenant(tenant_id)
                    .await?
                    .into_iter()
                    .map(|kv| Self::kv_to_record(&kv))
                    .collect::<Vec<_>>();
                jobs.sort_by(|a, b| {
                    b.started_at
                        .cmp(&a.started_at)
                        .then_with(|| a.id.cmp(&b.id))
                });
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(jobs);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        // Optimization: Use direct tenant_id filter (supported since migration 0100)
        // Optimized with INDEXED BY for migration 0210
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT rtj.id, rtj.repo_id, rtj.target_branch, rtj.base_version_id, rtj.draft_version_id,
                    rtj.code_commit_sha, rtj.training_config_json, rtj.status, rtj.progress_json,
                    rtj.started_at, rtj.completed_at, rtj.created_by, rtj.adapter_name,
                    rtj.template_id, rtj.created_at, rtj.metadata_json, rtj.config_hash_b3,
                    rtj.dataset_id, rtj.dataset_version_id, rtj.base_model_id, rtj.collection_id, rtj.tenant_id,
                    rtj.build_id, rtj.source_documents_json,
                    rtj.synthetic_mode, rtj.data_lineage_mode,
                    rtj.retryable, rtj.retry_of_job_id, rtj.stack_id, rtj.adapter_id,
                    rtj.weights_hash_b3, rtj.artifact_path, rtj.produced_version_id,
                    rtj.hyperparameters_json, rtj.data_spec_json, rtj.metrics_snapshot_id,
                    rtj.is_deterministic_run, rtj.global_seed_hex, rtj.determinism_config_json, rtj.seed_mode
             FROM repository_training_jobs rtj INDEXED BY idx_training_jobs_tenant_status_created_adapter
             WHERE rtj.tenant_id = ?
             ORDER BY rtj.created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to list training jobs for tenant: {}", e))
        })?;

        Ok(jobs)
    }

    /// Delete a training job
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql:25-40
    /// Pattern: Database schema for training jobs
    pub async fn delete_training_job(&self, job_id: &str) -> Result<()> {
        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo.delete_job(job_id).await {
                self.record_kv_write_fallback("training_jobs.delete");
                warn!(error = %e, job_id = %job_id, "KV delete training job failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query("DELETE FROM repository_training_jobs WHERE id = ?")
                .bind(job_id)
                .execute(self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for delete_training_job".to_string(),
            ));
        }

        info!(target: "audit.training", job_id = %job_id, "Training job deleted");
        Ok(())
    }

    /// Update training job with artifact metadata
    ///
    /// Called after training completes successfully to record:
    /// - artifact_path: Path to the packaged .aos file
    /// - adapter_id: Registered adapter identifier
    /// - weights_hash_b3: BLAKE3 hash of the trained weights
    /// - extra metadata fields (e.g., CoreML export status) are merged if provided
    ///
    /// Evidence: migrations/0050_training_jobs_extensions.sql:18-19
    /// Pattern: metadata_json column for artifact tracking
    pub async fn update_training_job_artifact(
        &self,
        job_id: &str,
        artifact_path: &str,
        adapter_id: &str,
        weights_hash_b3: &str,
        extra_metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let mut metadata = serde_json::json!({
            "artifact_path": artifact_path,
            "adapter_id": adapter_id,
            "weights_hash_b3": weights_hash_b3
        });
        if let Some(extra) = extra_metadata {
            if let (Some(base), Some(additional)) = (metadata.as_object_mut(), extra.as_object()) {
                for (k, v) in additional {
                    base.insert(k.clone(), v.clone());
                }
            }
        }
        let metadata_json = serde_json::to_string(&metadata).map_err(AosError::Serialization)?;

        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.metadata_json = Some(metadata_json.clone());
                    job.adapter_id = Some(adapter_id.to_string());
                    job.weights_hash_b3 = Some(weights_hash_b3.to_string());
                    job.artifact_path = Some(artifact_path.to_string());
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_artifact");
                warn!(error = %e, job_id = %job_id, "KV update artifact failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
             SET metadata_json = ?,
                 adapter_id = ?,
                 weights_hash_b3 = ?,
                 artifact_path = ?
             WHERE id = ?",
            )
            .bind(&metadata_json)
            .bind(adapter_id)
            .bind(weights_hash_b3)
            .bind(artifact_path)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_job_artifact".to_string(),
            ));
        }

        Ok(())
    }

    /// Update training job with adapter name
    ///
    /// Records the adapter_name field (from migration 0050)
    pub async fn update_training_job_adapter_name(
        &self,
        job_id: &str,
        adapter_name: &str,
    ) -> Result<()> {
        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.adapter_name = Some(adapter_name.to_string())
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_adapter_name");
                warn!(error = %e, job_id = %job_id, "KV update adapter name failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
             SET adapter_name = ?
             WHERE id = ?",
            )
            .bind(adapter_name)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_job_adapter_name".to_string(),
            ));
        }

        Ok(())
    }

    /// Get training job by adapter ID
    ///
    /// Looks up the training job that produced a given adapter by searching
    /// the metadata_json field for the adapter_id.
    ///
    /// Evidence: migrations/0050_training_jobs_extensions.sql:18-19
    /// Pattern: metadata_json column for artifact tracking
    pub async fn get_training_job_by_adapter_id(
        &self,
        adapter_id: &str,
    ) -> Result<Option<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let mut found: Option<TrainingJobRecord> = None;
                for job in repo.list_all_jobs().await? {
                    if job
                        .metadata_json
                        .as_ref()
                        .map(|m| m.contains(adapter_id))
                        .unwrap_or(false)
                    {
                        found = Some(Self::kv_to_record(&job));
                        break;
                    }
                }
                if found.is_some() && !self.storage_mode().sql_fallback_enabled() {
                    return Ok(found);
                }
                if found.is_some() {
                    return Ok(found);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(None);
        }

        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE metadata_json LIKE ?",
        )
        .bind(format!("%\"adapter_id\":\"{}\"%", adapter_id))
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(job)
    }

    /// Update training job with config hash
    ///
    /// Evidence: migrations/0099_training_config_hash.sql
    /// Pattern: Training reproducibility tracking
    pub async fn update_training_job_config_hash(
        &self,
        job_id: &str,
        config_hash_b3: &str,
    ) -> Result<()> {
        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.config_hash_b3 = Some(config_hash_b3.to_string())
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_config_hash");
                warn!(error = %e, job_id = %job_id, "KV update config hash failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
             SET config_hash_b3 = ?
             WHERE id = ?",
            )
            .bind(config_hash_b3)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_job_config_hash".to_string(),
            ));
        }

        Ok(())
    }

    /// Update training job determinism tracking fields.
    ///
    /// Records whether determinism was explicitly requested along with
    /// the global seed hash and determinism configuration snapshot.
    pub async fn update_training_job_determinism(
        &self,
        job_id: &str,
        is_deterministic_run: bool,
        global_seed_hex: Option<&str>,
        determinism_config_json: Option<&str>,
        seed_mode: Option<&str>,
    ) -> Result<()> {
        if let Some(repo) = self.get_training_job_kv_repo() {
            let global_seed_hex = global_seed_hex.map(str::to_string);
            let determinism_config_json = determinism_config_json.map(str::to_string);
            let seed_mode = seed_mode.map(str::to_string);

            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.is_deterministic_run = Some(is_deterministic_run);
                    if let Some(ref seed_hex) = global_seed_hex {
                        job.global_seed_hex = Some(seed_hex.clone());
                    }
                    if let Some(ref config_json) = determinism_config_json {
                        job.determinism_config_json = Some(config_json.clone());
                    }
                    if let Some(ref mode) = seed_mode {
                        job.seed_mode = Some(mode.clone());
                    }
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_determinism");
                warn!(error = %e, job_id = %job_id, "KV update determinism failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
                 SET is_deterministic_run = ?,
                     global_seed_hex = COALESCE(?, global_seed_hex),
                     determinism_config_json = COALESCE(?, determinism_config_json),
                     seed_mode = COALESCE(?, seed_mode)
                 WHERE id = ?",
            )
            .bind(if is_deterministic_run { 1 } else { 0 })
            .bind(global_seed_hex)
            .bind(determinism_config_json)
            .bind(seed_mode)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_job_determinism".to_string(),
            ));
        }

        Ok(())
    }

    /// Find training jobs with the same configuration hash
    ///
    /// Evidence: migrations/0099_training_config_hash.sql
    /// Pattern: Reproducibility verification
    ///
    /// # Returns
    /// List of training jobs that used identical training configuration
    pub async fn find_jobs_by_config_hash(
        &self,
        config_hash_b3: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let mut jobs: Vec<TrainingJobRecord> = repo
                    .list_all_jobs()
                    .await?
                    .into_iter()
                    .filter(|j| j.config_hash_b3.as_deref() == Some(config_hash_b3))
                    .map(|kv| Self::kv_to_record(&kv))
                    .collect();
                jobs.sort_by(|a, b| {
                    b.started_at
                        .cmp(&a.started_at)
                        .then_with(|| a.id.cmp(&b.id))
                });
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(jobs);
                }
                if !jobs.is_empty() {
                    return Ok(jobs);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE config_hash_b3 = ?
             ORDER BY started_at DESC",
        )
        .bind(config_hash_b3)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(jobs)
    }

    /// Create training job with full provenance tracking
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Evidence: migrations/0133_training_retryable.sql (retry_of_job_id column)
    /// Pattern: Complete provenance chain from dataset to adapter
    ///
    /// # Arguments
    /// * `job_id` - Optional job ID (if None, generates UUID v7)
    /// * `repo_id` - Repository ID for training context
    /// * `training_config_json` - JSON-serialized training configuration
    /// * `created_by` - User who initiated the training
    /// * `dataset_id` - Optional dataset ID for provenance tracking
    /// * `correlation_id` - Optional correlation ID for tracing
    /// * `base_model_id` - Optional base model ID
    /// * `collection_id` - Optional document collection ID
    /// * `tenant_id` - Tenant isolation identifier
    /// * `build_id` - Build/commit identifier for reproducibility
    /// * `source_documents_json` - JSON list of source document IDs
    /// * `retry_of_job_id` - Optional ID of original job this is a retry of (for retry chain tracking)
    /// * `synthetic_mode` - Whether the job explicitly opted into synthetic data
    /// * `data_lineage_mode` - Lineage quality for this job
    /// * `dataset_version_id` - Optional dataset version ID for reproducibility tracking
    /// * `dataset_version_ids` - Optional dataset version selections for provenance linking
    pub async fn create_training_job_with_provenance(
        &self,
        job_id: Option<&str>,
        repo_id: &str,
        training_config_json: &str,
        created_by: &str,
        dataset_id: Option<&str>,
        correlation_id: Option<&str>,
        dataset_version_id: Option<&str>,
        dataset_version_ids: Option<&[DatasetVersionSelection]>,
        base_model_id: Option<&str>,
        collection_id: Option<&str>,
        tenant_id: Option<&str>,
        build_id: Option<&str>,
        source_documents_json: Option<&str>,
        retry_of_job_id: Option<&str>,
        target_branch: Option<&str>,
        base_version_id: Option<&str>,
        draft_version_id: Option<&str>,
        code_commit_sha: Option<&str>,
        data_spec_json: Option<&str>,
        synthetic_mode: bool,
        data_lineage_mode: Option<&str>,
    ) -> Result<String> {
        let id = job_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| new_id(IdPrefix::Job));
        let progress = TrainingProgress {
            progress_pct: 0.0,
            current_epoch: 0,
            total_epochs: 3,
            current_loss: 0.0,
            learning_rate: 0.001,
            tokens_per_second: 0.0,
            error_message: None,
        };
        let progress_json = serde_json::to_string(&progress).map_err(AosError::Serialization)?;
        let primary_selection_id = dataset_version_ids.and_then(|versions| {
            versions.iter().find_map(|selection| {
                let trimmed = selection.dataset_version_id.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
        });
        let resolved_dataset_id = if let Some(dataset_id) = dataset_id {
            Some(dataset_id.to_string())
        } else if let Some(version_id) = dataset_version_id {
            self.resolve_dataset_id_from_version(version_id, tenant_id)
                .await?
        } else if let Some(ref selection_id) = primary_selection_id {
            self.resolve_dataset_id_from_version(selection_id, tenant_id)
                .await?
        } else {
            None
        };
        let mut resolved_dataset_version_id = dataset_version_id.map(|s| s.to_string());
        if resolved_dataset_version_id.is_none() {
            if let Some(ref selection_id) = primary_selection_id {
                resolved_dataset_version_id = Some(selection_id.clone());
            }
        }
        if resolved_dataset_version_id.is_none() {
            if let Some(dataset_id) = resolved_dataset_id.as_deref() {
                if let Some(version) = self
                    .get_latest_dataset_version_for_dataset(dataset_id)
                    .await?
                {
                    resolved_dataset_version_id = Some(version.id);
                }
            }
        }
        let resolved_dataset_version_id = resolved_dataset_version_id.as_deref();

        if self.storage_mode().write_to_sql() {
            sqlx::query(
            "INSERT INTO repository_training_jobs
             (id, repo_id, training_config_json, status, progress_json, created_by,
             dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
             retry_of_job_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
             data_spec_json, synthetic_mode, data_lineage_mode, produced_version_id,
             is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(repo_id)
            .bind(training_config_json)
            .bind("pending")
            .bind(&progress_json)
            .bind(created_by)
            .bind(resolved_dataset_id.as_deref())
            .bind(correlation_id)
            .bind(resolved_dataset_version_id)
            .bind(base_model_id)
            .bind(collection_id)
            .bind(tenant_id)
            .bind(build_id)
            .bind(source_documents_json)
            .bind(retry_of_job_id)
            .bind(target_branch)
            .bind(base_version_id)
            .bind(draft_version_id)
            .bind(code_commit_sha)
            .bind(data_spec_json)
            .bind(if synthetic_mode { 1 } else { 0 })
            .bind(data_lineage_mode)
            .bind(None::<String>)
            .bind(0)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(DEFAULT_SEED_MODE)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for create_training_job_with_provenance".to_string(),
            ));
        }

        if let Some(repo) = self.get_training_job_kv_repo() {
            let job = TrainingJobKv {
                id: id.clone(),
                repo_id: repo_id.to_string(),
                target_branch: target_branch.map(|s| s.to_string()),
                base_version_id: base_version_id.map(|s| s.to_string()),
                draft_version_id: draft_version_id.map(|s| s.to_string()),
                code_commit_sha: code_commit_sha.map(|s| s.to_string()),
                training_config_json: training_config_json.to_string(),
                status: "pending".to_string(),
                progress_json: progress_json.clone(),
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: None,
                created_by: created_by.to_string(),
                adapter_name: None,
                template_id: None,
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                metadata_json: None,
                config_hash_b3: None,
                dataset_id: resolved_dataset_id.clone(),
                correlation_id: correlation_id.map(|s| s.to_string()),
                dataset_version_id: resolved_dataset_version_id.map(|s| s.to_string()),
                base_model_id: base_model_id.map(|s| s.to_string()),
                collection_id: collection_id.map(|s| s.to_string()),
                tenant_id: tenant_id.map(|s| s.to_string()),
                build_id: build_id.map(|s| s.to_string()),
                source_documents_json: source_documents_json.map(|s| s.to_string()),
                synthetic_mode: Some(synthetic_mode),
                data_lineage_mode: data_lineage_mode.map(|s| s.to_string()),
                retryable: None,
                retry_of_job_id: retry_of_job_id.map(|s| s.to_string()),
                stack_id: None,
                adapter_id: None,
                weights_hash_b3: None,
                artifact_path: None,
                produced_version_id: None,
                hyperparameters_json: None,
                data_spec_json: data_spec_json.map(|s| s.to_string()),
                metrics_snapshot_id: None,
                is_deterministic_run: Some(false),
                global_seed_hex: None,
                determinism_config_json: None,
                seed_mode: Some(DEFAULT_SEED_MODE.to_string()),
                category: None,
                description: None,
                language: None,
                symbol_targets_json: None,
                framework_id: None,
                framework_version: None,
                lora_tier: None,
                lora_strength: None,
                scope: None,
                api_patterns_json: None,
                repo_scope: None,
                file_patterns_json: None,
                exclude_patterns_json: None,
                backend: None,
                backend_reason: None,
                backend_device: None,
                dataset_hash_b3: None,
            };
            if let Err(e) = repo.put_job(&job).await {
                self.record_kv_write_fallback("training_jobs.create_provenance");
                warn!(error = %e, job_id = %id, "KV write failed for training job provenance");
            }
        }

        if self.storage_mode().write_to_sql() {
            if let Some(dataset_id) = resolved_dataset_id.as_deref() {
                let link_params = LinkDatasetParams {
                    dataset_id: dataset_id.to_string(),
                    dataset_version_id: resolved_dataset_version_id.map(|s| s.to_string()),
                    role: Some("primary".to_string()),
                    ordinal: Some(0),
                    weight: None,
                    created_by: Some(created_by.to_string()),
                    metadata_json: None,
                };
                if let Err(err) = self
                    .link_dataset_to_training_job(&id, &link_params, tenant_id)
                    .await
                {
                    if !is_dataset_link_conflict(&err) {
                        return Err(err);
                    }
                }
            }

            let mut selections = Vec::new();
            let mut seen_versions = HashSet::new();

            if let Some(version_ids) = dataset_version_ids {
                for selection in version_ids {
                    let trimmed = selection.dataset_version_id.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if seen_versions.insert(trimmed.to_string()) {
                        selections.push(DataSpecDatasetVersion {
                            dataset_version_id: trimmed.to_string(),
                            weight: Some(selection.weight as f64),
                        });
                    }
                }
            }

            for selection in parse_dataset_version_selections(data_spec_json) {
                if seen_versions.insert(selection.dataset_version_id.clone()) {
                    selections.push(selection);
                }
            }

            self.link_datasets_from_selections(
                &id,
                selections,
                resolved_dataset_id.as_deref(),
                created_by,
                tenant_id,
            )
            .await?;
        }

        Ok(id)
    }

    #[allow(dead_code)]
    async fn link_datasets_from_data_spec(
        &self,
        job_id: &str,
        data_spec_json: Option<&str>,
        primary_dataset_id: Option<&str>,
        created_by: &str,
        tenant_id: Option<&str>,
    ) -> Result<()> {
        let selections = parse_dataset_version_selections(data_spec_json);
        self.link_datasets_from_selections(
            job_id,
            selections,
            primary_dataset_id,
            created_by,
            tenant_id,
        )
        .await
    }

    async fn link_datasets_from_selections(
        &self,
        job_id: &str,
        selections: Vec<DataSpecDatasetVersion>,
        primary_dataset_id: Option<&str>,
        created_by: &str,
        tenant_id: Option<&str>,
    ) -> Result<()> {
        if selections.is_empty() {
            return Ok(());
        }

        let mut linked_dataset_ids = HashSet::new();
        if let Some(primary_id) = primary_dataset_id {
            linked_dataset_ids.insert(primary_id.to_string());
        }

        let mut ordinal = if linked_dataset_ids.is_empty() { 0 } else { 1 };
        let mut has_primary = !linked_dataset_ids.is_empty();

        for selection in selections {
            let dataset_id = self
                .resolve_dataset_id_from_version(&selection.dataset_version_id, tenant_id)
                .await?;
            let Some(dataset_id) = dataset_id else {
                continue;
            };

            if linked_dataset_ids.contains(&dataset_id) {
                self.update_training_job_dataset_link_version(
                    job_id,
                    &dataset_id,
                    Some(selection.dataset_version_id.as_str()),
                    tenant_id,
                )
                .await?;
                continue;
            }

            let role = if has_primary {
                "supplementary"
            } else {
                "primary"
            };
            let link_params = LinkDatasetParams {
                dataset_id: dataset_id.clone(),
                dataset_version_id: Some(selection.dataset_version_id.clone()),
                role: Some(role.to_string()),
                ordinal: Some(ordinal),
                weight: selection.weight,
                created_by: Some(created_by.to_string()),
                metadata_json: None,
            };

            if let Err(err) = self
                .link_dataset_to_training_job(job_id, &link_params, tenant_id)
                .await
            {
                if is_dataset_link_conflict(&err) {
                    self.update_training_job_dataset_link_version(
                        job_id,
                        &dataset_id,
                        Some(selection.dataset_version_id.as_str()),
                        tenant_id,
                    )
                    .await?;
                } else {
                    return Err(err);
                }
            }

            linked_dataset_ids.insert(dataset_id);
            has_primary = true;
            ordinal += 1;
        }

        Ok(())
    }

    /// Get adapters trained on a specific dataset
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Pattern: Reverse lookup for provenance queries
    pub async fn get_adapters_trained_on_dataset(&self, dataset_id: &str) -> Result<Vec<String>> {
        let adapter_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT a.id
             FROM adapters a
             JOIN repository_training_jobs tj ON a.training_job_id = tj.id
             WHERE tj.dataset_id = ?",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(adapter_ids.into_iter().map(|(id,)| id).collect())
    }

    /// Update adapter's training_job_id link
    ///
    /// Evidence: migrations/0101_adapter_training_job_link.sql
    /// Pattern: Link adapter back to training job after training completes
    pub async fn update_adapter_training_job_id(
        &self,
        adapter_id: &str,
        training_job_id: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE adapters SET training_job_id = ? WHERE id = ?")
            .bind(training_job_id)
            .bind(adapter_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    /// Insert a single training metric
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
    /// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
    /// Pattern: Record fine-grained training metrics for monitoring and debugging
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `step` - Training step number (e.g., 1200)
    /// * `epoch` - Optional epoch number (e.g., 3)
    /// * `metric_name` - Name of the metric (e.g., "loss", "learning_rate", "tokens_per_second")
    /// * `value` - Metric value
    ///
    /// # Returns
    /// ID of the inserted metric record
    pub async fn insert_training_metric(
        &self,
        job_id: &str,
        step: i64,
        epoch: Option<i64>,
        metric_name: &str,
        value: f64,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Job);

        sqlx::query(
            "INSERT INTO repository_training_metrics
             (id, training_job_id, step, epoch, metric_name, metric_value)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(job_id)
        .bind(step)
        .bind(epoch)
        .bind(metric_name)
        .bind(value)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert training metric: {}", e)))?;

        Ok(id)
    }

    /// Insert multiple training metrics in a batch (for efficiency)
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
    /// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
    /// Pattern: Batch insertion for efficient metric recording
    ///
    /// # Arguments
    /// * `metrics` - Vector of training metric records to insert
    ///
    /// # Implementation Note
    /// This uses a transaction to ensure atomicity and improve performance
    /// when inserting multiple metrics at once (e.g., all metrics from a training step).
    pub async fn insert_training_metrics_batch(&self, metrics: &[TrainingMetricRow]) -> Result<()> {
        if metrics.is_empty() {
            return Ok(());
        }

        // Use a transaction for batch insertion
        let mut tx = self.begin_write_tx().await?;

        for metric in metrics {
            sqlx::query(
                "INSERT INTO repository_training_metrics
                 (id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&metric.id)
            .bind(&metric.training_job_id)
            .bind(metric.step)
            .bind(metric.epoch)
            .bind(&metric.metric_name)
            .bind(metric.metric_value)
            .bind(&metric.metric_timestamp)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to insert training metric in batch: {}", e))
            })?;
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit metrics batch: {}", e)))?;

        Ok(())
    }

    /// Get training metrics for a job, optionally filtered by metric name
    ///
    /// Evidence: migrations/0013_git_repository_integration.sql (training_metrics table)
    /// Evidence: migrations/0125_training_metrics_step_epoch.sql (step/epoch columns)
    /// Pattern: Retrieve time-series metrics for visualization and analysis
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `metric_name` - Optional filter for specific metric (e.g., "loss", "learning_rate")
    /// * `limit` - Optional limit on number of records returned
    ///
    /// # Returns
    /// Vector of training metrics ordered by step (oldest first)
    ///
    /// # Examples
    /// ```ignore
    /// // Get all metrics for a job
    /// let metrics = db.get_training_metrics("job-123", None, None).await?;
    ///
    /// // Get only loss metrics, limited to last 100 steps
    /// let loss = db.get_training_metrics("job-123", Some("loss"), Some(100)).await?;
    /// ```
    pub async fn get_training_metrics(
        &self,
        job_id: &str,
        metric_name: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<TrainingMetricRow>> {
        let query = if let Some(name) = metric_name {
            if let Some(lim) = limit {
                sqlx::query_as::<_, TrainingMetricRow>(
                    "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                     FROM repository_training_metrics
                     WHERE training_job_id = ? AND metric_name = ?
                     ORDER BY step ASC
                     LIMIT ?",
                )
                .bind(job_id)
                .bind(name)
                .bind(lim)
            } else {
                sqlx::query_as::<_, TrainingMetricRow>(
                    "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                     FROM repository_training_metrics
                     WHERE training_job_id = ? AND metric_name = ?
                     ORDER BY step ASC",
                )
                .bind(job_id)
                .bind(name)
            }
        } else if let Some(lim) = limit {
            sqlx::query_as::<_, TrainingMetricRow>(
                "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                 FROM repository_training_metrics
                 WHERE training_job_id = ?
                 ORDER BY step ASC
                 LIMIT ?",
            )
            .bind(job_id)
            .bind(lim)
        } else {
            sqlx::query_as::<_, TrainingMetricRow>(
                "SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
                 FROM repository_training_metrics
                 WHERE training_job_id = ?
                 ORDER BY step ASC",
            )
            .bind(job_id)
        };

        let metrics = query
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch training metrics: {}", e)))?;

        Ok(metrics)
    }

    /// Update the retryable flag on a training job
    ///
    /// Evidence: migrations/0124_training_retryable.sql
    /// Pattern: Mark failed jobs as retryable or non-retryable based on error type
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `retryable` - Whether the job can be retried (true for OOM/timeout, false for invalid config)
    ///
    /// # Implementation Note
    /// The retryable flag is used to filter failed jobs that can be safely retried
    /// from those that will fail again due to configuration issues.
    /// SQLite stores booleans as INTEGER (0 = false, 1 = true).
    pub async fn update_training_job_retryable(&self, job_id: &str, retryable: bool) -> Result<()> {
        let retryable_int = if retryable { 1 } else { 0 };

        sqlx::query(
            "UPDATE repository_training_jobs
             SET retryable = ?
             WHERE id = ?",
        )
        .bind(retryable_int)
        .bind(job_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to update training job retryable flag: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Get retry chain for a training job
    ///
    /// Returns all jobs that are retries of the given job ID (direct or transitive).
    /// Useful for displaying retry history in the UI.
    ///
    /// Evidence: migrations/0133_training_retryable.sql (retry_of_job_id column)
    /// Pattern: Traverse retry chain for audit and visualization
    ///
    /// # Arguments
    /// * `original_job_id` - The original job ID to find retries of
    ///
    /// # Returns
    /// Vector of training jobs that are retries of the original, ordered by creation time (newest first)
    pub async fn get_retry_chain(&self, original_job_id: &str) -> Result<Vec<TrainingJobRecord>> {
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE retry_of_job_id = ?
             ORDER BY started_at DESC",
        )
        .bind(original_job_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get retry chain for job {}: {}", original_job_id, e))
        })?;

        Ok(jobs)
    }

    /// Update retry_of_job_id for a training job
    ///
    /// Evidence: migrations/0133_training_retryable.sql (retry_of_job_id column)
    /// Pattern: Link retry job to its original for audit trail
    ///
    /// # Arguments
    /// * `job_id` - The retry job ID
    /// * `original_job_id` - The original job this is a retry of
    pub async fn update_training_job_retry_of(
        &self,
        job_id: &str,
        original_job_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE repository_training_jobs
             SET retry_of_job_id = ?
             WHERE id = ?",
        )
        .bind(original_job_id)
        .bind(job_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update retry_of_job_id: {}", e)))?;

        Ok(())
    }

    /// Update training job with stack_id and adapter_id after successful training
    ///
    /// Called by orchestrator after stack creation to persist the result IDs
    /// for the chat_bootstrap endpoint.
    ///
    /// Evidence: unified train-to-chat pipeline
    /// Evidence: migrations/0073_training_job_stack_adapter_ids.sql
    /// Pattern: Link training job to its output artifacts (adapter + stack)
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `stack_id` - Optional stack ID created from this training job
    /// * `adapter_id` - Optional adapter ID created from this training job
    ///
    /// # Notes
    /// This method is called when `post_actions.create_stack = true` to record
    /// the full provenance chain from training job to ready-to-use stack.
    pub async fn update_training_job_result_ids(
        &self,
        job_id: &str,
        stack_id: Option<&str>,
        adapter_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE repository_training_jobs
             SET stack_id = ?, adapter_id = ?
             WHERE id = ?",
        )
        .bind(stack_id)
        .bind(adapter_id)
        .bind(job_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update training job result IDs: {}", e))
        })?;

        Ok(())
    }

    /// Update training job priority (stored in metadata_json)
    ///
    /// Priority ranges from 0 (lowest) to 100 (highest), default is 50.
    /// Higher priority jobs are scheduled before lower priority ones.
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `tenant_id` - Tenant ID for isolation validation
    /// * `priority` - Priority value (0-100)
    ///
    /// # Returns
    /// Error if job not found or doesn't belong to tenant
    pub async fn update_training_job_priority(
        &self,
        job_id: &str,
        tenant_id: &str,
        priority: i32,
    ) -> Result<()> {
        // First verify the job exists and belongs to the tenant
        let job = self
            .get_training_job(job_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Training job not found: {}", job_id)))?;

        // Validate tenant isolation
        if job.tenant_id.as_deref() != Some(tenant_id) {
            return Err(AosError::PolicyViolation(format!(
                "Job {} does not belong to tenant {}",
                job_id, tenant_id
            )));
        }

        // Parse existing metadata or create new
        let mut metadata: serde_json::Value = job
            .metadata_json
            .as_ref()
            .and_then(|m| serde_json::from_str(m).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        // Update priority
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert("priority".to_string(), serde_json::json!(priority));
        }

        let metadata_json = serde_json::to_string(&metadata).map_err(AosError::Serialization)?;

        sqlx::query(
            "UPDATE repository_training_jobs
             SET metadata_json = ?
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(&metadata_json)
        .bind(job_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update training job priority: {}", e))
        })?;

        Ok(())
    }

    /// Get the priority of a training job (from metadata_json)
    ///
    /// Returns the priority value (0-100) or 50 if not set
    pub async fn get_training_job_priority(&self, job_id: &str) -> Result<i32> {
        let job = self
            .get_training_job(job_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Training job not found: {}", job_id)))?;

        let priority = job
            .metadata_json
            .as_ref()
            .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
            .and_then(|v| v.get("priority")?.as_i64())
            .map(|p| p as i32)
            .unwrap_or(50); // Default priority

        Ok(priority)
    }

    // ============================================================================
    // Dataset Linking Operations
    // ============================================================================

    async fn resolve_dataset_id_from_version(
        &self,
        dataset_version_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<Option<String>> {
        if let Some(tenant_id) = tenant_id {
            if let Some(version) = self
                .get_training_dataset_version_routed(tenant_id, dataset_version_id)
                .await?
            {
                return Ok(Some(version.dataset_id));
            }
        }

        if self.storage_mode().read_from_sql() {
            if let Some(version) = self
                .get_training_dataset_version(dataset_version_id)
                .await?
            {
                return Ok(Some(version.dataset_id));
            }
        }

        Ok(None)
    }

    async fn resolve_dataset_hash_snapshot(
        &self,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
    ) -> Result<Option<String>> {
        if let Some(version_id) = dataset_version_id {
            if let Some(version) = self.get_training_dataset_version(version_id).await? {
                return Ok(Some(version.hash_b3));
            }
        }

        if let Some(dataset) = self.get_training_dataset(dataset_id).await? {
            return Ok(Some(dataset.hash_b3));
        }

        Ok(None)
    }

    async fn link_primary_dataset_for_job(
        &self,
        job_id: &str,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<()> {
        if !self.storage_mode().write_to_sql() {
            return Ok(());
        }

        let tenant_id = if let Some(id) = tenant_id {
            Some(id.to_string())
        } else {
            self.get_training_job(job_id)
                .await?
                .and_then(|job| job.tenant_id)
        };
        let link_params = LinkDatasetParams {
            dataset_id: dataset_id.to_string(),
            dataset_version_id: dataset_version_id.map(|s| s.to_string()),
            role: Some("primary".to_string()),
            ordinal: Some(0),
            weight: None,
            created_by: None,
            metadata_json: None,
        };
        if let Err(err) = self
            .link_dataset_to_training_job(job_id, &link_params, tenant_id.as_deref())
            .await
        {
            if is_dataset_link_conflict(&err) {
                self.update_training_job_dataset_link_version(
                    job_id,
                    dataset_id,
                    dataset_version_id,
                    tenant_id.as_deref(),
                )
                .await?;
            } else {
                return Err(err);
            }
        }

        Ok(())
    }

    async fn update_training_job_dataset_link_version(
        &self,
        job_id: &str,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<()> {
        if !self.storage_mode().write_to_sql() {
            return Ok(());
        }

        let hash_b3_at_link = self
            .resolve_dataset_hash_snapshot(dataset_id, dataset_version_id)
            .await?;

        sqlx::query(
            "UPDATE training_job_datasets
             SET dataset_version_id = ?,
                 hash_b3_at_link = ?,
                 tenant_id = COALESCE(?, tenant_id)
             WHERE training_job_id = ? AND dataset_id = ?",
        )
        .bind(dataset_version_id)
        .bind(&hash_b3_at_link)
        .bind(tenant_id)
        .bind(job_id)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update training job dataset link: {}", e))
        })?;

        Ok(())
    }

    /// Link a training job to a dataset and optionally a dataset version
    ///
    /// Updates the dataset_id and dataset_version_id fields for provenance tracking.
    /// This establishes the relationship between the training job and the dataset
    /// it was trained on.
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Evidence: migrations/0177_dataset_trust_gates.sql
    /// Pattern: Dataset-to-job provenance chain
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `dataset_id` - Dataset identifier to link
    /// * `dataset_version_id` - Optional specific version of the dataset
    pub async fn link_training_job_to_dataset(
        &self,
        job_id: &str,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
    ) -> Result<()> {
        let mut resolved_version_id = dataset_version_id.map(|s| s.to_string());
        if resolved_version_id.is_none() {
            if let Some(version) = self
                .get_latest_dataset_version_for_dataset(dataset_id)
                .await?
            {
                resolved_version_id = Some(version.id);
            }
        }
        let resolved_version_id = resolved_version_id.as_deref();

        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.dataset_id = Some(dataset_id.to_string());
                    job.dataset_version_id = resolved_version_id.map(|s| s.to_string());
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.link_dataset");
                warn!(error = %e, job_id = %job_id, "KV update dataset link failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
                 SET dataset_id = ?, dataset_version_id = ?
                 WHERE id = ?",
            )
            .bind(dataset_id)
            .bind(resolved_version_id)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to link training job to dataset: {}", e))
            })?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for link_training_job_to_dataset".to_string(),
            ));
        }

        self.link_primary_dataset_for_job(job_id, dataset_id, resolved_version_id, None)
            .await?;

        Ok(())
    }

    /// List training jobs for a specific dataset
    ///
    /// Returns all training jobs that used the specified dataset for training.
    /// Useful for understanding dataset usage and impact analysis.
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Pattern: Reverse lookup for dataset provenance
    ///
    /// # Arguments
    /// * `dataset_id` - Dataset identifier to query
    ///
    /// # Returns
    /// Vector of training jobs that used this dataset, ordered by creation time (newest first)
    pub async fn list_training_jobs_by_dataset(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let mut jobs: Vec<TrainingJobRecord> = repo
                    .list_all_jobs()
                    .await?
                    .into_iter()
                    .filter(|j| j.dataset_id.as_deref() == Some(dataset_id))
                    .map(|kv| Self::kv_to_record(&kv))
                    .collect();
                jobs.sort_by(|a, b| {
                    b.started_at
                        .cmp(&a.started_at)
                        .then_with(|| a.id.cmp(&b.id))
                });
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(jobs);
                }
                if !jobs.is_empty() {
                    return Ok(jobs);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE dataset_id = ?
             ORDER BY started_at DESC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list training jobs for dataset {}: {}",
                dataset_id, e
            ))
        })?;

        Ok(jobs)
    }

    /// List training jobs for a specific dataset version
    ///
    /// Returns all training jobs that used the specified dataset version for training.
    /// Provides more precise provenance tracking than dataset-level queries.
    ///
    /// Evidence: migrations/0177_dataset_trust_gates.sql
    /// Pattern: Version-specific provenance chain
    ///
    /// # Arguments
    /// * `dataset_version_id` - Dataset version identifier to query
    ///
    /// # Returns
    /// Vector of training jobs that used this dataset version, ordered by creation time (newest first)
    pub async fn list_training_jobs_by_dataset_version(
        &self,
        dataset_version_id: &str,
    ) -> Result<Vec<TrainingJobRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_training_job_kv_repo() {
                let mut jobs: Vec<TrainingJobRecord> = repo
                    .list_all_jobs()
                    .await?
                    .into_iter()
                    .filter(|j| j.dataset_version_id.as_deref() == Some(dataset_version_id))
                    .map(|kv| Self::kv_to_record(&kv))
                    .collect();
                jobs.sort_by(|a, b| {
                    b.started_at
                        .cmp(&a.started_at)
                        .then_with(|| a.id.cmp(&b.id))
                });
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(jobs);
                }
                if !jobs.is_empty() {
                    return Ok(jobs);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, target_branch, base_version_id, draft_version_id, code_commit_sha,
                    training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, correlation_id, dataset_version_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    synthetic_mode, data_lineage_mode,
                    retryable, retry_of_job_id, stack_id, adapter_id, weights_hash_b3, artifact_path, produced_version_id,
                    hyperparameters_json, data_spec_json, metrics_snapshot_id,
                    is_deterministic_run, global_seed_hex, determinism_config_json, seed_mode
             FROM repository_training_jobs
             WHERE dataset_version_id = ?
             ORDER BY started_at DESC",
        )
        .bind(dataset_version_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list training jobs for dataset version {}: {}",
                dataset_version_id, e
            ))
        })?;

        Ok(jobs)
    }

    /// Count training jobs for a dataset (tenant-scoped)
    ///
    /// Returns the number of training jobs that used the specified dataset.
    /// Useful for impact analysis before dataset deletion or modification.
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Pattern: Dataset usage counting for lifecycle management
    ///
    /// # Arguments
    /// * `dataset_id` - Dataset identifier to count jobs for
    /// * `tenant_id` - Tenant identifier for isolation
    ///
    /// # Returns
    /// Count of training jobs using this dataset
    pub async fn count_training_jobs_by_dataset(
        &self,
        dataset_id: &str,
        tenant_id: &str,
    ) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM repository_training_jobs
             WHERE dataset_id = ? AND tenant_id = ?",
        )
        .bind(dataset_id)
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to count training jobs for dataset {}: {}",
                dataset_id, e
            ))
        })?;

        Ok(count.0)
    }

    /// Count active training jobs for a dataset
    ///
    /// Returns the number of active (pending, running, queued) training jobs
    /// using the specified dataset. Used for dataset deletion validation.
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Pattern: Pre-deletion validation check
    ///
    /// # Arguments
    /// * `dataset_id` - Dataset identifier to check
    ///
    /// # Returns
    /// Count of active training jobs using this dataset
    pub async fn count_active_training_jobs_by_dataset(&self, dataset_id: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM repository_training_jobs
             WHERE dataset_id = ? AND status IN ('pending', 'running', 'queued')",
        )
        .bind(dataset_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to count active training jobs for dataset {}: {}",
                dataset_id, e
            ))
        })?;

        Ok(count.0)
    }

    /// Update dataset version ID for a training job
    ///
    /// Updates the dataset_version_id field and fills dataset_id if missing.
    /// Preserves any existing dataset_id.
    /// Used when a specific version is resolved after initial job creation.
    ///
    /// Evidence: migrations/0177_dataset_trust_gates.sql
    /// Pattern: Deferred version resolution
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `dataset_version_id` - Dataset version to link
    pub async fn update_training_job_dataset_version(
        &self,
        job_id: &str,
        dataset_version_id: &str,
    ) -> Result<()> {
        let mut dataset_id_for_link = None;
        let mut tenant_id_for_link = None;
        let mut correlation_id = None;
        if let Some(job) = self.get_training_job(job_id).await? {
            dataset_id_for_link = job.dataset_id;
            tenant_id_for_link = job.tenant_id;
            correlation_id = job.correlation_id;
        }

        if dataset_id_for_link.is_none() {
            dataset_id_for_link = self
                .resolve_dataset_id_from_version(dataset_version_id, tenant_id_for_link.as_deref())
                .await?;
        }
        if correlation_id.is_none() {
            if let Some(dataset_id) = dataset_id_for_link.as_deref() {
                correlation_id = self.get_dataset_correlation_id(dataset_id).await?;
            }
        }

        if let Some(repo) = self.get_training_job_kv_repo() {
            let dataset_id_for_kv = dataset_id_for_link.clone();
            let correlation_for_kv = correlation_id.clone();
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.dataset_version_id = Some(dataset_version_id.to_string());
                    if job.dataset_id.is_none() {
                        job.dataset_id = dataset_id_for_kv.clone();
                    }
                    if let Some(ref corr) = correlation_for_kv {
                        if job.correlation_id.is_none() {
                            job.correlation_id = Some(corr.clone());
                        }
                    }
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_dataset_version");
                warn!(error = %e, job_id = %job_id, "KV update dataset version failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
                 SET dataset_version_id = ?,
                     dataset_id = COALESCE(dataset_id, ?),
                     correlation_id = COALESCE(correlation_id, ?)
                 WHERE id = ?",
            )
            .bind(dataset_version_id)
            .bind(dataset_id_for_link.as_deref())
            .bind(correlation_id.as_deref())
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to update training job dataset version: {}",
                    e
                ))
            })?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_job_dataset_version".to_string(),
            ));
        }

        if let Some(dataset_id) = dataset_id_for_link.as_deref() {
            self.link_primary_dataset_for_job(
                job_id,
                dataset_id,
                Some(dataset_version_id),
                tenant_id_for_link.as_deref(),
            )
            .await?;
        }

        Ok(())
    }

    /// Update training job data specification (scope information)
    ///
    /// Sets the data_spec_json field which contains scope information about
    /// what data was used in training (file paths, patterns, filters, etc.).
    /// This is essential for reproducibility and understanding the training data scope.
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `data_spec_json` - JSON string containing the data specification
    ///
    /// # Returns
    /// Error if job not found or update fails
    pub async fn update_training_job_data_spec(
        &self,
        job_id: &str,
        data_spec_json: &str,
    ) -> Result<()> {
        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.data_spec_json = Some(data_spec_json.to_string());
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.update_data_spec");
                warn!(error = %e, job_id = %job_id, "KV update data spec failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            let result = sqlx::query(
                "UPDATE repository_training_jobs
                 SET data_spec_json = ?
                 WHERE id = ?",
            )
            .bind(data_spec_json)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update training job data spec: {}", e))
            })?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!(
                    "Training job not found: {}",
                    job_id
                )));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_training_job_data_spec".to_string(),
            ));
        }

        Ok(())
    }

    /// Link dataset with full scope information to a training job
    ///
    /// Atomically updates the dataset_id, correlation_id, dataset_version_id, and data_spec_json
    /// fields for complete provenance and scope tracking. This is the preferred
    /// method when linking a dataset to a training job as it ensures all
    /// provenance information is recorded atomically.
    ///
    /// Evidence: migrations/0100_training_provenance.sql
    /// Evidence: migrations/0177_dataset_trust_gates.sql
    /// Pattern: Complete dataset and scope provenance linking
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `dataset_id` - Dataset ID to link
    /// * `dataset_version_id` - Optional specific version of the dataset
    /// * `data_spec_json` - Optional JSON data specification (scope information)
    ///
    /// # Returns
    /// Error if job not found or update fails
    pub async fn link_dataset_with_scope_to_training_job(
        &self,
        job_id: &str,
        dataset_id: &str,
        dataset_version_id: Option<&str>,
        data_spec_json: Option<&str>,
    ) -> Result<()> {
        let correlation_id = self.get_dataset_correlation_id(dataset_id).await?;
        let mut resolved_version_id = dataset_version_id.map(|s| s.to_string());
        if resolved_version_id.is_none() {
            if let Some(version) = self
                .get_latest_dataset_version_for_dataset(dataset_id)
                .await?
            {
                resolved_version_id = Some(version.id);
            }
        }
        let resolved_version_id = resolved_version_id.as_deref();

        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.dataset_id = Some(dataset_id.to_string());
                    job.dataset_version_id = resolved_version_id.map(|s| s.to_string());
                    if let Some(ref corr) = correlation_id {
                        if job.correlation_id.is_none() {
                            job.correlation_id = Some(corr.clone());
                        }
                    }
                    if let Some(spec) = data_spec_json {
                        job.data_spec_json = Some(spec.to_string());
                    }
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.link_dataset_with_scope");
                warn!(error = %e, job_id = %job_id, "KV link dataset with scope failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            let result = sqlx::query(
                "UPDATE repository_training_jobs
                 SET dataset_id = ?,
                     correlation_id = COALESCE(correlation_id, ?),
                     dataset_version_id = COALESCE(?, dataset_version_id),
                     data_spec_json = COALESCE(?, data_spec_json)
                 WHERE id = ?",
            )
            .bind(dataset_id)
            .bind(correlation_id.as_deref())
            .bind(resolved_version_id)
            .bind(data_spec_json)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to link dataset with scope to training job: {}",
                    e
                ))
            })?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!(
                    "Training job not found: {}",
                    job_id
                )));
            }

            let job = self.get_training_job(job_id).await?;
            let (tenant_id, created_by) = if let Some(record) = job {
                (record.tenant_id, Some(record.created_by))
            } else {
                (None, None)
            };
            let tenant_id = tenant_id.as_deref();
            let link_params = LinkDatasetParams {
                dataset_id: dataset_id.to_string(),
                dataset_version_id: resolved_version_id.map(|s| s.to_string()),
                role: Some("primary".to_string()),
                ordinal: Some(0),
                weight: None,
                created_by,
                metadata_json: None,
            };
            if let Err(err) = self
                .link_dataset_to_training_job(job_id, &link_params, tenant_id)
                .await
            {
                if is_dataset_link_conflict(&err) {
                    self.update_training_job_dataset_link_version(
                        job_id,
                        dataset_id,
                        resolved_version_id,
                        tenant_id,
                    )
                    .await?;
                } else {
                    return Err(err);
                }
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for link_dataset_with_scope_to_training_job".to_string(),
            ));
        }

        Ok(())
    }

    /// Get complete dataset and scope information for a training job
    ///
    /// Returns the dataset_id, dataset_version_id, and data_spec_json for a
    /// training job. This is useful for complete provenance queries.
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    ///
    /// # Returns
    /// Tuple of (Option<dataset_id>, Option<dataset_version_id>, Option<data_spec_json>)
    pub async fn get_training_job_dataset_scope(
        &self,
        job_id: &str,
    ) -> Result<(Option<String>, Option<String>, Option<String>)> {
        let job = self.get_training_job(job_id).await?;

        match job {
            Some(j) => Ok((j.dataset_id, j.dataset_version_id, j.data_spec_json)),
            None => Err(AosError::NotFound(format!(
                "Training job not found: {}",
                job_id
            ))),
        }
    }

    // =========================================================================
    // Training Job Dataset Links (Many-to-Many)
    // =========================================================================

    /// Link a dataset to a training job
    ///
    /// Creates a many-to-many relationship between a training job and a dataset,
    /// allowing jobs to use multiple datasets with different roles and configurations.
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    /// Pattern: Junction table for training job to dataset linking
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `params` - Link parameters including dataset_id, role, weight, etc.
    /// * `tenant_id` - Optional tenant ID for isolation
    ///
    /// # Returns
    /// ID of the created link record
    ///
    /// # Errors
    /// Returns error if job or dataset doesn't exist, or if link already exists
    pub async fn link_dataset_to_training_job(
        &self,
        job_id: &str,
        params: &LinkDatasetParams,
        tenant_id: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Job);
        let role = params.role.as_deref().unwrap_or("primary");
        let ordinal = params.ordinal.unwrap_or(0);
        let weight = params.weight.unwrap_or(1.0);

        let mut resolved_version_id = params.dataset_version_id.clone();
        if resolved_version_id.is_none() {
            if let Some(version) = self
                .get_latest_dataset_version_for_dataset(&params.dataset_id)
                .await?
            {
                resolved_version_id = Some(version.id);
            }
        }

        let hash_b3_at_link = self
            .resolve_dataset_hash_snapshot(&params.dataset_id, resolved_version_id.as_deref())
            .await?;

        sqlx::query(
            "INSERT INTO training_job_datasets (
                id, training_job_id, dataset_id, dataset_version_id,
                role, ordinal, weight, hash_b3_at_link, tenant_id,
                created_by, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(job_id)
        .bind(&params.dataset_id)
        .bind(&resolved_version_id)
        .bind(role)
        .bind(ordinal)
        .bind(weight)
        .bind(&hash_b3_at_link)
        .bind(tenant_id)
        .bind(&params.created_by)
        .bind(&params.metadata_json)
        .execute(self.pool())
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                AosError::Validation(format!(
                    "Dataset {} is already linked to job {}",
                    params.dataset_id, job_id
                ))
            } else {
                AosError::Database(format!("Failed to link dataset to training job: {}", e))
            }
        })?;

        Ok(id)
    }

    /// Link multiple datasets to a training job in a single transaction
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    /// Pattern: Batch insertion for efficiency
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `params_list` - List of link parameters for each dataset
    /// * `tenant_id` - Optional tenant ID for isolation
    ///
    /// # Returns
    /// Vector of created link IDs
    pub async fn link_datasets_to_training_job(
        &self,
        job_id: &str,
        params_list: &[LinkDatasetParams],
        tenant_id: Option<&str>,
    ) -> Result<Vec<String>> {
        if params_list.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.begin_write_tx().await?;
        let mut link_ids = Vec::with_capacity(params_list.len());

        for (idx, params) in params_list.iter().enumerate() {
            let id = new_id(IdPrefix::Job);
            let role = params.role.as_deref().unwrap_or("primary");
            // Use provided ordinal or fall back to index position
            let ordinal = params.ordinal.unwrap_or(idx as i32);
            let weight = params.weight.unwrap_or(1.0);
            let mut resolved_version_id = params.dataset_version_id.clone();
            if resolved_version_id.is_none() {
                if let Some(version) = self
                    .get_latest_dataset_version_for_dataset(&params.dataset_id)
                    .await?
                {
                    resolved_version_id = Some(version.id);
                }
            }
            let hash_b3_at_link = self
                .resolve_dataset_hash_snapshot(&params.dataset_id, resolved_version_id.as_deref())
                .await?;

            sqlx::query(
                "INSERT INTO training_job_datasets (
                    id, training_job_id, dataset_id, dataset_version_id,
                    role, ordinal, weight, hash_b3_at_link, tenant_id, created_by, metadata_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(job_id)
            .bind(&params.dataset_id)
            .bind(&resolved_version_id)
            .bind(role)
            .bind(ordinal)
            .bind(weight)
            .bind(&hash_b3_at_link)
            .bind(tenant_id)
            .bind(&params.created_by)
            .bind(&params.metadata_json)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to link dataset to training job: {}", e))
            })?;

            link_ids.push(id);
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit dataset links: {}", e)))?;

        Ok(link_ids)
    }

    /// Unlink a dataset from a training job
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `dataset_id` - Dataset to unlink
    ///
    /// # Returns
    /// True if a link was removed, false if no link existed
    pub async fn unlink_dataset_from_training_job(
        &self,
        job_id: &str,
        dataset_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM training_job_datasets WHERE training_job_id = ? AND dataset_id = ?",
        )
        .bind(job_id)
        .bind(dataset_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to unlink dataset from training job: {}", e))
        })?;

        Ok(result.rows_affected() > 0)
    }

    /// Get all datasets linked to a training job
    ///
    /// Returns datasets in order by role (primary first) then by ordinal.
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    /// Pattern: Query junction table for related datasets
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    ///
    /// # Returns
    /// Vector of dataset links ordered by role and ordinal
    pub async fn get_datasets_for_training_job(
        &self,
        job_id: &str,
    ) -> Result<Vec<TrainingJobDatasetLink>> {
        let links = sqlx::query_as::<_, TrainingJobDatasetLink>(
            "SELECT id, training_job_id, dataset_id, dataset_version_id,
                    role, ordinal, weight, hash_b3_at_link, tenant_id,
                    created_at, created_by, metadata_json
             FROM training_job_datasets
             WHERE training_job_id = ?
             ORDER BY
                CASE role WHEN 'primary' THEN 0 WHEN 'validation' THEN 1 ELSE 2 END,
                ordinal ASC",
        )
        .bind(job_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get datasets for training job: {}", e))
        })?;

        Ok(links)
    }

    /// Get all training jobs that used a specific dataset (via junction table)
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    /// Pattern: Reverse lookup for provenance queries
    ///
    /// # Arguments
    /// * `dataset_id` - Dataset identifier
    ///
    /// # Returns
    /// Vector of training job IDs that used this dataset
    pub async fn get_training_jobs_using_dataset(&self, dataset_id: &str) -> Result<Vec<String>> {
        let job_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT training_job_id
             FROM training_job_datasets
             WHERE dataset_id = ?
             ORDER BY created_at DESC",
        )
        .bind(dataset_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get training jobs for dataset: {}", e))
        })?;

        Ok(job_ids.into_iter().map(|(id,)| id).collect())
    }

    /// Get all training jobs that used a specific dataset version (via junction table)
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    /// Pattern: Version-specific provenance queries
    ///
    /// # Arguments
    /// * `dataset_version_id` - Dataset version identifier
    ///
    /// # Returns
    /// Vector of training job IDs that used this specific version
    pub async fn get_training_jobs_using_dataset_version(
        &self,
        dataset_version_id: &str,
    ) -> Result<Vec<String>> {
        let job_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT training_job_id
             FROM training_job_datasets
             WHERE dataset_version_id = ?
             ORDER BY created_at DESC",
        )
        .bind(dataset_version_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get training jobs for dataset version: {}",
                e
            ))
        })?;

        Ok(job_ids.into_iter().map(|(id,)| id).collect())
    }

    /// Get a specific dataset link by job and dataset IDs
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `dataset_id` - Dataset identifier
    ///
    /// # Returns
    /// The link record if it exists
    pub async fn get_training_job_dataset_link(
        &self,
        job_id: &str,
        dataset_id: &str,
    ) -> Result<Option<TrainingJobDatasetLink>> {
        let link = sqlx::query_as::<_, TrainingJobDatasetLink>(
            "SELECT id, training_job_id, dataset_id, dataset_version_id,
                    role, ordinal, weight, hash_b3_at_link, tenant_id,
                    created_at, created_by, metadata_json
             FROM training_job_datasets
             WHERE training_job_id = ? AND dataset_id = ?",
        )
        .bind(job_id)
        .bind(dataset_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get training job dataset link: {}", e))
        })?;

        Ok(link)
    }

    /// Update the role and weight of a dataset link
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `dataset_id` - Dataset identifier
    /// * `role` - New role for the dataset
    /// * `ordinal` - New ordinal position
    /// * `weight` - New weight value
    pub async fn update_training_job_dataset_link(
        &self,
        job_id: &str,
        dataset_id: &str,
        role: Option<&str>,
        ordinal: Option<i32>,
        weight: Option<f64>,
    ) -> Result<()> {
        // Build dynamic update query based on provided fields
        let mut updates = Vec::new();
        if role.is_some() {
            updates.push("role = ?");
        }
        if ordinal.is_some() {
            updates.push("ordinal = ?");
        }
        if weight.is_some() {
            updates.push("weight = ?");
        }

        if updates.is_empty() {
            return Ok(());
        }

        let query = format!(
            "UPDATE training_job_datasets SET {} WHERE training_job_id = ? AND dataset_id = ?",
            updates.join(", ")
        );

        let mut q = sqlx::query(&query);

        if let Some(r) = role {
            q = q.bind(r);
        }
        if let Some(o) = ordinal {
            q = q.bind(o);
        }
        if let Some(w) = weight {
            q = q.bind(w);
        }

        q.bind(job_id)
            .bind(dataset_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update training job dataset link: {}", e))
            })?;

        Ok(())
    }

    /// Get datasets for a training job filtered by role
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    /// * `role` - Role to filter by (e.g., "primary", "validation", "supplementary")
    ///
    /// # Returns
    /// Vector of dataset links with the specified role
    pub async fn get_datasets_for_training_job_by_role(
        &self,
        job_id: &str,
        role: &str,
    ) -> Result<Vec<TrainingJobDatasetLink>> {
        let links = sqlx::query_as::<_, TrainingJobDatasetLink>(
            "SELECT id, training_job_id, dataset_id, dataset_version_id,
                    role, ordinal, weight, hash_b3_at_link, tenant_id,
                    created_at, created_by, metadata_json
             FROM training_job_datasets
             WHERE training_job_id = ? AND role = ?
             ORDER BY ordinal ASC",
        )
        .bind(job_id)
        .bind(role)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get datasets for training job by role: {}",
                e
            ))
        })?;

        Ok(links)
    }

    /// Count datasets linked to a training job
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    ///
    /// # Returns
    /// Number of datasets linked to the job
    pub async fn count_datasets_for_training_job(&self, job_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM training_job_datasets WHERE training_job_id = ?")
                .bind(job_id)
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to count datasets for training job: {}", e))
                })?;

        Ok(count.0)
    }

    /// Delete all dataset links for a training job
    ///
    /// Typically called when deleting a training job to clean up the junction table.
    /// Note: This is also handled by ON DELETE CASCADE in the FK constraint.
    ///
    /// Evidence: migrations/0241_training_job_datasets.sql
    ///
    /// # Arguments
    /// * `job_id` - Training job identifier
    ///
    /// # Returns
    /// Number of links deleted
    pub async fn delete_all_dataset_links_for_training_job(&self, job_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM training_job_datasets WHERE training_job_id = ?")
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to delete dataset links for training job: {}",
                    e
                ))
            })?;

        Ok(result.rows_affected())
    }

    /// Find orphaned training jobs (running jobs without recent activity)
    ///
    /// Returns jobs that are in "running" status but haven't had any training metrics
    /// recorded within the specified staleness threshold. These jobs likely crashed
    /// or were interrupted without proper cleanup.
    ///
    /// # Arguments
    /// * `staleness_threshold` - Duration after which a running job without activity is considered orphaned
    ///
    /// # Returns
    /// Vector of orphaned training job records
    pub async fn find_orphaned_training_jobs(
        &self,
        staleness_threshold: std::time::Duration,
    ) -> Result<Vec<TrainingJobRecord>> {
        let threshold_seconds = staleness_threshold.as_secs() as i64;
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(threshold_seconds);
        let cutoff_str = cutoff.to_rfc3339();

        // Find running jobs where:
        // 1. No metrics have been recorded in the threshold period, OR
        // 2. No metrics exist at all AND started_at is older than threshold
        let jobs = sqlx::query_as::<_, TrainingJobRecord>(
            r#"
            SELECT j.id, j.repo_id, j.target_branch, j.base_version_id, j.draft_version_id, j.code_commit_sha,
                   j.training_config_json, j.status, j.progress_json,
                   j.started_at, j.completed_at, j.created_by, j.adapter_name, j.template_id,
                   j.created_at, j.metadata_json, j.config_hash_b3,
                   j.dataset_id, j.dataset_version_id, j.base_model_id, j.collection_id, j.tenant_id, j.build_id, j.source_documents_json,
                   j.synthetic_mode, j.data_lineage_mode,
                   j.retryable, j.retry_of_job_id, j.stack_id, j.adapter_id, j.weights_hash_b3, j.artifact_path, j.produced_version_id,
                   j.hyperparameters_json, j.data_spec_json, j.metrics_snapshot_id,
                   j.is_deterministic_run, j.global_seed_hex, j.determinism_config_json, j.seed_mode
            FROM repository_training_jobs j
            WHERE j.status = 'running'
              AND (
                  -- No metrics exist and job started before cutoff
                  (NOT EXISTS (SELECT 1 FROM repository_training_metrics m WHERE m.training_job_id = j.id)
                   AND j.started_at < ?)
                  OR
                  -- Metrics exist but all are older than cutoff
                  (EXISTS (SELECT 1 FROM repository_training_metrics m WHERE m.training_job_id = j.id)
                   AND NOT EXISTS (
                       SELECT 1 FROM repository_training_metrics m
                       WHERE m.training_job_id = j.id
                         AND m.metric_timestamp > ?
                   ))
              )
            ORDER BY j.started_at ASC
            "#,
        )
        .bind(&cutoff_str)
        .bind(&cutoff_str)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to find orphaned training jobs: {}", e)))?;

        Ok(jobs)
    }

    /// Mark a training job as interrupted (for recovery after restart)
    ///
    /// This transitions a job from "running" to "interrupted" status, which allows
    /// it to be retried via the existing retry chain mechanism.
    ///
    /// # Arguments
    /// * `job_id` - The job ID to mark as interrupted
    /// * `reason` - The reason for interruption (e.g., "server_restart", "orphaned_recovery")
    pub async fn mark_training_job_interrupted(&self, job_id: &str, reason: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(repo) = self.get_training_job_kv_repo() {
            if let Err(e) = repo
                .update_job(job_id, |job| {
                    job.status = "interrupted".to_string();
                    job.completed_at = Some(now.clone());
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.mark_interrupted");
                warn!(error = %e, job_id = %job_id, "KV mark interrupted failed");
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
                 SET status = 'interrupted', completed_at = ?
                 WHERE id = ? AND status = 'running'",
            )
            .bind(&now)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for mark_training_job_interrupted".to_string(),
            ));
        }

        info!(
            target: "audit.training",
            job_id = %job_id,
            reason = %reason,
            "Training job marked as interrupted"
        );

        Ok(())
    }

    /// Mark a training job as failed due to being orphaned (stale without progress)
    ///
    /// This transitions a job from "running" to "failed" status with the failure reason
    /// recorded in metadata for post-mortem analysis. Unlike "interrupted", "failed" is
    /// a terminal state that will not be automatically retried.
    ///
    /// ## ANCHOR, AUDIT, RECTIFY
    ///
    /// - **ANCHOR**: Only transitions jobs in "running" state (optimistic locking)
    /// - **AUDIT**: Records failure_reason in metadata_json and logs to audit target
    /// - **RECTIFY**: Terminal "failed" state prevents infinite retry loops for stuck jobs
    ///
    /// # Arguments
    /// * `job_id` - The job ID to mark as failed
    /// * `reason` - The reason for failure (e.g., "stale_no_progress_24h")
    /// * `threshold_hours` - The staleness threshold that triggered this cleanup
    pub async fn mark_training_job_failed_orphaned(
        &self,
        job_id: &str,
        reason: &str,
        threshold_hours: u64,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        // AUDIT: Build metadata with failure reason for post-mortem
        let failure_metadata = serde_json::json!({
            "failure_reason": reason,
            "failure_type": "orphaned",
            "threshold_hours": threshold_hours,
            "marked_failed_at": &now,
        });
        let metadata_str =
            serde_json::to_string(&failure_metadata).map_err(AosError::Serialization)?;

        if let Some(repo) = self.get_training_job_kv_repo() {
            let metadata_clone = metadata_str.clone();
            let now_clone = now.clone();
            if let Err(e) = repo
                .update_job(job_id, move |job| {
                    job.status = "failed".to_string();
                    job.completed_at = Some(now_clone);
                    job.metadata_json = Some(metadata_clone);
                })
                .await
            {
                self.record_kv_write_fallback("training_jobs.mark_failed_orphaned");
                warn!(error = %e, job_id = %job_id, "KV mark failed orphaned failed");
            }
        }

        // ANCHOR: Optimistic locking - only update if still in running state
        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE repository_training_jobs
                 SET status = 'failed', completed_at = ?, metadata_json = ?
                 WHERE id = ? AND status = 'running'",
            )
            .bind(&now)
            .bind(&metadata_str)
            .bind(job_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for mark_training_job_failed_orphaned".to_string(),
            ));
        }

        // AUDIT: Log to audit target for observability
        info!(
            target: "audit.training",
            job_id = %job_id,
            reason = %reason,
            threshold_hours = threshold_hours,
            "Training job marked as failed (orphaned)"
        );

        Ok(())
    }
}
