//! Request types for API endpoints.

use adapteros_api_types::{CreateTrainingJobRequest, InferRequest, TrainingConfigRequest};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

/// Request to update router feature importance weights
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateRouterWeightsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_hits_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_tokens_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_verb_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orthogonal_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diversity_weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_penalty: Option<f64>,
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

/// Canonical worker inference request shared by server and worker.
pub type WorkerInferRequest = adapteros_transport_types::WorkerInferenceRequest;

/// Worker request type (normal vs patch proposal).
pub type WorkerRequestType = adapteros_transport_types::WorkerRequestType;

/// Patch proposal request payload attached to `WorkerRequestType::PatchProposal`.
pub type WorkerPatchProposalRequest = adapteros_transport_types::WorkerPatchProposalRequest;

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

// CreateTrainingJobRequest is now imported from adapteros_api_types::training

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
        lr_schedule: None,
        final_lr: None,
        early_stopping: req.early_stopping,
        patience: req.patience,
        min_delta: req.min_delta,
        checkpoint_frequency: None,
        max_checkpoints: None,
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
        multi_module_training: req.multi_module_training.unwrap_or(false),
        lora_layer_indices: req.lora_layer_indices.clone().unwrap_or_default(),
    }
}

#[cfg(test)]
mod training_config_tests {
    use super::*;
    use adapteros_types::training::{TrainingBackendKind, TRAINING_DATA_CONTRACT_VERSION};

    #[test]
    fn training_config_from_request_preserves_coreml_intent_and_fallback() {
        let req = TrainingConfigRequest {
            rank: 4,
            alpha: 8,
            targets: vec!["q_proj".to_string()],
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: 0,
            coreml_placement: None,
            epochs: 1,
            learning_rate: 0.001,
            batch_size: 2,
            warmup_steps: None,
            max_seq_length: None,
            gradient_accumulation_steps: None,
            validation_split: None,
            preferred_backend: Some(TrainingBackendKind::CoreML),
            backend_policy: None,
            coreml_training_fallback: Some(TrainingBackendKind::Mlx),
            enable_coreml_export: None,
            require_gpu: Some(false),
            max_gpu_memory_mb: None,
            base_model_path: None,
            preprocessing: None,
            force_resume: None,
            multi_module_training: None,
            lora_layer_indices: None,
            early_stopping: None,
            patience: None,
            min_delta: None,
        };

        let cfg = training_config_from_request(req);
        assert_eq!(cfg.preferred_backend, Some(TrainingBackendKind::CoreML));
        assert_eq!(cfg.coreml_training_fallback, Some(TrainingBackendKind::Mlx));
    }

    #[test]
    fn training_config_from_request_preserves_early_stopping_fields() {
        let req = TrainingConfigRequest {
            rank: 4,
            alpha: 8,
            targets: vec!["q_proj".to_string()],
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: 0,
            coreml_placement: None,
            epochs: 1,
            learning_rate: 0.001,
            batch_size: 2,
            warmup_steps: None,
            max_seq_length: None,
            gradient_accumulation_steps: None,
            validation_split: None,
            preferred_backend: None,
            backend_policy: None,
            coreml_training_fallback: None,
            enable_coreml_export: None,
            require_gpu: Some(false),
            max_gpu_memory_mb: None,
            base_model_path: None,
            preprocessing: None,
            force_resume: None,
            multi_module_training: None,
            lora_layer_indices: None,
            early_stopping: Some(true),
            patience: Some(7),
            min_delta: Some(0.01),
        };

        let cfg = training_config_from_request(req);
        assert_eq!(cfg.early_stopping, Some(true));
        assert_eq!(cfg.patience, Some(7));
        assert_eq!(cfg.min_delta, Some(0.01));
    }
}
