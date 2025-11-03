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

/// Session information for active authentication sessions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub created_at: String,
    pub last_seen_at: String,
    pub is_current: bool,
}

/// Token rotation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RotateTokenResponse {
    pub token: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rotated_at: Option<String>,
}

/// Token metadata information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenMetadata {
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rotated_at: Option<String>,
    pub role: String,
    pub tenant_id: String,
}

/// Profile update request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Profile update response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProfileResponse {
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
}

/// Authentication configuration response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthConfigResponse {
    pub production_mode: bool,
    pub dev_token_enabled: bool,
    pub jwt_mode: String,
    pub token_expiry_hours: u32,
}

/// Authentication configuration update request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateAuthConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev_token_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwt_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expiry_hours: Option<u32>,
}
