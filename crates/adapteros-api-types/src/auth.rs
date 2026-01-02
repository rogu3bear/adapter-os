//! Authentication related types

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// Login request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantListResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub tenants: Vec<TenantSummary>,
}

/// Switch tenant request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SwitchTenantRequest {
    pub tenant_id: String,
}

/// Switch tenant response (reuses login response shape)
pub type SwitchTenantResponse = LoginResponse;

/// User information response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct MfaEnrollStartResponse {
    /// Base32-encoded TOTP secret
    pub secret: String,
    /// otpauth URI (for QR rendering)
    pub otpauth_url: String,
}

/// MFA enrollment verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct MfaEnrollVerifyRequest {
    /// First TOTP code to confirm the secret
    pub totp_code: String,
}

/// MFA enrollment verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct MfaEnrollVerifyResponse {
    /// Plaintext backup codes (shown once)
    pub backup_codes: Vec<String>,
}

/// Disable MFA request (requires TOTP or backup code)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct MfaDisableRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_code: Option<String>,
}

/// MFA status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct MfaStatusResponse {
    pub mfa_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrolled_at: Option<String>,
}

/// Logout request (empty for now, but extensible)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct LogoutRequest {
    // Future: could include session invalidation details
}

/// Session information for audit/management
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RefreshResponse {
    pub token: String,
    pub expires_at: i64,
}

/// Authentication configuration (public subset)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AuthConfigResponse {
    pub dev_bypass_allowed: bool,
    pub mfa_required: bool,
    pub session_timeout_seconds: i64,
    pub max_sessions_per_user: i32,
}

/// User role enum - simplified 3-role model
///
/// # Roles
/// - **Admin**: Full access to everything including system settings and user management
/// - **Operator**: Can run inference, training, manage adapters. Cannot change system settings or users.
/// - **Viewer**: Read-only access. Can view dashboards, logs, but cannot modify anything.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Full access to everything
    #[serde(rename = "admin")]
    Admin,
    /// Can run inference, training, manage adapters. Cannot change system settings or users.
    #[serde(rename = "operator")]
    Operator,
    /// Read-only access. Can view dashboards, logs, but cannot modify anything.
    #[serde(rename = "viewer")]
    Viewer,
}

impl Role {
    /// Check if this role has write (modify) access
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Admin | Role::Operator)
    }

    /// Check if this role has admin access (full permissions)
    pub fn can_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }

    /// Check if this role is viewer-only (read-only access)
    pub fn is_viewer(&self) -> bool {
        matches!(self, Role::Viewer)
    }

    /// Convert role to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Operator => "operator",
            Role::Viewer => "viewer",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "admin" => Ok(Role::Admin),
            "operator" => Ok(Role::Operator),
            "viewer" => Ok(Role::Viewer),
            // Backwards compatibility: map old roles to new ones
            "developer" => Ok(Role::Admin),      // Developer had full access like Admin
            "sre" => Ok(Role::Operator),         // SRE maps to Operator
            "compliance" => Ok(Role::Viewer),    // Compliance was read-focused
            "auditor" => Ok(Role::Viewer),       // Auditor was read-focused
            _ => Err(format!("invalid role: '{}', valid roles are: admin, operator, viewer", s)),
        }
    }
}

/// Backwards compatibility alias
#[deprecated(note = "Use Role instead")]
pub type UserRole = Role;
