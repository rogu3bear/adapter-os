// Re-export types from shared API types crate
pub use adapteros_api_types::*;

// Re-export canonical types from adapteros-types
pub use adapteros_types::{AdapterInfo, AdapterMetrics, AdapterState};

// Additional client-specific types not covered by shared types
use serde::{Deserialize, Serialize};

/// Memory usage response for adapter management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsageResponse {
    pub adapters: Vec<AdapterMemoryInfo>,
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
    pub memory_pressure_level: MemoryPressureLevel,
}

/// Adapter memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMemoryInfo {
    pub id: String,
    pub name: String,
    pub memory_usage_mb: u64,
    pub state: String,
    pub pinned: bool,
    pub category: String,
}

/// Memory pressure levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryPressureLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Training session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartTrainingRequest {
    pub repository_path: String,
    pub adapter_name: String,
    pub description: String,
    pub training_config: serde_json::Value,
    pub base_model_id: String,
    pub tenant_id: String,
}

/// Training session response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingSessionResponse {
    pub session_id: String,
    pub status: String,
    pub created_at: String,
    pub progress: Option<f64>,
    pub error: Option<String>,
}

/// Telemetry filters for event queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFilters {
    pub limit: Option<usize>,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub event_type: Option<String>,
    pub level: Option<String>,
}

/// Client telemetry event structure (DTO for client-side usage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientTelemetryEvent {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub level: String,
    pub message: String,
    pub component: Option<String>,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Conversion from canonical TelemetryEvent to client DTO
#[cfg(not(target_arch = "wasm32"))]
impl From<adapteros_telemetry::unified_events::TelemetryEvent> for ClientTelemetryEvent {
    fn from(ev: adapteros_telemetry::unified_events::TelemetryEvent) -> Self {
        ClientTelemetryEvent {
            id: ev.id,
            timestamp: ev.timestamp.to_rfc3339(),
            event_type: ev.event_type,
            level: format!("{:?}", ev.level).to_lowercase(),
            message: ev.message,
            component: ev.component,
            tenant_id: Some(ev.identity.tenant_id),
            user_id: ev.user_id,
            metadata: ev.metadata,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnWorkerRequest {
    pub tenant_id: String,
    pub plan_id: String,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteCPRequest {
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionResponse {
    pub cpid: String,
    pub plan_id: String,
    pub promoted_by: String,
    pub promoted_at: String,
    pub quality_metrics: QualityMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub arr: f32,
    pub ecs5: f32,
    pub hlr: f32,
    pub cr: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionGatesResponse {
    pub cpid: String,
    pub gates: Vec<GateStatus>,
    pub all_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateStatus {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub evidence_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackCPRequest {
    pub tenant_id: String,
    pub cpid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResponse {
    pub cpid: String,
    pub previous_plan_id: String,
    pub rolled_back_by: String,
    pub rolled_back_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportModelRequest {
    pub name: String,
    pub hash_b3: String,
    pub config_hash_b3: String,
    pub tokenizer_hash_b3: String,
    pub tokenizer_cfg_hash_b3: String,
    pub license_hash_b3: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPackResponse {
    pub cpid: String,
    pub content: String,
    pub hash_b3: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatePolicyRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub hash_b3: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPolicyRequest {
    pub cpid: String,
    pub content: String,
}

// ========== Code Intelligence Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRepoRequest {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRepoRequest {
    pub repo_id: String,
    pub commit: String,
    pub full_scan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoResponse {
    pub repo_id: String,
    pub path: String,
    pub status: String,
    pub frameworks: Vec<FrameworkInfo>,
    pub file_count: Option<usize>,
    pub symbol_count: Option<usize>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkInfo {
    pub name: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAdaptersResponse {
    pub adapters: Vec<AdapterInfo>,
    pub tier_breakdown: TierBreakdown,
}

// AdapterInfo is now imported from adapteros_types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierBreakdown {
    pub base: usize,
    pub code: usize,
    pub framework: usize,
    pub codebase: usize,
    pub ephemeral: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationData {
    pub adapter_id: String,
    pub percentage: f32,
    pub request_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDeltaRequest {
    pub repo_id: String,
    pub commit: String,
    pub parent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDeltaResponse {
    pub cdp_id: String,
    pub diff_summary: DiffSummary,
    pub changed_symbols: Vec<ChangedSymbol>,
    pub test_results: Option<TestResults>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedSymbol {
    pub symbol_id: String,
    pub name: String,
    pub change_type: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResults {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub failures: Vec<TestFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    pub test: String,
    pub file: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDetailsResponse {
    pub commit: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub branch: Option<String>,
    pub repo_id: String,
    pub changed_files: Vec<ChangedFile>,
    pub impacted_symbols: Vec<ChangedSymbol>,
    pub test_results: Option<TestResults>,
    pub ephemeral_adapter: Option<EphemeralAdapterStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralAdapterStatus {
    pub adapter_id: String,
    pub status: String,
    pub mode: String,
    pub rank: u32,
    pub ttl_hours: u32,
    pub activations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictAdapterRequest {
    pub adapter_id: String,
}

// ========== Routing Inspector Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterFeaturesRequest {
    pub prompt: String,
    pub context_file: String,
    pub repo_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterFeaturesResponse {
    pub language_scores: std::collections::HashMap<String, f32>,
    pub framework_boosts: std::collections::HashMap<String, f32>,
    pub symbol_hit_count: usize,
    pub path_tokens: Vec<String>,
    pub depth_score: f32,
    pub is_test: bool,
    pub is_config: bool,
    pub commit_hint: f32,
    pub prompt_verb: Option<String>,
    pub attention_entropy: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreAdaptersRequest {
    pub repo_path: String,
    pub adapter_ids: Vec<String>,
    pub features: Option<RouterFeaturesResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreAdaptersResponse {
    pub scores: Vec<AdapterScoreInfo>,
    pub selected_ids: Vec<String>,
    pub k_value: usize,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterScoreInfo {
    pub adapter_id: String,
    pub score: f32,
    pub selected: bool,
    pub gate_q15: Option<u16>,
}

// ========== Patch Lab Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposePatchRequest {
    pub prompt: String,
    pub context_files: Vec<String>,
    pub repo_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposePatchResponse {
    pub proposal_id: String,
    pub patches: Vec<PatchDiff>,
    pub rationale: String,
    pub evidence: Vec<EvidenceCitation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchDiff {
    pub file: String,
    pub hunks: Vec<PatchHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub line_type: String, // "add", "delete", "context"
    pub content: String,
    pub line_number: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCitation {
    pub citation_type: String, // "code", "test", "doc", "framework"
    pub description: String,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatePatchRequest {
    pub proposal_id: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatePatchResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub test_results: Option<TestResults>,
    pub lint_results: Option<LintResults>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResults {
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchRequest {
    pub proposal_id: String,
    pub confirm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchResponse {
    pub applied: bool,
    pub backup_id: String,
    pub files_modified: usize,
}

// ========== Code Policy Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePolicy {
    pub min_evidence_spans: usize,
    pub allow_auto_apply: bool,
    pub test_coverage_min: f32,
    pub path_allowlist: Vec<String>,
    pub path_denylist: Vec<String>,
    pub secret_patterns: Vec<String>,
    pub max_patch_size: usize,
}

impl Default for CodePolicy {
    fn default() -> Self {
        Self {
            min_evidence_spans: 1,
            allow_auto_apply: false,
            test_coverage_min: 0.8,
            path_allowlist: vec![
                "src/**".to_string(),
                "lib/**".to_string(),
                "tests/**".to_string(),
            ],
            path_denylist: vec![
                "**/.env*".to_string(),
                "**/secrets/**".to_string(),
                "**/*.pem".to_string(),
            ],
            secret_patterns: vec![],
            max_patch_size: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCodePolicyResponse {
    pub policy: CodePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCodePolicyRequest {
    pub policy: CodePolicy,
}

// ========== Metrics Dashboard Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetricsRequest {
    pub cpid: String,
    pub time_range: String, // "7d", "30d", "90d"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetricsResponse {
    pub cpid: String,
    pub time_range: String,
    pub acceptance_rate: f32,
    pub acceptance_trend: f32,
    pub compile_success: f32,
    pub test_pass_rate: f32,
    pub regression_rate: f32,
    pub evidence_coverage: f32,
    pub follow_up_fixes_rate: f32,
    pub secret_violations: usize,
    pub latency_p95_ms: f32,
    pub throughput_req_per_sec: f32,
    pub router_overhead_pct: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareMetricsRequest {
    pub old_cpid: String,
    pub new_cpid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareMetricsResponse {
    pub old_cpid: String,
    pub new_cpid: String,
    pub metrics_old: CodeMetricsResponse,
    pub metrics_new: CodeMetricsResponse,
    pub improvements: Vec<String>,
    pub regressions: Vec<String>,
}
