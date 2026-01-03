//! Response types for API endpoints.

use adapteros_api_types::{
    inference::RunReceipt, InferResponse, ModelLoadStatus, TrainingJobResponse,
    TrainingTemplateResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;

use super::sampling::PlacementTraceEntry;

/// Directory upsert response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DirectoryUpsertResponse {
    /// Deterministic adapter ID derived from directory fingerprint
    pub adapter_id: String,
    /// B3 hash identifier used as artifact name
    pub hash_b3: String,
    /// Whether the adapter was activated (loaded)
    pub activated: bool,
}

/// Base model status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BaseModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    pub status: ModelLoadStatus,
    pub loaded_at: Option<String>,
    pub unloaded_at: Option<String>,
    pub error_message: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
    pub updated_at: String,
}

/// Promotion response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromotionResponse {
    pub cpid: String,
    pub plan_id: String,
    pub promoted_by: String,
    pub promoted_at: String,
    pub quality_metrics: QualityMetrics,
}

/// Quality metrics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QualityMetrics {
    pub arr: f32,  // Answer Relevance Rate
    pub ecs5: f32, // Evidence Citation Score @ 5
    pub hlr: f32,  // Hallucination Rate
    pub cr: f32,   // Contradiction Rate
}

/// Worker fatal error message
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerFatal {
    /// Worker identifier
    pub worker_id: String,
    /// Fatal error reason/message
    pub reason: String,
    /// Optional backtrace snippet for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backtrace_snippet: Option<String>,
    /// Timestamp when the error occurred (RFC3339)
    pub timestamp: String,
}

/// Job response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct JobResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub created_at: String,
}

/// Rollback response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RollbackResponse {
    pub cpid: String,
    pub previous_plan_id: String,
    pub rolled_back_by: String,
    pub rolled_back_at: String,
}

/// Promotion gates response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromotionGatesResponse {
    pub cpid: String,
    pub gates: Vec<GateStatus>,
    pub all_passed: bool,
}

/// Gate status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GateStatus {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub evidence_id: Option<String>,
}

/// Policy pack response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyPackResponse {
    pub cpid: String,
    pub content: String,
    pub hash_b3: String,
    pub created_at: String,
}

/// Policy validation response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub hash_b3: Option<String>,
}

/// Batch inference response item containing either a result or error
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferItemResponse {
    /// Identifier corresponding to the original request
    pub id: String,
    /// Successful inference response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<InferResponse>,
    /// Error information if the request failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<adapteros_api_types::ErrorResponse>,
}

/// Batch inference aggregate response payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferResponse {
    /// Responses for each submitted request
    pub responses: Vec<BatchInferItemResponse>,
}

/// Batch job creation response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchJobResponse {
    /// Unique batch job identifier
    pub batch_id: String,
    /// Current job status
    pub status: String,
}

/// Batch job status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchStatusResponse {
    /// Unique batch job identifier
    pub batch_id: String,
    /// Current job status
    pub status: String,
    /// Total number of items in the batch
    pub total_items: i64,
    /// Number of items completed successfully
    pub completed_items: i64,
    /// Number of items that failed
    pub failed_items: i64,
    /// Job creation timestamp (RFC3339)
    pub created_at: String,
    /// Job start timestamp (RFC3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// Job completion timestamp (RFC3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Batch items response containing results
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchItemsResponse {
    /// Collection of batch item results
    pub items: Vec<BatchItemResultResponse>,
}

/// Individual batch item result
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchItemResultResponse {
    /// Item identifier
    pub id: String,
    /// Item processing status
    pub status: String,
    /// Successful inference response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<InferResponse>,
    /// Error message if the item failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Processing latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
}

/// Process log entry
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessLogResponse {
    pub id: String,
    pub worker_id: String,
    pub level: String,
    pub message: String,
    pub timestamp: String,
    pub metadata_json: Option<String>,
}

/// Process crash dump
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessCrashDumpResponse {
    pub id: String,
    pub worker_id: String,
    pub crash_type: String,
    pub stack_trace: Option<String>,
    pub memory_snapshot_json: Option<String>,
    pub crash_timestamp: String,
    pub recovery_action: Option<String>,
    pub recovered_at: Option<String>,
}

/// Process performance profile
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessPerformanceProfileResponse {
    pub id: String,
    pub worker_id: String,
    pub profile_type: String,
    pub profile_data_json: String,
    pub duration_seconds: i32,
    pub created_at: String,
}

/// Process debug session
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessDebugSessionResponse {
    pub id: String,
    pub worker_id: String,
    pub session_type: String,
    pub status: String,
    pub config_json: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub results_json: Option<String>,
}

/// Process troubleshooting step
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessTroubleshootingStepResponse {
    pub id: String,
    pub worker_id: String,
    pub step_name: String,
    pub step_type: String,
    pub status: String,
    pub command: Option<String>,
    pub output: Option<String>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

/// Process template
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessTemplateResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub config_json: String,
    pub plan_id: Option<String>,
    pub auto_scaling_config_json: Option<String>,
    pub dependencies_json: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process bulk operation
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessBulkOperationResponse {
    pub id: String,
    pub operation_type: String,
    pub tenant_id: String,
    pub target_workers_json: String,
    pub config_json: Option<String>,
    pub status: String,
    pub progress_json: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub created_by: Option<String>,
}

/// Process auto-scaling rule
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessAutoScalingRuleResponse {
    pub id: String,
    pub tenant_id: String,
    pub rule_name: String,
    pub enabled: bool,
    pub metric_type: String,
    pub threshold_value: f64,
    pub threshold_duration_seconds: i32,
    pub scale_action: String,
    pub scale_factor: f64,
    pub min_workers: i32,
    pub max_workers: i32,
    pub cooldown_seconds: i32,
    pub last_triggered_at: Option<String>,
    pub created_at: String,
}

/// Process migration
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMigrationResponse {
    pub id: String,
    pub worker_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub migration_type: String,
    pub status: String,
    pub migration_config_json: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub rollback_data_json: Option<String>,
}

/// Process orchestration workflow
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessOrchestrationWorkflowResponse {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub workflow_type: String,
    pub steps_json: String,
    pub triggers_json: Option<String>,
    pub status: String,
    pub last_executed_at: Option<String>,
    pub execution_count: i32,
    pub success_count: i32,
    pub failure_count: i32,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process configuration template
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessConfigTemplateResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub config_schema_json: String,
    pub default_values_json: Option<String>,
    pub validation_rules_json: Option<String>,
    pub environment_specific_configs_json: Option<String>,
    pub version: String,
    pub is_active: bool,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process configuration instance
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessConfigInstanceResponse {
    pub id: String,
    pub template_id: String,
    pub worker_id: String,
    pub environment: String,
    pub config_values_json: String,
    pub validation_status: String,
    pub validation_errors_json: Option<String>,
    pub applied_at: Option<String>,
    pub applied_by: Option<String>,
    pub rollback_config_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process configuration history entry
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessConfigHistoryResponse {
    pub id: String,
    pub instance_id: String,
    pub version: String,
    pub config_values_json: String,
    pub change_type: String,
    pub change_description: Option<String>,
    pub changed_by: Option<String>,
    pub changed_at: String,
    pub diff_json: Option<String>,
}

/// Process configuration validation result
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessConfigValidationResponse {
    pub id: String,
    pub instance_id: String,
    pub validation_type: String,
    pub status: String,
    pub message: String,
    pub details_json: Option<String>,
    pub validated_at: String,
    pub validated_by: Option<String>,
}

/// Process configuration deployment
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessConfigDeploymentResponse {
    pub id: String,
    pub instance_id: String,
    pub deployment_type: String,
    pub status: String,
    pub scheduled_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub deployed_by: Option<String>,
    pub deployment_config_json: Option<String>,
    pub rollback_plan_json: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
}

/// Process configuration compliance check
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessConfigComplianceResponse {
    pub id: String,
    pub instance_id: String,
    pub compliance_standard: String,
    pub check_name: String,
    pub status: String,
    pub details_json: Option<String>,
    pub remediation_steps_json: Option<String>,
    pub checked_at: String,
    pub checked_by: Option<String>,
}

/// Commit response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommitResponse {
    pub id: String,
    pub repo_id: String,
    pub sha: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub branch: Option<String>,
    pub changed_files: Vec<String>,
    pub impacted_symbols: Vec<String>,
    pub ephemeral_adapter_id: Option<String>,
}

/// Commit diff response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommitDiffResponse {
    pub sha: String,
    pub diff: String,
    pub stats: DiffStats,
}

/// Diff statistics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiffStats {
    pub files_changed: i32,
    pub insertions: i32,
    pub deletions: i32,
}

/// Routing debug response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RoutingDebugResponse {
    pub features: FeatureVector,
    pub adapter_scores: Vec<AdapterScore>,
    pub selected_adapters: Vec<String>,
    pub explanation: String,
}

/// Feature vector
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FeatureVector {
    pub language: Option<String>,
    pub frameworks: Vec<String>,
    pub symbol_hits: i32,
    pub path_tokens: Vec<String>,
    pub verb: String,
}

/// Adapter score
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterScore {
    pub adapter_id: String,
    pub score: f64,
    pub gate_value: f64,
    pub selected: bool,
}

/// Propose patch response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProposePatchResponse {
    pub proposal_id: String,
    pub status: String,
    pub message: String,
}

/// Patch proposal inference response (from worker)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PatchProposalInferResponse {
    pub text: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<RefusalResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_proposal: Option<PatchProposalData>,
}

/// Refusal response from worker
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RefusalResponse {
    pub status: String,
    pub message: String,
}

/// Patch proposal data from worker
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PatchProposalData {
    pub proposal_id: String,
    pub rationale: String,
    pub patches: Vec<FilePatchData>,
    pub citations: Vec<CitationData>,
    pub confidence: f32,
}

/// File patch data
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FilePatchData {
    pub file_path: String,
    pub hunks: Vec<PatchHunkData>,
}

/// Patch hunk data
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PatchHunkData {
    pub start_line: usize,
    pub end_line: usize,
    pub old_content: String,
    pub new_content: String,
}

/// Citation data
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CitationData {
    pub source_type: String,
    pub reference: String,
    pub relevance: f32,
}

/// Worker inference response (from worker via UDS)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerInferResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: WorkerTrace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<RunReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    /// Backend used to execute the request (e.g., metal, coreml, mlx)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
    /// Backend version/build identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_version: Option<String>,
    /// Whether backend fallback occurred during execution
    #[serde(default)]
    pub fallback_triggered: bool,
    /// Requested CoreML compute preference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_preference: Option<String>,
    /// CoreML compute units actually used (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_units: Option<String>,
    /// Whether CoreML leveraged GPU for this inference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_gpu_used: Option<bool>,
    /// Hash of the fused CoreML package manifest used (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_package_hash: Option<String>,
    /// Expected fused CoreML package hash if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_expected_package_hash: Option<String>,
    /// Whether the actual hash mismatched the expected value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_hash_mismatch: Option<bool>,
    /// Backend selected after fallback (if different from requested)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,
    /// Determinism mode applied after resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism_mode_applied: Option<String>,
    /// Pinned adapters that were unavailable (CHAT-PIN-02)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode when pinned adapters are unavailable
    ///
    /// - `None`: All pinned adapters were available (or no pins configured)
    /// - `Some("partial")`: Some pinned adapters unavailable, using available pins + stack
    /// - `Some("stack_only")`: All pinned adapters unavailable, routing uses stack only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_routing_fallback: Option<String>,
    /// Placement trace emitted by the worker (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_trace: Option<Vec<PlacementTraceEntry>>,

    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    /// Stop reason code explaining why generation terminated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_policy_digest_b3: Option<String>,
}

/// Worker trace
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerTrace {
    pub router_summary: RouterSummary,
    #[serde(default)]
    pub token_count: usize,
    /// Detailed router decisions per step (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decisions: Option<Vec<adapteros_api_types::inference::RouterDecision>>,
    /// Cryptographically chained router decisions (per token)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decision_chain:
        Option<Vec<adapteros_api_types::inference::RouterDecisionChainEntry>>,
    /// Model type for this trace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<adapteros_api_types::inference::RouterModelType>,
}

/// Router summary
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RouterSummary {
    pub adapters_used: Vec<String>,
}

/// Token usage computed by the worker tokenizer.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub billed_input_tokens: u32,
    pub billed_output_tokens: u32,
}

/// Meta information response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MetaResponse {
    pub version: String,
    pub build_hash: String,
    pub build_date: String,
    /// Runtime environment: "dev", "staging", or "prod"
    pub environment: String,
    /// Whether production mode is enabled in config
    pub production_mode: bool,
    /// Whether dev login bypass is enabled
    pub dev_login_enabled: bool,
}

fn default_limit() -> usize {
    50
}

/// Routing decisions query parameters with comprehensive filters
#[derive(Debug, Deserialize, ToSchema)]
pub struct RoutingDecisionsQuery {
    pub tenant: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub offset: Option<usize>,
    pub since: Option<String>, // ISO-8601 timestamp
    pub until: Option<String>, // ISO-8601 timestamp
    pub stack_id: Option<String>,
    pub adapter_id: Option<String>,
    pub request_id: Option<String>,
    /// Optional chat source_type filter; matched via chat_sessions on request_id
    pub source_type: Option<String>,
    pub min_entropy: Option<f64>,
    pub max_overhead_pct: Option<f64>,
    #[serde(default)]
    pub anomalies_only: bool, // Filter to high overhead or low entropy
}

/// Routing history query parameters (simpler than RoutingDecisionsQuery)
#[derive(Debug, Deserialize, ToSchema)]
pub struct RoutingHistoryQuery {
    /// Maximum number of results (default: 50)
    pub limit: Option<usize>,
}

/// Router candidate with gate value
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct RouterCandidateInfo {
    pub adapter_idx: u16,
    pub adapter_name: Option<String>,
    pub raw_score: f32,
    pub gate_q15: i16,
    pub gate_float: f32, // Q15 converted to float for display
    pub selected: bool,  // Whether this adapter was selected (gate > 0)
}

/// Single routing decision with full candidate details
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RoutingDecision {
    pub id: String,
    pub tenant_id: String,
    pub timestamp: String,
    pub request_id: Option<String>,

    // Decision Context
    pub step: i64,
    pub input_token_id: Option<i64>,
    pub stack_id: Option<String>,
    pub stack_name: Option<String>,
    pub stack_hash: Option<String>,

    // Routing Parameters
    pub entropy: f64,
    pub tau: f64,
    pub entropy_floor: f64,
    pub k_value: Option<i64>,

    // Candidates (parsed from JSON)
    pub candidates: Vec<RouterCandidateInfo>,

    // Timing Metrics
    pub router_latency_us: Option<i64>,
    pub total_inference_latency_us: Option<i64>,
    pub overhead_pct: Option<f64>,

    // Legacy fields for compatibility
    pub adapters_used: Vec<String>,
    pub activations: Vec<f64>,
    pub reason: String,
    pub trace_id: String,
}

/// Routing decisions response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RoutingDecisionsResponse {
    pub items: Vec<RoutingDecision>,
}

/// Audits query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct AuditsQuery {
    pub tenant: String,
    pub limit: Option<usize>,
}

/// Extended audit record with before/after CPID
#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct AuditExtended {
    pub id: String,
    pub tenant_id: String,
    pub cpid: String,
    pub arr: Option<f64>,
    pub ecs5: Option<f64>,
    pub hlr: Option<f64>,
    pub cr: Option<f64>,
    pub status: Option<String>,
    pub before_cpid: Option<String>,
    pub after_cpid: Option<String>,
    pub created_at: String,
}

/// Audits response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuditsResponse {
    pub items: Vec<AuditExtended>,
}

/// Promotion record with signature
#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct PromotionRecord {
    pub id: String,
    pub cpid: String,
    pub promoted_by: String,
    pub promoted_at: String,
    pub signature_b64: String,
    pub signer_key_id: String,
    pub quality_json: String,
    pub before_cpid: Option<String>,
}

/// Sign policy response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SignPolicyResponse {
    pub cpid: String,
    pub signature: String,
    pub signed_at: String,
    pub signed_by: String,
}

/// Verify policy signature response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyPolicyResponse {
    pub cpid: String,
    pub signature: String,
    pub is_valid: bool,
    pub public_key: String,
    pub verified_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Policy assignment response (PRD-RBAC-01)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyAssignmentResponse {
    pub id: String,
    pub policy_pack_id: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub priority: i32,
    pub enforced: bool,
    pub assigned_at: String,
    pub assigned_by: String,
    pub expires_at: Option<String>,
}

/// Policy violation response (PRD-RBAC-01)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyViolationResponse {
    pub id: String,
    pub policy_pack_id: String,
    pub policy_assignment_id: Option<String>,
    pub violation_type: String,
    pub severity: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub tenant_id: String,
    pub violation_message: String,
    pub violation_details_json: Option<String>,
    pub detected_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
    pub resolution_notes: Option<String>,
}

/// Policy comparison response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyComparisonResponse {
    pub cpid_1: String,
    pub cpid_2: String,
    pub differences: Vec<String>,
    pub identical: bool,
}

/// Export policy response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportPolicyResponse {
    pub cpid: String,
    pub policy_json: String,
    pub signature: Option<String>,
    pub exported_at: String,
}

/// Stack policies response - returns policies assigned to a stack with compliance info
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StackPoliciesResponse {
    /// Stack ID
    pub stack_id: String,
    /// Stack name
    pub stack_name: String,
    /// Policies directly assigned to this stack
    pub assignments: Vec<PolicyAssignmentDetail>,
    /// Compliance summary for the stack
    pub compliance: StackComplianceSummary,
    /// Recent policy violations (last 24h)
    pub recent_violations: Vec<PolicyViolationSummary>,
    /// Response timestamp (RFC3339)
    pub timestamp: String,
}

/// Detailed policy assignment information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyAssignmentDetail {
    /// Assignment ID
    pub id: String,
    /// Policy pack ID (e.g., "cp-egress-001")
    pub policy_pack_id: String,
    /// Policy type (e.g., "egress", "determinism", "naming")
    pub policy_type: String,
    /// Human-readable policy name
    pub policy_name: String,
    /// Policy version
    pub version: String,
    /// Policy status (active, deprecated, draft)
    pub status: String,
    /// Whether the policy is enforced (true) or audit-only (false)
    pub enforced: bool,
    /// Priority for conflict resolution (higher = higher priority)
    pub priority: i32,
    /// When the policy was assigned (RFC3339)
    pub assigned_at: String,
    /// Who assigned the policy
    pub assigned_by: String,
    /// Optional expiration date (RFC3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Stack compliance summary with overall score and category breakdown
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StackComplianceSummary {
    /// Overall compliance score (0-100)
    pub overall_score: f64,
    /// Compliance status: "compliant", "warning", "non_compliant"
    pub status: String,
    /// Compliance breakdown by category
    pub by_category: HashMap<String, CategoryComplianceScore>,
    /// When compliance was last calculated (RFC3339)
    pub last_calculated: String,
}

/// Compliance score for a specific category
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CategoryComplianceScore {
    /// Category score (0-100)
    pub score: f64,
    /// Number of checks that passed
    pub passed: i32,
    /// Number of checks that failed
    pub failed: i32,
}

/// Summary of a policy violation
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyViolationSummary {
    /// Violation ID
    pub id: String,
    /// Policy pack ID that was violated
    pub policy_pack_id: String,
    /// Violation severity: "critical", "high", "medium", "low"
    pub severity: String,
    /// Human-readable violation message
    pub message: String,
    /// When the violation was detected (RFC3339)
    pub detected_at: String,
    /// When the violation was resolved (RFC3339), if resolved
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

/// Policy violation error response - returned when an operation is blocked by policy
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyViolationErrorResponse {
    /// Error message
    pub error: String,
    /// Error code (always "POLICY_VIOLATION")
    pub code: String,
    /// HTTP status code (403 or 422)
    pub status: u16,
    /// Detailed violation information
    pub details: PolicyViolationErrorDetails,
}

/// Details about policy violations that blocked an operation
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyViolationErrorDetails {
    /// List of violations that occurred
    pub violations: Vec<PolicyViolationItem>,
    /// Whether an admin can override these violations
    pub can_override: bool,
}

/// Single policy violation item in an error response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyViolationItem {
    /// Policy ID that was violated
    pub policy_id: String,
    /// Human-readable policy name
    pub policy_name: String,
    /// Violation severity
    pub severity: String,
    /// Detailed violation message
    pub message: String,
    /// Whether this violation blocks the operation
    pub blocking: bool,
    /// Whether this specific violation can be overridden
    pub can_override: bool,
    /// Suggested remediation steps
    pub remediation: String,
}

/// SSE event for stack policy streaming
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StackPolicyStreamEvent {
    /// Event type: policy_assigned, policy_revoked, violation_detected, violation_resolved, compliance_changed
    pub event: String,
    /// Event data (varies by event type)
    pub data: serde_json::Value,
    /// Event timestamp (RFC3339)
    pub timestamp: String,
}

/// Dry run promotion response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DryRunPromotionResponse {
    pub cpid: String,
    pub would_promote: bool,
    pub gates_status: Vec<GateStatus>,
    pub warnings: Vec<String>,
}

/// Promotion history response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromotionHistoryResponse {
    pub id: String,
    pub cpid: String,
    pub promoted_at: String,
    pub promoted_by: String,
    pub gates_passed: bool,
}

/// Repository report response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RepositoryReportResponse {
    pub repo_id: String,
    pub total_lines: i64,
    pub total_files: i64,
    pub complexity_score: f64,
    pub risk_level: String,
    pub languages: Vec<LanguageStats>,
    pub generated_at: String,
}

/// Language statistics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LanguageStats {
    pub language: String,
    pub line_count: i64,
    pub file_count: i64,
    pub percentage: f64,
}

/// Assign policies response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignPoliciesResponse {
    pub tenant_id: String,
    pub assigned_cpids: Vec<String>,
    pub assigned_at: String,
}

/// Assign adapters response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignAdaptersResponse {
    pub tenant_id: String,
    pub assigned_adapter_ids: Vec<String>,
    pub assigned_at: String,
}

/// Promotion history entry
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromotionHistoryEntry {
    pub cpid: String,
    pub promoted_at: String,
    pub promoted_by: String,
    pub previous_cpid: Option<String>,
    pub gate_results_summary: String,
}

/// Response from adapter hot-swap operation
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterSwapResponse {
    /// Whether the swap was successful
    pub success: bool,
    /// Message describing the result
    pub message: String,
    /// Old adapter ID that was replaced
    pub old_adapter_id: String,
    /// New adapter ID that was loaded
    pub new_adapter_id: String,
    /// VRAM change in megabytes (positive = increase)
    pub vram_delta_mb: Option<i64>,
    /// Duration of the swap operation in milliseconds
    pub duration_ms: u64,
    /// Whether this was a dry run
    pub dry_run: bool,
}

/// Detailed adapter statistics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterStatsResponse {
    /// Adapter ID
    pub adapter_id: String,
    /// Activation percentage (0-100)
    pub activation_percentage: f64,
    /// Memory usage in bytes
    pub memory_bytes: i64,
    /// Total number of requests served
    pub request_count: i64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// P95 latency in milliseconds
    pub p95_latency_ms: f64,
    /// P99 latency in milliseconds
    pub p99_latency_ms: f64,
    /// Total activations
    pub total_activations: i64,
    /// Number of times selected by router
    pub selected_count: i64,
    /// Average gate value
    pub avg_gate_value: f64,
    /// Selection rate percentage
    pub selection_rate: f64,
    /// Current lifecycle state
    pub lifecycle_state: String,
    /// Last activated timestamp
    pub last_activated: Option<String>,
    /// Created at timestamp
    pub created_at: String,
}

/// Category policy response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CategoryPolicyResponse {
    /// Category name
    pub category: String,
    /// Minimum time before promotion in milliseconds
    pub promotion_threshold_ms: u64,
    /// Maximum time before demotion in milliseconds
    pub demotion_threshold_ms: u64,
    /// Memory limit in bytes
    pub memory_limit: usize,
    /// Eviction priority
    pub eviction_priority: String,
    /// Whether to auto-promote based on usage
    pub auto_promote: bool,
    /// Whether to auto-demote based on inactivity
    pub auto_demote: bool,
    /// Maximum number of adapters to keep in memory
    pub max_in_memory: Option<usize>,
    /// Priority boost for routing
    pub routing_priority: f32,
}

/// List of category policies
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CategoryPoliciesResponse {
    /// List of category policies
    pub policies: Vec<CategoryPolicyResponse>,
}

/// Response from stopping a worker
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerStopResponse {
    /// Worker ID that was stopped
    pub worker_id: String,
    /// Whether the stop was successful
    pub success: bool,
    /// Message describing the result
    pub message: String,
    /// Previous worker status
    pub previous_status: String,
    /// Timestamp when stop was initiated
    pub stopped_at: String,
}

// GitStatusResponse is defined in handlers/git.rs and also in adapteros-api-types
// with different fields. Use the handler's version for file-based status
// or adapteros_api_types::GitStatusResponse for session-based status.

/// Golden run summary response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoldenRunSummary {
    /// Name of the golden baseline
    pub name: String,
    /// Unique run identifier
    pub run_id: String,
    /// Control point ID
    pub cpid: String,
    /// Plan identifier
    pub plan_id: String,
    /// BLAKE3 hash of the bundle
    pub bundle_hash: String,
    /// Number of layers in epsilon stats
    pub layer_count: usize,
    /// Maximum epsilon value across all layers
    pub max_epsilon: f64,
    /// Mean epsilon value across all layers
    pub mean_epsilon: f64,
    /// Toolchain summary string
    pub toolchain_summary: String,
    /// List of adapters used in the run
    pub adapters: Vec<String>,
    /// RFC3339 timestamp of creation
    pub created_at: String,
    /// Whether the archive has a signature
    pub has_signature: bool,
}

/// Query parameters for audit logs
#[derive(Debug, Deserialize, ToSchema)]
pub struct AuditLogsQuery {
    /// Filter by user ID
    pub user_id: Option<String>,
    /// Filter by action (e.g., "adapter.register", "training.start")
    pub action: Option<String>,
    /// Filter by resource type (e.g., "adapter", "tenant")
    pub resource_type: Option<String>,
    /// Filter by resource ID
    pub resource_id: Option<String>,
    /// Filter by status ("success" or "failure")
    pub status: Option<String>,
    /// Filter by tenant ID
    pub tenant_id: Option<String>,
    /// Start time (RFC3339 format)
    pub from_time: Option<String>,
    /// End time (RFC3339 format)
    pub to_time: Option<String>,
    /// Maximum number of results (default: 100, max: 1000)
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Single audit log record response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuditLogResponse {
    pub id: String,
    pub timestamp: String,
    pub user_id: String,
    pub user_role: String,
    pub tenant_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub metadata_json: Option<String>,
}

/// Audit logs list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuditLogsResponse {
    pub logs: Vec<AuditLogResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Response from tenant-wide token revocation operation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenRevocationResponse {
    /// Timestamp when revocation was executed (RFC3339)
    pub revoked_at: String,
    /// Human-readable message explaining the effect
    pub message: String,
}

/// Convert orchestrator TrainingJob to TrainingJobResponse
pub fn training_job_to_response(job: adapteros_orchestrator::TrainingJob) -> TrainingJobResponse {
    TrainingJobResponse::from(job)
}

/// Convert orchestrator TrainingTemplate to TrainingTemplateResponse
pub fn training_template_to_response(
    template: adapteros_orchestrator::TrainingTemplate,
) -> TrainingTemplateResponse {
    TrainingTemplateResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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

/// Adapter metadata for training provenance export
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingExportAdapter {
    /// Adapter ID
    pub id: String,
    /// Adapter name
    pub name: String,
    /// Adapter version
    pub version: String,
    /// Base model used for training
    pub base_model: Option<String>,
    /// LoRA rank
    pub rank: i32,
    /// LoRA alpha
    pub alpha: f64,
    /// Creation timestamp (RFC3339)
    pub created_at: String,
}

/// Training job reference for provenance export
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingExportJob {
    /// Training job ID
    pub id: String,
    /// BLAKE3 hash of training configuration
    pub config_hash: Option<String>,
    /// Full training configuration JSON
    pub training_config: Value,
    /// Job start timestamp (RFC3339)
    pub started_at: String,
    /// Job completion timestamp (RFC3339)
    pub completed_at: Option<String>,
    /// Job status
    pub status: String,
}

/// Dataset reference for provenance export
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingExportDataset {
    /// Dataset ID
    pub id: String,
    /// Dataset name
    pub name: String,
    /// BLAKE3 hash of dataset
    pub hash: String,
    /// Source location URI/path
    pub source_location: Option<String>,
}

/// Document reference for provenance export
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingExportDocument {
    /// Document ID
    pub id: String,
    /// Document name
    pub name: String,
    /// BLAKE3 hash of document content
    pub hash: String,
    /// Number of pages (for PDFs)
    pub page_count: Option<i32>,
    /// Upload timestamp (RFC3339)
    pub created_at: String,
}

/// Configuration versions for provenance export
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingExportConfigVersions {
    /// Chunking configuration used for document processing
    pub chunking_config: Option<Value>,
    /// Training hyperparameters configuration
    pub training_config: Option<Value>,
}

/// Complete training provenance export response
///
/// Returns full provenance data for an adapter including:
/// - Adapter metadata (id, name, version, base_model)
/// - Training jobs that produced this adapter
/// - Datasets used for training
/// - Documents with their content hashes
/// - Configuration versions (chunking, training)
/// - Export timestamp and integrity hash
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingProvenanceExportResponse {
    /// Schema version for this response format
    pub schema_version: String,
    /// Adapter metadata
    pub adapter: TrainingExportAdapter,
    /// Training jobs that contributed to this adapter
    pub training_jobs: Vec<TrainingExportJob>,
    /// Datasets used for training
    pub datasets: Vec<TrainingExportDataset>,
    /// Documents with content hashes
    pub documents: Vec<TrainingExportDocument>,
    /// Configuration versions for reproducibility
    pub config_versions: TrainingExportConfigVersions,
    /// Export generation timestamp (RFC3339)
    pub export_timestamp: String,
    /// BLAKE3 hash of the entire export for integrity verification
    pub export_hash: String,
}

/// Query parameters for batch items retrieval
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct BatchItemsQuery {
    /// Filter by item status
    pub status: Option<String>,
    /// Maximum number of items to return
    pub limit: Option<i32>,
    /// Number of items to skip
    pub offset: Option<i32>,
}

/// List commits query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct ListCommitsQuery {
    pub repo_id: Option<String>,
    pub branch: Option<String>,
    pub limit: Option<i64>,
}

/// List adapters query parameters
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ListAdaptersQuery {
    pub tier: Option<String>,
    pub framework: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}
