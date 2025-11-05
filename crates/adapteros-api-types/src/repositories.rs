//! Repository management types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

fn default_main_branch() -> String {
    "main".to_string()
}

/// Register repository request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterRepositoryRequest {
    /// Remote repository URL (https or ssh)
    pub url: String,
    /// Target branch to track. Defaults to `main` if omitted.
    #[serde(default = "default_main_branch")]
    pub branch: String,
    /// Optional deterministic repository identifier for legacy clients
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Optional filesystem path supplied by legacy clients
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional tenant override supplied by legacy clients
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Optional language hints supplied by legacy clients
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    /// Optional legacy default branch field (superseded by `branch`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

/// Minimal repository summary returned to clients
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RepositorySummary {
    /// Internal identifier for the repository (tenant scoped)
    pub id: String,
    /// Repository URL (may be a repo_id fallback if remote inference failed)
    pub url: String,
    /// Whether the URL is a fallback repo_id (true) or real remote URL (false)
    #[serde(default)]
    pub url_is_fallback: bool,
    /// Default branch being tracked
    pub branch: String,
    /// Path to the local checkout (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Total commits ingested for this repository
    pub commit_count: u64,
    /// Timestamp of the last successful scan
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scan: Option<String>,
}

/// Trigger scan request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TriggerScanRequest {
    pub repo_id: String,
}

/// Scan status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ScanStatusResponse {
    pub repo_id: String,
    pub status: String,
    pub progress: Option<f32>,
    pub message: Option<String>,
}
