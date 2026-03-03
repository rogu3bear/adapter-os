//! Settings management types

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// System settings categories
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SystemSettings {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub general: GeneralSettings,
    pub models: ModelSettings,
    pub server: ServerSettings,
    pub security: SecuritySettings,
    pub performance: PerformanceSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restart_required_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_restart_fields: Vec<String>,
}

/// General system settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct GeneralSettings {
    pub system_name: String,
    pub environment: String,
    pub api_base_url: String,
}

/// Model discovery and selection settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ModelSettings {
    #[serde(default)]
    pub discovery_roots: Vec<String>,
    /// Optional worker base-model directory used for startup when env overrides are unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_model_path: Option<String>,
    /// Optional explicit worker manifest path used for startup when env overrides are unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_manifest_path: Option<String>,
}

/// Server settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ServerSettings {
    pub http_port: u16,
    pub https_port: Option<u16>,
    pub uds_socket_path: Option<String>,
    pub production_mode: bool,
}

/// Security settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SecuritySettings {
    pub jwt_mode: String,
    pub token_ttl_seconds: u32,
    pub require_mfa: bool,
    pub egress_enabled: bool,
    pub require_pf_deny: bool,
}

/// Performance settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct PerformanceSettings {
    pub max_adapters: u32,
    pub max_workers: u32,
    pub memory_threshold_pct: f64,
    pub cache_size_mb: u64,
}

/// Update settings request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct UpdateSettingsRequest {
    pub general: Option<GeneralSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<ModelSettings>,
    pub server: Option<ServerSettings>,
    pub security: Option<SecuritySettings>,
    pub performance: Option<PerformanceSettings>,
}

/// Settings update response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SettingsUpdateResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub success: bool,
    pub restart_required: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_live: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_for_restart: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_restart_fields: Vec<String>,
}

/// Effective source entry for a managed key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct EffectiveSettingsEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub effective_source: String,
}

/// Effective settings response with source metadata for managed keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct EffectiveSettingsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub entries: Vec<EffectiveSettingsEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub managed_keys: Vec<String>,
}

/// Reconcile response for runtime config file/DB dual-write state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SettingsReconcileResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub success: bool,
    pub status: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
}
