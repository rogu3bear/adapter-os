//! Admin management types
//!
//! Types for user management and admin operations.

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// User response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct UserResponse {
    pub user_id: String,
    /// Alias for user_id
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub tenant_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
}

/// List users response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListUsersResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub users: Vec<UserResponse>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// List users query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListUsersParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_page_size() -> i64 {
    100
}

/// Create user request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CreateUserRequest {
    pub email: String,
    pub display_name: String,
    pub password: String,
    pub role: String,
    #[serde(default = "default_tenant")]
    pub tenant_id: String,
}

fn default_tenant() -> String {
    "default".to_string()
}

/// Update user request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct UpdateUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// Admin status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AdminStatusResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub status: String,
    pub version: String,
    pub runtime_mode: String,
    pub lifecycle: String,
    pub strict_mode: bool,
    pub maintenance_mode: bool,
    pub draining_mode: bool,
    pub in_flight_requests: u64,
    pub registered_workers: usize,
    pub rag_enabled: bool,
}

/// Sanitized admin configuration response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AdminConfigResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub production_mode: bool,
    pub dev_bypass_enabled: bool,
    pub require_mfa: bool,
    pub allow_registration: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwt_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_ttl_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token_ttl_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_ttl_seconds: Option<u64>,
    pub ssrf_protection: bool,
    pub metrics_enabled: bool,
    pub review_webhook_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_adapters: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_workers: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrent_requests: Option<usize>,
    pub max_concurrent_training_jobs: usize,
    pub worker_heartbeat_interval_secs: u64,
    pub streaming_heartbeat_interval_secs: u64,
    pub streaming_idle_timeout_secs: u64,
    pub streaming_circuit_failure_threshold: u32,
    pub streaming_circuit_recovery_timeout_secs: u64,
    pub self_hosting_mode: String,
    pub self_hosting_repo_allowlist_count: usize,
}
