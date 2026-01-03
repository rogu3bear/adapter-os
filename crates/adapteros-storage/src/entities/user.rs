//! User entity KV schema
//!
//! This module defines the canonical user entity for key-value storage,
//! replacing the SQL `users` table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User role enumeration - simplified 3-role model
///
/// # Roles
/// - **Admin**: Full access to everything including system settings and user management
/// - **Operator**: Can run inference, training, manage adapters. Cannot change system settings or users.
/// - **Viewer**: Read-only access. Can view dashboards, logs, but cannot modify anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// Convert role to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Operator => "operator",
            Role::Viewer => "viewer",
        }
    }

    /// Parse role from string
    pub fn parse_role(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "operator" => Some(Role::Operator),
            "viewer" => Some(Role::Viewer),
            // Backwards compatibility: map old roles to new ones
            "developer" => Some(Role::Admin),
            "sre" => Some(Role::Operator),
            "compliance" => Some(Role::Viewer),
            "auditor" => Some(Role::Viewer),
            _ => None,
        }
    }

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
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Error type for role parsing failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleParseError {
    pub invalid_role: String,
}

impl std::fmt::Display for RoleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid role: '{}', valid roles are: admin, operator, viewer",
            self.invalid_role
        )
    }
}

impl std::error::Error for RoleParseError {}

impl std::str::FromStr for Role {
    type Err = RoleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "admin" => Ok(Role::Admin),
            "operator" => Ok(Role::Operator),
            "viewer" => Ok(Role::Viewer),
            // Backwards compatibility: map old roles to new ones
            "developer" => Ok(Role::Admin),
            "sre" => Ok(Role::Operator),
            "compliance" => Ok(Role::Viewer),
            "auditor" => Ok(Role::Viewer),
            _ => Err(RoleParseError {
                invalid_role: s.to_string(),
            }),
        }
    }
}

/// Canonical user entity for KV storage
///
/// This struct represents the authoritative schema for user entities in the
/// key-value storage backend. It includes all fields from the SQL `users` table
/// with proper type conversions.
///
/// **Key Design:**
/// - Primary key: `user/{id}`
/// - Secondary indexes:
///   - `user-by-email/{email}` -> `{id}`
///   - `tenant/{tenant_id}/users` -> Set<{id}>
///   - `users-by-role/{role}` -> Set<{id}>
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserKv {
    // Core identity
    pub id: String,
    pub email: String,
    pub display_name: String,

    // Authentication (never serialized in logs/telemetry)
    #[serde(default)]
    pub pw_hash: String,

    // Authorization
    pub role: Role,
    pub tenant_id: String,

    // Status
    pub disabled: bool,
    #[serde(default)]
    pub failed_attempts: i64,
    #[serde(default)]
    pub last_failed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub lockout_until: Option<DateTime<Utc>>,

    // Timestamps
    pub created_at: DateTime<Utc>,

    // MFA
    #[serde(default)]
    pub mfa_enabled: bool,
    #[serde(default)]
    pub mfa_secret_enc: Option<String>,
    #[serde(default)]
    pub mfa_backup_codes_json: Option<String>,
    #[serde(default)]
    pub mfa_enrolled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub mfa_last_verified_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub mfa_recovery_last_used_at: Option<DateTime<Utc>>,

    // Security tracking
    #[serde(default)]
    pub password_rotated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub token_rotated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_login_at: Option<DateTime<Utc>>,
}

impl UserKv {
    /// Check if the user is active (not disabled)
    pub fn is_active(&self) -> bool {
        !self.disabled
    }

    /// Check if the user has admin role (full access)
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Check if the user has admin role (full access)
    pub fn has_full_access(&self) -> bool {
        self.role == Role::Admin
    }

    /// Check if the user can write (admin or operator)
    pub fn can_write(&self) -> bool {
        self.role.can_write()
    }

    /// Check if the user has operator role or higher (admin or operator)
    pub fn is_operator_or_higher(&self) -> bool {
        matches!(self.role, Role::Admin | Role::Operator)
    }

    /// Check if the user is a viewer (read-only access)
    pub fn is_viewer(&self) -> bool {
        self.role.is_viewer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_conversion() {
        // New roles
        assert_eq!(Role::parse_role("admin"), Some(Role::Admin));
        assert_eq!(Role::parse_role("operator"), Some(Role::Operator));
        assert_eq!(Role::parse_role("viewer"), Some(Role::Viewer));

        // Backwards compatibility
        assert_eq!(Role::parse_role("developer"), Some(Role::Admin));
        assert_eq!(Role::parse_role("sre"), Some(Role::Operator));
        assert_eq!(Role::parse_role("compliance"), Some(Role::Viewer));

        // Invalid
        assert_eq!(Role::parse_role("invalid"), None);
    }

    #[test]
    fn test_role_permissions() {
        let admin = UserKv {
            id: "user-1".to_string(),
            email: "admin@example.com".to_string(),
            display_name: "Admin User".to_string(),
            pw_hash: "hash".to_string(),
            role: Role::Admin,
            tenant_id: "tenant-1".to_string(),
            disabled: false,
            failed_attempts: 0,
            last_failed_at: None,
            lockout_until: None,
            created_at: Utc::now(),
            mfa_enabled: false,
            mfa_secret_enc: None,
            mfa_backup_codes_json: None,
            mfa_enrolled_at: None,
            mfa_last_verified_at: None,
            mfa_recovery_last_used_at: None,
            password_rotated_at: None,
            token_rotated_at: None,
            last_login_at: None,
        };

        assert!(admin.is_admin());
        assert!(admin.has_full_access());
        assert!(admin.can_write());
        assert!(admin.is_operator_or_higher());
        assert!(!admin.is_viewer());

        let operator = UserKv {
            id: "user-2".to_string(),
            email: "operator@example.com".to_string(),
            display_name: "Operator User".to_string(),
            pw_hash: "hash".to_string(),
            role: Role::Operator,
            tenant_id: "tenant-1".to_string(),
            disabled: false,
            failed_attempts: 0,
            last_failed_at: None,
            lockout_until: None,
            created_at: Utc::now(),
            mfa_enabled: false,
            mfa_secret_enc: None,
            mfa_backup_codes_json: None,
            mfa_enrolled_at: None,
            mfa_last_verified_at: None,
            mfa_recovery_last_used_at: None,
            password_rotated_at: None,
            token_rotated_at: None,
            last_login_at: None,
        };

        assert!(!operator.is_admin());
        assert!(!operator.has_full_access());
        assert!(operator.can_write());
        assert!(operator.is_operator_or_higher());
        assert!(!operator.is_viewer());

        let viewer = UserKv {
            id: "user-3".to_string(),
            email: "viewer@example.com".to_string(),
            display_name: "Viewer User".to_string(),
            pw_hash: "hash".to_string(),
            role: Role::Viewer,
            tenant_id: "tenant-1".to_string(),
            disabled: false,
            failed_attempts: 0,
            last_failed_at: None,
            lockout_until: None,
            created_at: Utc::now(),
            mfa_enabled: false,
            mfa_secret_enc: None,
            mfa_backup_codes_json: None,
            mfa_enrolled_at: None,
            mfa_last_verified_at: None,
            mfa_recovery_last_used_at: None,
            password_rotated_at: None,
            token_rotated_at: None,
            last_login_at: None,
        };

        assert!(!viewer.is_admin());
        assert!(!viewer.has_full_access());
        assert!(!viewer.can_write());
        assert!(!viewer.is_operator_or_higher());
        assert!(viewer.is_viewer());
    }

    #[test]
    fn test_user_status() {
        let mut user = UserKv {
            id: "user-1".to_string(),
            email: "user@example.com".to_string(),
            display_name: "Test User".to_string(),
            pw_hash: "hash".to_string(),
            role: Role::Operator,
            tenant_id: "tenant-1".to_string(),
            disabled: false,
            failed_attempts: 0,
            last_failed_at: None,
            lockout_until: None,
            created_at: Utc::now(),
            mfa_enabled: false,
            mfa_secret_enc: None,
            mfa_backup_codes_json: None,
            mfa_enrolled_at: None,
            mfa_last_verified_at: None,
            mfa_recovery_last_used_at: None,
            password_rotated_at: None,
            token_rotated_at: None,
            last_login_at: None,
        };

        assert!(user.is_active());

        user.disabled = true;
        assert!(!user.is_active());
    }
}
