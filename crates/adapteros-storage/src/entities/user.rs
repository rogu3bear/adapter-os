//! User entity KV schema
//!
//! This module defines the canonical user entity for key-value storage,
//! replacing the SQL `users` table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User role enumeration
///
/// Defines the canonical roles in AdapterOS RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[serde(rename = "admin")]
    Admin,
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "operator")]
    Operator,
    #[serde(rename = "sre")]
    SRE,
    #[serde(rename = "compliance")]
    Compliance,
    #[serde(rename = "viewer")]
    Viewer,
}

impl Role {
    /// Convert role to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Developer => "developer",
            Role::Operator => "operator",
            Role::SRE => "sre",
            Role::Compliance => "compliance",
            Role::Viewer => "viewer",
        }
    }

    /// Parse role from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "developer" => Some(Role::Developer),
            "operator" => Some(Role::Operator),
            "sre" => Some(Role::SRE),
            "compliance" => Some(Role::Compliance),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
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
        write!(f, "invalid role: '{}'", self.invalid_role)
    }
}

impl std::error::Error for RoleParseError {}

impl std::str::FromStr for Role {
    type Err = RoleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "admin" => Ok(Role::Admin),
            "developer" => Ok(Role::Developer),
            "operator" => Ok(Role::Operator),
            "sre" => Ok(Role::SRE),
            "compliance" => Ok(Role::Compliance),
            "viewer" => Ok(Role::Viewer),
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
}

impl UserKv {
    /// Check if the user is active (not disabled)
    pub fn is_active(&self) -> bool {
        !self.disabled
    }

    /// Check if the user has admin role
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Check if the user has developer role (full access like admin)
    pub fn is_developer(&self) -> bool {
        self.role == Role::Developer
    }

    /// Check if the user has admin or developer role (full access)
    pub fn has_full_access(&self) -> bool {
        matches!(self.role, Role::Admin | Role::Developer)
    }

    /// Check if the user has operator role or higher
    pub fn is_operator_or_higher(&self) -> bool {
        matches!(self.role, Role::Admin | Role::Developer | Role::Operator)
    }

    /// Check if the user has SRE role or higher
    pub fn is_sre_or_higher(&self) -> bool {
        matches!(self.role, Role::Admin | Role::Developer | Role::SRE)
    }

    /// Check if the user can perform compliance operations
    pub fn can_access_compliance(&self) -> bool {
        matches!(self.role, Role::Admin | Role::Developer | Role::SRE | Role::Compliance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_conversion() {
        assert_eq!(Role::from_str("admin"), Some(Role::Admin));
        assert_eq!(Role::from_str("operator"), Some(Role::Operator));
        assert_eq!(Role::from_str("sre"), Some(Role::SRE));
        assert_eq!(Role::from_str("compliance"), Some(Role::Compliance));
        assert_eq!(Role::from_str("viewer"), Some(Role::Viewer));
        assert_eq!(Role::from_str("invalid"), None);
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
        };

        assert!(admin.is_admin());
        assert!(admin.is_operator_or_higher());
        assert!(admin.is_sre_or_higher());
        assert!(admin.can_access_compliance());

        let viewer = UserKv {
            id: "user-2".to_string(),
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
        };

        assert!(!viewer.is_admin());
        assert!(!viewer.is_operator_or_higher());
        assert!(!viewer.is_sre_or_higher());
        assert!(!viewer.can_access_compliance());
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
        };

        assert!(user.is_active());

        user.disabled = true;
        assert!(!user.is_active());
    }
}
