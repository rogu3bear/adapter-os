//! Settings management types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// System settings categories
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SystemSettings {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub general: GeneralSettings,
    pub server: ServerSettings,
    pub security: SecuritySettings,
    pub performance: PerformanceSettings,
}

/// General system settings
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct GeneralSettings {
    pub system_name: String,
    pub environment: String,
    pub api_base_url: String,
}

/// Server settings
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ServerSettings {
    pub http_port: u16,
    pub https_port: Option<u16>,
    pub uds_socket_path: Option<String>,
    pub production_mode: bool,
}

/// Security settings
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SecuritySettings {
    pub jwt_mode: String,
    pub token_ttl_seconds: u32,
    pub require_mfa: bool,
    pub egress_enabled: bool,
    pub require_pf_deny: bool,
}

/// Performance settings
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PerformanceSettings {
    pub max_adapters: u32,
    pub max_workers: u32,
    pub memory_threshold_pct: f64,
    pub cache_size_mb: u64,
}

/// Update settings request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateSettingsRequest {
    pub general: Option<GeneralSettings>,
    pub server: Option<ServerSettings>,
    pub security: Option<SecuritySettings>,
    pub performance: Option<PerformanceSettings>,
}

/// Settings update response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SettingsUpdateResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub success: bool,
    pub restart_required: bool,
    pub message: String,
}
