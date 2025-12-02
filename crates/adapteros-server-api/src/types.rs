use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;

// Re-export shared API types
pub use adapteros_api_types::*;

// ErrorResponse is imported from adapteros_api_types via the pub use above

// ===== Operation Progress Event Type =====

/// Event tracking progress of long-running model operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OperationProgressEvent {
    /// Unique operation identifier
    pub operation_id: String,
    /// Model ID being operated on
    pub model_id: String,
    /// Operation type: "load", "unload", "validate"
    pub operation: String,
    /// Current operation status: "started", "in_progress", "completed", "failed"
    pub status: String,
    /// Progress percentage (0-100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<u8>,
    /// Duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Error message if status is "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Event creation timestamp (RFC3339)
    #[schema(value_type = String)]
    pub created_at: DateTime<Utc>,
}

impl OperationProgressEvent {
    /// Create a new operation progress event with "started" status
    pub fn new(operation_id: String, model_id: String, operation: String) -> Self {
        Self {
            operation_id,
            model_id,
            operation,
            status: "started".to_string(),
            progress_percent: None,
            duration_ms: None,
            error_message: None,
            created_at: Utc::now(),
        }
    }
}

// ===== Telemetry Response Types =====

/// Single metric data point with timestamp
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricDataPointResponse {
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
    /// Metric value
    pub value: f64,
    /// Optional labels/tags for the data point
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
}

/// Time series data for a single metric
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsSeriesResponse {
    /// Name of the metric series
    pub series_name: String,
    /// Data points in the series
    pub points: Vec<MetricDataPointResponse>,
}

/// Current metrics snapshot with counters, gauges, and histograms
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsSnapshotResponse {
    /// Counter metrics (monotonically increasing values)
    pub counters: HashMap<String, f64>,
    /// Gauge metrics (point-in-time values)
    pub gauges: HashMap<String, f64>,
    /// Histogram metrics (distribution summaries)
    pub histograms: HashMap<String, Vec<f64>>,
    /// Timestamp when snapshot was taken (RFC3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// Activity event for recent activity feed
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActivityEventResponse {
    /// Unique event identifier
    pub id: String,
    /// Event timestamp (RFC3339)
    pub timestamp: String,
    /// Type of event (e.g., "adapter.loaded", "training.completed")
    pub event_type: String,
    /// Log level (debug, info, warn, error, critical)
    pub level: String,
    /// Human-readable event message
    pub message: String,
    /// Component that generated the event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Tenant ID associated with the event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// User ID that triggered the event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Additional event metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

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
    pub error: Option<ErrorResponse>,
}

/// Batch inference aggregate response payload
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchInferResponse {
    /// Responses for each submitted request
    pub responses: Vec<BatchInferItemResponse>,
}

// ErrorResponse methods and IntoResponse impl are in adapteros-api-types

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

// Auth, Tenant, and Node types are now imported from adapteros-api-types

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

/// Base model status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BaseModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    pub status: String,
    pub loaded_at: Option<String>,
    pub unloaded_at: Option<String>,
    pub error_message: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
    pub updated_at: String,
}

// BuildPlanRequest is now imported from adapteros-api-types

/// Promote CP request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromoteCPRequest {
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
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

/// Spawn worker request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SpawnWorkerRequest {
    pub tenant_id: String,
    pub plan_id: String,
    pub node_id: String,
    #[serde(default = "default_uid")]
    pub uid: u32,
    #[serde(default = "default_gid")]
    pub gid: u32,
}

fn default_uid() -> u32 {
    1000
}

fn default_gid() -> u32 {
    1000
}

/// Job response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct JobResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub created_at: String,
}

// HealthResponse, InferRequest, InferResponse, InferenceTrace, RouterDecision, and WorkerResponse
// are now imported from adapteros-api-types

/// Rollback CP request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RollbackCPRequest {
    pub tenant_id: String,
    pub cpid: String,
}

/// Rollback response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RollbackResponse {
    pub cpid: String,
    pub previous_plan_id: String,
    pub rolled_back_by: String,
    pub rolled_back_at: String,
}

// UserInfoResponse and PlanResponse are now imported from adapteros-api-types

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

/// Validate policy request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ValidatePolicyRequest {
    pub content: String,
}

/// Policy validation response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PolicyValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub hash_b3: Option<String>,
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

// ===== Process Debugging Types =====

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

// ===== Advanced Process Control Types =====

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

// ===== Process Configuration Management Types =====

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

// Adapter types, Plan management types, Repository types, and Metrics types
// are now imported from adapteros-api-types

// ===== Commit Types =====

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

// ===== Routing Types =====

/// Routing debug request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RoutingDebugRequest {
    pub prompt: String,
    pub context: Option<String>,
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

// ===== Query Types =====

/// List commits query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct ListCommitsQuery {
    pub repo_id: Option<String>,
    pub branch: Option<String>,
    pub limit: Option<i64>,
}

/// List adapters query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct ListAdaptersQuery {
    pub tier: Option<i32>,
    pub framework: Option<String>,
}

/// Telemetry bundle response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TelemetryBundleResponse {
    pub id: String,
    pub cpid: String,
    pub event_count: u64,
    pub size_bytes: u64,
    pub created_at: String,
}

/// Propose patch request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProposePatchRequest {
    pub repo_id: String,
    pub commit_sha: String,
    pub description: String,
    pub target_files: Vec<String>,
}

/// Propose patch response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProposePatchResponse {
    pub proposal_id: String,
    pub status: String,
    pub message: String,
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

/// Worker inference request (for UDS communication)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerInferRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    pub require_evidence: bool,
}

/// Worker inference response (from worker via UDS)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerInferResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: WorkerTrace,
}

/// Worker trace
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerTrace {
    pub router_summary: RouterSummary,
}

/// Router summary
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RouterSummary {
    pub adapters_used: Vec<String>,
}

// Agent D Contract Endpoints

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
    pub min_entropy: Option<f64>,
    pub max_overhead_pct: Option<f64>,
    #[serde(default)]
    pub anomalies_only: bool, // Filter to high overhead or low entropy
}

fn default_limit() -> usize {
    50
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

// ===== Policy Management Types (Phase 6) =====

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

/// Compare policies request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComparePoliciesRequest {
    pub cpid_1: String,
    pub cpid_2: String,
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

// ===== Stack Policy Types =====

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

// ===== Promotion Execution Types (Phase 7) =====

/// Dry run promotion request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DryRunPromotionRequest {
    pub cpid: String,
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

// ===== Telemetry Types (Phase 8) =====

/// Bundle verification response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BundleVerificationResponse {
    pub bundle_id: String,
    pub verified: bool,
    pub signature_valid: bool,
    pub merkle_root_valid: bool,
    pub verified_at: String,
}

/// Purge bundles request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeBundlesRequest {
    pub keep_count: Option<usize>,
    pub older_than_days: Option<i64>,
}

/// Purge bundles response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeBundlesResponse {
    pub purged_count: usize,
    pub retained_count: usize,
}

// ===== Repository Types (Phase 9) =====

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

// ===== Tenant Management Types (Phase 10) =====

// UpdateTenantRequest, AssignPoliciesRequest, and AssignAdaptersRequest are imported from adapteros-api-types

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

// TenantUsageResponse is now imported from adapteros-api-types

/// Export telemetry bundle response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportTelemetryBundleResponse {
    pub bundle_id: String,
    pub events_count: i64,
    pub size_bytes: i64,
    pub download_url: String,
    pub expires_at: String,
}

/// Verify bundle signature response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyBundleSignatureResponse {
    pub bundle_id: String,
    pub valid: bool,
    pub signature: String,
    pub signed_by: String,
    pub signed_at: String,
    pub verification_error: Option<String>,
}

/// Purge old bundles request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeOldBundlesRequest {
    pub keep_bundles_per_cpid: i32,
}

/// Purge old bundles response  
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeOldBundlesResponse {
    pub purged_count: i32,
    pub retained_count: i32,
    pub freed_bytes: i64,
    pub purged_cpids: Vec<String>,
}

/// Policy comparison request
pub type PolicyComparisonRequest = ComparePoliciesRequest;

/// Promotion history entry
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PromotionHistoryEntry {
    pub cpid: String,
    pub promoted_at: String,
    pub promoted_by: String,
    pub previous_cpid: Option<String>,
    pub gate_results_summary: String,
}

// ============================================================================
// Contacts API Types
// ============================================================================
// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6

/// Contact query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct ContactsQuery {
    pub tenant: String,
    pub category: Option<String>,
    pub limit: Option<usize>,
}

/// Contact database row
#[derive(Debug, sqlx::FromRow)]
pub struct ContactRow {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub email: Option<String>,
    pub category: String,
    pub role: Option<String>,
    pub metadata_json: Option<String>,
    pub avatar_url: Option<String>,
    pub discovered_at: String,
    pub discovered_by: Option<String>,
    pub last_interaction: Option<String>,
    pub interaction_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Contact response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ContactResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub email: Option<String>,
    pub category: String,
    pub role: Option<String>,
    pub metadata_json: Option<String>,
    pub avatar_url: Option<String>,
    pub discovered_at: String,
    pub discovered_by: Option<String>,
    pub last_interaction: Option<String>,
    pub interaction_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ContactRow> for ContactResponse {
    fn from(row: ContactRow) -> Self {
        Self {
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name,
            email: row.email,
            category: row.category,
            role: row.role,
            metadata_json: row.metadata_json,
            avatar_url: row.avatar_url,
            discovered_at: row.discovered_at,
            discovered_by: row.discovered_by,
            last_interaction: row.last_interaction,
            interaction_count: row.interaction_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Contacts list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ContactsResponse {
    pub contacts: Vec<ContactResponse>,
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

/// Contact interaction query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct ContactInteractionsQuery {
    pub limit: Option<usize>,
}

/// Contact interaction database row
#[derive(Debug, sqlx::FromRow)]
pub struct ContactInteractionRow {
    pub id: String,
    pub contact_id: String,
    pub trace_id: String,
    pub cpid: String,
    pub interaction_type: String,
    pub context_json: Option<String>,
    pub created_at: String,
}

/// Contact interaction response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ContactInteractionResponse {
    pub id: String,
    pub contact_id: String,
    pub trace_id: String,
    pub cpid: String,
    pub interaction_type: String,
    pub context_json: Option<String>,
    pub created_at: String,
}

impl From<ContactInteractionRow> for ContactInteractionResponse {
    fn from(row: ContactInteractionRow) -> Self {
        Self {
            id: row.id,
            contact_id: row.contact_id,
            trace_id: row.trace_id,
            cpid: row.cpid,
            interaction_type: row.interaction_type,
            context_json: row.context_json,
            created_at: row.created_at,
        }
    }
}

/// Contact interactions list response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ContactInteractionsResponse {
    pub interactions: Vec<ContactInteractionResponse>,
}

// ============================================================================
// Streaming API Types
// ============================================================================
// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4

/// Stream query parameters (for training and contacts streams)
#[derive(Debug, Deserialize, ToSchema)]
pub struct StreamQuery {
    pub tenant: String,
}

/// Discovery stream query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscoveryStreamQuery {
    pub tenant: String,
    pub repo: Option<String>,
}

// ============================================================================
// Training API Types - Type definitions are imported from adapteros-api-types
// Helper functions for orchestrator integration (can't use From trait due to orphan rules)
// ============================================================================

/// Convert TrainingConfigRequest to orchestrator TrainingConfig
pub fn training_config_from_request(
    req: TrainingConfigRequest,
) -> adapteros_orchestrator::TrainingConfig {
    adapteros_orchestrator::TrainingConfig {
        rank: req.rank,
        alpha: req.alpha,
        targets: req.targets,
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
    }
}

/// Convert orchestrator TrainingJob to TrainingJobResponse
pub fn training_job_to_response(job: adapteros_orchestrator::TrainingJob) -> TrainingJobResponse {
    // Calculate estimated completion time for running jobs
    let estimated_completion = calculate_estimated_completion(&job);

    TrainingJobResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: job.id,
        adapter_name: job.adapter_name,
        template_id: job.template_id,
        repo_id: job.repo_id,
        dataset_id: job.dataset_id,
        status: format!("{:?}", job.status).to_lowercase(),
        progress_pct: job.progress_pct,
        current_epoch: job.current_epoch,
        total_epochs: job.total_epochs,
        current_loss: job.current_loss,
        learning_rate: job.learning_rate,
        tokens_per_second: job.tokens_per_second,
        created_at: job.created_at,
        started_at: job.started_at,
        completed_at: job.completed_at,
        error_message: job.error_message,
        estimated_completion,
        base_model_id: None,
        collection_id: None,
        build_id: None,
        config_hash_b3: None,
        adapter_id: None,
        weights_hash_b3: None,
        category: None,
        description: None,
        language: None,
        framework_id: None,
        framework_version: None,
    }
}

/// Calculate estimated completion time for a training job
///
/// Uses progress percentage and elapsed time to estimate when the job will complete.
/// Returns None if the job is not running or progress data is insufficient.
fn calculate_estimated_completion(job: &adapteros_orchestrator::TrainingJob) -> Option<String> {
    use chrono::{DateTime, Duration, Utc};

    // Only calculate for running jobs with meaningful progress
    if !matches!(
        job.status,
        adapteros_orchestrator::TrainingJobStatus::Running
    ) {
        return None;
    }

    // Need started_at timestamp and non-zero progress
    let started_at = job.started_at.as_ref()?;
    if job.progress_pct <= 0.0 || job.progress_pct >= 100.0 {
        return None;
    }

    // Parse started_at as RFC3339 timestamp
    let start_time: DateTime<Utc> = started_at.parse().ok()?;
    let now = Utc::now();

    // Calculate elapsed time
    let elapsed = now.signed_duration_since(start_time);
    if elapsed.num_seconds() <= 0 {
        return None;
    }

    // Estimate total time based on current progress
    // Formula: total_time = elapsed_time / (progress_pct / 100)
    let progress_fraction = job.progress_pct as f64 / 100.0;
    let estimated_total_seconds = elapsed.num_seconds() as f64 / progress_fraction;
    let remaining_seconds = (estimated_total_seconds - elapsed.num_seconds() as f64).max(0.0);

    // Add remaining time to now
    let estimated_completion = now + Duration::seconds(remaining_seconds as i64);

    Some(estimated_completion.to_rfc3339())
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

// Domain Adapter types are now imported from adapteros-api-types

// ===== Advanced Process Monitoring and Alerting Types =====

/// Process monitoring rule
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringRuleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub rule_type: String,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: String,
    pub severity: String,
    pub evaluation_window_seconds: i32,
    pub cooldown_seconds: i32,
    pub is_active: bool,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process health metric
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessHealthMetricResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_unit: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub collected_at: String,
}

/// Process alert
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessAlertResponse {
    pub id: String,
    pub rule_id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub alert_type: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub metric_value: Option<f64>,
    pub threshold_value: Option<f64>,
    pub status: String,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<String>,
    pub resolved_at: Option<String>,
    pub suppression_reason: Option<String>,
    pub suppression_until: Option<String>,
    pub escalation_level: i32,
    pub notification_sent: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Process anomaly
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessAnomalyResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub anomaly_type: String,
    pub metric_name: String,
    pub detected_value: f64,
    pub expected_range_min: Option<f64>,
    pub expected_range_max: Option<f64>,
    pub confidence_score: f64,
    pub severity: String,
    pub description: Option<String>,
    pub detection_method: String,
    pub model_version: Option<String>,
    pub status: String,
    pub investigated_by: Option<String>,
    pub investigation_notes: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

/// Process performance baseline
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessPerformanceBaselineResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: String,
    pub calculation_period_days: i32,
    pub confidence_interval: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub percentile_95: Option<f64>,
    pub percentile_99: Option<f64>,
    pub is_active: bool,
    pub calculated_at: String,
    pub expires_at: Option<String>,
}

/// Process monitoring dashboard
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringDashboardResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub dashboard_config: serde_json::Value,
    pub is_shared: bool,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process monitoring widget
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringWidgetResponse {
    pub id: String,
    pub dashboard_id: String,
    pub widget_type: String,
    pub widget_config: serde_json::Value,
    pub position_x: i32,
    pub position_y: i32,
    pub width: i32,
    pub height: i32,
    pub refresh_interval_seconds: i32,
    pub is_visible: bool,
    pub created_at: String,
}

/// Process monitoring notification
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringNotificationResponse {
    pub id: String,
    pub alert_id: String,
    pub notification_type: String,
    pub recipient: String,
    pub message: String,
    pub status: String,
    pub sent_at: Option<String>,
    pub delivered_at: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub created_at: String,
}

/// Process monitoring schedule
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringScheduleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub schedule_type: String,
    pub schedule_config: serde_json::Value,
    pub is_active: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process monitoring report
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringReportResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub report_type: String,
    pub report_config: serde_json::Value,
    pub generated_at: String,
    pub report_data: Option<serde_json::Value>,
    pub file_path: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub created_by: Option<String>,
}

/// Create monitoring rule request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub rule_type: String,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: String,
    pub severity: String,
    pub evaluation_window_seconds: Option<i32>,
    pub cooldown_seconds: Option<i32>,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
}

/// Create monitoring dashboard request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringDashboardRequest {
    pub name: String,
    pub description: Option<String>,
    pub dashboard_config: serde_json::Value,
    pub is_shared: Option<bool>,
}

/// Create monitoring widget request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringWidgetRequest {
    pub dashboard_id: String,
    pub widget_type: String,
    pub widget_config: serde_json::Value,
    pub position_x: i32,
    pub position_y: i32,
    pub width: i32,
    pub height: i32,
    pub refresh_interval_seconds: Option<i32>,
}

/// Create monitoring schedule request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringScheduleRequest {
    pub name: String,
    pub description: Option<String>,
    pub schedule_type: String,
    pub schedule_config: serde_json::Value,
}

/// Create monitoring report request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringReportRequest {
    pub name: String,
    pub description: Option<String>,
    pub report_type: String,
    pub report_config: serde_json::Value,
}

/// Acknowledge alert request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AcknowledgeProcessAlertRequest {
    pub alert_id: String,
    pub acknowledgment_note: Option<String>,
}

/// Resolve alert request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ResolveProcessAlertRequest {
    pub alert_id: String,
    pub resolution_note: Option<String>,
}

/// Suppress alert request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SuppressProcessAlertRequest {
    pub alert_id: String,
    pub suppression_reason: String,
    pub suppression_until: Option<String>,
}

/// Update anomaly status request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateProcessAnomalyStatusRequest {
    pub anomaly_id: String,
    pub status: String,
    pub investigation_notes: Option<String>,
}

/// Git status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GitStatusResponse {
    pub branch: String,
    pub modified_files: Vec<String>,
    pub staged_files: Vec<String>,
    pub untracked_files: Vec<String>,
}

/// Backpressure response for rate limiting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackpressureResponse {
    /// Memory pressure level (e.g., "high", "critical")
    pub level: String,
    /// Suggested retry delay in seconds
    pub retry_after_secs: u64,
    /// Suggested action to take
    pub suggested_action: String,
}

// ===== Golden Run Types =====

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

// ===== Audit Logs API Types =====

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

// ============================================================================
// Adapter Hot-Swap API Types
// ============================================================================

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

// ============================================================================
// Adapter Statistics API Types
// ============================================================================

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

// ============================================================================
// Category Policy API Types
// ============================================================================

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

// ============================================================================
// Worker Stop API Types
// ============================================================================

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

// ============================================================================
// Training Provenance Export Types
// ============================================================================

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
