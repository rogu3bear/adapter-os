//! Code repository management types.

use serde::{Deserialize, Serialize};

/// Register repository request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RegisterRepositoryRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
}

/// Register repository response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RegisterRepositoryResponse {
    pub status: String,
    pub repo_id: String,
    pub message: String,
}

/// Scan repository request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ScanRepositoryRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub commit: String,
    pub full_scan: bool,
}

/// Scan job response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ScanJobResponse {
    pub status: String,
    pub job_id: String,
    pub repo_id: String,
    pub commit: String,
    pub estimated_duration_seconds: Option<u32>,
}

/// Scan job status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ScanJobStatusResponse {
    pub job_id: String,
    pub status: String,
    pub progress: ScanJobProgress,
    pub result: Option<ScanJobResult>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ScanJobProgress {
    pub current_stage: Option<String>,
    pub percentage: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ScanJobResult {
    pub code_graph_hash: String,
    pub symbol_index_hash: Option<String>,
    pub vector_index_hash: Option<String>,
    pub test_map_hash: Option<String>,
    pub file_count: i32,
    pub symbol_count: i32,
    pub test_count: i32,
}

/// List repositories query
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ListRepositoriesQuery {
    pub page: Option<i32>,
    pub limit: Option<i32>,
    pub status: Option<String>,
}

/// Repository list response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RepositoryListResponse {
    pub repos: Vec<RepositoryInfo>,
    pub pagination: Pagination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RepositoryInfo {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub status: String,
    pub latest_scan_commit: Option<String>,
    pub latest_scan_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct Pagination {
    pub page: i32,
    pub limit: i32,
    pub total: i64,
}

/// Repository detail response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RepositoryDetailResponse {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub latest_scan_commit: Option<String>,
    pub latest_scan_at: Option<String>,
    pub status: String,
    pub latest_graph_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Commit delta request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CommitDeltaRequest {
    pub tenant_id: String,
    pub repo_id: String,
    pub base_commit: String,
    pub head_commit: String,
}

/// Commit delta response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CommitDeltaResponse {
    pub status: String,
    pub job_id: String,
    pub message: String,
}
