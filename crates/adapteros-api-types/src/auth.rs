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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_code: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenants: Option<Vec<TenantSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_level: Option<String>,
}

/// Minimal tenant summary for tenant picker
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TenantSummary {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub status: Option<String>,
    pub created_at: Option<String>,
}

/// Current user's tenant list response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TenantListResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub tenants: Vec<TenantSummary>,
}

/// Switch tenant request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SwitchTenantRequest {
    pub tenant_id: String,
}

/// Switch tenant response (reuses login response shape)
pub type SwitchTenantResponse = LoginResponse;

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admin_tenants: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_last_rotated_at: Option<String>,
}

/// MFA enrollment start response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MfaEnrollStartResponse {
    /// Base32-encoded TOTP secret
    pub secret: String,
    /// otpauth URI (for QR rendering)
    pub otpauth_url: String,
}

/// MFA enrollment verification request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MfaEnrollVerifyRequest {
    /// First TOTP code to confirm the secret
    pub totp_code: String,
}

/// MFA enrollment verification response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MfaEnrollVerifyResponse {
    /// Plaintext backup codes (shown once)
    pub backup_codes: Vec<String>,
}

/// Disable MFA request (requires TOTP or backup code)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MfaDisableRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_code: Option<String>,
}

/// MFA status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MfaStatusResponse {
    pub mfa_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrolled_at: Option<String>,
}

/// Logout request (empty for now, but extensible)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct LogoutRequest {
    // Future: could include session invalidation details
}

/// Session information for audit/management
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SessionInfo {
    pub jti: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    pub last_activity: String,
}

/// Token refresh response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RefreshResponse {
    pub token: String,
    pub expires_at: i64,
}

/// Authentication configuration (public subset)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AuthConfigResponse {
    pub dev_bypass_allowed: bool,
    pub mfa_required: bool,
    pub session_timeout_seconds: i64,
    pub max_sessions_per_user: i32,
}

/// User role enum (must match DB)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Developer,
    Operator,
    Sre,
    Compliance,
    Auditor,
    Viewer,
}
