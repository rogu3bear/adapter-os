//! Setup self-service API types.

use serde::{Deserialize, Serialize};

/// A discovered model entry returned by setup discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SetupDiscoveredModel {
    pub name: String,
    pub model_path: String,
    pub format: String,
    pub backend: String,
}

/// Response for setup model discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SetupDiscoverModelsResponse {
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    pub root: String,
    pub models: Vec<SetupDiscoveredModel>,
    pub total: usize,
}

/// Request for setup model seeding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SetupSeedModelsRequest {
    pub model_paths: Vec<String>,
    #[serde(default)]
    pub force: bool,
}

/// Seed status for an individual model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SetupSeedModelStatus {
    Seeded,
    Skipped,
    Failed,
}

/// Per-model result for setup seeding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SetupSeedModelResult {
    pub name: String,
    pub model_path: String,
    pub status: SetupSeedModelStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response for setup model seeding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SetupSeedModelsResponse {
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    pub total: usize,
    pub seeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub results: Vec<SetupSeedModelResult>,
}

/// Response for setup migration trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SetupMigrateResponse {
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    pub status: String,
    pub message: String,
}
