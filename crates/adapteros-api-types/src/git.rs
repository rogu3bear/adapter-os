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
