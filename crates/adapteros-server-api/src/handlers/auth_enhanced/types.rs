//! Types for enhanced authentication handlers
//!
//! Contains request/response structs used across auth handlers.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request for user self-registration
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// User email address
    pub email: String,
    /// Password (minimum 12 characters)
    pub password: String,
    /// Display name (optional, defaults to email prefix)
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Response from registration endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterResponse {
    /// Created user ID
    pub user_id: String,
    /// Created tenant ID (each user gets their own tenant)
    pub tenant_id: String,
    /// JWT access token for immediate authentication
    pub token: String,
    /// Token expiration in seconds
    pub expires_in: u64,
}

/// Request for bootstrapping initial admin user
#[derive(Debug, Deserialize, ToSchema)]
pub struct BootstrapRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

/// Response from bootstrap endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct BootstrapResponse {
    pub user_id: String,
    pub message: String,
}

/// Response from logout endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    pub message: String,
}

/// Response from token refresh endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct RefreshResponse {
    pub token: String,
    pub expires_at: i64,
}

/// Response from auth health endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthHealthResponse {
    pub status: String,
    pub db: String,
    pub signing_keys: String,
    pub idp_configured: bool,
}

/// Information about an active session
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionInfo {
    pub jti: String,
    pub created_at: String,
    pub ip_address: Option<String>,
    pub last_activity: String,
}

/// Response listing user's active sessions
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionInfo>,
}

/// Authentication configuration response for frontend
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuthConfigResponse {
    /// Whether user registration is allowed
    pub allow_registration: bool,
    /// Whether email verification is required
    pub require_email_verification: bool,
    /// Access token lifetime in minutes
    pub access_token_ttl_minutes: u32,
    /// Session timeout in minutes
    pub session_timeout_minutes: u32,
    /// Maximum failed login attempts before lockout
    pub max_login_attempts: u32,
    /// Minimum password length
    pub password_min_length: u32,
    /// Whether MFA is required
    pub mfa_required: bool,
    /// Allowed email domains for registration (empty = all)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    /// Whether running in production mode
    pub production_mode: bool,
    /// Whether dev login bypass is enabled in config
    pub dev_token_enabled: bool,
    /// Whether dev bypass is actually allowed (computed from config)
    pub dev_bypass_allowed: bool,
    /// JWT signing mode (eddsa or hmac)
    pub jwt_mode: String,
    /// Token expiry in hours
    pub token_expiry_hours: u32,
}

/// Request for dev bootstrap endpoint
#[cfg(all(feature = "dev-bypass", debug_assertions))]
#[derive(Debug, Deserialize, ToSchema)]
pub struct DevBootstrapRequest {
    /// Admin email (defaults to "dev-admin@adapteros.local")
    #[serde(default = "default_dev_email")]
    pub email: String,
    /// Admin password (defaults to "dev-password-123")
    #[serde(default = "default_dev_password")]
    pub password: String,
}

#[cfg(all(feature = "dev-bypass", debug_assertions))]
fn default_dev_email() -> String {
    "dev-admin@adapteros.local".to_string()
}

#[cfg(all(feature = "dev-bypass", debug_assertions))]
fn default_dev_password() -> String {
    "dev-password-123".to_string()
}

/// Response from dev bootstrap endpoint
#[cfg(all(feature = "dev-bypass", debug_assertions))]
#[derive(Debug, Serialize, ToSchema)]
pub struct DevBootstrapResponse {
    /// The system tenant ID
    pub system_tenant_id: String,
    /// The created admin user ID
    pub admin_user_id: String,
    /// Admin email
    pub email: String,
    /// Admin password (only shown once)
    pub password: String,
    /// JWT token for immediate use
    pub token: String,
    /// Instructions message
    pub message: String,
}
