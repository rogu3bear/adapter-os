use adapteros_config::PlacementWeights as ConfigPlacementWeights;
use adapteros_core::{determinism::DeterminismContext, BackendKind, SeedMode};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::training::LoraTier;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;

// Re-export shared API types
pub use adapteros_api_types::*;
pub mod run_envelope;
pub use run_envelope::{new_run_envelope, set_policy_mask, set_router_seed, set_worker_context};

// ErrorResponse is imported from adapteros_api_types via the pub use above

/// Standard error envelope returned by the API for all 4xx/5xx responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorBody {
    /// Machine-readable error code (e.g., "ADAPTER_NOT_FOUND")
    pub code: String,
    /// Human-readable message suitable for UI display
    pub message: String,
    /// Actionable hint for common failures
    pub hint: String,
    /// Optional developer-facing detail (stack trace, context, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Correlation ID that matches the `x-request-id` header and server logs
    pub request_id: String,
}

/// Structured UMA backpressure error payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UmaBackpressureError {
    /// UMA pressure level (Low, Medium, High, Critical)
    pub level: String,
    /// Suggested retry interval in seconds
    pub retry_after_secs: u32,
    /// Suggested client action
    pub action: String,
}

impl UmaBackpressureError {
    pub fn new(level: impl Into<String>) -> Self {
        Self {
            level: level.into(),
            retry_after_secs: 30,
            action: "reduce max_tokens or retry later".to_string(),
        }
    }
}

impl From<UmaBackpressureError> for ErrorResponse {
    fn from(err: UmaBackpressureError) -> Self {
        ErrorResponse::new("service under memory pressure")
            .with_code("BACKPRESSURE")
            .with_details(serde_json::json!({
                "level": err.level,
                "retry_after_secs": err.retry_after_secs,
                "action": err.action,
            }))
    }
}

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
    /// API schema version for frontend compatibility
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Counter metrics (monotonically increasing values)
    pub counters: HashMap<String, f64>,
    /// Gauge metrics (point-in-time values)
    pub gauges: HashMap<String, f64>,
    /// Histogram metrics (distribution summaries)
    pub histograms: HashMap<String, Vec<f64>>,
    /// Timestamp when snapshot was taken (RFC3339)
    pub timestamp: String,
    /// Flattened metrics map for frontend compatibility (union of counters and gauges)
    #[serde(default)]
    pub metrics: HashMap<String, f64>,
}

fn default_schema_version() -> String {
    adapteros_api_types::API_SCHEMA_VERSION.to_string()
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
    pub status: ModelLoadStatus,
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
    /// Model cache budget in megabytes (propagated to worker as AOS_MODEL_CACHE_MAX_MB)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_cache_max_mb: Option<u64>,
    /// Path to config TOML file (propagated to worker as AOS_CONFIG_TOML)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_toml_path: Option<String>,
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
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ListAdaptersQuery {
    pub tier: Option<String>,
    pub framework: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
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
    pub coreml_mode: Option<CoreMLMode>,
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
    pub policy_mask_digest: Option<[u8; 32]>,

    /// Enable UTF-8 token healing (default: true)
    /// When enabled, incomplete multi-byte UTF-8 sequences are buffered until complete
    #[serde(default = "default_utf8_healing_worker")]
    pub utf8_healing: bool,
}

fn default_utf8_healing_worker() -> bool {
    true
}

/// Placement decision trace entry (per token)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct PlacementTraceEntry {
    pub step: usize,
    pub lane: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f32>,
    pub utilization: f32,
}

/// Worker inference response (from worker via UDS)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerInferResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: WorkerTrace,
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
    /// Detailed router decisions per step (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decisions: Option<Vec<adapteros_api_types::inference::RouterDecision>>,
    /// Cryptographically chained router decisions (per token)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decision_chain:
        Option<Vec<adapteros_api_types::inference::RouterDecisionChainEntry>>,
    /// MoE model information (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moe_info: Option<adapteros_api_types::inference::MoEInfo>,
    /// Expert routing data per token (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Vec<Vec<usize>>>>)]
    pub expert_routing: Option<adapteros_api_types::inference::SequenceExpertRouting>,
    /// Flattened expert IDs per token (for visualization)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_experts: Option<Vec<Vec<u8>>>,
    /// Model type for this trace (dense vs MoE)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<adapteros_api_types::inference::RouterModelType>,
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
    /// Optional chat source_type filter; matched via chat_sessions on request_id
    pub source_type: Option<String>,
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
    }
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

// ===== Inference Core Types (PRD-05) =====

/// Internal unified inference request used by all handlers.
///
/// This is the canonical representation that flows through `route_and_infer()`.
/// All HTTP handlers map their external request types into this internal model.
#[derive(Debug, Clone)]
pub struct InferenceRequestInternal {
    // === Core Fields ===
    /// Unique request ID for tracing and correlation
    pub request_id: String,
    /// Control plane ID (tenant identifier)
    pub cpid: String,
    /// Input prompt text
    pub prompt: String,
    /// Canonical execution envelope for determinism/audit
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,
    /// Enable reasoning-aware routing and hot-swaps
    pub reasoning_mode: bool,
    /// Admin override flag to bypass cluster routing restrictions
    pub admin_override: bool,

    // === Delivery Mode ===
    /// Whether to stream tokens via SSE
    pub stream: bool,
    /// Batch item ID (for batch requests only)
    pub batch_item_id: Option<String>,

    // === RAG Options ===
    /// Enable RAG context retrieval
    pub rag_enabled: bool,
    /// Collection ID for scoped RAG retrieval
    pub rag_collection_id: Option<String>,
    /// Dataset version ID for deterministic dataset pinning
    pub dataset_version_id: Option<String>,

    // === Adapter Selection ===
    /// Adapter stack to use for inference
    ///
    /// Legacy: this is an explicit list of adapter IDs, **not** a stack_id alias.
    pub adapter_stack: Option<Vec<String>>,
    /// Specific adapters to use (alternative to adapter_stack)
    ///
    /// Explicit adapter IDs for this request. Takes precedence over stack_id.
    pub adapters: Option<Vec<String>>,
    /// Adapter stack identifier (preferred over adapter_stack list)
    ///
    /// References a stack in the DB; resolved to adapter IDs before sending to the worker.
    pub stack_id: Option<String>,
    /// Optional domain hint for routing/package selection
    pub domain_hint: Option<String>,
    /// Stack version for telemetry/audit (populated when stack_id resolves)
    pub stack_version: Option<i64>,
    /// Determinism mode configured on the resolved stack (if any)
    pub stack_determinism_mode: Option<String>,
    /// Routing determinism mode configured on the resolved stack (if any)
    pub stack_routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Effective adapter IDs after control plane resolution
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Per-adapter strength overrides (session/request scoped)
    ///
    /// Values multiply the adapter's configured lora_strength. Defaults to 1.0.
    pub adapter_strength_overrides: Option<std::collections::HashMap<String, f32>>,
    /// Resolved determinism mode applied to this request
    pub determinism_mode: Option<String>,
    /// Routing determinism mode applied to this request (deterministic/adaptive)
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Seed mode requested for per-request RNG derivation
    pub seed_mode: Option<SeedMode>,
    /// Request-scoped seed derived by control plane
    pub request_seed: Option<[u8; 32]>,
    /// Backend profile selected for execution
    pub backend_profile: Option<BackendKind>,
    /// CoreML mode selected for this request
    pub coreml_mode: Option<CoreMLMode>,

    // === Sampling Parameters ===
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Sampling temperature
    pub temperature: f32,
    /// Top-K sampling
    pub top_k: Option<usize>,
    /// Top-P (nucleus) sampling
    pub top_p: Option<f32>,
    /// Random seed for reproducibility (PRD-02: deterministic sampling)
    pub seed: Option<u64>,
    /// Router seed for audit purposes (PRD-02: replay)
    ///
    /// **Note:** The router uses a deterministic algorithm (sorted by score,
    /// then by index for tie-breaking). This seed is stored for audit trail
    /// purposes but does NOT currently affect routing decisions. Replays
    /// produce identical routing given identical inputs.
    pub router_seed: Option<String>,

    // === Evidence & Session ===
    /// Require evidence recording
    pub require_evidence: bool,
    /// Chat session ID for trace linkage
    pub session_id: Option<String>,
    /// Pinned adapter IDs for this inference (session-level preference, CHAT-PIN-02)
    ///
    /// These adapters receive PINNED_BOOST added to their priors during routing,
    /// making them more likely to be selected while still allowing non-pinned
    /// adapters to win with sufficiently high feature scores. When an effective
    /// adapter set is present, pins must also be members of that set.
    pub pinned_adapter_ids: Option<Vec<String>>,
    /// BLAKE3 hash of sorted message IDs for multi-turn context verification
    ///
    /// When a session_id is provided and multi-turn context is built, this hash
    /// enables deterministic replay verification. Stored in replay_metadata.
    pub chat_context_hash: Option<String>,
    /// User claims for policy enforcement
    pub claims: Option<crate::auth::Claims>,
    /// BLAKE3 digest of policy decisions applied during request processing
    ///
    /// This captures the policy enforcement state for deterministic replay.
    /// Computed from the sorted policy_pack_ids, hooks, and decisions.
    pub policy_mask_digest: Option<[u8; 32]>,

    // === Model Selection ===
    /// Model identifier (if specific model requested)
    pub model: Option<String>,

    // === Stop Controller ===
    /// Stop policy specification (PRD: Hard Deterministic Stop Controller)
    pub stop_policy: Option<adapteros_api_types::inference::StopPolicySpec>,

    // === Timing ===
    /// Request creation timestamp
    pub created_at: std::time::Instant,
    /// Optional auth token used to reach the worker (ApiKey)
    pub worker_auth_token: Option<String>,

    // === Streaming Options ===
    /// Enable UTF-8 token healing (default: true)
    /// When enabled, incomplete multi-byte UTF-8 sequences are buffered until complete
    pub utf8_healing: Option<bool>,
}

impl InferenceRequestInternal {
    /// Create a new internal request with generated ID
    pub fn new(cpid: String, prompt: String) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            cpid,
            prompt,
            run_envelope: None,
            reasoning_mode: false,
            admin_override: false,
            stream: false,
            batch_item_id: None,
            rag_enabled: false,
            rag_collection_id: None,
            dataset_version_id: None,
            adapter_stack: None,
            adapters: None,
            stack_id: None,
            domain_hint: None,
            stack_version: None,
            stack_determinism_mode: None,
            stack_routing_determinism_mode: None,
            effective_adapter_ids: None,
            adapter_strength_overrides: None,
            determinism_mode: None,
            routing_determinism_mode: None,
            seed_mode: None,
            request_seed: None,
            backend_profile: None,
            coreml_mode: None,
            max_tokens: 100,
            temperature: 0.7,
            top_k: None,
            top_p: None,
            seed: None,
            router_seed: None,
            require_evidence: false,
            session_id: None,
            pinned_adapter_ids: None,
            chat_context_hash: None,
            claims: None,
            policy_mask_digest: None,
            model: None,
            stop_policy: None,
            created_at: std::time::Instant::now(),
            worker_auth_token: None,
            utf8_healing: None,
        }
    }

    /// Set streaming mode
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Set RAG options
    pub fn with_rag(mut self, collection_id: String) -> Self {
        self.rag_enabled = true;
        self.rag_collection_id = Some(collection_id);
        self
    }
}

/// Result from inference execution via InferenceCore
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferenceResult {
    /// Generated text
    pub text: String,
    /// Number of tokens generated
    pub tokens_generated: usize,
    /// Reason for stopping (e.g., "stop", "length", "error")
    pub finish_reason: String,
    /// Adapters used during inference
    pub adapters_used: Vec<String>,
    /// Router decisions made during inference
    pub router_decisions: Vec<RouterDecisionRecord>,
    /// Cryptographically chained router decisions (per token)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decision_chain:
        Option<Vec<adapteros_api_types::inference::RouterDecisionChainEntry>>,
    /// MoE model information (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moe_info: Option<adapteros_api_types::inference::MoEInfo>,
    /// Expert routing data per token (MoE models)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Vec<Vec<usize>>>>)]
    pub expert_routing: Option<adapteros_api_types::inference::SequenceExpertRouting>,
    /// Flattened expert IDs per token for visualization
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_experts: Option<Vec<Vec<u8>>>,
    /// Model type for this trace (dense vs MoE)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<adapteros_api_types::inference::RouterModelType>,
    /// RAG evidence if RAG was used
    pub rag_evidence: Option<RagEvidence>,
    /// Source citations derived from training files or RAG
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<adapteros_api_types::inference::Citation>,
    /// Total latency in milliseconds
    pub latency_ms: u64,
    /// Request ID for correlation
    pub request_id: String,
    /// Pinned adapter IDs that were unavailable (CHAT-PIN-02)
    ///
    /// These are adapters that were in the session's pinned set but were not
    /// available in the candidate adapter set. Returned for UI warning display.
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode when pinned adapters are unavailable (PRD-6A)
    ///
    /// - `None`: All pinned adapters were available (or no pins configured)
    /// - `Some("partial")`: Some pinned adapters unavailable, using available pins + stack
    /// - `Some("stack_only")`: All pinned adapters unavailable, routing uses stack only
    pub pinned_routing_fallback: Option<String>,
    /// Effective adapter set applied for this inference (if any)
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Backend used to execute the inference (e.g., coreml, metal, mlx)
    pub backend_used: Option<String>,
    /// Deterministic receipt for audit/replay metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deterministic_receipt: Option<adapteros_api_types::inference::DeterministicReceipt>,
    /// Whether backend fallback occurred during execution
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
    /// Backend selected after fallback (if different from requested)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,
    /// Determinism mode applied after resolution
    pub determinism_mode_applied: Option<String>,
    /// Replay guarantee level computed for this inference
    pub replay_guarantee: Option<ReplayGuarantee>,
    /// Canonical run envelope for this execution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,
    /// Placement trace returned by worker (optional)
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

/// Router decision record for audit trail
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RouterDecisionRecord {
    /// Token generation step
    pub step: usize,
    /// Input token ID that triggered this decision
    pub input_token_id: Option<u32>,
    /// Candidate adapters considered
    pub candidates: Vec<RouterCandidateRecord>,
    /// Shannon entropy of gate distribution
    pub entropy: f64,
    /// Selected adapter IDs
    pub selected_adapters: Vec<String>,
    /// Fusion interval identifier active for this decision
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_id: Option<String>,
}

/// Router candidate record for decision audit
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RouterCandidateRecord {
    /// Adapter index
    pub adapter_idx: u16,
    /// Raw score before softmax
    pub raw_score: f32,
    /// Quantized gate value (Q15)
    pub gate_q15: i16,
}

/// RAG evidence for provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RagEvidence {
    /// Collection ID used for retrieval
    pub collection_id: String,
    /// Chunks used for context
    pub chunks_used: Vec<ChunkReference>,
    /// BLAKE3 hash of the combined context
    pub context_hash: String,
}

/// Reference to a document chunk used in RAG
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChunkReference {
    /// Document ID
    pub document_id: String,
    /// Chunk ID within document
    pub chunk_id: String,
    /// Page number (if applicable)
    pub page_number: Option<i32>,
    /// Relevance score
    pub relevance_score: f32,
    /// Rank in retrieval results
    pub rank: usize,
}

/// Structured error details from worker responses
///
/// This enum mirrors `adapteros_lora_worker::InferenceErrorDetails` for
/// deserialization from worker UDS responses and API transport.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum WorkerErrorDetails {
    /// Model cache budget exceeded during eviction
    #[serde(rename = "cache_budget_exceeded")]
    CacheBudgetExceeded {
        /// Memory needed in megabytes
        needed_mb: u64,
        /// Memory freed during eviction attempt in megabytes
        freed_mb: u64,
        /// Number of pinned entries that blocked eviction
        pinned_count: usize,
        /// Number of active entries that blocked eviction
        active_count: usize,
        /// Maximum cache budget in megabytes
        max_mb: u64,
        /// Optional model key identifier (for diagnostics)
        model_key: Option<String>,
    },
    /// Generic worker error (fallback for unstructured errors)
    #[serde(rename = "worker_error")]
    WorkerError {
        /// Error message
        message: String,
    },
}

/// Error type for inference operations
#[derive(Debug, Clone)]
pub enum InferenceError {
    /// Prompt validation failed
    ValidationError(String),
    /// Worker not available
    WorkerNotAvailable(String),
    /// Worker communication failed
    WorkerError(String),
    /// Request timeout
    Timeout(String),
    /// Request cancelled due to client disconnect
    ClientClosed(String),
    /// RAG retrieval failed
    RagError(String),
    /// Permission denied
    PermissionDenied(String),
    /// Memory pressure too high
    BackpressureError(String),
    /// Routing was bypassed (should never happen)
    RoutingBypass(String),
    /// Base model not ready for routing
    ModelNotReady(String),
    /// No compatible worker available for the required manifest
    NoCompatibleWorker {
        required_hash: String,
        tenant_id: String,
        available_count: usize,
        /// Specific reason why no compatible workers were found
        reason: String,
    },
    /// Worker discovery failed but system is in degraded mode (dev mode only)
    ///
    /// This error indicates that no compatible worker was found after retries,
    /// but the system is in dev mode and can operate in a degraded state.
    WorkerDegraded {
        tenant_id: String,
        /// Reason for degradation
        reason: String,
    },
    /// Adapter not found or not loadable (archived/purged)
    AdapterNotFound(String),
    /// Worker ID unavailable for token generation
    ///
    /// When worker authentication is enabled (signing keypair present), we require
    /// a valid worker_id to generate tokens. This error occurs when worker selection
    /// fails to provide a worker_id.
    WorkerIdUnavailable {
        /// Tenant ID for the request
        tenant_id: String,
        /// Reason worker ID is unavailable
        reason: String,
    },
    /// Model cache budget exceeded in worker
    ///
    /// This error occurs when the worker's model cache cannot free enough
    /// memory to accommodate a new model load.
    CacheBudgetExceeded {
        /// Memory needed in megabytes
        needed_mb: u64,
        /// Memory freed during eviction attempt in megabytes
        freed_mb: u64,
        /// Number of pinned entries that blocked eviction
        pinned_count: usize,
        /// Number of active entries that blocked eviction
        active_count: usize,
        /// Maximum cache budget in megabytes
        max_mb: u64,
        /// Optional model key identifier (for diagnostics)
        model_key: Option<String>,
    },
    /// Policy violation blocked inference
    PolicyViolation {
        /// Tenant ID for the request
        tenant_id: String,
        /// ID of the policy that was violated
        policy_id: String,
        /// Reason for the violation
        reason: String,
    },
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            Self::WorkerNotAvailable(msg) => write!(f, "Worker not available: {}", msg),
            Self::WorkerError(msg) => write!(f, "Worker error: {}", msg),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::ClientClosed(msg) => write!(f, "Client closed request: {}", msg),
            Self::RagError(msg) => write!(f, "RAG error: {}", msg),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            Self::BackpressureError(msg) => write!(f, "Backpressure: {}", msg),
            Self::RoutingBypass(msg) => write!(f, "Routing bypass: {}", msg),
            Self::ModelNotReady(msg) => write!(f, "Model not ready: {}", msg),
            Self::NoCompatibleWorker {
                required_hash,
                tenant_id,
                available_count,
                reason,
            } => write!(
                f,
                "No compatible worker for tenant {} with manifest {} ({} workers available). Reason: {}",
                tenant_id, required_hash, available_count, reason
            ),
            Self::WorkerDegraded { tenant_id, reason } => write!(
                f,
                "Worker degraded for tenant {}: {}",
                tenant_id, reason
            ),
            Self::AdapterNotFound(msg) => write!(f, "Adapter not found: {}", msg),
            Self::WorkerIdUnavailable { tenant_id, reason } => write!(
                f,
                "Worker ID unavailable for tenant {}: {}",
                tenant_id, reason
            ),
            Self::CacheBudgetExceeded {
                needed_mb,
                freed_mb,
                pinned_count,
                active_count,
                max_mb,
                ..
            } => write!(
                f,
                "Model cache budget exceeded: needed {} MB, freed {} MB (pinned={}, active={}), max {} MB",
                needed_mb, freed_mb, pinned_count, active_count, max_mb
            ),
            Self::PolicyViolation {
                tenant_id,
                policy_id,
                reason,
            } => write!(
                f,
                "Policy violation for tenant {} (policy: {}): {}",
                tenant_id, policy_id, reason
            ),
        }
    }
}

impl std::error::Error for InferenceError {}

impl InferenceError {
    /// Convert to HTTP status code
    pub fn status_code(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::WorkerNotAvailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::WorkerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Timeout(_) => StatusCode::REQUEST_TIMEOUT,
            Self::ClientClosed(_) => StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
            Self::RagError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::PermissionDenied(_) => StatusCode::FORBIDDEN,
            Self::BackpressureError(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::RoutingBypass(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ModelNotReady(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::NoCompatibleWorker { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::WorkerDegraded { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::AdapterNotFound(_) => StatusCode::NOT_FOUND,
            Self::WorkerIdUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::CacheBudgetExceeded { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::PolicyViolation { .. } => StatusCode::FORBIDDEN,
        }
    }

    /// Convert to error code string
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::ValidationError(_) => "VALIDATION_ERROR",
            Self::WorkerNotAvailable(_) => "SERVICE_UNAVAILABLE",
            Self::WorkerError(_) => "INTERNAL_ERROR",
            Self::Timeout(_) => "REQUEST_TIMEOUT",
            Self::ClientClosed(_) => "CLIENT_CLOSED_REQUEST",
            Self::RagError(_) => "RAG_ERROR",
            Self::PermissionDenied(_) => "PERMISSION_DENIED",
            Self::BackpressureError(_) => "BACKPRESSURE",
            Self::RoutingBypass(_) => "ROUTING_BYPASS",
            Self::ModelNotReady(_) => "MODEL_NOT_READY",
            Self::NoCompatibleWorker { .. } => "NO_COMPATIBLE_WORKER",
            Self::WorkerDegraded { .. } => "WORKER_DEGRADED",
            Self::AdapterNotFound(_) => "ADAPTER_NOT_FOUND",
            Self::WorkerIdUnavailable { .. } => "WORKER_ID_UNAVAILABLE",
            Self::CacheBudgetExceeded { .. } => "CACHE_BUDGET_EXCEEDED",
            Self::PolicyViolation { .. } => "POLICY_VIOLATION",
        }
    }

    /// Map to structured failure codes for observability.
    pub fn failure_code(&self) -> Option<FailureCode> {
        match self {
            Self::PermissionDenied(_) => Some(FailureCode::TenantAccessDenied),
            Self::BackpressureError(_) => Some(FailureCode::OutOfMemory),
            Self::RoutingBypass(_) | Self::ModelNotReady(_) => Some(FailureCode::PolicyDivergence),
            Self::WorkerError(msg) | Self::WorkerNotAvailable(msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("out of memory") || lower.contains("oom") {
                    Some(FailureCode::OutOfMemory)
                } else if lower.contains("load") || lower.contains("model") {
                    Some(FailureCode::ModelLoadFailed)
                } else if lower.contains("fallback") {
                    Some(FailureCode::BackendFallback)
                } else {
                    None
                }
            }
            Self::Timeout(_) => None,
            Self::ValidationError(_) => None,
            Self::ClientClosed(_) => None,
            Self::RagError(msg) => {
                if msg.to_lowercase().contains("trace") {
                    Some(FailureCode::TraceWriteFailed)
                } else {
                    None
                }
            }
            Self::NoCompatibleWorker { .. } => Some(FailureCode::BackendFallback),
            Self::WorkerDegraded { .. } => Some(FailureCode::BackendFallback),
            Self::AdapterNotFound(_) => None,
            Self::WorkerIdUnavailable { .. } => Some(FailureCode::BackendFallback),
            Self::CacheBudgetExceeded { .. } => Some(FailureCode::OutOfMemory),
            Self::PolicyViolation { .. } => Some(FailureCode::PolicyDivergence),
        }
    }
}

// ===== From Implementations for InferenceRequestInternal =====

use crate::auth::Claims;

/// Convert from standard InferRequest + Claims to internal format
impl From<(&InferRequest, &Claims)> for InferenceRequestInternal {
    fn from((req, claims): (&InferRequest, &Claims)) -> Self {
        let is_admin = claims.role.eq_ignore_ascii_case("admin")
            || claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            cpid: claims.tenant_id.clone(),
            prompt: req.prompt.clone(),
            run_envelope: None,
            reasoning_mode: req.reasoning_mode.unwrap_or(false),
            admin_override: is_admin,
            stream: req.stream.unwrap_or(false),
            batch_item_id: None,
            rag_enabled: req.rag_enabled.unwrap_or(false),
            rag_collection_id: req.collection_id.clone(),
            dataset_version_id: req.dataset_version_id.clone(),
            adapter_stack: req.adapter_stack.clone(),
            adapters: req.adapters.clone(),
            stack_id: req.stack_id.clone(),
            domain_hint: req.domain.clone(),
            stack_version: None,
            stack_determinism_mode: None,
            stack_routing_determinism_mode: None,
            effective_adapter_ids: None, // Computed in InferenceCore
            adapter_strength_overrides: None,
            determinism_mode: None,
            routing_determinism_mode: req.routing_determinism_mode,
            seed_mode: None,
            request_seed: None,
            backend_profile: req.backend,
            coreml_mode: req.coreml_mode,
            max_tokens: req.max_tokens.unwrap_or(100),
            temperature: req.temperature.unwrap_or(0.7),
            top_k: req.top_k,
            top_p: req.top_p,
            seed: req.seed,
            router_seed: None,
            require_evidence: req.require_evidence.unwrap_or(false),
            session_id: req.session_id.clone(),
            pinned_adapter_ids: None, // Populated by InferenceCore from session
            chat_context_hash: None,
            claims: Some(claims.clone()),
            policy_mask_digest: None, // Computed by handler from enforce_at_hook
            model: req.model.clone(),
            stop_policy: req.stop_policy.clone(),
            created_at: std::time::Instant::now(),
            worker_auth_token: None,
            utf8_healing: None,
        }
    }
}

/// Convert from batch item + Claims to internal format
impl From<(&BatchInferItemRequest, &Claims)> for InferenceRequestInternal {
    fn from((item, claims): (&BatchInferItemRequest, &Claims)) -> Self {
        let mut internal = Self::from((&item.request, claims));
        internal.batch_item_id = Some(item.id.clone());
        internal
    }
}

/// Convert InferenceResult to InferResponse for API compatibility
impl From<InferenceResult> for InferResponse {
    fn from(result: InferenceResult) -> Self {
        let model = result
            .deterministic_receipt
            .as_ref()
            .and_then(|receipt| receipt.model.clone());

        Self {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: result.request_id,
            text: result.text,
            tokens: vec![],
            tokens_generated: result.tokens_generated,
            finish_reason: result.finish_reason,
            latency_ms: result.latency_ms,
            run_receipt: None,
            deterministic_receipt: result.deterministic_receipt,
            run_envelope: result.run_envelope.clone(),
            adapters_used: result.adapters_used.clone(),
            citations: result.citations,
            trace: InferenceTrace {
                adapters_used: result.adapters_used,
                router_decisions: result
                    .router_decisions
                    .into_iter()
                    .map(|rd| {
                        RouterDecision {
                            step: rd.step,
                            input_token_id: rd.input_token_id,
                            candidate_adapters: rd
                                .candidates
                                .into_iter()
                                .map(|c| RouterCandidate {
                                    adapter_idx: c.adapter_idx,
                                    raw_score: c.raw_score,
                                    gate_q15: c.gate_q15,
                                })
                                .collect(),
                            entropy: rd.entropy as f32,
                            tau: 1.0,            // Default tau
                            entropy_floor: 0.02, // Default entropy floor
                            stack_hash: None,
                            interval_id: rd.interval_id.clone(),
                            allowed_mask: None,
                            policy_mask_digest: None,
                            policy_overrides_applied: None,
                            model_type: adapteros_api_types::inference::RouterModelType::Dense,
                            active_experts: None,
                        }
                    })
                    .collect(),
                router_decision_chain: result.router_decision_chain,
                latency_ms: result.latency_ms,
                fusion_intervals: None,
                moe_info: result.moe_info,
                expert_routing: result.expert_routing,
                active_experts: result.active_experts,
                model_type: result.model_type,
            },
            model,
            prompt_tokens: None,
            error: None,
            unavailable_pinned_adapters: result.unavailable_pinned_adapters,
            pinned_routing_fallback: result.pinned_routing_fallback,
            backend_used: result.backend_used,
            coreml_compute_preference: result.coreml_compute_preference,
            coreml_compute_units: result.coreml_compute_units,
            coreml_gpu_used: result.coreml_gpu_used,
            fallback_backend: result.fallback_backend,
            fallback_triggered: result.fallback_triggered,
            determinism_mode_applied: result.determinism_mode_applied,
            replay_guarantee: result.replay_guarantee,
            // Stop Controller fields
            stop_reason_code: result.stop_reason_code,
            stop_reason_token_index: result.stop_reason_token_index,
            stop_policy_digest_b3: result.stop_policy_digest_b3,
        }
    }
}

/// Convert InferenceError to ErrorResponse for API compatibility
impl From<InferenceError> for (axum::http::StatusCode, axum::Json<ErrorResponse>) {
    fn from(err: InferenceError) -> Self {
        let status = err.status_code();
        let code = err.error_code();
        let message = err.to_string();
        let failure_code = err.failure_code();
        let mut response = ErrorResponse::new(&message).with_code(code);
        if let Some(fc) = failure_code {
            response = response.with_failure_code(fc);
        }
        (status, axum::Json(response))
    }
}

// ============================================================================
// Deterministic Replay Types
// ============================================================================

/// Current sampling algorithm version for replay compatibility checking
pub const SAMPLING_ALGORITHM_VERSION: &str = "v1.0.0";

/// Maximum size for stored prompt/response text (64KB)
pub const MAX_REPLAY_TEXT_SIZE: usize = 64 * 1024;

/// Placement metadata captured for replay/audit.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlacementReplay {
    /// Mode applied (balanced/latency/energy/thermal/off)
    pub mode: String,
    /// Weights used for the cost model
    pub weights: PlacementWeightsSchema,
    /// Optional per-step device trace
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<PlacementTraceEntry>,
}

/// API-safe placement weights schema (decouples utoipa from config crate)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlacementWeightsSchema {
    pub latency: f32,
    pub energy: f32,
    pub thermal: f32,
}

impl From<ConfigPlacementWeights> for PlacementWeightsSchema {
    fn from(w: ConfigPlacementWeights) -> Self {
        Self {
            latency: w.latency,
            energy: w.energy,
            thermal: w.thermal,
        }
    }
}

impl From<PlacementWeightsSchema> for ConfigPlacementWeights {
    fn from(w: PlacementWeightsSchema) -> Self {
        ConfigPlacementWeights {
            latency: w.latency,
            energy: w.energy,
            thermal: w.thermal,
        }
    }
}

/// Sampling parameters for inference replay
///
/// Captures all parameters that affect token generation for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SamplingParams {
    /// Sampling temperature (0.0 - 2.0)
    pub temperature: f32,
    /// Top-K sampling (None to disable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Top-P nucleus sampling (None to disable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Random seed for reproducibility (None for non-deterministic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Error code captured for failed inference metadata (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Seed mode applied for request seed derivation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_mode: Option<SeedMode>,
    /// Backend profile requested for execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_profile: Option<BackendKind>,
    /// Request seed (hex) provided to worker
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_seed_hex: Option<String>,
    /// Placement metadata (device selection trace)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementReplay>,
    /// Canonical run envelope serialized for replay metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,
    /// BLAKE3 hashes of adapters used (ordered)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_hashes_b3: Option<Vec<String>>,
    /// BLAKE3 hash for the dataset manifest used by this request (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_k: Some(50),
            top_p: Some(0.95),
            max_tokens: 512,
            seed: None,
            error_code: None,
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
            run_envelope: None,
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
        }
    }
}

/// Replay key containing all inputs needed for deterministic reproduction
///
/// This is the "recipe" for recreating an inference operation exactly.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayKey {
    /// BLAKE3 hash of the manifest used
    pub manifest_hash: String,
    /// Router seed for audit purposes (stored but currently unused)
    ///
    /// The router uses a deterministic algorithm (sorted by score, then by
    /// index for tie-breaking). This seed is stored for audit trail purposes
    /// but does NOT currently affect routing decisions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_seed: Option<String>,
    /// Sampling parameters used
    pub sampler_params: SamplingParams,
    /// Backend used (CoreML, MLX, Metal)
    pub backend: String,
    /// Version of the sampling algorithm
    pub sampling_algorithm_version: String,
    /// BLAKE3 hash of sorted RAG document hashes (null if no RAG)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_snapshot_hash: Option<String>,
    /// Adapter IDs selected by router
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_ids: Option<Vec<String>>,
    /// Whether the inference ran in base-only mode (no adapters)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_only: Option<bool>,
    /// Dataset version ID for deterministic RAG replay (pins to specific dataset version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
}

/// Replay availability status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayStatus {
    /// Exact replay possible (all conditions match)
    Available,
    /// RAG context changed but documents exist
    Approximate,
    /// Some RAG documents are missing
    Degraded,
    /// Original inference failed (no replayable output)
    FailedInference,
    /// Replay metadata capture failed (record incomplete)
    FailedCapture,
    /// Critical components missing (manifest, backend)
    Unavailable,
}

impl std::fmt::Display for ReplayStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Available => write!(f, "available"),
            Self::Approximate => write!(f, "approximate"),
            Self::Degraded => write!(f, "degraded"),
            Self::FailedInference => write!(f, "failed_inference"),
            Self::FailedCapture => write!(f, "failed_capture"),
            Self::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// Match status after replay execution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMatchStatus {
    /// Token-for-token identical output
    Exact,
    /// Semantically similar but not identical
    Semantic,
    /// Significantly different output
    Divergent,
    /// Error during replay execution
    Error,
}

impl std::fmt::Display for ReplayMatchStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::Semantic => write!(f, "semantic"),
            Self::Divergent => write!(f, "divergent"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Request to execute a deterministic replay
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayRequest {
    /// Inference ID to replay (lookup metadata by ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_id: Option<String>,
    /// Alternatively, provide full replay key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_key: Option<ReplayKey>,
    /// Override prompt (uses stored prompt if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Allow approximate/degraded replay (default: false)
    #[serde(default)]
    pub allow_approximate: bool,
    /// Skip RAG retrieval (test pure model determinism)
    #[serde(default)]
    pub skip_rag: bool,
}

/// RAG reproducibility details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RagReproducibility {
    /// Score from 0.0 (no overlap) to 1.0 (all docs available)
    pub score: f32,
    /// Number of original documents still available
    pub matching_docs: usize,
    /// Total number of documents in original inference
    pub total_original_docs: usize,
    /// Document IDs that are no longer available
    pub missing_doc_ids: Vec<String>,
}

/// Details about response divergence
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DivergenceDetails {
    /// Character position where divergence was detected (None if exact match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence_position: Option<usize>,
    /// Whether the backend changed from original
    pub backend_changed: bool,
    /// Whether the manifest hash changed
    pub manifest_changed: bool,
    /// Human-readable reasons for approximation
    pub approximation_reasons: Vec<String>,
}

/// Statistics from replay execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayStats {
    /// Estimated token count (~4 chars/token heuristic).
    /// Note: This is an approximation since the worker doesn't report actual token counts.
    /// Do not use for chargeback or precise token accounting.
    pub estimated_tokens: usize,
    /// Replay latency in milliseconds
    pub latency_ms: u64,
    /// Original inference latency (if recorded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_latency_ms: Option<u64>,
}

/// Response from replay execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayResponse {
    /// Unique ID for this replay execution
    pub replay_id: String,
    /// Original inference ID that was replayed
    pub original_inference_id: String,
    /// Mode used for replay (exact, approximate, degraded)
    pub replay_mode: String,
    /// Generated response text
    pub response: String,
    /// Whether response was truncated to 64KB limit
    pub response_truncated: bool,
    /// Match status compared to original
    pub match_status: ReplayMatchStatus,
    /// RAG reproducibility details (if RAG was used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_reproducibility: Option<RagReproducibility>,
    /// Divergence details (if not exact match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence: Option<DivergenceDetails>,
    /// Original response for comparison
    pub original_response: String,
    /// Execution statistics
    pub stats: ReplayStats,
}

/// Response from checking replay availability
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayAvailabilityResponse {
    /// Inference ID checked
    pub inference_id: String,
    /// Current replay status
    pub status: ReplayStatus,
    /// Whether exact replay is possible
    pub can_replay_exact: bool,
    /// Whether approximate replay is possible
    pub can_replay_approximate: bool,
    /// Reasons why replay is unavailable (if applicable)
    pub unavailable_reasons: Vec<String>,
    /// Warnings about approximations (if approximate)
    pub approximation_warnings: Vec<String>,
    /// Warning if dataset version has changed since original inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_consistency_warning: Option<String>,
    /// The replay key (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_key: Option<ReplayKey>,
}

/// Single replay execution record for history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayExecutionRecord {
    /// Replay execution ID
    pub id: String,
    /// Original inference ID
    pub original_inference_id: String,
    /// Mode used (exact, approximate, degraded)
    pub replay_mode: String,
    /// Match status result
    pub match_status: ReplayMatchStatus,
    /// RAG reproducibility score (if RAG used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_reproducibility_score: Option<f32>,
    /// Execution timestamp (RFC3339)
    pub executed_at: String,
    /// User who executed the replay
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_by: Option<String>,
    /// Error message if match_status is Error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Response containing replay execution history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayHistoryResponse {
    /// Original inference ID
    pub inference_id: String,
    /// List of replay executions
    pub executions: Vec<ReplayExecutionRecord>,
    /// Total count of executions
    pub total_count: usize,
}

// ===== Replay Execution Context =====

/// Context for replay execution through InferenceCore
///
/// Contains the constraints and metadata needed to execute a deterministic
/// replay of a previous inference operation.
#[derive(Debug, Clone)]
pub struct ReplayContext {
    /// Original inference ID being replayed (for correlation/audit)
    pub original_inference_id: String,
    /// Required manifest hash - worker must match this exactly
    pub required_manifest_hash: String,
    /// Required backend (CoreML, MLX, Metal) - worker must be compatible
    pub required_backend: String,
    /// If true, don't capture new replay metadata for this execution
    /// (prevents recursive replay metadata creation)
    pub skip_metadata_capture: bool,
    /// Original policy ID that was in effect during the original inference
    pub original_policy_id: Option<String>,
    /// Original policy version that was in effect
    pub original_policy_version: Option<i64>,
}

// ===== Tenant Token Revocation =====

/// Response from tenant-wide token revocation operation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenRevocationResponse {
    /// Timestamp when revocation was executed (RFC3339)
    pub revoked_at: String,
    /// Human-readable message explaining the effect
    pub message: String,
}
