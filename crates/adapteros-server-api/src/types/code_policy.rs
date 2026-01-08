use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetCodePolicyResponse {
    pub policy: CodePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCodePolicyRequest {
    pub policy: CodePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CodeMetricsRequest {
    pub cpid: String,
    pub time_range: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompareMetricsRequest {
    pub old_cpid: String,
    pub new_cpid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompareMetricsResponse {
    pub old_cpid: String,
    pub new_cpid: String,
    pub metrics_old: CodeMetricsResponse,
    pub metrics_new: CodeMetricsResponse,
    pub improvements: Vec<String>,
    pub regressions: Vec<String>,
}
