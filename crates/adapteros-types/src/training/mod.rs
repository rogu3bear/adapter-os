//! Training job and configuration types
//! This module provides canonical training types that consolidate definitions
//! from adapteros-core, adapteros-orchestrator, and adapteros-api-types into
//! a single source of truth.
//!
//! # Type Hierarchy
//!
//! - `TrainingJobStatus` - State machine for training job lifecycle
//! - `TrainingJob` - Complete training job information with metadata
//! - `TrainingConfig` - Training hyperparameters and configuration
//! - `TrainingTemplate` - Reusable training templates
//!
//! # Canonical Consolidation
//!
//! This module consolidates 3 previous definitions:
//! 1. `adapteros-core/src/training.rs` - Base definition with artifact metadata
//! 2. `adapteros-orchestrator/src/training.rs` - Orchestrator-specific variant
//! 3. Used by `adapteros-api-types/src/training.rs` for API responses

use crate::coreml::CoreMLPlacementSpec;
use serde::{Deserialize, Serialize};
use std::fmt;

pub mod example;
pub use example::{
    metadata_from_pairs, provenance_from_map, provenance_from_pairs, sample_role_from_metadata,
    sample_role_from_provenance, validate_training_contract_config, validate_training_example,
    validate_training_examples, weight_from_metadata, weight_from_provenance, ExampleMetadataV1,
    TrainingDataContractConfig, TrainingExampleBatchSummary, TrainingExampleV1,
    TrainingExampleValidationError, TrainingTokenLocation, TRAINING_DATA_CONTRACT_VERSION,
};
pub mod preprocessed_example;
pub use preprocessed_example::{
    PreprocessedExampleV1, PREPROCESSED_EXAMPLE_SCHEMA_VERSION,
    PREPROCESSED_FEATURE_BACKEND_COREML, PREPROCESSED_FEATURE_DTYPE_F32,
};

/// Training report schema version.
pub const TRAINING_REPORT_VERSION: u32 = 1;

fn default_training_report_version() -> u32 {
    TRAINING_REPORT_VERSION
}

fn default_dataset_weight() -> f32 {
    1.0
}

/// Dataset version selector with optional sampling weight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionSelection {
    /// Unique identifier for the dataset version to use.
    pub dataset_version_id: String,
    /// Sampling weight for this dataset version (default: 1.0).
    #[serde(default = "default_dataset_weight")]
    pub weight: f32,
}

/// Snapshot of dataset trust_state captured at training time.
///
/// Trust semantics:
/// - Training proceeds only for `allowed` or `allowed_with_warning`.
/// - `blocked`, `needs_approval`, and `unknown` are rejected.
/// - Adapter trust aggregates worst-of datasets (blocked > warn > unknown > allowed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DatasetVersionTrustSnapshot {
    /// Unique identifier for the dataset version.
    pub dataset_version_id: String,
    /// Trust state captured at training time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_at_training_time: Option<String>,
    /// Parent dataset ID (resolved from dataset_version_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    /// Dataset display name (resolved from dataset_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_name: Option<String>,
}

/// Lineage quality for training data provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DataLineageMode {
    /// Fully versioned datasets were provided.
    Versioned,
    /// Only a dataset handle was provided (no immutable versions).
    DatasetOnly,
    /// Synthetic or diagnostic data with no dataset linkage.
    Synthetic,
    /// Legacy unpinned adapters (no dataset lineage; allowed for old jobs only).
    LegacyUnpinned,
}

impl DataLineageMode {
    /// Canonical string representation for serialization/logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            DataLineageMode::Versioned => "versioned",
            DataLineageMode::DatasetOnly => "dataset_only",
            DataLineageMode::Synthetic => "synthetic",
            DataLineageMode::LegacyUnpinned => "legacy_unpinned",
        }
    }
}

/// Classification for adapter branches controlling promotion safeguards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BranchClassification {
    /// Protected branches (e.g., main) with strict promotion rules.
    Protected,
    /// High-sensitivity branches (treat like protected).
    High,
    /// Sandbox branches that allow relaxed promotion for legacy/unpinned adapters.
    Sandbox,
}

impl BranchClassification {
    /// Canonical string representation for serialization/logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            BranchClassification::Protected => "protected",
            BranchClassification::High => "high",
            BranchClassification::Sandbox => "sandbox",
        }
    }
}

/// LoRA adapter tier for routing and UI badges.
///
/// # OpenAPI
///
/// This enum is exposed in the OpenAPI schema with proper enum constraints
/// rather than as a plain string type. Valid values: `micro`, `standard`, `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LoraTier {
    /// Minimal footprint adapters
    Micro,
    /// Balanced/default adapters
    Standard,
    /// Maximum capacity adapters
    Max,
}

/// Backend preference for training requests.
///
/// This mirrors the inference `BackendKind` variants but lives in `adapteros-types`
/// to avoid introducing dependency cycles. Conversion to the runtime backend
/// type happens in the orchestrator/worker boundary.
///
/// CoreML is a first-class training target for LoRA-only runs. Callers may
/// provide an explicit fallback when CoreML assets/devices are unavailable, but
/// fallbacks must be surfaced explicitly (no silent redirects).
///
/// # Note on MlxBridge
///
/// The `MlxBridge` variant (subprocess bridge for MoE models) from the canonical
/// `BackendKind` enum is intentionally excluded here. Training always uses the
/// MLX FFI backend directly—`MlxBridge` is only relevant for inference of
/// mixture-of-experts models that MLX FFI doesn't support. When converting from
/// `BackendKind`, `MlxBridge` maps to `Mlx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrainingBackendKind {
    /// Deterministic auto-selection (best available)
    #[serde(alias = "autodev", alias = "auto_dev", alias = "default")]
    #[default]
    Auto,
    /// CoreML / ANE acceleration (inference/export target)
    #[serde(alias = "core-ml", alias = "ane")]
    CoreML,
    /// MLX backend (training/export)
    #[serde(alias = "mlx")]
    Mlx,
    /// Metal GPU backend (deterministic fallback)
    #[serde(alias = "metal")]
    Metal,
    /// CPU-only execution
    #[serde(alias = "cpu_only", alias = "cpu-only")]
    Cpu,
}

impl TrainingBackendKind {
    /// Canonical lower-case string representation for serialization/logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            TrainingBackendKind::Auto => "auto",
            TrainingBackendKind::CoreML => "coreml",
            TrainingBackendKind::Mlx => "mlx",
            TrainingBackendKind::Metal => "metal",
            TrainingBackendKind::Cpu => "cpu",
        }
    }
}

impl fmt::Display for TrainingBackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Backend policy describing how to handle CoreML preference and fallbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrainingBackendPolicy {
    /// Deterministic auto-selection (existing behavior).
    #[default]
    Auto,
    /// Require CoreML; fail fast if CoreML cannot be used.
    CoremlOnly,
    /// Prefer CoreML, allow explicit fallback if CoreML is unavailable.
    CoremlElseFallback,
}

impl TrainingBackendPolicy {
    /// Canonical string representation for serialization/logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            TrainingBackendPolicy::Auto => "auto",
            TrainingBackendPolicy::CoremlOnly => "coreml_only",
            TrainingBackendPolicy::CoremlElseFallback => "coreml_else_fallback",
        }
    }
}

/// Training job state machine
///
/// Represents the complete lifecycle of a training job from creation to completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainingJobStatus {
    /// Job created but not yet started
    Pending,
    /// Job currently executing training
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed during execution
    Failed,
    /// Job cancelled by user or system
    Cancelled,
}

impl TrainingJobStatus {
    /// Whether this status indicates the job is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TrainingJobStatus::Completed | TrainingJobStatus::Failed | TrainingJobStatus::Cancelled
        )
    }

    /// Whether this status allows state transitions
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TrainingJobStatus::Pending | TrainingJobStatus::Running
        )
    }
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

/// Canonical training job information
///
/// Complete training job record including runtime metrics, configuration,
/// and artifact metadata. This is the canonical definition consolidating
/// previous definitions from adapteros-core and adapteros-orchestrator.
///
/// All timestamps are in RFC3339 format (ISO 8601).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingJob {
    /// Unique training job identifier
    #[serde(rename = "id")]
    pub id: String,

    /// Name of the adapter being trained
    #[serde(rename = "adapter_name")]
    pub adapter_name: String,

    /// Optional reference to training template used
    #[serde(rename = "template_id")]
    pub template_id: Option<String>,

    /// Optional reference to source repository
    #[serde(rename = "repo_id")]
    pub repo_id: Option<String>,

    /// Human-readable repository name (for artifact pathing)
    #[serde(rename = "repo_name", skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,

    /// Target branch for the produced adapter version
    #[serde(rename = "target_branch", skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,

    /// Optional parent adapter version (for finetuning)
    #[serde(rename = "base_version_id", skip_serializing_if = "Option::is_none")]
    pub base_version_id: Option<String>,

    /// Draft adapter version created for this training job
    #[serde(rename = "draft_version_id", skip_serializing_if = "Option::is_none")]
    pub draft_version_id: Option<String>,

    /// Adapter version ID created for this training run
    #[serde(rename = "adapter_version_id", skip_serializing_if = "Option::is_none")]
    pub adapter_version_id: Option<String>,

    /// Final produced adapter version (after promotion)
    #[serde(
        rename = "produced_version_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub produced_version_id: Option<String>,

    /// Human-friendly version label (per branch)
    #[serde(rename = "version_label", skip_serializing_if = "Option::is_none")]
    pub version_label: Option<String>,

    /// Source control commit used during training
    #[serde(rename = "code_commit_sha", skip_serializing_if = "Option::is_none")]
    pub code_commit_sha: Option<String>,

    /// Raw data specification JSON (normalized)
    #[serde(rename = "data_spec_json", skip_serializing_if = "Option::is_none")]
    pub data_spec_json: Option<String>,

    /// BLAKE3 hash of data_spec_json
    #[serde(rename = "data_spec_hash", skip_serializing_if = "Option::is_none")]
    pub data_spec_hash: Option<String>,

    /// Optional reference to training dataset
    #[serde(rename = "dataset_id")]
    pub dataset_id: Option<String>,

    /// Correlation ID for tracing dataset -> training -> inference
    #[serde(rename = "correlation_id", skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,

    /// Dataset versions used for training (with optional sampling weights)
    #[serde(
        rename = "dataset_version_ids",
        skip_serializing_if = "Option::is_none"
    )]
    pub dataset_version_ids: Option<Vec<DatasetVersionSelection>>,
    /// Trust snapshot for dataset versions at job creation time.
    #[serde(
        rename = "dataset_version_trust",
        skip_serializing_if = "Option::is_none"
    )]
    pub dataset_version_trust: Option<Vec<DatasetVersionTrustSnapshot>>,
    /// BLAKE3 hash of the dataset manifest used for training (combined when multi-dataset).
    #[serde(rename = "dataset_hash_b3", skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
    /// Whether this training run explicitly opted into synthetic/diagnostic data
    #[serde(rename = "synthetic_mode", default)]
    pub synthetic_mode: bool,
    /// Data lineage quality for this job
    #[serde(rename = "data_lineage_mode", skip_serializing_if = "Option::is_none")]
    pub data_lineage_mode: Option<DataLineageMode>,

    /// Optional reference to base model used for training
    #[serde(rename = "base_model_id", skip_serializing_if = "Option::is_none")]
    pub base_model_id: Option<String>,

    /// Optional reference to document collection used
    #[serde(rename = "collection_id", skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,

    /// Build ID for CI/CD traceability (git commit, version, etc.)
    #[serde(rename = "build_id", skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,

    /// Immutable snapshot of source documents used for training (JSON)
    #[serde(
        rename = "source_documents_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_documents_json: Option<String>,

    /// BLAKE3 hash of training config for reproducibility
    #[serde(rename = "config_hash_b3", skip_serializing_if = "Option::is_none")]
    pub config_hash_b3: Option<String>,

    /// Current job status in lifecycle
    #[serde(rename = "status")]
    pub status: TrainingJobStatus,

    /// Training progress percentage [0.0, 100.0]
    #[serde(rename = "progress_pct")]
    pub progress_pct: f32,

    /// Current epoch in training (0-indexed)
    #[serde(rename = "current_epoch")]
    pub current_epoch: u32,

    /// Total epochs configured for training
    #[serde(rename = "total_epochs")]
    pub total_epochs: u32,

    /// Current loss value (lower is better)
    #[serde(rename = "current_loss")]
    pub current_loss: f32,

    /// Learning rate for current training session
    #[serde(rename = "learning_rate")]
    pub learning_rate: f32,

    /// Throughput metric: tokens processed per second
    #[serde(rename = "tokens_per_second")]
    pub tokens_per_second: f32,

    /// Job creation timestamp (RFC3339)
    #[serde(rename = "created_at")]
    pub created_at: String,

    /// Job start timestamp if running (RFC3339)
    #[serde(rename = "started_at", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,

    /// Job completion timestamp if terminal (RFC3339)
    #[serde(rename = "completed_at", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,

    /// Error message if failed
    #[serde(rename = "error_message", skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Structured error code for failed jobs (e.g., "TRAIN_E101_GPU_OOM").
    /// Use `aosctl explain <code>` for remediation guidance.
    #[serde(rename = "error_code", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,

    /// Training hyperparameters
    #[serde(rename = "config")]
    pub config: TrainingConfig,

    /// Path to generated adapter artifact (.aos file)
    #[serde(rename = "artifact_path", skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,

    /// Adapter ID after packaging (populated on completion)
    #[serde(rename = "adapter_id", skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,

    /// BLAKE3 hash of adapter weights (for verification)
    #[serde(rename = "weights_hash_b3", skip_serializing_if = "Option::is_none")]
    pub weights_hash_b3: Option<String>,

    /// Tenant ID that owns this training job
    #[serde(rename = "tenant_id", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Stack ID created for this adapter (populated on completion)
    #[serde(rename = "stack_id", skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,

    /// User who initiated this training job (for audit logging)
    #[serde(rename = "initiated_by", skip_serializing_if = "Option::is_none")]
    pub initiated_by: Option<String>,

    /// Role of user who initiated this training job (for audit logging)
    #[serde(rename = "initiated_by_role", skip_serializing_if = "Option::is_none")]
    pub initiated_by_role: Option<String>,

    // Category metadata for adapter training
    /// Adapter category: code, framework, codebase, docs, domain
    #[serde(rename = "category", skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Human-readable description of the adapter
    #[serde(rename = "description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Programming language (for code adapters)
    #[serde(rename = "language", skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Symbol targets (for code adapters) - JSON array
    #[serde(
        rename = "symbol_targets_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub symbol_targets_json: Option<String>,

    /// Framework ID (for framework adapters)
    #[serde(rename = "framework_id", skip_serializing_if = "Option::is_none")]
    pub framework_id: Option<String>,

    /// Framework version (for framework adapters)
    #[serde(rename = "framework_version", skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,
    /// Marketing/operational tier for routing (micro/standard/max)
    #[serde(rename = "lora_tier", skip_serializing_if = "Option::is_none")]
    pub lora_tier: Option<LoraTier>,
    /// Optional adapter strength multiplier [0.0, 1.0]
    #[serde(rename = "lora_strength", skip_serializing_if = "Option::is_none")]
    pub lora_strength: Option<f32>,
    /// Logical scope for adapter visibility (e.g., project, tenant)
    #[serde(rename = "scope", skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// API patterns to focus on (for framework adapters) - JSON array
    #[serde(rename = "api_patterns_json", skip_serializing_if = "Option::is_none")]
    pub api_patterns_json: Option<String>,

    /// Repository scope (for codebase adapters)
    #[serde(rename = "repo_scope", skip_serializing_if = "Option::is_none")]
    pub repo_scope: Option<String>,

    /// File patterns to include (for codebase adapters) - JSON array
    #[serde(rename = "file_patterns_json", skip_serializing_if = "Option::is_none")]
    pub file_patterns_json: Option<String>,

    /// File patterns to exclude (for codebase adapters) - JSON array
    #[serde(
        rename = "exclude_patterns_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub exclude_patterns_json: Option<String>,

    /// Post-training actions configuration - JSON
    #[serde(rename = "post_actions_json", skip_serializing_if = "Option::is_none")]
    pub post_actions_json: Option<String>,

    /// Whether failed job can be retried
    #[serde(rename = "retryable", skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,

    /// If this is a retry, the original job ID
    #[serde(rename = "retry_of_job_id", skip_serializing_if = "Option::is_none")]
    pub retry_of_job_id: Option<String>,

    // Backend and determinism (new)
    /// Backend requested by caller (audit/export signal)
    #[serde(rename = "requested_backend", skip_serializing_if = "Option::is_none")]
    pub requested_backend: Option<String>,
    /// Backend policy requested by caller (audit/export signal)
    #[serde(rename = "backend_policy", skip_serializing_if = "Option::is_none")]
    pub backend_policy: Option<String>,
    /// Explicit fallback when CoreML was requested (audit/export signal)
    #[serde(
        rename = "coreml_training_fallback",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_training_fallback: Option<String>,
    /// Backend selected by trainer (CoreML (ANE), Metal, MLX, CPU)
    #[serde(rename = "backend", skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Reason/notes for backend selection or fallback
    #[serde(rename = "backend_reason", skip_serializing_if = "Option::is_none")]
    pub backend_reason: Option<String>,
    /// Hardware/device identifier for the selected backend (if available)
    #[serde(rename = "backend_device", skip_serializing_if = "Option::is_none")]
    pub backend_device: Option<String>,
    /// Whether a CoreML export was requested for this job
    #[serde(
        rename = "coreml_export_requested",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_export_requested: Option<bool>,
    /// Current CoreML export status: pending/running/succeeded/metadata_only/failed/skipped
    #[serde(
        rename = "coreml_export_status",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_export_status: Option<String>,
    /// Reason for CoreML export failure or skip
    #[serde(
        rename = "coreml_export_reason",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_export_reason: Option<String>,
    /// Hash of the fused CoreML package/manifest
    #[serde(
        rename = "coreml_fused_package_hash",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_fused_package_hash: Option<String>,
    /// Path to the fused CoreML package
    #[serde(
        rename = "coreml_package_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_package_path: Option<String>,
    /// Path to CoreML export metadata JSON
    #[serde(
        rename = "coreml_metadata_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_metadata_path: Option<String>,
    /// Hash of the base CoreML manifest used for fusion
    #[serde(
        rename = "coreml_base_manifest_hash",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_base_manifest_hash: Option<String>,
    /// Hash of the adapter payload used for fusion
    #[serde(
        rename = "coreml_adapter_hash_b3",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_adapter_hash_b3: Option<String>,
    /// Whether the CoreML export used fusion verification.
    /// If false, the package may be stub/metadata-only and not production-ready.
    #[serde(
        rename = "coreml_fusion_verified",
        skip_serializing_if = "Option::is_none"
    )]
    pub coreml_fusion_verified: Option<bool>,
    /// Determinism mode (e.g., hkdf_seeded, nondet_fallback)
    #[serde(rename = "determinism_mode", skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// 64-bit deterministic training seed (audit)
    #[serde(rename = "training_seed", skip_serializing_if = "Option::is_none")]
    pub training_seed: Option<u64>,
    /// Seed derivation inputs captured for determinism auditing (JSON)
    #[serde(rename = "seed_inputs_json", skip_serializing_if = "Option::is_none")]
    pub seed_inputs_json: Option<String>,
    /// Whether GPU was required for this run
    #[serde(rename = "require_gpu", skip_serializing_if = "Option::is_none")]
    pub require_gpu: Option<bool>,
    /// Max GPU memory budget in MB (0 = unlimited)
    #[serde(rename = "max_gpu_memory_mb", skip_serializing_if = "Option::is_none")]
    pub max_gpu_memory_mb: Option<u64>,

    // Extended metrics (new)
    /// Total examples processed
    #[serde(rename = "examples_processed", skip_serializing_if = "Option::is_none")]
    pub examples_processed: Option<u64>,
    /// Total tokens processed (if available)
    #[serde(rename = "tokens_processed", skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<u64>,
    /// Wall-clock training time (ms)
    #[serde(rename = "training_time_ms", skip_serializing_if = "Option::is_none")]
    pub training_time_ms: Option<u64>,
    /// Examples/second throughput
    #[serde(
        rename = "throughput_examples_per_sec",
        skip_serializing_if = "Option::is_none"
    )]
    pub throughput_examples_per_sec: Option<f32>,
    /// Average GPU utilization percentage
    #[serde(
        rename = "gpu_utilization_pct",
        skip_serializing_if = "Option::is_none"
    )]
    pub gpu_utilization_pct: Option<f32>,
    /// Peak GPU memory usage (MB)
    #[serde(rename = "peak_gpu_memory_mb", skip_serializing_if = "Option::is_none")]
    pub peak_gpu_memory_mb: Option<f32>,

    // Packaging summary (new)
    /// Path to .aos archive (if packaged)
    #[serde(rename = "aos_path", skip_serializing_if = "Option::is_none")]
    pub aos_path: Option<String>,
    /// Hash of packaged archive (BLAKE3)
    #[serde(rename = "package_hash_b3", skip_serializing_if = "Option::is_none")]
    pub package_hash_b3: Option<String>,
    /// Manifest hash BLAKE3 for the packaged adapter (alias for artifact hash when available)
    #[serde(rename = "manifest_hash_b3", skip_serializing_if = "Option::is_none")]
    pub manifest_hash_b3: Option<String>,
    /// Adapter manifest rank (if available)
    #[serde(rename = "manifest_rank", skip_serializing_if = "Option::is_none")]
    pub manifest_rank: Option<u32>,
    /// Adapter manifest base model (if available)
    #[serde(
        rename = "manifest_base_model",
        skip_serializing_if = "Option::is_none"
    )]
    pub manifest_base_model: Option<String>,
    /// Whether per-layer hashes are present in manifest
    #[serde(
        rename = "manifest_per_layer_hashes",
        skip_serializing_if = "Option::is_none"
    )]
    pub manifest_per_layer_hashes: Option<bool>,
    /// Signature verification status for package
    #[serde(rename = "signature_status", skip_serializing_if = "Option::is_none")]
    pub signature_status: Option<String>,
}

impl TrainingJob {
    /// Create a new training job in Pending state
    pub fn new(id: String, adapter_name: String, config: TrainingConfig) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        let requested_backend = config.preferred_backend.map(|b| b.as_str().to_string());
        let coreml_training_fallback = config
            .coreml_training_fallback
            .map(|b| b.as_str().to_string());
        let backend_policy = config.backend_policy.map(|p| p.as_str().to_string());
        Self {
            id,
            adapter_name,
            template_id: None,
            repo_id: None,
            repo_name: None,
            target_branch: None,
            base_version_id: None,
            draft_version_id: None,
            adapter_version_id: None,
            produced_version_id: None,
            version_label: None,
            code_commit_sha: None,
            data_spec_json: None,
            data_spec_hash: None,
            dataset_id: None,
            correlation_id: None,
            dataset_version_ids: None,
            dataset_version_trust: None,
            dataset_hash_b3: None,
            synthetic_mode: false,
            data_lineage_mode: None,
            base_model_id: None,
            collection_id: None,
            build_id: None,
            source_documents_json: None,
            config_hash_b3: None,
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
            error_code: None,
            config,
            artifact_path: None,
            adapter_id: None,
            weights_hash_b3: None,
            tenant_id: None,
            stack_id: None,
            initiated_by: None,
            initiated_by_role: None,
            // Category metadata
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
            post_actions_json: None,
            // Retry metadata
            retryable: None,
            retry_of_job_id: None,
            // Backend/determinism defaults
            requested_backend,
            backend_policy,
            coreml_training_fallback,
            backend: None,
            backend_reason: None,
            backend_device: None,
            coreml_export_requested: None,
            coreml_export_status: None,
            coreml_export_reason: None,
            coreml_fused_package_hash: None,
            coreml_package_path: None,
            coreml_metadata_path: None,
            coreml_base_manifest_hash: None,
            coreml_adapter_hash_b3: None,
            coreml_fusion_verified: None,
            determinism_mode: None,
            training_seed: None,
            seed_inputs_json: None,
            require_gpu: None,
            max_gpu_memory_mb: None,
            // Extended metrics defaults
            examples_processed: None,
            tokens_processed: None,
            training_time_ms: None,
            throughput_examples_per_sec: None,
            gpu_utilization_pct: None,
            peak_gpu_memory_mb: None,
            // Packaging defaults
            aos_path: None,
            package_hash_b3: None,
            manifest_hash_b3: None,
            manifest_rank: None,
            manifest_base_model: None,
            manifest_per_layer_hashes: None,
            signature_status: None,
        }
    }

    /// Builder method to set template ID
    pub fn with_template_id(mut self, template_id: String) -> Self {
        self.template_id = Some(template_id);
        self
    }

    /// Builder method to set repository ID
    pub fn with_repo_id(mut self, repo_id: String) -> Self {
        self.repo_id = Some(repo_id);
        self
    }

    /// Builder method to set dataset ID
    pub fn with_dataset_id(mut self, dataset_id: String) -> Self {
        self.dataset_id = Some(dataset_id);
        self
    }

    /// Set dataset versions (with weights) used for training
    pub fn with_dataset_versions(mut self, dataset_versions: Vec<DatasetVersionSelection>) -> Self {
        self.dataset_version_ids = Some(dataset_versions);
        self
    }

    /// Builder method to set tenant ID
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Builder method to set artifact path
    pub fn with_artifact_path(mut self, artifact_path: String) -> Self {
        self.artifact_path = Some(artifact_path);
        self
    }

    /// Builder method to set adapter ID
    pub fn with_adapter_id(mut self, adapter_id: String) -> Self {
        self.adapter_id = Some(adapter_id);
        self
    }

    /// Builder method to set weights hash
    pub fn with_weights_hash(mut self, hash: String) -> Self {
        self.weights_hash_b3 = Some(hash);
        self
    }

    /// Builder method to set base model ID
    pub fn with_base_model_id(mut self, base_model_id: String) -> Self {
        self.base_model_id = Some(base_model_id);
        self
    }

    /// Builder method to set collection ID
    pub fn with_collection_id(mut self, collection_id: String) -> Self {
        self.collection_id = Some(collection_id);
        self
    }

    /// Builder method to set build ID
    pub fn with_build_id(mut self, build_id: String) -> Self {
        self.build_id = Some(build_id);
        self
    }

    /// Builder method to set source documents JSON
    pub fn with_source_documents(mut self, source_documents_json: String) -> Self {
        self.source_documents_json = Some(source_documents_json);
        self
    }

    /// Builder method to set config hash
    pub fn with_config_hash(mut self, config_hash: String) -> Self {
        self.config_hash_b3 = Some(config_hash);
        self
    }

    /// Builder method to set category
    pub fn with_category(mut self, category: String) -> Self {
        self.category = Some(category);
        self
    }

    /// Builder method to set description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Builder method to set language
    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    /// Builder method to set framework ID
    pub fn with_framework_id(mut self, framework_id: String) -> Self {
        self.framework_id = Some(framework_id);
        self
    }

    /// Builder method to set framework version
    pub fn with_framework_version(mut self, framework_version: String) -> Self {
        self.framework_version = Some(framework_version);
        self
    }

    /// Builder method to set post actions JSON
    pub fn with_post_actions_json(mut self, post_actions_json: String) -> Self {
        self.post_actions_json = Some(post_actions_json);
        self
    }

    /// Mark job as started (transition from Pending to Running)
    pub fn start(&mut self) {
        if self.status == TrainingJobStatus::Pending {
            self.status = TrainingJobStatus::Running;
            self.started_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Update training progress (typically called per epoch)
    pub fn update_progress(&mut self, epoch: u32, loss: f32, tokens_per_sec: f32) {
        self.current_epoch = epoch;
        self.current_loss = loss;
        self.tokens_per_second = tokens_per_sec;
        if self.total_epochs > 0 {
            self.progress_pct = (epoch as f32 / self.total_epochs as f32) * 100.0;
        }
        if self.status == TrainingJobStatus::Pending {
            self.start();
        }
    }

    /// Mark job as completed
    pub fn complete(&mut self) {
        self.status = TrainingJobStatus::Completed;
        self.progress_pct = 100.0;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Mark job as failed with error message
    pub fn fail(&mut self, error: String) {
        self.status = TrainingJobStatus::Failed;
        self.error_message = Some(error);
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Cancel job
    pub fn cancel(&mut self) {
        if self.status.is_active() {
            self.status = TrainingJobStatus::Cancelled;
            self.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Check if job is in terminal state
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Get elapsed time in seconds since creation
    pub fn elapsed_seconds(&self) -> Option<u64> {
        use chrono::DateTime;

        let created = DateTime::parse_from_rfc3339(&self.created_at).ok()?;
        let reference = match &self.completed_at {
            Some(completed) => DateTime::parse_from_rfc3339(completed).ok()?,
            None => chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0)?),
        };

        let duration = reference.signed_duration_since(created);
        Some(duration.num_seconds() as u64)
    }
}

/// Optional compression choices for cached preprocessing tensors.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PreprocessCompression {
    /// No compression (store f32 tensors).
    None,
    /// Q15 fixed-point compression (i16 + scale).
    Q15,
}

impl PreprocessCompression {
    /// Return the canonical string identifier for this compression choice.
    pub fn as_str(&self) -> &'static str {
        match self {
            PreprocessCompression::None => "none",
            PreprocessCompression::Q15 => "q15",
        }
    }
}

/// Output feature selection for preprocessing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PreprocessOutputFeature {
    /// Emit per-token embedding features.
    Embedding,
    /// Emit the last hidden state token.
    HiddenStateLast,
    /// Emit a pooled (mean) hidden state.
    #[default]
    Pooled,
}

impl PreprocessOutputFeature {
    /// Return the canonical string identifier for this output feature.
    pub fn as_str(&self) -> &'static str {
        match self {
            PreprocessOutputFeature::Embedding => "embedding",
            PreprocessOutputFeature::HiddenStateLast => "hidden_state_last",
            PreprocessOutputFeature::Pooled => "pooled",
        }
    }
}

/// Optional preprocessing stage for tokenized inputs.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct PreprocessingConfigV1 {
    /// Explicitly enable preprocessing when set to true.
    #[serde(default)]
    pub enabled: bool,
    /// Optional CoreML model identifier (resolved via model cache).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_model_id: Option<String>,
    /// Optional CoreML model path for preprocessing (mlpackage or mlmodelc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub coreml_model_path: Option<std::path::PathBuf>,
    /// Output feature selection (embedding/hidden_state_last/pooled).
    #[serde(default)]
    pub output_feature: PreprocessOutputFeature,
    /// Layer key aligned to hidden_state_layer naming (optional override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_key: Option<String>,
    /// Maximum sequence length for preprocessing (0 = use input length).
    #[serde(default)]
    pub max_seq_len: u32,
    /// Batch size hint for preprocessing (0 = no batching).
    #[serde(default)]
    pub batch_size: u32,
    /// Optional feature compression to apply to cached tensors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<PreprocessCompression>,
    /// Optional cache directory override (defaults to dataset artifacts root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub cache_dir: Option<std::path::PathBuf>,
    /// Optional seed to pin preprocessing determinism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

/// Current preprocessing config type alias (v1).
pub type PreprocessingConfig = PreprocessingConfigV1;

/// Training hyperparameters and configuration
///
/// Defines LoRA training configuration including rank, alpha scaling,
/// target modules, optimization parameters, and advanced options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingConfig {
    /// LoRA rank dimension (typically 4, 8, 16, 32)
    #[serde(rename = "rank")]
    pub rank: u32,

    /// LoRA alpha scaling factor (typically 2x rank)
    #[serde(rename = "alpha")]
    pub alpha: u32,

    /// Target linear layer names to apply LoRA
    #[serde(rename = "targets")]
    pub targets: Vec<String>,

    /// Training data contract version.
    #[serde(rename = "training_contract_version")]
    pub training_contract_version: String,

    /// Explicit pad token ID.
    #[serde(rename = "pad_token_id")]
    pub pad_token_id: u32,

    /// Explicit ignore index for loss masking (-1 disables masking).
    #[serde(rename = "ignore_index")]
    pub ignore_index: i32,

    /// Optional CoreML placement spec to align training/inference
    #[serde(rename = "coreml_placement", skip_serializing_if = "Option::is_none")]
    pub coreml_placement: Option<CoreMLPlacementSpec>,

    /// Number of training epochs
    #[serde(rename = "epochs")]
    pub epochs: u32,

    /// Learning rate for optimizer (e.g., 0.001)
    #[serde(rename = "learning_rate")]
    pub learning_rate: f32,

    /// Batch size for training
    #[serde(rename = "batch_size")]
    pub batch_size: u32,

    /// Warmup steps for learning rate schedule (optional)
    #[serde(rename = "warmup_steps", skip_serializing_if = "Option::is_none")]
    pub warmup_steps: Option<u32>,

    /// Maximum sequence length (optional, default 2048)
    #[serde(rename = "max_seq_length", skip_serializing_if = "Option::is_none")]
    pub max_seq_length: Option<u32>,

    /// Gradient accumulation steps for larger effective batch size (optional)
    #[serde(
        rename = "gradient_accumulation_steps",
        skip_serializing_if = "Option::is_none"
    )]
    pub gradient_accumulation_steps: Option<u32>,

    /// Preferred training backend (coreml, mlx, metal, cpu). None = auto.
    ///
    /// CoreML is an inference/export target only; training requests that set
    /// `preferred_backend = coreml` will train on a fallback backend instead
    /// (see `coreml_training_fallback`) and record that decision for audit.
    #[serde(
        rename = "preferred_backend",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub preferred_backend: Option<TrainingBackendKind>,

    /// Backend policy for CoreML preference and fallback semantics.
    #[serde(
        rename = "backend_policy",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub backend_policy: Option<TrainingBackendPolicy>,

    /// Explicit fallback to use when CoreML is requested for training. If
    /// unset, the worker applies a deterministic policy of MLX → Metal →
    /// CPU (when GPU is optional). This field is ignored when the preferred
    /// backend is not CoreML.
    #[serde(
        rename = "coreml_training_fallback",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub coreml_training_fallback: Option<TrainingBackendKind>,

    /// Request a CoreML export after successful training (optional, default: false)
    #[serde(
        rename = "enable_coreml_export",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub enable_coreml_export: Option<bool>,

    /// Require GPU acceleration (error if no GPU backend can be initialized)
    #[serde(rename = "require_gpu", default)]
    pub require_gpu: bool,

    /// Maximum GPU memory to use in MB (0/unset = unlimited, best-effort)
    #[serde(
        rename = "max_gpu_memory_mb",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub max_gpu_memory_mb: Option<u64>,

    /// Advanced weight group configuration (optional, format TBD)
    #[serde(
        rename = "weight_group_config",
        skip_serializing_if = "Option::is_none"
    )]
    pub weight_group_config: Option<serde_json::Value>,

    /// Learning rate schedule type (constant, linear, cosine)
    #[serde(rename = "lr_schedule", skip_serializing_if = "Option::is_none")]
    pub lr_schedule: Option<String>,

    /// Final learning rate for decay schedules
    #[serde(rename = "final_lr", skip_serializing_if = "Option::is_none")]
    pub final_lr: Option<f32>,

    /// Enable early stopping
    #[serde(rename = "early_stopping", skip_serializing_if = "Option::is_none")]
    pub early_stopping: Option<bool>,

    /// Early stopping patience (epochs to wait)
    #[serde(rename = "patience", skip_serializing_if = "Option::is_none")]
    pub patience: Option<u32>,

    /// Minimum delta for early stopping improvement
    #[serde(rename = "min_delta", skip_serializing_if = "Option::is_none")]
    pub min_delta: Option<f32>,

    /// Save checkpoints every N epochs
    #[serde(
        rename = "checkpoint_frequency",
        skip_serializing_if = "Option::is_none"
    )]
    pub checkpoint_frequency: Option<u32>,

    /// Maximum number of checkpoints to keep
    #[serde(rename = "max_checkpoints", skip_serializing_if = "Option::is_none")]
    pub max_checkpoints: Option<u32>,

    /// Path to base model for hidden state extraction (required for training)
    #[serde(rename = "base_model_path", skip_serializing_if = "Option::is_none")]
    pub base_model_path: Option<std::path::PathBuf>,

    /// Hidden state layer to extract from base model (e.g., "model.layers.31.output")
    #[serde(rename = "hidden_state_layer", skip_serializing_if = "Option::is_none")]
    pub hidden_state_layer: Option<String>,

    /// Fraction of dataset to use for validation (0.0-0.5, default 0.0 = no validation)
    #[serde(rename = "validation_split", skip_serializing_if = "Option::is_none")]
    pub validation_split: Option<f32>,
    /// Optional CoreML preprocessing stage for tokenized inputs (disabled by default).
    #[serde(rename = "preprocessing", skip_serializing_if = "Option::is_none")]
    pub preprocessing: Option<PreprocessingConfig>,
    /// Force resume even when pipeline/checkpoint compatibility checks fail.
    #[serde(rename = "force_resume", default)]
    pub force_resume: bool,

    /// Enable multi-module training (train separate weights per target module).
    /// When false (default), trains a single A/B pair applied to all targets.
    /// When true, trains separate LoRA weights for each module in `targets`.
    #[serde(rename = "multi_module_training", default)]
    pub multi_module_training: bool,

    /// Layer indices for LoRA injection (e.g., [0, 8, 16, 24, 31]).
    /// If empty, defaults to last layer only for backward compatibility.
    /// When combined with multi_module_training, trains separate weights for each (layer, module) pair.
    #[serde(rename = "lora_layer_indices", default)]
    pub lora_layer_indices: Vec<usize>,
}

impl TrainingConfig {
    /// Create default training configuration
    pub fn default_for_adapter() -> Self {
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
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: -100,
            coreml_placement: None,
            epochs: 3,
            learning_rate: 0.001,
            batch_size: 32,
            warmup_steps: Some(100),
            max_seq_length: Some(2048),
            gradient_accumulation_steps: Some(4),
            weight_group_config: None,
            lr_schedule: Some("cosine".to_string()),
            final_lr: Some(0.0001),
            early_stopping: Some(false),
            patience: Some(5),
            min_delta: Some(0.001),
            checkpoint_frequency: Some(5),
            max_checkpoints: Some(3),
            preferred_backend: None,
            backend_policy: None,
            coreml_training_fallback: None,
            enable_coreml_export: None,
            require_gpu: false,
            max_gpu_memory_mb: None,
            base_model_path: None,
            hidden_state_layer: None,
            validation_split: None,
            preprocessing: None,
            force_resume: false,
            multi_module_training: false,
            lora_layer_indices: Vec::new(),
        }
    }

    /// Create minimal quick-training configuration
    pub fn quick_training() -> Self {
        Self {
            rank: 8,
            alpha: 16,
            targets: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
            ],
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: -100,
            coreml_placement: None,
            epochs: 1,
            learning_rate: 0.002,
            batch_size: 16,
            warmup_steps: None,
            max_seq_length: Some(2048),
            gradient_accumulation_steps: None,
            weight_group_config: None,
            lr_schedule: Some("constant".to_string()),
            final_lr: None,
            early_stopping: Some(false),
            patience: None,
            min_delta: None,
            checkpoint_frequency: None,
            max_checkpoints: None,
            preferred_backend: None,
            backend_policy: None,
            coreml_training_fallback: None,
            enable_coreml_export: None,
            require_gpu: false,
            max_gpu_memory_mb: None,
            base_model_path: None,
            hidden_state_layer: None,
            validation_split: None,
            preprocessing: None,
            force_resume: false,
            multi_module_training: false,
            lora_layer_indices: Vec::new(),
        }
    }

    /// Create deep training configuration
    pub fn deep_training() -> Self {
        Self {
            rank: 32,
            alpha: 64,
            targets: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
                "o_proj".to_string(),
                "gate_proj".to_string(),
                "up_proj".to_string(),
                "down_proj".to_string(),
                "mlp.dense_h_to_4h".to_string(),
                "mlp.dense_4h_to_h".to_string(),
            ],
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: -100,
            coreml_placement: None,
            epochs: 5,
            learning_rate: 0.0005,
            batch_size: 64,
            warmup_steps: Some(500),
            max_seq_length: Some(4096),
            gradient_accumulation_steps: Some(8),
            weight_group_config: None,
            lr_schedule: Some("linear".to_string()),
            final_lr: Some(0.00001),
            early_stopping: Some(true),
            patience: Some(10),
            min_delta: Some(0.0001),
            checkpoint_frequency: Some(2),
            max_checkpoints: Some(5),
            preferred_backend: None,
            backend_policy: None,
            coreml_training_fallback: None,
            enable_coreml_export: None,
            require_gpu: false,
            max_gpu_memory_mb: None,
            base_model_path: None,
            hidden_state_layer: None,
            validation_split: None,
            preprocessing: None,
            force_resume: false,
            multi_module_training: false,
            lora_layer_indices: Vec::new(),
        }
    }

    /// Normalize the config for deterministic hashing and reproducibility.
    ///
    /// This ensures two logically-equivalent configs produce the same hash
    /// regardless of field ordering or cosmetic differences:
    /// - Sorts `targets` alphabetically
    /// - Sorts `lora_layer_indices` numerically and deduplicates
    /// - Clamps `learning_rate` to \[1e-7, 1.0\] (NaN/Inf → 0.001)
    /// - Clamps `batch_size` to minimum 1
    /// - Clamps `epochs` to minimum 1
    /// - Fills empty `training_contract_version` with current default
    pub fn normalize(&mut self) -> &mut Self {
        self.targets.sort();
        self.targets.dedup();

        self.lora_layer_indices.sort();
        self.lora_layer_indices.dedup();

        if self.learning_rate.is_nan() || self.learning_rate.is_infinite() {
            self.learning_rate = 0.001;
        } else {
            self.learning_rate = self.learning_rate.clamp(1e-7, 1.0);
        }

        if self.batch_size == 0 {
            self.batch_size = 1;
        }
        if self.epochs == 0 {
            self.epochs = 1;
        }

        if self.training_contract_version.is_empty() {
            self.training_contract_version = TRAINING_DATA_CONTRACT_VERSION.to_string();
        }

        self
    }
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self::default_for_adapter()
    }
}

/// Training template for reusable configurations
///
/// Provides pre-configured training setups for common scenarios.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingTemplate {
    /// Unique template identifier
    #[serde(rename = "id")]
    pub id: String,

    /// Human-readable template name
    #[serde(rename = "name")]
    pub name: String,

    /// Template description and use cases
    #[serde(rename = "description")]
    pub description: String,

    /// Template category (e.g., "code", "creative", "domain-specific")
    #[serde(rename = "category")]
    pub category: String,

    /// Embedded training configuration
    #[serde(rename = "config")]
    pub config: TrainingConfig,
}

impl TrainingTemplate {
    /// Create a new training template
    pub fn new(
        id: String,
        name: String,
        description: String,
        category: String,
        config: TrainingConfig,
    ) -> Self {
        Self {
            id,
            name,
            description,
            category,
            config,
        }
    }
}

/// Optimizer configuration summary for reports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct OptimizerConfigSummary {
    /// Optimizer type (adam, adamw, sgd).
    pub optimizer_type: String,
    /// First moment decay (Adam/AdamW).
    pub beta1: f32,
    /// Second moment decay (Adam/AdamW).
    pub beta2: f32,
    /// Numerical stability epsilon.
    pub epsilon: f32,
    /// Weight decay factor.
    pub weight_decay: f32,
    /// Momentum factor (SGD).
    pub momentum: f32,
}

/// Curve metrics captured in a training report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingReportCurves {
    /// Training loss per step.
    pub train_loss: Vec<f32>,
    /// Training perplexity per step.
    pub train_ppl: Vec<f32>,
    /// Validation loss per step.
    pub val_loss: Vec<f32>,
    /// Validation perplexity per step.
    pub val_ppl: Vec<f32>,
}

/// Summary metrics captured in a training report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingReportSummary {
    /// Best epoch observed during training.
    pub best_epoch: u32,
    /// Final epoch completed.
    pub final_epoch: u32,
    /// Whether early stopping triggered.
    pub early_stopped: bool,
    /// Total optimization steps.
    pub total_steps: u64,
    /// Total tokens processed.
    pub total_tokens: u64,
}

/// Metric definitions to avoid "mystery numbers" in reports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingReportMetricDefinitions {
    /// Definition for train_loss curve values.
    pub train_loss: String,
    /// Definition for train_ppl curve values.
    pub train_ppl: String,
    /// Definition for val_loss curve values.
    pub val_loss: String,
    /// Definition for val_ppl curve values.
    pub val_ppl: String,
    /// Definition for best_epoch summary value.
    pub best_epoch: String,
    /// Definition for final_epoch summary value.
    pub final_epoch: String,
    /// Definition for early_stopped summary value.
    pub early_stopped: String,
    /// Definition for total_steps summary value.
    pub total_steps: String,
    /// Definition for total_tokens summary value.
    pub total_tokens: String,
}

/// Training report artifact (v1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TrainingReportV1 {
    #[serde(default = "default_training_report_version")]
    /// Report schema version.
    pub report_version: u32,
    /// Pipeline identifier for this training run.
    pub pipeline_id: String,
    /// Dataset identifier used for training.
    pub dataset_id: String,
    /// Content hash of the dataset.
    pub dataset_content_hash: String,
    /// Hash of the dataset split definition.
    pub split_hash: String,
    /// Base model identifier.
    pub base_model_id: String,
    /// Base model hash.
    pub base_model_hash: String,
    /// Optimizer configuration summary.
    pub optimizer: OptimizerConfigSummary,
    /// Hash of the training configuration.
    pub training_config_hash: String,
    /// Loss/perplexity curves.
    pub curves: TrainingReportCurves,
    /// Summary metrics.
    pub summary: TrainingReportSummary,
    /// Metric definitions for report fields.
    pub metric_definitions: TrainingReportMetricDefinitions,
    /// Report generation timestamp (unix ms).
    pub generated_at_unix_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_training_job_status_display() {
        assert_eq!(TrainingJobStatus::Pending.to_string(), "pending");
        assert_eq!(TrainingJobStatus::Running.to_string(), "running");
        assert_eq!(TrainingJobStatus::Completed.to_string(), "completed");
        assert_eq!(TrainingJobStatus::Failed.to_string(), "failed");
        assert_eq!(TrainingJobStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_training_job_status_terminal() {
        assert!(!TrainingJobStatus::Pending.is_terminal());
        assert!(!TrainingJobStatus::Running.is_terminal());
        assert!(TrainingJobStatus::Completed.is_terminal());
        assert!(TrainingJobStatus::Failed.is_terminal());
        assert!(TrainingJobStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_training_job_status_active() {
        assert!(TrainingJobStatus::Pending.is_active());
        assert!(TrainingJobStatus::Running.is_active());
        assert!(!TrainingJobStatus::Completed.is_active());
        assert!(!TrainingJobStatus::Failed.is_active());
        assert!(!TrainingJobStatus::Cancelled.is_active());
    }

    #[test]
    fn test_training_job_creation() {
        let config = TrainingConfig::default();
        let job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        assert_eq!(job.id, "job-123");
        assert_eq!(job.adapter_name, "my-adapter");
        assert_eq!(job.status, TrainingJobStatus::Pending);
        assert_eq!(job.progress_pct, 0.0);
        assert_eq!(job.total_epochs, 3);
    }

    #[test]
    fn test_training_job_builder() {
        let config = TrainingConfig::default();
        let job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config)
            .with_template_id("general-code".to_string())
            .with_repo_id("repo-456".to_string())
            .with_dataset_id("ds-789".to_string());

        assert_eq!(job.template_id, Some("general-code".to_string()));
        assert_eq!(job.repo_id, Some("repo-456".to_string()));
        assert_eq!(job.dataset_id, Some("ds-789".to_string()));
    }

    #[test]
    fn test_training_job_lifecycle() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        // Start job
        job.start();
        assert_eq!(job.status, TrainingJobStatus::Running);
        assert!(job.started_at.is_some());

        // Update progress
        job.update_progress(1, 0.5, 1000.0);
        assert_eq!(job.current_epoch, 1);
        assert_eq!(job.current_loss, 0.5);
        assert!(job.progress_pct > 0.0);

        // Complete job
        job.complete();
        assert_eq!(job.status, TrainingJobStatus::Completed);
        assert_eq!(job.progress_pct, 100.0);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_training_job_pause_resume() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        job.start();
        assert_eq!(job.status, TrainingJobStatus::Running);

        // Pause/resume removed; ensure running remains unchanged by no-op pattern
        job.update_progress(1, 0.5, 1000.0);
        assert_eq!(job.status, TrainingJobStatus::Running);
    }

    #[test]
    fn test_training_job_cancel() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        job.start();
        job.cancel();
        assert_eq!(job.status, TrainingJobStatus::Cancelled);
        assert!(job.completed_at.is_some());
        assert!(job.is_terminal());
    }

    #[test]
    fn test_training_job_fail() {
        let config = TrainingConfig::default();
        let mut job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config);

        job.fail("Out of memory".to_string());
        assert_eq!(job.status, TrainingJobStatus::Failed);
        assert_eq!(job.error_message, Some("Out of memory".to_string()));
        assert!(job.is_terminal());
    }

    #[test]
    fn test_training_config_variants() {
        let quick = TrainingConfig::quick_training();
        assert_eq!(quick.rank, 8);
        assert_eq!(quick.epochs, 1);

        let deep = TrainingConfig::deep_training();
        assert_eq!(deep.rank, 32);
        assert_eq!(deep.epochs, 5);

        let default = TrainingConfig::default();
        assert_eq!(default.rank, 16);
        assert_eq!(default.epochs, 3);
    }

    #[test]
    fn test_training_config_serialization() {
        let config = TrainingConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");

        // Verify snake_case serialization
        assert!(json.contains("\"rank\":"));
        assert!(json.contains("\"learning_rate\":"));
        assert!(json.contains("\"batch_size\":"));
        assert!(json.contains("\"warmup_steps\":"));
    }

    #[test]
    fn test_training_job_serialization() {
        let config = TrainingConfig::default();
        let job = TrainingJob::new("job-123".to_string(), "my-adapter".to_string(), config)
            .with_weights_hash("abc123".to_string());

        let json = serde_json::to_value(&job).expect("serialize");

        // Verify snake_case serialization
        assert!(json.get("adapter_name").is_some());
        assert!(json.get("current_epoch").is_some());
        assert!(json.get("progress_pct").is_some());
        assert!(json.get("learning_rate").is_some());
        assert!(json.get("tokens_per_second").is_some());
        assert!(json.get("weights_hash_b3").is_some());
    }
}
