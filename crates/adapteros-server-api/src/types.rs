use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::collections::HashMap;

/// API error response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Login request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response with JWT token
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub role: String,
}

/// Create tenant request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTenantRequest {
    pub name: String,
    pub itar_flag: bool,
}

/// Tenant response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TenantResponse {
    pub id: String,
    pub name: String,
    pub itar_flag: bool,
    pub created_at: String,
}

/// Register node request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterNodeRequest {
    pub hostname: String,
    pub agent_endpoint: String,
}

/// Node response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NodeResponse {
    pub id: String,
    pub hostname: String,
    pub agent_endpoint: String,
    pub status: String,
    pub last_seen_at: Option<String>,
}

/// Node ping response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NodePingResponse {
    pub node_id: String,
    pub status: String,
    pub latency_ms: f64,
}

/// Worker info for node details
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerInfo {
    pub id: String,
    pub tenant_id: String,
    pub plan_id: String,
    pub status: String,
}

/// Node details response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NodeDetailsResponse {
    pub id: String,
    pub hostname: String,
    pub agent_endpoint: String,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub workers: Vec<WorkerInfo>,
    pub recent_logs: Vec<String>,
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

/// Build plan request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BuildPlanRequest {
    pub tenant_id: String,
    pub manifest_hash_b3: String,
}

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

/// Health check response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Inference request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InferRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_evidence: Option<bool>,
}

/// Inference response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InferResponse {
    pub text: String,
    pub tokens: Vec<u32>,
    pub finish_reason: String,
    pub trace: InferenceTrace,
}

/// Inference trace for observability
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InferenceTrace {
    pub adapters_used: Vec<String>,
    pub router_decisions: Vec<RouterDecision>,
    pub latency_ms: u64,
}

/// Router decision at a specific position
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RouterDecision {
    pub position: usize,
    pub adapter_ids: Vec<u16>,
    pub gates: Vec<u16>,
}

/// Worker response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkerResponse {
    pub id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub pid: Option<i32>,
    pub status: String,
    pub started_at: String,
    pub last_seen_at: Option<String>,
}

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

/// User info response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserInfoResponse {
    pub user_id: String,
    pub email: String,
    pub role: String,
}

/// Plan response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PlanResponse {
    pub id: String,
    pub tenant_id: String,
    pub manifest_hash_b3: String,
    pub kernel_hash_b3: Option<String>,
    pub layout_hash_b3: Option<String>,
    pub status: String,
    pub created_at: String,
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
}

// ===== Adapter Types =====

/// Adapter response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterResponse {
    pub id: String,
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,
    pub languages: Vec<String>,
    pub framework: Option<String>,
    pub created_at: String,
    pub stats: Option<AdapterStats>,
}

/// Adapter statistics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterStats {
    pub total_activations: i64,
    pub selected_count: i64,
    pub avg_gate_value: f64,
    pub selection_rate: f64,
}

/// Register adapter request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterAdapterRequest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,
    pub languages: Vec<String>,
    pub framework: Option<String>,
}

/// Adapter activation response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterActivationResponse {
    pub id: String,
    pub adapter_id: String,
    pub request_id: Option<String>,
    pub gate_value: f64,
    pub selected: bool,
    pub created_at: String,
}

/// Adapter state transition response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterStateResponse {
    pub adapter_id: String,
    pub old_state: String,
    pub new_state: String,
    pub timestamp: String,
}

/// Adapter manifest for download
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterManifest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,
    pub framework: Option<String>,
    pub languages_json: Option<String>,
    pub category: Option<String>,
    pub scope: Option<String>,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Adapter health status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterHealthResponse {
    pub adapter_id: String,
    pub total_activations: i32,
    pub selected_count: i32,
    pub avg_gate_value: f64,
    pub memory_usage_mb: f64,
    pub policy_violations: Vec<String>,
    pub recent_activations: Vec<AdapterActivationResponse>,
}

// ===== Plan Management Types =====

/// Plan details response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PlanDetailsResponse {
    pub id: String,
    pub tenant_id: String,
    pub manifest_hash_b3: String,
    pub kernel_hash_b3: Option<String>,
    pub routing_config: serde_json::Value,
    pub created_at: String,
}

/// Plan rebuild response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PlanRebuildResponse {
    pub old_plan_id: String,
    pub new_plan_id: String,
    pub diff_summary: String,
    pub timestamp: String,
}

/// Compare plans request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComparePlansRequest {
    pub plan_id_1: String,
    pub plan_id_2: String,
}

/// Plan comparison response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PlanComparisonResponse {
    pub plan_id_1: String,
    pub plan_id_2: String,
    pub differences: Vec<String>,
    pub identical: bool,
}

// ===== Repository Types =====

/// Repository response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RepositoryResponse {
    pub id: String,
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub status: String,
    pub frameworks: Vec<String>,
    pub file_count: Option<i64>,
    pub symbol_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Register repository request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterRepositoryRequest {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
}

/// Trigger scan request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TriggerScanRequest {
    pub repo_id: String,
}

/// Scan status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ScanStatusResponse {
    pub repo_id: String,
    pub status: String,
    pub progress: Option<f32>,
    pub message: Option<String>,
}

// ===== Metrics Types =====

/// Quality metrics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QualityMetricsResponse {
    pub arr: f32,  // Answer Relevance Rate
    pub ecs5: f32, // Evidence Citation Score @ 5
    pub hlr: f32,  // Hallucination Rate
    pub cr: f32,   // Contradiction Rate
    pub timestamp: String,
}

/// Adapter metrics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterMetricsResponse {
    pub adapters: Vec<AdapterPerformance>,
}

/// Adapter performance metrics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterPerformance {
    pub adapter_id: String,
    pub name: String,
    pub activation_rate: f64,
    pub avg_gate_value: f64,
    pub total_requests: i64,
}

/// System metrics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SystemMetricsResponse {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub active_workers: i32,
    pub requests_per_second: f32,
    pub avg_latency_ms: f32,
    pub disk_usage: f32,
    pub network_bandwidth: f32,
    pub gpu_utilization: f32,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverageResponse,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoadAverageResponse {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

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
}

/// Routing decisions query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct RoutingDecisionsQuery {
    pub tenant: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub since: Option<String>, // ISO-8601 timestamp
}

fn default_limit() -> usize {
    50
}

/// Single routing decision
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RoutingDecision {
    pub ts: String,
    pub tenant_id: String,
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

/// Update tenant request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub itar_flag: Option<bool>,
    pub quotas: Option<serde_json::Value>,
    pub namespace: Option<String>,
}

/// Assign policies request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignPoliciesRequest {
    pub policy_ids: Vec<String>,
}

/// Assign policies response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignPoliciesResponse {
    pub tenant_id: String,
    pub assigned_cpids: Vec<String>,
    pub assigned_at: String,
}

/// Assign adapters request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignAdaptersRequest {
    pub adapter_ids: Vec<String>,
}

/// Assign adapters response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignAdaptersResponse {
    pub tenant_id: String,
    pub assigned_adapter_ids: Vec<String>,
    pub assigned_at: String,
}

/// Tenant usage response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TenantUsageResponse {
    pub tenant_id: String,
    pub cpu_usage_pct: f64,
    pub gpu_usage_pct: f64,
    pub memory_used_gb: f64,
    pub memory_total_gb: f64,
    pub inference_count_24h: i64,
    pub active_adapters_count: i32,
    // Optional legacy fields
    pub avg_latency_ms: Option<f64>,
    pub estimated_cost_usd: Option<f64>,
}

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
// Training API Types
// ============================================================================

/// Training configuration request
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct TrainingConfigRequest {
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    pub warmup_steps: Option<u32>,
    pub max_seq_length: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
}

impl From<TrainingConfigRequest> for adapteros_orchestrator::TrainingConfig {
    fn from(req: TrainingConfigRequest) -> Self {
        Self {
            rank: req.rank,
            alpha: req.alpha,
            targets: req.targets,
            epochs: req.epochs,
            learning_rate: req.learning_rate,
            batch_size: req.batch_size,
            warmup_steps: req.warmup_steps,
            max_seq_length: req.max_seq_length,
            gradient_accumulation_steps: req.gradient_accumulation_steps,
        }
    }
}

/// Start training request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartTrainingRequest {
    pub adapter_name: String,
    pub config: TrainingConfigRequest,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
}

/// Training job response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrainingJobResponse {
    pub id: String,
    pub adapter_name: String,
    pub template_id: Option<String>,
    pub repo_id: Option<String>,
    pub status: String,
    pub progress_pct: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub current_loss: f32,
    pub learning_rate: f32,
    pub tokens_per_second: f32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
}

impl From<adapteros_orchestrator::TrainingJob> for TrainingJobResponse {
    fn from(job: adapteros_orchestrator::TrainingJob) -> Self {
        Self {
            id: job.id,
            adapter_name: job.adapter_name,
            template_id: job.template_id,
            repo_id: job.repo_id,
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
        }
    }
}

/// Training metrics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrainingMetricsResponse {
    pub loss: f32,
    pub tokens_per_second: f32,
    pub learning_rate: f32,
    pub current_epoch: u32,
    pub total_epochs: u32,
    pub progress_pct: f32,
}

/// Training template response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TrainingTemplateResponse {
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

impl From<adapteros_orchestrator::TrainingTemplate> for TrainingTemplateResponse {
    fn from(template: adapteros_orchestrator::TrainingTemplate) -> Self {
        Self {
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

// Domain Adapter Types

/// Domain adapter response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DomainAdapterResponse {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: HashMap<String, serde_json::Value>,
    pub status: String,
    pub epsilon_stats: Option<EpsilonStatsResponse>,
    pub last_execution: Option<String>,
    pub execution_count: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// Epsilon statistics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EpsilonStatsResponse {
    pub mean_error: f64,
    pub max_error: f64,
    pub error_count: u64,
    pub last_updated: String,
}

/// Create domain adapter request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateDomainAdapterRequest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: HashMap<String, serde_json::Value>,
}

/// Test domain adapter request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TestDomainAdapterRequest {
    pub adapter_id: String,
    pub input_data: String,
    pub expected_output: Option<String>,
    pub iterations: Option<u32>,
}

/// Test domain adapter response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TestDomainAdapterResponse {
    pub test_id: String,
    pub adapter_id: String,
    pub input_data: String,
    pub actual_output: String,
    pub expected_output: Option<String>,
    pub epsilon: Option<f64>,
    pub passed: bool,
    pub iterations: u32,
    pub execution_time_ms: u64,
    pub executed_at: String,
}

/// Domain adapter manifest response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DomainAdapterManifestResponse {
    pub adapter_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: HashMap<String, serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// Load domain adapter request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoadDomainAdapterRequest {
    pub adapter_id: String,
    pub executor_config: Option<HashMap<String, serde_json::Value>>,
}

/// Domain adapter execution response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DomainAdapterExecutionResponse {
    pub execution_id: String,
    pub adapter_id: String,
    pub input_hash: String,
    pub output_hash: String,
    pub epsilon: f64,
    pub execution_time_ms: u64,
    pub trace_events: Vec<String>,
    pub executed_at: String,
}
