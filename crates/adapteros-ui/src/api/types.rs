//! API response and request types
//!
//! UI-specific types for API responses not available in adapteros-api-types (wasm feature).
//! These types are used by the API client for serialization/deserialization.

pub use adapteros_api_types::training::JsonlValidationDiagnostic;

/// Checkpoint verification response (mirrors server-side `CheckpointVerifyResponse`)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckpointVerifyResponse {
    /// BLAKE3 hash of the checkpoint content (hex)
    pub blake3_hash: String,
    /// Key ID of the signer
    pub signer_key_id: String,
    /// ISO 8601 timestamp when the checkpoint was signed
    pub signed_at: String,
    /// Schema version of the sidecar
    pub schema_version: u8,
    /// Whether verification passed
    pub verified: bool,
}

/// Simple inference request for chat
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferenceRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

// ============================================================================
// Local types for API responses not in adapteros-api-types (wasm feature)
// ============================================================================

/// System overview response with complete system state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemOverviewResponse {
    #[serde(default)]
    pub schema_version: String,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverageInfo,
    pub resource_usage: ResourceUsageInfo,
    pub services: Vec<ServiceStatus>,
    pub active_sessions: i32,
    pub active_workers: i32,
    pub adapter_count: i32,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_node_id: Option<String>,
}

/// Load average information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadAverageInfo {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Resource usage information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceUsageInfo {
    pub cpu_usage_percent: f32,
    pub memory_usage_percent: f32,
    pub disk_usage_percent: f32,
    pub network_rx_mbps: f32,
    pub network_tx_mbps: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_used_gb: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_total_gb: Option<f32>,
}

/// Service status in system overview
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<u64>,
}

/// Error alert rule response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorAlertRuleResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    pub cooldown_minutes: i32,
    pub severity: String,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Request to create an error alert rule
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateErrorAlertRuleRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    #[serde(default = "default_cooldown_minutes")]
    pub cooldown_minutes: i32,
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
}

fn default_cooldown_minutes() -> i32 {
    15
}

fn default_severity() -> String {
    "warning".to_string()
}

/// Request to update an error alert rule
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UpdateErrorAlertRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_window_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
}

/// List error alert rules response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorAlertRulesListResponse {
    pub rules: Vec<ErrorAlertRuleResponse>,
    pub total: usize,
}

/// Error alert history item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorAlertHistoryResponse {
    pub id: String,
    pub rule_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name: Option<String>,
    pub tenant_id: String,
    pub triggered_at: String,
    pub error_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_error_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<String>,
}

/// List error alert history response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorAlertHistoryListResponse {
    pub alerts: Vec<ErrorAlertHistoryResponse>,
    pub total: usize,
}

/// Model architecture summary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelArchitectureSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_layers: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vocab_size: Option<usize>,
}

/// Model with stats response (from /internal/models endpoint)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelWithStatsResponse {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub config_hash_b3: String,
    pub tokenizer_hash_b3: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub adapter_count: i64,
    #[serde(default)]
    pub training_job_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(alias = "architecture_summary")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<ModelArchitectureSummary>,
}

/// Model list response (uses UI-specific ModelWithStatsResponse)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelListResponse {
    pub models: Vec<ModelWithStatsResponse>,
    pub total: usize,
}

// Model types (AneMemoryStatus, ModelStatusResponse, BaseModelStatusResponse,
// AllModelsStatusResponse, SeedModelRequest, SeedModelResponse) are now
// imported from adapteros_api_types::models

/// Workflow type for adapter stacks
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Parallel,
    UpstreamDownstream,
    Sequential,
}

/// Create stack request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateStackRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<WorkflowType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<String>,
}

/// Update stack request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateStackRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<WorkflowType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<String>,
}

/// Stack response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StackResponse {
    #[serde(default)]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<WorkflowType>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
    #[serde(default)]
    pub is_default: bool,
    pub version: i64,
    pub lifecycle_state: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Policy pack response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyPackResponse {
    pub cpid: String,
    pub content: String,
    pub hash_b3: String,
    pub created_at: String,
}

/// Validate policy request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidatePolicyRequest {
    pub content: String,
}

/// Policy validation response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub hash_b3: Option<String>,
}

/// Apply policy request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplyPolicyRequest {
    pub cpid: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activate: Option<bool>,
}

// CreateTrainingJobRequest and TrainingConfigRequest are now imported from adapteros_api_types::training
pub use adapteros_api_types::training::{CreateTrainingJobRequest, TrainingConfigRequest};

// WorkerMetricsResponse is now imported from adapteros_api_types::workers

// ============================================================================
// Collection types
// ============================================================================

/// Collection response (from /v1/collections endpoint)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionResponse {
    pub schema_version: String,
    pub collection_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub document_count: i32,
    pub tenant_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Collection detail response (includes documents)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionDetailResponse {
    pub schema_version: String,
    pub collection_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub document_count: i32,
    pub tenant_id: String,
    pub documents: Vec<CollectionDocumentInfo>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Document info within a collection
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionDocumentInfo {
    pub document_id: String,
    pub name: String,
    pub size_bytes: i64,
    pub status: String,
    pub added_at: String,
}

/// Create collection request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Add document to collection request
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddDocumentRequest {
    pub document_id: String,
}

/// Paginated collection list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionListResponse {
    pub schema_version: String,
    pub data: Vec<CollectionResponse>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub pages: u32,
}

// ============================================================================
// Audit types
// ============================================================================

/// Audit log entry response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: String,
    pub user_id: String,
    pub user_role: String,
    pub tenant_id: String,
    pub action: String,
    pub resource_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// Audit logs response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditLogsResponse {
    pub logs: Vec<AuditLogEntry>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Audit chain entry with hash linkage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditChainEntry {
    pub id: String,
    pub timestamp: String,
    pub action: String,
    pub resource_type: String,
    pub status: String,
    pub entry_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    pub chain_sequence: i64,
    pub verified: bool,
}

/// Audit chain response with verification status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditChainResponse {
    pub entries: Vec<AuditChainEntry>,
    pub chain_valid: bool,
    pub total_entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
}

/// Chain verification response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChainVerificationResponse {
    pub chain_valid: bool,
    pub total_entries: usize,
    pub verified_entries: usize,
    pub first_invalid_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
    pub verification_timestamp: String,
}

/// Federation audit response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationAuditResponse {
    pub total_hosts: usize,
    pub total_signatures: usize,
    pub verified_signatures: usize,
    pub quarantined: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quarantine_reason: Option<String>,
    pub host_chains: Vec<HostChainSummary>,
    pub timestamp: String,
}

/// Host chain summary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostChainSummary {
    pub host_id: String,
    pub bundle_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_bundle: Option<String>,
}

/// Compliance audit response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComplianceAuditResponse {
    pub compliance_rate: f64,
    pub total_controls: usize,
    pub compliant_controls: usize,
    pub active_violations: usize,
    pub controls: Vec<ComplianceControl>,
    pub timestamp: String,
}

/// Compliance control
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComplianceControl {
    pub control_id: String,
    pub control_name: String,
    pub status: String,
    pub last_checked: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub findings: Vec<String>,
}

/// Audit query parameters
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct AuditLogsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

// ============================================================================
// Trace/Telemetry API methods
// ============================================================================

// ============================================================================
// Trace/Telemetry types
// ============================================================================

/// Trace search query parameters
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TraceSearchQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_ns: Option<u64>,
}

/// Trace event (from trace buffer)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceEvent {
    pub trace_id: String,
    pub span_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub operation: String,
    pub status: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Inference trace summary response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceTraceResponse {
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub created_at: String,
    pub latency_ms: u64,
    pub token_count: u32,
    pub adapters_used: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Detailed inference trace with token-level breakdown
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceTraceDetailResponse {
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<String>,
    pub created_at: String,
    pub latency_ms: u64,
    pub adapters_used: Vec<String>,
    #[serde(default)]
    pub token_decisions: Vec<TokenDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_decisions_next_cursor: Option<u32>,
    #[serde(default)]
    pub token_decisions_has_more: bool,
    pub timing_breakdown: TimingBreakdown,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<TraceReceiptSummary>,
    /// Backend used (e.g., coreml, metal, mlx)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
}

/// UI-only inference trace detail response with extended receipt fields.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiInferenceTraceDetailResponse {
    pub trace_id: String,
    pub request_id: Option<String>,
    pub created_at: String,
    pub latency_ms: u64,
    pub adapters_used: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<String>,
    #[serde(default)]
    pub token_decisions: Vec<TokenDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_decisions_next_cursor: Option<u32>,
    #[serde(default)]
    pub token_decisions_has_more: bool,
    pub timing_breakdown: TimingBreakdown,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<UiTraceReceiptSummary>,
    /// Backend used (e.g., coreml, metal, mlx)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
}

/// Per-token routing decision
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenDecision {
    pub token_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<u32>,
    pub adapter_ids: Vec<String>,
    pub gates_q15: Vec<i16>,
    pub entropy: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<String>,
    /// Backend ID for this specific token (if different from trace)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
    /// Kernel version ID used for this token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version_id: Option<String>,
}

/// Timing breakdown for latency analysis
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TimingBreakdown {
    pub total_ms: u64,
    pub routing_ms: u64,
    pub inference_ms: u64,
    pub policy_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefill_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_ms: Option<u64>,
}

/// Receipt summary for trace verification
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceReceiptSummary {
    pub receipt_digest: String,
    pub run_head_hash: String,
    pub output_digest: String,
    pub logical_prompt_tokens: u32,
    pub logical_output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    pub verified: bool,
    /// Hardware/Equipment attestation fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane_version: Option<String>,
    /// Cache metrics
    #[serde(default)]
    pub prefix_cache_hit: bool,
    #[serde(default)]
    pub prefix_kv_bytes: u64,
}

/// UI-only receipt summary with extended provenance fields.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiTraceReceiptSummary {
    pub receipt_digest: String,
    pub run_head_hash: String,
    pub output_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_digest_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_lineage_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_attestation_b3: Option<String>,
    /// BLAKE3 hashes of training datasets for adapters used in this inference.
    /// Enables verification of which training data influenced the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_training_digests: Option<Vec<String>>,
    pub logical_prompt_tokens: u32,
    pub logical_output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    /// Hardware/Equipment attestation fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane_version: Option<String>,
    /// Cache metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_cache_hit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_kv_bytes: Option<u64>,
}

// ============================================================================
// Document types
// ============================================================================

/// Document response from the API
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentResponse {
    #[serde(default)]
    pub schema_version: String,
    pub document_id: String,
    pub name: String,
    pub hash_b3: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub storage_path: String,
    /// Status: "processing", "indexed", "failed"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_count: Option<i32>,
    pub tenant_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// True if this response points to a pre-existing document with identical content
    #[serde(default)]
    pub deduplicated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default)]
    pub retry_count: i32,
    #[serde(default)]
    pub max_retries: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_completed_at: Option<String>,
}

/// Document list response with pagination
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentListResponse {
    #[serde(default)]
    pub schema_version: String,
    pub data: Vec<DocumentResponse>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub pages: u32,
}

/// Document chunk response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkResponse {
    #[serde(default)]
    pub schema_version: String,
    pub chunk_id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

/// Chunk list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkListResponse {
    #[serde(default)]
    pub schema_version: String,
    pub chunks: Vec<ChunkResponse>,
    pub document_id: String,
    pub total_chunks: i32,
}

/// Document list query parameters
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DocumentListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Process document response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessDocumentResponse {
    #[serde(default)]
    pub schema_version: String,
    pub document_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================================
// Search types
// ============================================================================

/// Search result item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResultItem {
    /// Result type: "adapter", "page", etc.
    pub result_type: String,
    /// Unique ID
    pub id: String,
    /// Display title
    pub title: String,
    /// Subtitle/description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Link/path to navigate to
    pub path: String,
    /// Relevance score (0.0 - 1.0)
    pub score: f32,
}

/// Search response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResponse {
    /// Search results
    pub results: Vec<SearchResultItem>,
    /// Total count (may be approximate)
    pub total: u32,
    /// Query execution time in milliseconds
    #[serde(default)]
    pub took_ms: u64,
}

// ============================================================================
// Dataset types
// ============================================================================

/// Dataset response from the API (from /v1/datasets endpoints)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetResponse {
    #[serde(default)]
    pub schema_version: String,
    #[serde(alias = "dataset_id")]
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub format: String,
    #[serde(alias = "hash")]
    #[serde(alias = "hash_b3")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_diagnostics: Option<Vec<JsonlValidationDiagnostic>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_type: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Response for listing datasets
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetListResponse {
    #[serde(default)]
    pub schema_version: String,
    pub datasets: Vec<DatasetResponse>,
    #[serde(default)]
    pub total: i64,
}

/// Dataset statistics response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetStatisticsResponse {
    #[serde(default)]
    pub schema_version: String,
    pub dataset_id: String,
    #[serde(default)]
    pub num_examples: i64,
    #[serde(default)]
    pub avg_input_length: f64,
    #[serde(default)]
    pub avg_target_length: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_distribution: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type_distribution: Option<serde_json::Value>,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default)]
    pub computed_at: String,
}

/// Dataset preview response (first N examples).
///
/// Returned from `GET /v1/datasets/{dataset_id}/preview`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetPreviewResponse {
    pub dataset_id: String,
    pub format: String,
    #[serde(default)]
    pub total_examples: usize,
    #[serde(default)]
    pub examples: Vec<serde_json::Value>,
}

/// Preprocessed cache count response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreprocessedCacheCountResponse {
    #[serde(default)]
    pub schema_version: String,
    pub count: u64,
    pub dataset_count: u64,
}

/// Preprocessed cache entry summary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreprocessedCacheEntry {
    pub dataset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_name: Option<String>,
    pub preprocess_id: String,
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produced_at: Option<String>,
    pub example_count: usize,
}

/// Preprocessed cache list response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreprocessedCacheListResponse {
    #[serde(default)]
    pub schema_version: String,
    pub entries: Vec<PreprocessedCacheEntry>,
    pub total: u64,
}

fn default_validation_mode() -> String {
    "quick".to_string()
}

/// Request parameters for file validation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidateFileRequest {
    /// Validation mode: "quick" or "deep"
    #[serde(default = "default_validation_mode")]
    pub mode: String,
    /// Whether to check required fields for JSONL training format
    #[serde(default)]
    pub check_training_format: bool,
    /// Custom required fields to validate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_fields: Option<Vec<String>>,
}

/// Detailed file validation error
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileValidationError {
    pub severity: String,
    pub category: String,
    pub message: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Response from file validation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidateFileResponse {
    pub schema_version: String,
    pub file_id: String,
    pub file_name: String,
    pub is_valid: bool,
    pub validation_mode: String,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub entries_validated: usize,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FileValidationError>>,
    pub validated_at: String,
}

/// Response from validating all files in a dataset
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidateAllFilesResponse {
    pub schema_version: String,
    pub dataset_id: String,
    pub is_valid: bool,
    pub validation_mode: String,
    pub files_validated: usize,
    pub total_error_count: usize,
    pub total_warning_count: usize,
    pub total_entries_validated: usize,
    pub duration_ms: u64,
    pub file_results: Vec<ValidateFileResponse>,
    pub validated_at: String,
}

// ============================================================================
// Dataset Preprocessing types (PII scrub, deduplication)
// ============================================================================

/// Request to start preprocessing on a dataset
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StartDatasetPreprocessRequest {
    /// Whether to scrub PII from the dataset
    #[serde(default)]
    pub pii_scrub: bool,
    /// Whether to deduplicate the dataset
    #[serde(default)]
    pub dedupe: bool,
}

/// Response from starting a preprocessing job
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StartDatasetPreprocessResponse {
    /// Unique job ID for tracking progress
    pub job_id: String,
    /// Dataset ID being preprocessed
    pub dataset_id: String,
    /// Initial status
    pub status: String,
    /// Message describing the job
    pub message: String,
}

/// Response for preprocessing status check
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetPreprocessStatusResponse {
    /// Job ID
    pub job_id: String,
    /// Dataset ID being preprocessed
    pub dataset_id: String,
    /// Current status: pending, running, completed, failed
    pub status: String,
    /// Whether PII scrubbing was requested
    pub pii_scrub: bool,
    /// Whether deduplication was requested
    pub dedupe: bool,
    /// Number of lines processed so far
    pub lines_processed: usize,
    /// Number of lines removed (duplicates or PII-containing)
    pub lines_removed: usize,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// When the job started (ISO 8601)
    pub started_at: String,
    /// When the job completed (ISO 8601), if finished
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

// ============================================================================
// Code Policy types
// ============================================================================

/// Code policy settings for code generation safety constraints
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodePolicy {
    /// Minimum number of evidence spans required
    #[serde(default = "default_min_evidence_spans")]
    pub min_evidence_spans: usize,
    /// Whether auto-apply is allowed
    #[serde(default)]
    pub allow_auto_apply: bool,
    /// Minimum test coverage threshold (0.0 - 1.0)
    #[serde(default = "default_test_coverage_min")]
    pub test_coverage_min: f32,
    /// Allowed file paths (glob patterns)
    #[serde(default)]
    pub path_allowlist: Vec<String>,
    /// Denied file paths (glob patterns)
    #[serde(default)]
    pub path_denylist: Vec<String>,
    /// Secret detection patterns (regex)
    #[serde(default)]
    pub secret_patterns: Vec<String>,
    /// Maximum patch size in bytes
    #[serde(default = "default_max_patch_size")]
    pub max_patch_size: usize,
}

fn default_min_evidence_spans() -> usize {
    1
}
fn default_test_coverage_min() -> f32 {
    0.8
}
fn default_max_patch_size() -> usize {
    100_000
}

impl Default for CodePolicy {
    fn default() -> Self {
        Self {
            min_evidence_spans: 1,
            allow_auto_apply: false,
            test_coverage_min: 0.8,
            path_allowlist: vec![],
            path_denylist: vec![],
            secret_patterns: vec![],
            max_patch_size: 100_000,
        }
    }
}

/// Response containing code policy
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetCodePolicyResponse {
    pub policy: CodePolicy,
}

/// Request to update code policy
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateCodePolicyRequest {
    pub policy: CodePolicy,
}

// ============================================================================
// Health endpoint types (UI-only)
// ============================================================================

/// Component health status from /healthz/all and /system/ready
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ComponentStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Individual component health check
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComponentHealth {
    pub component: String,
    pub status: ComponentStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub timestamp: u64,
}

/// Aggregate health response for /healthz/all
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemHealthResponse {
    pub overall_status: ComponentStatus,
    pub components: Vec<ComponentHealth>,
    pub timestamp: u64,
}

/// Single readiness check in /readyz
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReadyzCheck {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// Readiness checks summary in /readyz
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReadyzChecks {
    pub db: ReadyzCheck,
    pub worker: ReadyzCheck,
    pub models_seeded: ReadyzCheck,
}

/// Readiness response from /readyz
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReadyzResponse {
    pub ready: bool,
    pub checks: ReadyzChecks,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub build_id: Option<String>,
}

/// System readiness response from /system/ready
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemReadyResponse {
    pub ready: bool,
    pub overall_status: ComponentStatus,
    #[serde(default)]
    pub components: Vec<ComponentHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_elapsed_ms: Option<u64>,
    #[serde(default)]
    pub critical_degraded: Vec<String>,
    #[serde(default)]
    pub non_critical_degraded: Vec<String>,
    #[serde(default)]
    pub maintenance: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

// ============================================================================
// Process Monitoring types
// ============================================================================

/// Process log entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessLogResponse {
    pub id: String,
    pub worker_id: String,
    pub level: String,
    pub message: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// Process crash dump
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessCrashDumpResponse {
    pub id: String,
    pub worker_id: String,
    pub crash_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_snapshot_json: Option<String>,
    pub crash_timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovered_at: Option<String>,
}

/// Process health metric
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessHealthMetricResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric_unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<serde_json::Value>,
    pub collected_at: String,
}

/// Process monitoring rule
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessMonitoringRuleResponse {
    pub id: String,
    pub name: String,
    pub rule_type: String,
    pub condition_json: String,
    pub action_json: String,
    pub enabled: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Process alert
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessAlertResponse {
    pub id: String,
    pub rule_id: String,
    pub worker_id: String,
    pub severity: String,
    pub message: String,
    pub status: String,
    pub triggered_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_by: Option<String>,
}

/// Process anomaly detection result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessAnomalyResponse {
    pub id: String,
    pub worker_id: String,
    pub anomaly_type: String,
    pub severity: String,
    pub description: String,
    pub status: String,
    pub detected_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

/// Request to create a monitoring rule
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateMonitoringRuleRequest {
    pub name: String,
    pub rule_type: String,
    pub condition: serde_json::Value,
    pub action: serde_json::Value,
    #[serde(default)]
    pub enabled: bool,
}

// ============================================================================
// Routing Decision Types
// ============================================================================

/// Query parameters for routing decisions
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionsQuery {
    pub tenant: Option<String>,
    pub stack_id: Option<String>,
    pub adapter_id: Option<String>,
    pub anomalies_only: Option<bool>,
    pub min_entropy: Option<f64>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Paginated routing decisions response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionsResponse {
    pub decisions: Vec<RoutingDecisionResponse>,
    pub total: usize,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: usize,
}

/// A single routing decision
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionResponse {
    pub id: String,
    pub tenant_id: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    pub step: i32,
    pub entropy: f64,
    pub k_value: i32,
    pub tau: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overhead_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_inference_latency_us: Option<i64>,
    pub timestamp: String,
    #[serde(default)]
    pub candidates: Vec<RoutingCandidateResponse>,
    #[serde(default)]
    pub selected_adapter_ids: Vec<String>,
}

/// A routing candidate adapter
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingCandidateResponse {
    pub adapter_id: String,
    pub gate_value: f64,
    pub rank: i32,
    pub selected: bool,
}

/// Request for routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDebugRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
}

/// Response from routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDebugResponse {
    pub detected_features: DetectedFeaturesResponse,
    pub adapter_scores: Vec<AdapterScoreResponse>,
    pub selected_adapters: Vec<String>,
    pub entropy: f64,
    pub k_value: i32,
    pub explanation: String,
}

/// Detected features from routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectedFeaturesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frameworks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

/// Adapter score from routing debug
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterScoreResponse {
    pub adapter_id: String,
    pub score: f64,
    pub gate_value: f64,
    pub selected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Routing decision chain response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecisionChainResponse {
    pub inference_id: String,
    pub tenant_id: String,
    pub decisions: Vec<RoutingDecisionResponse>,
    pub chain_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_hash: Option<String>,
}

// ============================================================================
// Dataset Safety Types
// ============================================================================

/// Result of checking if a dataset is safe for training
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetSafetyCheckResult {
    /// Whether the dataset is safe for training
    pub is_safe: bool,
    /// Trust state: allowed, allowed_with_warning, blocked, needs_approval, unknown
    pub trust_state: String,
    /// Individual safety signal statuses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_signals: Option<SafetySignals>,
    /// Reasons why training is blocked (if trust_state is blocked)
    #[serde(default)]
    pub blocking_reasons: Vec<String>,
    /// Warnings that don't block training
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Individual safety signal statuses
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SafetySignals {
    /// PII detection status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pii_status: Option<String>,
    /// Toxicity detection status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toxicity_status: Option<String>,
    /// Data leak detection status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leak_status: Option<String>,
    /// Anomaly detection status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anomaly_status: Option<String>,
    /// Overall safety status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overall_safety: Option<String>,
}

// ============================================================================
// Admin Types
// ============================================================================

// Admin types (UserResponse, ListUsersResponse) are now imported from
// adapteros_api_types::admin

// ============================================================================
// API Key Types
// ============================================================================

// API key types (CreateApiKeyRequest, CreateApiKeyResponse, ApiKeyInfo,
// ApiKeyListResponse, RevokeApiKeyResponse) are now imported from
// adapteros_api_types::api_keys

// ============================================================================
// Discrepancy Types
// ============================================================================

/// Request to create a discrepancy case
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateDiscrepancyRequest {
    /// Inference run ID that produced the discrepancy
    pub inference_id: String,
    /// Type of discrepancy (incorrect_answer, incomplete_answer, hallucination, etc.)
    pub discrepancy_type: String,
    /// Document ID referenced during inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Page number in source document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,
    /// BLAKE3 hash of the chunk used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_hash_b3: Option<String>,
    /// Whether to store plaintext content (requires explicit opt-in)
    #[serde(default)]
    pub store_content: bool,
    /// The user's original question
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_question: Option<String>,
    /// The model's answer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_answer: Option<String>,
    /// The correct/expected answer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ground_truth: Option<String>,
    /// Additional notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Response containing a discrepancy case
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscrepancyResponse {
    pub id: String,
    pub tenant_id: String,
    pub inference_id: String,
    pub discrepancy_type: String,
    pub resolution_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_answer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ground_truth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

// ============================================================================
// Verdict Types
// ============================================================================

/// Response containing verdict details
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerdictResponse {
    /// Unique verdict ID
    pub id: String,
    /// The inference ID this verdict applies to
    pub inference_id: String,
    /// Verdict level (high, medium, low, paused)
    pub verdict: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Evaluator type (rule, human, model)
    pub evaluator_type: String,
    /// Evaluator identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluator_id: Option<String>,
    /// Warnings/notes as JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings_json: Option<serde_json::Value>,
    /// Extraction confidence score
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_confidence_score: Option<f64>,
    /// Trust state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<String>,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
}

/// Request to derive a verdict using rules
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeriveVerdictRequest {
    /// Optional inference ID (required if store=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_id: Option<String>,
    /// Extraction confidence score from upstream processing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_confidence_score: Option<f64>,
    /// Trust state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<String>,
    /// Whether to store the derived verdict (default: false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
}

/// Response from verdict derivation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeriveVerdictResponse {
    /// Derived verdict level
    pub verdict: String,
    /// Confidence score
    pub confidence: f64,
    /// Warning message if rules triggered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// Structured warnings as JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings_json: Option<serde_json::Value>,
}

// ============================================================================
// Replay Session Types
// ============================================================================

/// Response for a replay session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplaySessionResponse {
    pub id: String,
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub snapshot_at: String,
    pub seed_global_b3: String,
    pub manifest_hash_b3: String,
    pub policy_hash_b3: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_hash_b3: Option<String>,
    pub telemetry_bundle_ids: Vec<String>,
    pub adapter_state: AdapterStateSnapshot,
    pub routing_decisions: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_traces: Option<Vec<serde_json::Value>>,
    pub signature: String,
    pub created_at: String,
}

/// Adapter state snapshot for replay sessions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterStateSnapshot {
    pub adapters: Vec<serde_json::Value>,
    pub timestamp: String,
    pub memory_usage_bytes: u64,
}

/// Request to create a replay session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateReplaySessionRequest {
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub telemetry_bundle_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_at: Option<String>,
}

/// Replay verification response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplayVerificationResponse {
    pub session_id: String,
    pub signature_valid: bool,
    pub hash_chain_valid: bool,
    pub manifest_verified: bool,
    pub policy_verified: bool,
    pub kernel_verified: bool,
    pub telemetry_verified: bool,
    pub overall_valid: bool,
    pub divergences: Vec<ReplayDivergence>,
    pub verified_at: String,
}

/// Divergence detected during replay verification
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplayDivergence {
    pub divergence_type: String,
    pub expected_hash: String,
    pub actual_hash: String,
    pub context: String,
}

// ============================================================================
// SSE Lifecycle Event Types
//
// Mirror the backend enum shapes in adapteros-server-api::sse::lifecycle_events.
// Tagged JSON (`"event"` field) allows serde to discriminate variants.
// ============================================================================

// ============================================================================
// Chat Collaboration Types (PASS 2)
// ============================================================================

/// Request to share a chat session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShareSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub permission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Information about a session share
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionShareInfo {
    pub share_id: String,
    pub user_id: String,
    pub permission: String,
    pub shared_at: String,
}

/// Response containing session shares
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSharesResponse {
    pub shares: Vec<SessionShareInfo>,
}

/// A session shared with the current user
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedSessionInfo {
    pub session_id: String,
    pub name: String,
    pub shared_by: String,
    pub permission: String,
    pub shared_at: String,
}

/// Response containing sessions shared with the current user
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedWithMeResponse {
    pub sessions: Vec<SharedSessionInfo>,
}

/// A chat tag
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatTagResponse {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to create a chat tag
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateChatTagRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to assign tags to a session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssignTagsRequest {
    pub tag_ids: Vec<String>,
}

/// Response containing tags on a session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionTagsResponse {
    pub tags: Vec<ChatTagResponse>,
}

/// Request to fork a chat session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForkSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub include_messages: bool,
}

/// Response from forking a chat session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForkSessionResponse {
    pub session_id: String,
    pub name: String,
    pub created_at: String,
    #[serde(default)]
    pub forked_from: Option<ForkedFromInfo>,
}

/// Source session info for a forked session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForkedFromInfo {
    pub session_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// Chat session list item (for archived/trash lists)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatSessionListItem {
    pub id: String,
    pub name: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

// ============================================================================
// Replay Execution Types (PASS 3)
// ============================================================================

/// Request to execute a replay session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecuteReplayRequest {
    #[serde(default)]
    pub use_original_rag_docs: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    pub max_tokens: u32,
}

/// Response from executing a replay session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecuteReplayResponse {
    pub session_id: String,
    pub output: String,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub missing_doc_ids: Vec<String>,
    #[serde(default)]
    pub no_rag_state_stored: bool,
    pub latency_ms: u64,
    #[serde(default)]
    pub verified_at: Option<String>,
}

/// Receipt verification result (reused for trace and bundle verify)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReceiptVerificationResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub pass: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_head_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_digest: Option<String>,
    #[serde(default)]
    pub signature_checked: bool,
    #[serde(default)]
    pub signature_valid: bool,
}

// ============================================================================
// Policy Governance Types (PASS 4)
// ============================================================================

/// Response from signing a policy
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignPolicyResponse {
    pub cpid: String,
    pub signature: String,
    pub signed_at: String,
    pub signed_by: String,
}

/// Response from verifying a policy signature
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerifyPolicyResponse {
    pub cpid: String,
    pub is_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub verified_at: Option<String>,
}

/// Request to compare two policies
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyComparisonRequest {
    pub cpid_1: String,
    pub cpid_2: String,
}

/// Response from comparing two policies
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyComparisonResponse {
    #[serde(default)]
    pub cpid_1: Option<String>,
    #[serde(default)]
    pub cpid_2: Option<String>,
    pub differences: Vec<String>,
    pub identical: bool,
}

/// Response from exporting a policy
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportPolicyResponse {
    pub cpid: String,
    pub policy_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub exported_at: String,
}

/// Policy assignment response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyAssignmentResponse {
    pub id: String,
    pub policy_pack_id: String,
    pub target_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(default)]
    pub enforced: bool,
}

/// Policy violation response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyViolationResponse {
    pub id: String,
    pub tenant_id: String,
    pub resource_type: String,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub resolved: bool,
    pub created_at: String,
}

// ============================================================================
// Session Security Types (PASS 5)
// ============================================================================

/// Response containing active auth sessions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionsResponse {
    #[serde(default)]
    pub schema_version: String,
    pub sessions: Vec<adapteros_api_types::auth::SessionInfo>,
}

// ============================================================================
// Storage Visibility Types (PASS 6)
// ============================================================================

/// Storage mode response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageModeResponse {
    pub mode: String,
    pub description: String,
    #[serde(default)]
    pub kv_available: bool,
    #[serde(default)]
    pub dual_write_active: bool,
}

/// Storage statistics response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageStatsResponse {
    pub mode: String,
    #[serde(default)]
    pub sql_counts: serde_json::Value,
    #[serde(default)]
    pub kv_counts: serde_json::Value,
    #[serde(default)]
    pub kv_metrics: Option<serde_json::Value>,
    #[serde(default)]
    pub safe_to_cutover: Option<bool>,
    #[serde(default)]
    pub cutover_evidence: Vec<String>,
    pub collected_at: String,
}

/// Tenant storage usage response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TenantStorageUsageResponse {
    pub tenant_id: String,
    #[serde(default)]
    pub dataset_bytes: u64,
    #[serde(default)]
    pub artifact_bytes: u64,
    #[serde(default)]
    pub dataset_versions: i64,
    #[serde(default)]
    pub adapter_versions: i64,
    #[serde(default)]
    pub soft_limit_bytes: u64,
    #[serde(default)]
    pub hard_limit_bytes: u64,
    #[serde(default)]
    pub soft_exceeded: bool,
    #[serde(default)]
    pub hard_exceeded: bool,
}

/// WASM-friendly adapter version summary for the version list UI.
///
/// This mirrors the server-only `AdapterVersionResponse` but omits fields
/// that pull in server-only dependencies (e.g., `DatasetVersionTrustSnapshot`).
/// Unknown fields are silently dropped via `#[serde(default)]`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterVersionSummary {
    pub id: String,
    pub repo_id: String,
    pub version: String,
    pub branch: String,
    #[serde(default)]
    pub release_state: String,
    #[serde(default)]
    pub adapter_trust_state: String,
    #[serde(default)]
    pub serveable: bool,
    #[serde(default)]
    pub serveable_reason: Option<String>,
    #[serde(default)]
    pub training_backend: Option<String>,
    #[serde(default)]
    pub coreml_used: Option<bool>,
    pub created_at: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Adapter lifecycle events from SSE stream `/v1/stream/adapters`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AdapterLifecycleEvent {
    /// Adapter tier promoted (e.g. persistent -> warm -> ephemeral)
    Promoted {
        adapter_id: String,
        from_state: String,
        to_state: String,
    },
    /// Adapter loaded into memory
    Loaded {
        adapter_id: String,
        load_time_ms: u64,
    },
    /// Adapter load failed
    LoadFailed { adapter_id: String, error: String },
    /// Adapter unloaded / evicted from memory
    Evicted { adapter_id: String, reason: String },
}

/// Adapter version lifecycle events from SSE stream `/v1/stream/adapters`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AdapterVersionEvent {
    /// A version was promoted to active
    VersionPromoted {
        version_id: String,
        repo_id: String,
        branch: String,
    },
    /// A branch was rolled back to a previous version
    VersionRolledBack {
        repo_id: String,
        branch: String,
        target_version_id: String,
    },
}

/// Training lifecycle events from SSE stream `/v1/stream/training`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TrainingLifecycleEvent {
    /// Training job started
    JobStarted {
        job_id: String,
        adapter_id: String,
        config_summary: String,
    },
    /// An epoch completed
    EpochCompleted {
        job_id: String,
        epoch: u32,
        total_epochs: u32,
        loss: f64,
        learning_rate: f64,
    },
    /// Checkpoint saved to disk
    CheckpointSaved {
        job_id: String,
        epoch: u32,
        path: String,
    },
    /// Training job completed successfully
    JobCompleted {
        job_id: String,
        adapter_id: String,
        final_loss: f64,
        duration_secs: u64,
    },
    /// Training job failed
    JobFailed {
        job_id: String,
        error: String,
        last_epoch: u32,
    },
}

/// System health transition events from SSE stream `/v1/stream/telemetry` (alerts).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SystemHealthTransitionEvent {
    /// Worker lifecycle state changed
    WorkerStateChanged {
        worker_id: String,
        previous: String,
        current: String,
        reason: String,
    },
    /// Drain phase started
    DrainStarted {
        worker_id: String,
        previous_status: String,
    },
}
