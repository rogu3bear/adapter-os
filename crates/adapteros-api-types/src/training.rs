//! Training API types for request/response schemas.
//!
//! # Schema Design Notes
//!
//! ## Deprecated Fields
//!
//! The following fields are deprecated and will be removed in version 2.0.0:
//! - `artifact_path` → use `aos_path`
//! - `artifact_hash_b3` → use `package_hash_b3`
//!
//! Both deprecated fields are populated as aliases for backward compatibility.
//!
//! ## Trust State Normalization
//!
//! Trust states are normalized server-side before being returned to clients:
//! - `"warn"` → `"allowed_with_warning"` (legacy format)
//! - `"blocked_regressed"` → `"blocked"` (legacy format)
//! - Unknown values → `"unknown"` (with server warning log)
//!
//! Canonical trust states: `allowed`, `allowed_with_warning`, `blocked`,
//! `needs_approval`, `unknown`.
//!
//! ## Optional Progress Fields
//!
//! The following fields use `Option` to distinguish "no data yet" from "0 value":
//! - `progress_pct`: None for pending jobs, Some(0.0-100.0) for active jobs
//! - `current_epoch`: None for pending jobs, Some(0+) for active jobs
//! - `current_loss`: None for pending jobs, Some(value) when computed
//! - `tokens_per_second`: None for pending jobs, Some(value) when measured
//!
//! ## Metrics Granularity
//!
//! The `TrainingMetricEntry` type represents per-step time-series metrics:
//! - `loss`: Stored per-step in DB
//! - `tokens_processed`: Merged from separate DB metric rows when available
//! - `learning_rate`: Optional - not currently stored per-step in DB (job-level only)
//!
//! For real-time learning rate, query the job's `learning_rate` field instead.

#![allow(deprecated)]

#[cfg(feature = "server")]
use adapteros_core::B3Hash;
#[cfg(feature = "server")]
use adapteros_types::{
    coreml::CoreMLPlacementSpec,
    training::{
        BranchClassification, DataLineageMode,
        DatasetVersionSelection as CoreDatasetVersionSelection, LoraTier, TrainingBackendKind,
        PreprocessingConfig, TrainingBackendPolicy, TrainingConfig, TrainingJob, TrainingTemplate,
        TRAINING_DATA_CONTRACT_VERSION,
    },
};
use adapteros_types::training::TrainingReportV1;
use serde::{Deserialize, Serialize};

use crate::schema_version;

// ===== Deprecation Constants =====

/// Sunset version for deprecated fields.
///
/// Fields marked with this constant will be removed in the specified version.
/// Used for consistent deprecation messaging across the crate.
pub const DEPRECATED_FIELD_SUNSET_VERSION: &str = "2.0.0";

// ===== Core Enums =====

/// Training job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TrainingStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Paused,
}

impl TrainingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrainingStatus::Pending => "pending",
            TrainingStatus::Running => "running",
            TrainingStatus::Completed => "completed",
            TrainingStatus::Failed => "failed",
            TrainingStatus::Cancelled => "cancelled",
            TrainingStatus::Paused => "paused",
        }
    }

    /// Parses a database string into a `TrainingStatus`.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not a recognized status.
    /// This ensures invalid database values are caught at parse time
    /// rather than silently defaulting to `Pending`.
    ///
    /// # Valid Values
    ///
    /// - `"pending"` -> `Pending`
    /// - `"running"` -> `Running`
    /// - `"completed"` -> `Completed`
    /// - `"failed"` -> `Failed`
    /// - `"cancelled"` -> `Cancelled`
    /// - `"paused"` -> `Paused`
    ///
    /// Case is normalized (lowercase comparison).
    pub fn from_db_string(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "paused" => Ok(Self::Paused),
            _ => Err(format!(
                "Unknown TrainingStatus value: '{}'. Valid values: pending, running, completed, failed, cancelled, paused",
                value
            )),
        }
    }
}

impl std::fmt::Display for TrainingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Dataset trust state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    Allowed,
    AllowedWithWarning,
    Blocked,
    NeedsApproval,
    Unknown,
}

impl TrustState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustState::Allowed => "allowed",
            TrustState::AllowedWithWarning => "allowed_with_warning",
            TrustState::Blocked => "blocked",
            TrustState::NeedsApproval => "needs_approval",
            TrustState::Unknown => "unknown",
        }
    }

    pub fn from_db_string(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "allowed" => Self::Allowed,
            "allowed_with_warning" => Self::AllowedWithWarning,
            "blocked" => Self::Blocked,
            "needs_approval" => Self::NeedsApproval,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for TrustState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Dataset source type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DatasetSourceType {
    CodeRepo,
    UploadedFiles,
    Generated,
}

impl DatasetSourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DatasetSourceType::CodeRepo => "code_repo",
            DatasetSourceType::UploadedFiles => "uploaded_files",
            DatasetSourceType::Generated => "generated",
        }
    }

    pub fn from_db_string(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "code_repo" => Self::CodeRepo,
            "uploaded_files" => Self::UploadedFiles,
            "generated" => Self::Generated,
            _ => Self::Generated,
        }
    }
}

impl std::fmt::Display for DatasetSourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

fn default_dataset_weight() -> f32 {
    1.0
}

/// Dataset version selector with optional sampling weight (API surface).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionSelection {
    pub dataset_version_id: String,
    #[serde(default = "default_dataset_weight")]
    pub weight: f32,
}

/// Trust snapshot for a dataset version captured at training time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionTrustSnapshot {
    pub dataset_version_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_at_training_time: Option<String>,
}

#[cfg(feature = "server")]
impl From<CoreDatasetVersionSelection> for DatasetVersionSelection {
    fn from(core: CoreDatasetVersionSelection) -> Self {
        Self {
            dataset_version_id: core.dataset_version_id,
            weight: core.weight,
        }
    }
}

/// Dataset validation status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DatasetValidationStatus {
    Pending,
    Validating,
    Valid,
    Invalid,
    Skipped,
}

impl DatasetValidationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DatasetValidationStatus::Pending => "pending",
            DatasetValidationStatus::Validating => "validating",
            DatasetValidationStatus::Valid => "valid",
            DatasetValidationStatus::Invalid => "invalid",
            DatasetValidationStatus::Skipped => "skipped",
        }
    }

    pub fn from_db_string(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "validating" => Self::Validating,
            "valid" => Self::Valid,
            "invalid" => Self::Invalid,
            "skipped" => Self::Skipped,
            _ => Self::Pending,
        }
    }
}

impl std::fmt::Display for DatasetValidationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ===== Request/Response Types =====

/// Training configuration request
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingConfigRequest {
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,
    pub training_contract_version: String,
    pub pad_token_id: u32,
    pub ignore_index: i32,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    pub warmup_steps: Option<u32>,
    pub max_seq_length: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
    /// Fraction of dataset to use for validation (0.0-0.5).
    #[serde(default)]
    pub validation_split: Option<f32>,
    /// Optional GPU backend preference (coreml, mlx, metal, cpu)
    #[serde(default)]
    #[schema(value_type = String)]
    pub preferred_backend: Option<TrainingBackendKind>,
    /// Backend policy when CoreML is preferred (coreml_only/coreml_else_fallback/auto)
    #[serde(default)]
    #[schema(value_type = String)]
    pub backend_policy: Option<TrainingBackendPolicy>,
    /// Explicit fallback when CoreML is requested and unavailable
    #[serde(default)]
    #[schema(value_type = String)]
    pub coreml_training_fallback: Option<TrainingBackendKind>,
    /// Optional CoreML placement spec for training/export alignment
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = serde_json::Value)]
    pub coreml_placement: Option<CoreMLPlacementSpec>,
    /// Opt-in CoreML export after successful training
    #[serde(default)]
    pub enable_coreml_export: Option<bool>,
    /// Require GPU acceleration (error if no GPU backend can be initialized)
    #[serde(default)]
    pub require_gpu: Option<bool>,
    /// Maximum GPU memory in MB (best-effort, 0/unset = unlimited)
    #[serde(default)]
    pub max_gpu_memory_mb: Option<u64>,
    /// Path to base model for training. Required for correct adapter generation.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub base_model_path: Option<std::path::PathBuf>,
    /// Optional CoreML preprocessing stage configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preprocessing: Option<PreprocessingConfig>,
    /// Force resume even when pipeline/checkpoint compatibility checks fail.
    #[serde(default)]
    pub force_resume: Option<bool>,
}

#[cfg(feature = "server")]
impl TrainingConfigRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.rank == 0 {
            errors.push("rank must be > 0".to_string());
        }
        if self.learning_rate <= 0.0 {
            errors.push("learning_rate must be > 0".to_string());
        }
        if self.training_contract_version != TRAINING_DATA_CONTRACT_VERSION {
            errors.push(format!(
                "training_contract_version must be {}",
                TRAINING_DATA_CONTRACT_VERSION
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Start training request
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct StartTrainingRequest {
    pub adapter_name: String,
    pub config: TrainingConfigRequest,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    /// Target branch for the produced adapter version
    pub target_branch: Option<String>,
    /// Branch classification controlling promotion guardrails (protected/high/sandbox)
    pub branch_classification: Option<BranchClassification>,
    /// Base adapter version ID (for finetuning an existing version)
    pub base_version_id: Option<String>,
    /// Code commit SHA when training is tied to source control
    pub code_commit_sha: Option<String>,
    /// Data spec (DSL or JSON) used for this run
    pub data_spec: Option<String>,
    /// Canonical hash of the dataset manifest(s) used when the job was created
    pub data_spec_hash: Option<String>,
    /// Hyperparameters payload (structured JSON)
    pub hyperparameters: Option<String>,
    pub dataset_id: Option<String>,
    /// Dataset versions to train on (with optional sampling weights)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
    /// Allow synthetic/diagnostic training data instead of datasets
    #[serde(default)]
    pub synthetic_mode: bool,
    /// Caller-declared lineage quality (overrides computed default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_lineage_mode: Option<DataLineageMode>,
    /// Base model ID for provenance tracking
    pub base_model_id: String,
    /// Document collection ID for provenance tracking
    pub collection_id: Option<String>,
    /// Marketing/operational tier for routing and UI badges (micro/standard/max)
    ///
    /// # OpenAPI
    /// Uses proper enum schema with values: `micro`, `standard`, `max`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_tier: Option<LoraTier>,
    /// Logical scope for adapter visibility (e.g., project, tenant)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    // Category & metadata
    /// Adapter category: code, framework, codebase, docs, domain
    pub category: Option<String>,
    /// Human-readable description
    pub description: Option<String>,

    // Category-specific configuration
    /// Programming language (for code adapters)
    pub language: Option<String>,
    /// Symbol targets (for code adapters)
    pub symbol_targets: Option<Vec<String>>,
    /// Framework ID (for framework adapters)
    pub framework_id: Option<String>,
    /// Framework version (for framework adapters)
    pub framework_version: Option<String>,
    /// API patterns to focus on (for framework adapters)
    pub api_patterns: Option<Vec<String>>,
    /// Repository scope (for codebase adapters)
    pub repo_scope: Option<String>,
    /// File patterns to include (for codebase adapters)
    pub file_patterns: Option<Vec<String>>,
    /// File patterns to exclude (for codebase adapters)
    pub exclude_patterns: Option<Vec<String>>,

    // Post-training actions
    /// Actions to perform after training completes
    pub post_actions: Option<PostActionsRequest>,
}

/// Post-training actions configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct PostActionsRequest {
    /// Package adapter after training (default: true)
    pub package: Option<bool>,
    /// Register adapter in registry after packaging (default: true)
    pub register: Option<bool>,
    /// Create a new stack with the adapter after registration (default: true).
    /// Note: The new stack will NOT be set as the tenant's default stack.
    pub create_stack: Option<bool>,
    /// Tier to assign: persistent, warm, ephemeral (default: warm)
    pub tier: Option<String>,
    /// Custom adapters root directory
    pub adapters_root: Option<String>,
}

/// Training job response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingJobResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub adapter_name: String,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    /// Alias of `repo_id` for clarity in clients that treat this as an adapter repo identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_repo_id: Option<String>,
    pub repo_name: Option<String>,
    pub target_branch: Option<String>,
    pub base_version_id: Option<String>,
    pub draft_version_id: Option<String>,
    pub adapter_version_id: Option<String>,
    pub produced_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_hash: Option<String>,
    pub dataset_id: Option<String>,
    /// Dataset manifest hash captured at job creation (combined when multi-dataset).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
    /// Dataset versions used for this job (order preserved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
    /// Trust snapshot for dataset versions at training start
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_trust: Option<Vec<DatasetVersionTrustSnapshot>>,
    #[serde(default)]
    pub synthetic_mode: bool,
    #[cfg(feature = "server")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_lineage_mode: Option<DataLineageMode>,
    /// Base model ID used for training
    pub base_model_id: Option<String>,
    /// Document collection ID used for training
    pub collection_id: Option<String>,
    /// Build ID for CI/CD traceability
    pub build_id: Option<String>,
    /// BLAKE3 hash of training config for reproducibility
    pub config_hash_b3: Option<String>,
    /// Adapter ID after packaging (populated on completion)
    pub adapter_id: Option<String>,
    /// BLAKE3 hash of adapter weights (for verification)
    pub weights_hash_b3: Option<String>,

    // Category metadata
    /// Adapter category
    pub category: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Programming language
    pub language: Option<String>,
    /// Framework ID
    pub framework_id: Option<String>,
    /// Framework version
    pub framework_version: Option<String>,
    /// Marketing/operational tier for routing and UI badges (micro/standard/max)
    ///
    /// # OpenAPI
    /// Uses proper enum schema with values: `micro`, `standard`, `max`.
    #[cfg(feature = "server")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_tier: Option<LoraTier>,
    /// Logical scope for adapter visibility (e.g., project, tenant)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    // Training progress
    /// Current training status (pending, running, completed, failed, cancelled)
    pub status: String,
    /// Training progress percentage (0.0-100.0).
    /// None if progress data is not yet available (distinguishes from 0% progress).
    ///
    /// # Serialization Behavior
    /// - On serialization: Omitted when `None` (via `skip_serializing_if`)
    /// - On deserialization: Absent fields deserialize to `None` (via `#[serde(default)]`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_pct: Option<f32>,
    /// Current training epoch (0-indexed).
    /// None if progress data is not yet available.
    ///
    /// # Serialization Behavior
    /// - On serialization: Omitted when `None` (via `skip_serializing_if`)
    /// - On deserialization: Absent fields deserialize to `None` (via `#[serde(default)]`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_epoch: Option<u32>,
    /// Total number of training epochs
    pub total_epochs: u32,
    /// Current training loss value.
    /// None if progress data is not yet available.
    ///
    /// # Serialization Behavior
    /// - On serialization: Omitted when `None` (via `skip_serializing_if`)
    /// - On deserialization: Absent fields deserialize to `None` (via `#[serde(default)]`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_loss: Option<f32>,
    /// Current learning rate
    pub learning_rate: f32,
    /// Tokens processed per second.
    /// None if progress data is not yet available.
    ///
    /// # Serialization Behavior
    /// - On serialization: Omitted when `None` (via `skip_serializing_if`)
    /// - On deserialization: Absent fields deserialize to `None` (via `#[serde(default)]`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_per_second: Option<f32>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    /// Structured error code for failed jobs (e.g., "TRAIN_E101_GPU_OOM").
    /// Use `aosctl explain <code>` for remediation guidance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub estimated_completion: Option<String>,

    // Backend and determinism
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_training_fallback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_export_requested: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_export_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_export_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_fused_package_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_package_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_metadata_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_base_manifest_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_adapter_hash_b3: Option<String>,
    /// Whether the CoreML export used fusion verification.
    /// If false, the package may be stub/metadata-only and not production-ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_fusion_verified: Option<bool>,
    /// Warning messages related to training or export.
    /// Populated when there are non-fatal issues that users should be aware of.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_inputs_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_gpu: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_gpu_memory_mb: Option<u64>,

    // Extended metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_examples_per_sec: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_gpu_memory_mb: Option<f32>,

    // Packaging summary
    /// Alias of `aos_path` for clients expecting an "artifact path" surface.
    ///
    /// **Deprecated since 1.0.0**: Use `aos_path` instead.
    /// - **Removal timeline**: Will be removed in version 2.0.0 (see [`DEPRECATED_FIELD_SUNSET_VERSION`]).
    /// - **Migration**: Replace `artifact_path` references with `aos_path`.
    /// - Both fields currently return the same value for backward compatibility.
    ///
    /// # OpenAPI
    /// This field is marked deprecated in the OpenAPI schema.
    #[deprecated(
        since = "1.0.0",
        note = "Use aos_path instead. Will be removed in v2.0.0."
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", schema(deprecated))]
    pub artifact_path: Option<String>,
    /// Alias of `package_hash_b3` for clients expecting an "artifact hash" surface.
    ///
    /// **Deprecated since 1.0.0**: Use `package_hash_b3` instead.
    /// - **Removal timeline**: Will be removed in version 2.0.0 (see [`DEPRECATED_FIELD_SUNSET_VERSION`]).
    /// - **Migration**: Replace `artifact_hash_b3` references with `package_hash_b3`.
    /// - Both fields currently return the same value for backward compatibility.
    ///
    /// # OpenAPI
    /// This field is marked deprecated in the OpenAPI schema.
    #[deprecated(
        since = "1.0.0",
        note = "Use package_hash_b3 instead. Will be removed in v2.0.0."
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", schema(deprecated))]
    pub artifact_hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aos_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash_b3: Option<String>,
    /// Indicates the source of manifest_hash_b3: "manifest" or "package_fallback"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_base_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_per_layer_hashes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_spec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperparameters: Option<String>,
}

#[cfg(feature = "server")]
#[allow(deprecated)] // artifact_path and artifact_hash_b3 are deprecated aliases
impl From<TrainingJob> for TrainingJobResponse {
    fn from(job: TrainingJob) -> Self {
        #[derive(Serialize)]
        struct TrainingConfigHashParams {
            rank: usize,
            alpha: f32,
            learning_rate: f32,
            batch_size: usize,
            epochs: usize,
            hidden_dim: usize,
            preprocessing: Option<PreprocessingConfig>,
        }

        let adapter_version_id = job
            .adapter_version_id
            .clone()
            .or_else(|| job.draft_version_id.clone())
            .or_else(|| job.produced_version_id.clone());

        let produced_version_id = job
            .produced_version_id
            .clone()
            .or_else(|| adapter_version_id.clone());

        let adapter_repo_id = job.repo_id.clone();

        let data_spec_hash = job.data_spec_hash.clone().or_else(|| {
            job.data_spec_json
                .as_deref()
                .map(|spec| B3Hash::hash(spec.as_bytes()).to_hex())
        });

        let config_hash_b3 = job.config_hash_b3.clone().or_else(|| {
            let params = TrainingConfigHashParams {
                rank: job.config.rank as usize,
                alpha: job.config.alpha as f32,
                learning_rate: job.config.learning_rate,
                batch_size: job.config.batch_size as usize,
                epochs: job.config.epochs as usize,
                hidden_dim: 768,
                preprocessing: job.config.preprocessing.clone(),
            };
            serde_json::to_string(&params)
                .ok()
                .map(|json| B3Hash::hash(json.as_bytes()).to_hex())
        });

        let aos_path = job.aos_path.clone().or_else(|| job.artifact_path.clone());
        let package_hash_b3 = job
            .package_hash_b3
            .clone()
            .or_else(|| job.weights_hash_b3.clone());

        let artifact_path = aos_path.clone();
        let artifact_hash_b3 = package_hash_b3.clone();

        // Compute warnings before struct initialization to avoid borrow issues
        let warnings = {
            let mut warnings = Vec::new();
            // Add warning if CoreML export was requested but fusion was not verified
            if job.coreml_export_requested == Some(true)
                && job.coreml_fusion_verified == Some(false)
            {
                warnings.push(
                    "CoreML export was stubbed - package may not be functional. \
                     Run on macOS with --features coreml-backend for production use."
                        .to_string(),
                );
            }
            // Add warning if CoreML export status indicates metadata-only
            if job.coreml_export_status.as_deref() == Some("metadata_only") {
                warnings.push(
                    "CoreML package is metadata-only (fused manifest matches base). \
                     This may indicate stub mode was used."
                        .to_string(),
                );
            }
            warnings
        };

        Self {
            schema_version: schema_version(),
            id: job.id,
            adapter_name: job.adapter_name,
            template_id: job.template_id,
            repo_id: job.repo_id,
            adapter_repo_id,
            repo_name: job.repo_name,
            target_branch: job.target_branch,
            base_version_id: job.base_version_id,
            draft_version_id: job.draft_version_id,
            adapter_version_id,
            produced_version_id,
            code_commit_sha: job.code_commit_sha,
            data_spec: job.data_spec_json,
            data_spec_hash,
            dataset_id: job.dataset_id,
            dataset_hash_b3: job.dataset_hash_b3,
            dataset_version_ids: job.dataset_version_ids.map(|versions| {
                versions
                    .into_iter()
                    .map(DatasetVersionSelection::from)
                    .collect()
            }),
            dataset_version_trust: job.dataset_version_trust.map(|entries| {
                entries
                    .into_iter()
                    .map(|snapshot| DatasetVersionTrustSnapshot {
                        dataset_version_id: snapshot.dataset_version_id,
                        trust_at_training_time: snapshot.trust_at_training_time,
                    })
                    .collect()
            }),
            synthetic_mode: job.synthetic_mode,
            data_lineage_mode: job.data_lineage_mode,
            base_model_id: job.base_model_id,
            collection_id: job.collection_id,
            build_id: job.build_id,
            config_hash_b3,
            adapter_id: job.adapter_id,
            weights_hash_b3: job.weights_hash_b3,
            // Category metadata - will be populated when TrainingJob is extended
            category: job.category,
            description: job.description,
            language: job.language,
            framework_id: job.framework_id,
            framework_version: job.framework_version,
            lora_tier: job.lora_tier,
            scope: job.scope,
            // Training progress
            // For pending jobs (not yet started), return None to distinguish "no data" from "0%"
            status: job.status.to_string(),
            progress_pct: if job.status == adapteros_types::training::TrainingJobStatus::Pending {
                None
            } else {
                Some(job.progress_pct)
            },
            current_epoch: if job.status == adapteros_types::training::TrainingJobStatus::Pending {
                None
            } else {
                Some(job.current_epoch)
            },
            total_epochs: job.total_epochs,
            current_loss: if job.status == adapteros_types::training::TrainingJobStatus::Pending {
                None
            } else {
                Some(job.current_loss)
            },
            learning_rate: job.learning_rate,
            tokens_per_second: if job.status
                == adapteros_types::training::TrainingJobStatus::Pending
            {
                None
            } else {
                Some(job.tokens_per_second)
            },
            created_at: job.created_at,
            started_at: job.started_at,
            completed_at: job.completed_at,
            error_message: job.error_message,
            error_code: job.error_code,
            estimated_completion: None, // Calculate if needed
            // Backend/determinism
            requested_backend: job.requested_backend,
            backend_policy: job.backend_policy,
            coreml_training_fallback: job.coreml_training_fallback,
            backend: job.backend,
            backend_reason: job.backend_reason,
            backend_device: job.backend_device,
            coreml_export_requested: job.coreml_export_requested,
            coreml_export_reason: job.coreml_export_reason,
            coreml_fused_package_hash: job.coreml_fused_package_hash,
            coreml_package_path: job.coreml_package_path,
            coreml_metadata_path: job.coreml_metadata_path,
            coreml_base_manifest_hash: job.coreml_base_manifest_hash,
            coreml_adapter_hash_b3: job.coreml_adapter_hash_b3,
            coreml_fusion_verified: job.coreml_fusion_verified,
            warnings,
            coreml_export_status: job.coreml_export_status,
            determinism_mode: job.determinism_mode,
            training_seed: job.training_seed,
            seed_inputs_json: job.seed_inputs_json,
            require_gpu: job.require_gpu,
            max_gpu_memory_mb: job.max_gpu_memory_mb,
            // Extended metrics
            examples_processed: job.examples_processed,
            tokens_processed: job.tokens_processed,
            training_time_ms: job.training_time_ms,
            throughput_examples_per_sec: job.throughput_examples_per_sec,
            gpu_utilization_pct: job.gpu_utilization_pct,
            peak_gpu_memory_mb: job.peak_gpu_memory_mb,
            // Packaging summary
            artifact_path,
            artifact_hash_b3,
            aos_path,
            package_hash_b3: package_hash_b3.clone(),
            manifest_hash_b3: job.manifest_hash_b3.clone().or(package_hash_b3.clone()),
            manifest_hash_source: if job.manifest_hash_b3.is_some() {
                Some("manifest".to_string())
            } else if package_hash_b3.is_some() {
                Some("package_fallback".to_string())
            } else {
                None
            },
            manifest_rank: job.manifest_rank,
            manifest_base_model: job.manifest_base_model,
            manifest_per_layer_hashes: job.manifest_per_layer_hashes,
            signature_status: job.signature_status,
            metrics_snapshot_id: None,
            hyperparameters: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn training_job_response_includes_dataset_versions() {
        let mut job = TrainingJob::new(
            "job-1".to_string(),
            "adapter".to_string(),
            TrainingConfig::default(),
        );
        job.dataset_version_ids = Some(vec![adapteros_types::training::DatasetVersionSelection {
            dataset_version_id: "ds-ver-1".to_string(),
            weight: 1.0,
        }]);
        job.dataset_version_trust = Some(vec![
            adapteros_types::training::DatasetVersionTrustSnapshot {
                dataset_version_id: "ds-ver-1".to_string(),
                trust_at_training_time: Some("allowed".to_string()),
            },
        ]);
        job.data_lineage_mode = Some(adapteros_types::training::DataLineageMode::Versioned);

        let resp: TrainingJobResponse = job.into();
        let versions = resp.dataset_version_ids.expect("dataset_version_ids");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].dataset_version_id, "ds-ver-1");
        let trust = resp.dataset_version_trust.expect("dataset_version_trust");
        assert_eq!(trust[0].dataset_version_id, "ds-ver-1");
        assert_eq!(trust[0].trust_at_training_time.as_deref(), Some("allowed"));
        assert!(!resp.synthetic_mode);
        assert_eq!(resp.data_lineage_mode, Some(DataLineageMode::Versioned));
    }

    #[test]
    fn training_job_response_exposes_artifact_and_version_metadata() {
        #[derive(Serialize)]
        struct TrainingConfigHashParams {
            rank: usize,
            alpha: f32,
            learning_rate: f32,
            batch_size: usize,
            epochs: usize,
            hidden_dim: usize,
            preprocessing: Option<PreprocessingConfig>,
        }

        let mut job = TrainingJob::new(
            "job-2".to_string(),
            "adapter-meta".to_string(),
            TrainingConfig::default(),
        );
        job.repo_id = Some("repo-1".to_string());
        job.draft_version_id = Some("ver-draft-1".to_string());
        job.artifact_path = Some("/var/aos/adapters/repo-1/v1.aos".to_string());
        job.weights_hash_b3 = Some(B3Hash::hash(b"weights").to_hex());
        job.data_spec_json = Some(r#"{"dataset":"v1"}"#.to_string());

        let expected_data_spec_hash =
            B3Hash::hash(job.data_spec_json.as_deref().unwrap().as_bytes()).to_hex();
        let params = TrainingConfigHashParams {
            rank: job.config.rank as usize,
            alpha: job.config.alpha as f32,
            learning_rate: job.config.learning_rate,
            batch_size: job.config.batch_size as usize,
            epochs: job.config.epochs as usize,
            hidden_dim: 768,
            preprocessing: job.config.preprocessing.clone(),
        };
        let expected_config_hash =
            B3Hash::hash(serde_json::to_string(&params).unwrap().as_bytes()).to_hex();

        let resp: TrainingJobResponse = job.into();

        assert_eq!(resp.repo_id.as_deref(), Some("repo-1"));
        assert_eq!(resp.adapter_repo_id.as_deref(), Some("repo-1"));
        assert_eq!(resp.adapter_version_id.as_deref(), Some("ver-draft-1"));
        assert_eq!(
            resp.config_hash_b3.as_deref(),
            Some(expected_config_hash.as_str())
        );
        assert_eq!(
            resp.data_spec_hash.as_deref(),
            Some(expected_data_spec_hash.as_str())
        );

        assert_eq!(
            resp.artifact_path.as_deref(),
            Some("/var/aos/adapters/repo-1/v1.aos")
        );
        assert_eq!(
            resp.aos_path.as_deref(),
            Some("/var/aos/adapters/repo-1/v1.aos")
        );
        assert_eq!(resp.artifact_hash_b3, resp.package_hash_b3);
        assert!(resp.artifact_hash_b3.is_some());
    }
}

/// Training template response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingTemplateResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
}

#[cfg(feature = "server")]
impl From<TrainingConfigRequest> for TrainingConfig {
    fn from(req: TrainingConfigRequest) -> Self {
        Self {
            rank: req.rank,
            alpha: req.alpha,
            targets: req.targets,
            training_contract_version: req.training_contract_version,
            pad_token_id: req.pad_token_id,
            ignore_index: req.ignore_index,
            coreml_placement: req.coreml_placement,
            epochs: req.epochs,
            learning_rate: req.learning_rate,
            batch_size: req.batch_size,
            warmup_steps: req.warmup_steps,
            max_seq_length: req.max_seq_length,
            gradient_accumulation_steps: req.gradient_accumulation_steps,
            weight_group_config: None,
            lr_schedule: Some("cosine".to_string()),
            final_lr: Some(req.learning_rate * 0.1),
            early_stopping: Some(false),
            patience: Some(5),
            min_delta: Some(0.001),
            checkpoint_frequency: Some(5),
            max_checkpoints: Some(3),
            preferred_backend: req.preferred_backend,
            backend_policy: req.backend_policy,
            coreml_training_fallback: req.coreml_training_fallback,
            enable_coreml_export: req.enable_coreml_export,
            require_gpu: req.require_gpu.unwrap_or(false),
            max_gpu_memory_mb: req.max_gpu_memory_mb,
            base_model_path: req.base_model_path.clone(),
            hidden_state_layer: None,
            validation_split: req.validation_split,
            preprocessing: req.preprocessing,
            force_resume: req.force_resume.unwrap_or(false),
        }
    }
}

#[cfg(feature = "server")]
impl From<TrainingTemplate> for TrainingTemplateResponse {
    fn from(template: TrainingTemplate) -> Self {
        Self {
            schema_version: schema_version(),
            id: template.id,
            name: template.name,
            description: template.description,
            category: template.category,
            rank: template.config.rank,
            alpha: template.config.alpha,
            targets: template.config.targets,
            epochs: template.config.epochs,
            learning_rate: template.config.learning_rate,
            batch_size: template.config.batch_size,
        }
    }
}

/// Training metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingMetricsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub loss: f32,
    pub tokens_per_second: f32,
    pub learning_rate: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub progress_pct: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_examples_per_sec: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_gpu_memory_mb: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub using_gpu: Option<bool>,
}

/// Individual training metric entry for time-series data
///
/// Note: `learning_rate` is optional because per-step LR is not currently stored
/// in the training_metrics table. It's available at job level via TrainingProgress.
/// `tokens_processed` is populated when available from the "tokens_processed" metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingMetricEntry {
    /// Metric step (training iteration)
    pub step: i64,
    /// Loss value at this step
    pub loss: f64,
    /// Learning rate at this step (optional - not stored per-step in current schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learning_rate: Option<f64>,
    /// Training epoch
    pub epoch: i32,
    /// Tokens processed up to this point
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<i64>,
    /// Timestamp of this metric
    pub timestamp: String,
}

/// Training metrics list response for time-series metrics endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingMetricsListResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Training job ID
    pub job_id: String,
    /// Time-series metrics
    pub metrics: Vec<TrainingMetricEntry>,
}

/// Training report response for report artifact retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingReportResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Training report artifact.
    pub report: TrainingReportV1,
}

// ===== Dataset Types =====

/// Upload dataset request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct UploadDatasetRequest {
    pub name: String,
    pub description: Option<String>,
    pub format: String, // 'patches', 'jsonl', 'txt', 'custom'
}

/// Upload dataset response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct UploadDatasetResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    /// The dataset version ID created for this upload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub format: String,
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub reused: bool,
    pub created_at: String,
}

/// Dataset response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DatasetResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    /// Latest trusted dataset version (effective trust applied; may be None if no trusted versions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub file_count: i32,
    pub total_size_bytes: i64,
    pub format: String,
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
    pub storage_path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub validation_status: DatasetValidationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<String>>,
    /// Effective trust_state for the selected dataset_version_id (allowed/allowed_with_warning/blocked/needs_approval)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Summary of a dataset version (used for dataset detail views and selectors)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionSummary {
    pub dataset_version_id: String,
    pub version_number: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<String>,
    /// Effective trust_state for this version (includes overrides)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<String>,
    /// Repository slug for identifying the source repository (e.g., "org/repo-name")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    pub created_at: String,
}

/// Dataset versions list response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub versions: Vec<DatasetVersionSummary>,
}

/// Dataset statistics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DatasetStatisticsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub num_examples: i32,
    pub avg_input_length: f64,
    pub avg_target_length: f64,
    pub language_distribution: Option<serde_json::Value>,
    pub file_type_distribution: Option<serde_json::Value>,
    pub total_tokens: i64,
    pub computed_at: String,
}

/// Dataset file response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DatasetFileResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub hash: String,
    pub mime_type: Option<String>,
    pub created_at: String,
}

/// Dataset validation request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ValidateDatasetRequest {
    pub check_format: Option<bool>,
}

/// Dataset validation response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ValidateDatasetResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub dataset_id: String,
    pub is_valid: bool,
    pub validation_status: DatasetValidationStatus,
    pub errors: Option<Vec<String>>,
    pub validated_at: String,
}

// ===== Training Job List Types =====

/// Training job list query parameters.
///
/// # Pagination
///
/// This endpoint uses 1-indexed pagination:
/// - `page` is 1-indexed (first page = 1, not 0)
/// - `page_size` controls items per page (default: 20, min: 1, max: 100)
/// - Offset calculation: `offset = (page - 1) * page_size`
///
/// Example: `page=2, page_size=20` returns items 21-40.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema, utoipa::IntoParams))]
pub struct TrainingListParams {
    /// Filter by status (pending, running, completed, failed, cancelled)
    pub status: Option<String>,
    /// Page number (1-indexed). Values less than 1 are normalized to 1.
    /// Default: 1.
    pub page: Option<u32>,
    /// Number of items per page. Clamped to range [1, 100].
    /// Default: 20.
    pub page_size: Option<u32>,
    /// Filter by adapter name
    pub adapter_name: Option<String>,
    /// Filter by template ID
    pub template_id: Option<String>,
    /// Filter by dataset ID
    pub dataset_id: Option<String>,
}

/// Training job list response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingJobListResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub jobs: Vec<TrainingJobResponse>,
    pub total: usize,
    pub page: u32,
    pub page_size: u32,
}

impl Default for TrainingJobListResponse {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            jobs: vec![],
            total: 0,
            page: 1,
            page_size: 20,
        }
    }
}

// ============================================================================
// Chat Bootstrap Types
// ============================================================================

/// Response for GET /v1/training/jobs/{id}/chat_bootstrap
///
/// Returns the "recipe" for starting a chat from a completed training job.
/// Used by any UI flow to quickly get the payload needed to create a chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ChatBootstrapResponse {
    /// Whether the training job is ready for chat (completed with stack)
    pub ready: bool,
    /// Stack ID created from training (if ready)
    pub stack_id: Option<String>,
    /// Adapter IDs in the stack
    pub adapter_ids: Vec<String>,
    /// Base model ID used for training
    pub base_model: Option<String>,
    /// RAG collection ID if training involved RAG
    pub collection_id: Option<String>,
    /// Suggested title for the chat session
    pub suggested_chat_title: String,

    // Provenance fields for Bundle E readiness
    /// Training job ID (always present, echoed from path)
    pub training_job_id: String,
    /// Training job status ("pending"|"running"|"completed"|"failed"|"cancelled")
    pub status: String,
    /// Primary adapter ID from training job (set after training completes)
    pub adapter_id: Option<String>,
    /// Adapter version ID for display (e.g., adapter@version)
    pub adapter_version_id: Option<String>,
    /// Training dataset ID
    pub dataset_id: Option<String>,
    /// Dataset version ID for citation scoping (immutable snapshot)
    pub dataset_version_id: Option<String>,
    /// Dataset name for display
    pub dataset_name: Option<String>,
}

/// Request for POST /v1/chats/from_training_job
///
/// Creates a chat session bound to a training job's stack in one call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreateChatFromJobRequest {
    /// Training job ID to create chat from
    pub training_job_id: String,
    /// Optional override for chat session name
    pub name: Option<String>,
    /// Optional metadata JSON for the chat session
    pub metadata_json: Option<String>,
}

/// Response for POST /v1/chats/from_training_job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreateChatFromJobResponse {
    /// Created chat session ID
    pub session_id: String,
    /// Stack ID the session is bound to
    pub stack_id: String,
    /// Session name (either provided or generated)
    pub name: String,
    /// Creation timestamp
    pub created_at: String,

    // Provenance fields for Bundle E readiness
    /// Training job ID (echoed from request for confirmation)
    pub training_job_id: String,
    /// Primary adapter ID from the training job
    pub adapter_id: Option<String>,
    /// Training dataset ID
    pub dataset_id: Option<String>,
    /// RAG collection ID if linked
    pub collection_id: Option<String>,
}

// ============================================================================
// Adapter Publish + Attach Modes v1
// ============================================================================

/// Attach mode for adapter versions.
///
/// Controls how an adapter can be attached to inference stacks:
/// - `Free`: Adapter can be attached without specific dataset context
/// - `RequiresDataset`: Adapter requires a specific dataset version context for inference
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AttachMode {
    /// Adapter can be attached without specific dataset context
    #[default]
    Free,
    /// Adapter requires specific dataset version context for inference
    RequiresDataset,
}

impl AttachMode {
    /// Convert to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachMode::Free => "free",
            AttachMode::RequiresDataset => "requires_dataset",
        }
    }

    /// Parse from database string representation
    pub fn from_db_string(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "requires_dataset" => AttachMode::RequiresDataset,
            _ => AttachMode::Free,
        }
    }
}

impl std::fmt::Display for AttachMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Request to publish an adapter version.
///
/// Publishing makes an adapter version available for use in inference stacks
/// and configures its attach mode behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct PublishAdapterVersionRequest {
    /// Display name for the published adapter (optional, defaults to repo name + version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Short description for the adapter version (max 280 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,

    /// Attach mode: "free" (default) or "requires_dataset"
    #[serde(default)]
    pub attach_mode: AttachMode,

    /// Required dataset version ID when attach_mode is "requires_dataset".
    /// Must be a dataset version that was used in training this adapter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_scope_dataset_version_id: Option<String>,
}

/// Response from publishing an adapter version
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct PublishAdapterVersionResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,

    /// The published adapter version ID
    pub version_id: String,

    /// Repository ID
    pub repo_id: String,

    /// The configured attach mode
    pub attach_mode: AttachMode,

    /// Required dataset version ID (if attach_mode is requires_dataset)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_scope_dataset_version_id: Option<String>,

    /// Timestamp when the adapter was published
    pub published_at: String,

    /// Short description (echoed from request)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,
}

/// Request to archive an adapter version
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ArchiveAdapterVersionRequest {
    /// Reason for archiving (optional, for audit trail)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response from archive/unarchive operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ArchiveAdapterVersionResponse {
    /// The adapter version ID
    pub version_id: String,

    /// Current archive state
    pub is_archived: bool,

    /// Timestamp of the operation
    pub updated_at: String,
}

// ============================================================================
// Start Training From Version Types
// ============================================================================

/// Request to start training from an existing adapter version
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct StartTrainingFromVersionRequest {
    /// Optional training config ID to use (overrides version's config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config_id: Option<String>,

    /// Optional hyperparameters override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperparams: Option<serde_json::Value>,

    /// Optional target branch (defaults to version's branch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
}

/// Response from starting training from a version
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct StartTrainingResponse {
    /// The created training job ID
    pub job_id: String,

    /// Initial job status
    pub status: String,

    /// Draft version ID created for this training run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_version_id: Option<String>,
}

// ============================================================================
// Training Queue Status Types
// ============================================================================

/// Response for GET /v1/training/queue
///
/// Returns the current training queue status including counts by status
/// and estimated wait times.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingQueueResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,

    /// Total number of jobs in the queue (pending + running)
    pub queue_depth: usize,

    /// Number of jobs waiting to start
    pub pending_count: usize,

    /// Number of currently running jobs
    pub running_count: usize,

    /// Average wait time for pending jobs in seconds (0 if no pending jobs)
    pub avg_wait_time_secs: f64,

    /// Oldest pending job's wait time in seconds (None if no pending jobs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_wait_time_secs: Option<f64>,

    /// Average training duration for running jobs in seconds (0 if no running jobs)
    pub avg_training_duration_secs: f64,

    /// Summary of pending jobs (limited to first 10)
    pub pending_jobs: Vec<TrainingQueueJobSummary>,

    /// Summary of running jobs
    pub running_jobs: Vec<TrainingQueueJobSummary>,
}

/// Summary of a job in the training queue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingQueueJobSummary {
    /// Job ID
    pub id: String,

    /// Adapter name being trained
    pub adapter_name: String,

    /// Job status
    pub status: String,

    /// Progress percentage (0-100)
    pub progress_pct: f32,

    /// When the job was created
    pub created_at: String,

    /// When the job started (if running)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,

    /// Tenant ID (for admin view)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}
