//! Authentication related types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Login request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response with JWT token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub role: String,
}

/// User information response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserInfoResponse {
    pub user_id: String,
    pub email: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_last_rotated_at: Option<String>,
    /// Legacy field - kept for backwards compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Logout request (empty for now, but extensible)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogoutRequest {
    // Future: could include session invalidation details
}
