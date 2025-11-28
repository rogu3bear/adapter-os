//! Authentication related types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Login request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub username: Option<String>,
    pub email: String,
    pub password: String,
}

/// Login response with JWT token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct LoginResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub token: String,
    pub user_id: String,
    pub tenant_id: String,
    pub role: String,
    pub expires_in: u64,
}

/// User information response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UserInfoResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub created_at: String,
    pub tenant_id: String,
    pub display_name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_last_rotated_at: Option<String>,
}

/// Logout request (empty for now, but extensible)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct LogoutRequest {
    // Future: could include session invalidation details
}
