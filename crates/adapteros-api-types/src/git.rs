//! Git integration types

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// Git status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitStatusResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub enabled: bool,
    pub active_sessions: u32,
    pub repositories_tracked: u32,
    pub last_scan: Option<String>,
}

/// Start git session request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct StartGitSessionRequest {
    pub repository_path: String,
    pub branch: Option<String>,
}

/// Git session response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitSessionResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub session_id: String,
    pub repository_path: String,
    pub branch: String,
    pub started_at: String,
}

/// Git branch information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitBranchInfo {
    pub name: String,
    pub is_current: bool,
    pub last_commit: String,
    pub ahead: u32,
    pub behind: u32,
}

/// File change event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct FileChangeEvent {
    pub file_path: String,
    pub change_type: String, // "added", "modified", "deleted"
    pub timestamp: String,
    pub session_id: String,
}

/// Working-tree status response for a repository
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct WorkingTreeStatusResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub branch: String,
    pub modified_files: Vec<String>,
    pub untracked_files: Vec<String>,
    pub staged_files: Vec<String>,
}

/// Working-tree operation request for a single file path
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct WorkingTreeFileOperationRequest {
    pub file_path: String,
}

/// Generic working-tree operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct WorkingTreeOperationResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub success: bool,
}

/// Working-tree diff response for repository/path
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct WorkingTreeDiffResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub diff: String,
}

/// Create commit request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitCommitRequest {
    pub message: String,
}

/// Create commit response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitCommitResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub success: bool,
    pub commit_sha: String,
}

/// Checkout branch request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitCheckoutRequest {
    pub branch: String,
}

/// Checkout branch response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitCheckoutResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub success: bool,
    pub branch: String,
}

/// Lightweight git log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GitLogEntry {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: String,
}
