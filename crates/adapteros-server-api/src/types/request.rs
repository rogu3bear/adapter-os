//! Request types for API endpoints.

use adapteros_api_types::{InferRequest, TrainingConfigRequest};
use adapteros_core::{determinism::DeterminismContext, BackendKind, SeedMode};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::training::LoraTier;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::sampling::PlacementReplay;

/// Single request item within a batch inference call
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferItemRequest {
    /// Client-provided identifier used to correlate responses
    pub id: String,
    /// Embedded inference request parameters
    #[serde(flatten)]
    pub request: InferRequest,
}

/// Batch inference request payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferRequest {
    /// Collection of inference requests to run together
    pub requests: Vec<BatchInferItemRequest>,
}

/// Create batch job request (async batch processing)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateBatchJobRequest {
    /// Collection of inference requests to process asynchronously
    pub requests: Vec<BatchInferItemRequest>,
    /// Optional timeout in seconds for the entire batch job
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,
    /// Optional maximum number of concurrent requests to process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<i32>,
}

/// Upsert directory adapter request (synthetic, optional activation)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DirectoryUpsertRequest {
    /// Tenant ID (scopes adapter namespace)
    pub tenant_id: String,
    /// Absolute repository root path
    pub root: String,
    /// Relative path under root to analyze
    pub path: String,
    /// If true, immediately load the adapter after registration
    #[serde(default)]
    pub activate: bool,
}

/// Import model request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportModelRequest {
    pub name: String,
    pub hash_b3: String,
    pub config_hash_b3: String,
    pub tokenizer_hash_b3: String,
    pub tokenizer_cfg_hash_b3: String,
    pub license_hash_b3: Option<String>,
    pub metadata_json: Option<String>,
}

/// Promote CP request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromoteCPRequest {
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
}

/// Rollback CP request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RollbackCPRequest {
    pub tenant_id: String,
    pub cpid: String,
}

/// Validate policy request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ValidatePolicyRequest {
    pub content: String,
}

/// Apply policy request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApplyPolicyRequest {
    pub cpid: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activate: Option<bool>,
}

/// Start debug session request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartDebugSessionRequest {
    pub worker_id: String,
    pub session_type: String,
    pub config_json: String,
}

/// Run troubleshooting step request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RunTroubleshootingStepRequest {
    pub worker_id: String,
    pub step_name: String,
    pub step_type: String,
    pub command: Option<String>,
}

/// Create process template request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessTemplateRequest {
    pub name: String,
    pub description: Option<String>,
    pub config_json: String,
    pub plan_id: Option<String>,
    pub auto_scaling_config_json: Option<String>,
    pub dependencies_json: Option<String>,
}

/// Start bulk operation request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartBulkOperationRequest {
    pub operation_type: String,
    pub target_workers_json: String,
    pub config_json: Option<String>,
}

/// Create auto-scaling rule request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateAutoScalingRuleRequest {
    pub rule_name: String,
    pub metric_type: String,
    pub threshold_value: f64,
    pub threshold_duration_seconds: i32,
    pub scale_action: String,
    pub scale_factor: f64,
    pub min_workers: i32,
    pub max_workers: i32,
    pub cooldown_seconds: i32,
}

/// Start process migration request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartProcessMigrationRequest {
    pub worker_id: String,
    pub target_node_id: String,
    pub migration_type: String,
    pub migration_config_json: Option<String>,
}

/// Create orchestration workflow request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateOrchestrationWorkflowRequest {
    pub name: String,
    pub workflow_type: String,
    pub steps_json: String,
    pub triggers_json: Option<String>,
}

/// Create configuration template request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateConfigTemplateRequest {
    pub name: String,
    pub description: Option<String>,
    pub config_schema_json: String,
    pub default_values_json: Option<String>,
    pub validation_rules_json: Option<String>,
    pub environment_specific_configs_json: Option<String>,
}

/// Create configuration instance request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateConfigInstanceRequest {
    pub template_id: String,
    pub worker_id: String,
    pub environment: String,
    pub config_values_json: String,
}

/// Validate configuration request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ValidateConfigRequest {
    pub instance_id: String,
    pub validation_types: Vec<String>,
}

/// Deploy configuration request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeployConfigRequest {
    pub instance_id: String,
    pub deployment_type: String,
    pub scheduled_at: Option<String>,
    pub deployment_config_json: Option<String>,
}

/// Rollback configuration request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RollbackConfigRequest {
    pub instance_id: String,
    pub target_version: Option<String>,
    pub reason: String,
}

/// Routing debug request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RoutingDebugRequest {
    pub prompt: String,
    pub context: Option<String>,
}

/// Propose patch request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProposePatchRequest {
    pub repo_id: String,
    pub commit_sha: String,
    pub description: String,
    pub target_files: Vec<String>,
}

/// Patch proposal inference request (for UDS communication)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PatchProposalInferRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    pub require_evidence: bool,
    pub request_type: PatchProposalRequestType,
}

/// Patch proposal request type
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PatchProposalRequestType {
    pub repo_id: String,
    pub commit_sha: Option<String>,
    pub target_files: Vec<String>,
    pub description: String,
}

fn default_utf8_healing_worker() -> bool {
    true
}

/// Worker inference request (for UDS communication)
///
/// Includes all sampling parameters required for deterministic replay.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerInferRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    /// Optional request ID for worker-side tracing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Canonical run envelope for worker observability
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,
    pub require_evidence: bool,
    /// Admin override for cluster routing restrictions
    #[serde(default)]
    pub admin_override: bool,
    /// Enable reasoning-aware routing mid-generation
    #[serde(default)]
    pub reasoning_mode: bool,
    /// Stack identifier associated with this inference (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_version: Option<i64>,
    /// Domain hint used for routing/package selection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_hint: Option<String>,
    /// Sampling temperature (0.0 = deterministic, higher = more random)
    #[serde(default)]
    pub temperature: f32,
    /// Top-K sampling (limits vocabulary to K most likely tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Top-P (nucleus) sampling (limits vocabulary to tokens with cumulative prob <= P)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Random seed for deterministic sampling (critical for replay)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Router seed for audit purposes
    ///
    /// **Note:** The router uses a deterministic algorithm (sorted by score,
    /// then by index for tie-breaking). This seed is stored for audit trail
    /// purposes but does NOT currently affect routing decisions. Replays
    /// produce identical routing given identical inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_seed: Option<String>,
    /// Seed mode for request-scoped RNG derivation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_mode: Option<SeedMode>,
    /// Request-scoped seed used by the worker (32 bytes)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_seed: Option<[u8; 32]>,
    /// Canonical determinism context for routing and replay
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    pub determinism: Option<DeterminismContext>,
    /// Backend profile requested for execution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_profile: Option<BackendKind>,
    /// CoreML mode for backend selection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<super::CoreMLMode>,
    /// Determinism mode to apply in the worker (strict, besteffort, relaxed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// Routing determinism mode for adapter selection (deterministic/adaptive)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Pinned adapter IDs that receive prior boost in routing (CHAT-PIN-02)
    ///
    /// These adapters receive PINNED_BOOST (0.3) added to their prior scores
    /// before the router's scoring algorithm runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_adapter_ids: Option<Vec<String>>,
    /// Strict mode disables backend fallback in the worker
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict_mode: Option<bool>,
    /// Effective adapter set resolved by the control plane
    ///
    /// When provided, the worker must restrict routing to this set and error
    /// if adapters outside the set are requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_adapter_ids: Option<Vec<String>>,

    /// Routing policy resolved for this tenant/request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_policy: Option<adapteros_api_types::RoutingPolicy>,

    /// Placement override for deterministic replay
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementReplay>,

    /// Per-adapter strength overrides (session/request scoped)
    ///
    /// Values multiply the adapter's configured lora_strength (default 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_strength_overrides: Option<std::collections::HashMap<String, f32>>,

    /// Stop policy specification (PRD: Hard Deterministic Stop Controller)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy: Option<adapteros_api_types::inference::StopPolicySpec>,

    /// BLAKE3 digest of policy decisions applied during request processing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_mask_digest_b3: Option<[u8; 32]>,

    /// Enable UTF-8 token healing (default: true)
    /// When enabled, incomplete multi-byte UTF-8 sequences are buffered until complete
    #[serde(default = "default_utf8_healing_worker")]
    pub utf8_healing: bool,
}

/// Compare policies request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComparePoliciesRequest {
    pub cpid_1: String,
    pub cpid_2: String,
}

/// Assign policy request (PRD-RBAC-01)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignPolicyRequest {
    pub policy_pack_id: String,
    pub target_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforced: Option<bool>,
}

/// Dry run promotion request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DryRunPromotionRequest {
    pub cpid: String,
}

/// Create contact request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateContactRequest {
    pub tenant_id: String,
    pub name: String,
    pub email: Option<String>,
    pub category: String,
    pub role: Option<String>,
    pub metadata_json: Option<String>,
}

/// Minimal training job creation request (workspace-scoped)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateTrainingJobRequest {
    /// Workspace identifier used for scoping and provenance
    pub workspace_id: String,
    /// Base model to tune against
    pub base_model_id: String,
    /// Dataset to train on (version will be resolved automatically)
    pub dataset_id: String,
    /// Optional dataset version override (defaults to latest)
    pub dataset_version_id: Option<String>,
    /// Optional explicit adapter name; autogenerated when omitted
    pub adapter_name: Option<String>,
    /// Training hyperparameters
    pub params: TrainingConfigRequest,
    /// Optional LoRA tier hint
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub lora_tier: Option<LoraTier>,
}

/// Request to hot-swap adapters
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterSwapRequest {
    /// ID of the adapter to replace
    pub old_adapter_id: String,
    /// ID of the adapter to load
    pub new_adapter_id: String,
    /// If true, only validate the swap without executing
    #[serde(default)]
    pub dry_run: bool,
}

/// Category policy request for creating or updating
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CategoryPolicyRequest {
    /// Minimum time before promotion in seconds
    pub promotion_threshold_secs: u64,
    /// Maximum time before demotion in seconds
    pub demotion_threshold_secs: u64,
    /// Memory limit in bytes for this category
    pub memory_limit: usize,
    /// Eviction priority (never, low, normal, high, critical)
    pub eviction_priority: String,
    /// Whether to auto-promote based on usage
    pub auto_promote: bool,
    /// Whether to auto-demote based on inactivity
    pub auto_demote: bool,
    /// Maximum number of adapters of this category to keep in memory
    pub max_in_memory: Option<usize>,
    /// Priority boost for routing (default 1.0)
    pub routing_priority: f32,
}

/// Golden compare request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoldenCompareRequest {
    /// Name of the golden baseline to compare against
    pub golden: String,
    /// Bundle ID to compare
    pub bundle_id: String,
    /// Strictness level: "bitwise", "epsilon-tolerant", or "statistical"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strictness: Option<String>,
    /// Verify toolchain matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_toolchain: Option<bool>,
    /// Verify adapters match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_adapters: Option<bool>,
    /// Verify signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_signature: Option<bool>,
    /// Verify device matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_device: Option<bool>,
}

/// Policy comparison request alias
pub type PolicyComparisonRequest = ComparePoliciesRequest;

/// Convert TrainingConfigRequest to orchestrator TrainingConfig
pub fn training_config_from_request(
    req: TrainingConfigRequest,
) -> adapteros_orchestrator::TrainingConfig {
    adapteros_orchestrator::TrainingConfig {
        rank: req.rank,
        alpha: req.alpha,
        targets: req.targets,
        coreml_placement: req.coreml_placement,
        epochs: req.epochs,
        learning_rate: req.learning_rate,
        batch_size: req.batch_size,
        warmup_steps: req.warmup_steps,
        max_seq_length: req.max_seq_length,
        gradient_accumulation_steps: req.gradient_accumulation_steps,
        weight_group_config: None,
        lr_schedule: None,
        final_lr: None,
        early_stopping: None,
        patience: None,
        min_delta: None,
        checkpoint_frequency: None,
        max_checkpoints: None,
        preferred_backend: req.preferred_backend,
        backend_policy: req.backend_policy,
        coreml_training_fallback: req.coreml_training_fallback,
        enable_coreml_export: req.enable_coreml_export,
        require_gpu: req.require_gpu.unwrap_or(false),
        max_gpu_memory_mb: req.max_gpu_memory_mb,
        base_model_path: None,
        hidden_state_layer: None,
        validation_split: None,
    }
}

#[cfg(test)]
mod training_config_tests {
    use super::*;
    use adapteros_types::training::TrainingBackendKind;

    #[test]
    fn training_config_from_request_preserves_coreml_intent_and_fallback() {
        let req = TrainingConfigRequest {
            rank: 4,
            alpha: 8,
            targets: vec!["q_proj".to_string()],
            coreml_placement: None,
            epochs: 1,
            learning_rate: 0.001,
            batch_size: 2,
            warmup_steps: None,
            max_seq_length: None,
            gradient_accumulation_steps: None,
            preferred_backend: Some(TrainingBackendKind::CoreML),
            backend_policy: None,
            coreml_training_fallback: Some(TrainingBackendKind::Mlx),
            enable_coreml_export: None,
            require_gpu: Some(false),
            max_gpu_memory_mb: None,
        };

        let cfg = training_config_from_request(req);
        assert_eq!(cfg.preferred_backend, Some(TrainingBackendKind::CoreML));
        assert_eq!(cfg.coreml_training_fallback, Some(TrainingBackendKind::Mlx));
    }
}
