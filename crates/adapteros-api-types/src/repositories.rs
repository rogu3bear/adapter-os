//! Repository management types

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// Register repository request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RegisterRepositoryRequest {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
}

/// Repository response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RepositoryResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
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

/// Trigger scan request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TriggerScanRequest {
    pub repo_id: String,
}

/// Scan status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ScanStatusResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub repo_id: String,
    pub status: String,
    pub progress: Option<f32>,
    pub message: Option<String>,
}
