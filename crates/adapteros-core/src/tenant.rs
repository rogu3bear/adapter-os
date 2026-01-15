//! Tenant and workspace identity types
//!
//! Provides strongly-typed identifiers for multi-tenant isolation.
//! These types ensure tenant context is explicit and validated at compile time.
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::tenant::{TenantId, WorkspaceId, TenantContext};
//!
//! // Create validated tenant ID
//! let tenant = TenantId::new("acme-corp").unwrap();
//! assert_eq!(tenant.as_str(), "acme-corp");
//!
//! // Use default for single-tenant mode
//! let default = TenantId::single_tenant_default();
//! assert_eq!(default.as_str(), "primary");
//!
//! // Create tenant context for request handling
//! let ctx = TenantContext::new(tenant);
//! ```

use crate::{AosError, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::Hash;

/// Validation regex: alphanumeric start/end, hyphens and underscores allowed in middle
/// Length: 1-64 characters
static TENANT_ID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9_-]{0,62}[a-zA-Z0-9])?$").unwrap());

/// Strongly-typed tenant identifier
///
/// TenantId is the primary isolation boundary in adapterOS. All tenant-scoped
/// resources (adapters, datasets, documents, RAG indices) must be associated
/// with a TenantId.
///
/// # Validation Rules
///
/// - 1-64 characters
/// - Must start and end with alphanumeric character
/// - May contain alphanumeric, hyphens (`-`), and underscores (`_`)
/// - Cannot contain path traversal sequences (`..`, `/`, `\`)
///
/// # Examples
///
/// ```rust
/// use adapteros_core::tenant::TenantId;
///
/// // Valid tenant IDs
/// assert!(TenantId::new("primary").is_ok());
/// assert!(TenantId::new("acme-corp").is_ok());
/// assert!(TenantId::new("tenant_123").is_ok());
/// assert!(TenantId::new("a").is_ok());  // Single char OK
///
/// // Invalid tenant IDs
/// assert!(TenantId::new("").is_err());           // Empty
/// assert!(TenantId::new("../etc").is_err());     // Path traversal
/// assert!(TenantId::new("-invalid").is_err());   // Starts with hyphen
/// ```
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(String);

impl TenantId {
    /// Create a new TenantId with validation
    ///
    /// # Errors
    ///
    /// Returns `AosError::Validation` if the ID fails validation rules.
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        Self::validate(&id)?;
        Ok(Self(id))
    }

    /// Create TenantId without validation (for trusted internal use)
    ///
    /// Use only for DB reads where data is known to be valid, or for
    /// migration scenarios where validation would break existing data.
    ///
    /// # Safety
    ///
    /// This function does not validate the input. Callers must ensure
    /// the ID is valid or accept the consequences of invalid IDs.
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Validate a tenant ID string
    fn validate(id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(AosError::Validation(
                "Tenant ID cannot be empty".to_string(),
            ));
        }

        if id.len() > 64 {
            return Err(AosError::Validation(format!(
                "Tenant ID '{}' exceeds 64 character limit (got {})",
                id,
                id.len()
            )));
        }

        // Path traversal protection
        if id.contains("..") {
            return Err(AosError::Validation(format!(
                "Tenant ID '{}' cannot contain '..' (path traversal)",
                id
            )));
        }

        if id.contains('/') || id.contains('\\') {
            return Err(AosError::Validation(format!(
                "Tenant ID '{}' cannot contain path separators ('/' or '\\')",
                id
            )));
        }

        if !TENANT_ID_REGEX.is_match(id) {
            return Err(AosError::Validation(format!(
                "Invalid tenant ID '{}': must start and end with alphanumeric, \
                 may contain alphanumeric, hyphens, or underscores",
                id
            )));
        }

        Ok(())
    }

    /// Get the default tenant ID for single-tenant deployments
    ///
    /// Returns a TenantId with value "primary". Use this as the default
    /// when `single_tenant_mode` is enabled in configuration.
    pub fn single_tenant_default() -> Self {
        Self("primary".to_string())
    }

    /// Get the system tenant ID for internal operations
    ///
    /// Returns a TenantId with value "system". Use this for audit logs
    /// and other internal operations that are not tenant-scoped.
    pub fn system() -> Self {
        Self("system".to_string())
    }

    /// Get tenant ID from environment variable TENANT_ID, falling back to single_tenant_default()
    ///
    /// Attempts to read the `TENANT_ID` environment variable and parse it as a valid TenantId.
    /// If the variable is not set or contains an invalid value, returns `single_tenant_default()`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use adapteros_core::tenant::TenantId;
    ///
    /// // With TENANT_ID set to valid value
    /// std::env::set_var("TENANT_ID", "acme-corp");
    /// let tenant = TenantId::from_env();
    /// assert_eq!(tenant.as_str(), "acme-corp");
    ///
    /// // With TENANT_ID unset or invalid
    /// std::env::remove_var("TENANT_ID");
    /// let tenant = TenantId::from_env();
    /// assert_eq!(tenant.as_str(), "primary");
    /// ```
    pub fn from_env() -> Self {
        std::env::var("TENANT_ID")
            .ok()
            .and_then(|id| TenantId::new(id).ok())
            .unwrap_or_else(Self::single_tenant_default)
    }

    /// Get the tenant ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TenantId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TenantId({})", self.0)
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for TenantId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TenantId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        TenantId::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::str::FromStr for TenantId {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

/// Strongly-typed workspace identifier
///
/// WorkspaceId provides sub-tenant isolation for resources that can be
/// organized into workspaces (datasets, chat sessions, etc.). A tenant
/// may have multiple workspaces.
///
/// # Validation Rules
///
/// Same as TenantId:
/// - 1-64 characters
/// - Must start and end with alphanumeric character
/// - May contain alphanumeric, hyphens (`-`), and underscores (`_`)
/// - Cannot contain path traversal sequences
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    /// Create a new WorkspaceId with validation
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        Self::validate(&id)?;
        Ok(Self(id))
    }

    /// Create WorkspaceId without validation (for trusted internal use)
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Validate a workspace ID string (same rules as TenantId)
    fn validate(id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(AosError::Validation(
                "Workspace ID cannot be empty".to_string(),
            ));
        }

        if id.len() > 64 {
            return Err(AosError::Validation(format!(
                "Workspace ID '{}' exceeds 64 character limit (got {})",
                id,
                id.len()
            )));
        }

        if id.contains("..") {
            return Err(AosError::Validation(format!(
                "Workspace ID '{}' cannot contain '..' (path traversal)",
                id
            )));
        }

        if id.contains('/') || id.contains('\\') {
            return Err(AosError::Validation(format!(
                "Workspace ID '{}' cannot contain path separators",
                id
            )));
        }

        if !TENANT_ID_REGEX.is_match(id) {
            return Err(AosError::Validation(format!(
                "Invalid workspace ID '{}': must start and end with alphanumeric, \
                 may contain alphanumeric, hyphens, or underscores",
                id
            )));
        }

        Ok(())
    }

    /// Get the default workspace ID
    ///
    /// Returns a WorkspaceId with value "default". Use this when a tenant
    /// has not explicitly created workspaces.
    pub fn default_workspace() -> Self {
        Self("default".to_string())
    }

    /// Get the workspace ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for WorkspaceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WorkspaceId({})", self.0)
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for WorkspaceId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for WorkspaceId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        WorkspaceId::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::str::FromStr for WorkspaceId {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

/// Context for tenant-scoped operations
///
/// TenantContext bundles a TenantId with an optional WorkspaceId for
/// request-scoped operations. Most code paths should accept a TenantContext
/// rather than separate tenant/workspace parameters.
///
/// # Examples
///
/// ```rust
/// use adapteros_core::tenant::{TenantId, WorkspaceId, TenantContext};
///
/// // Tenant-only context
/// let ctx = TenantContext::new(TenantId::new("acme-corp").unwrap());
/// assert!(ctx.workspace.is_none());
///
/// // Tenant + workspace context
/// let ctx = TenantContext::with_workspace(
///     TenantId::new("acme-corp").unwrap(),
///     WorkspaceId::new("production").unwrap(),
/// );
/// assert!(ctx.workspace.is_some());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantContext {
    /// The tenant ID (always present)
    pub tenant: TenantId,
    /// Optional workspace ID for sub-tenant scoping
    pub workspace: Option<WorkspaceId>,
}

impl TenantContext {
    /// Create a new tenant context without workspace
    pub fn new(tenant: TenantId) -> Self {
        Self {
            tenant,
            workspace: None,
        }
    }

    /// Create a new tenant context with workspace
    pub fn with_workspace(tenant: TenantId, workspace: WorkspaceId) -> Self {
        Self {
            tenant,
            workspace: Some(workspace),
        }
    }

    /// Create a context for single-tenant deployments
    pub fn single_tenant() -> Self {
        Self::new(TenantId::single_tenant_default())
    }

    /// Create a context for system operations
    pub fn system() -> Self {
        Self::new(TenantId::system())
    }

    /// Get the tenant ID as a string slice (convenience method)
    pub fn tenant_str(&self) -> &str {
        self.tenant.as_str()
    }

    /// Get the workspace ID as a string slice if present
    pub fn workspace_str(&self) -> Option<&str> {
        self.workspace.as_ref().map(|w| w.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TenantId tests

    #[test]
    fn test_valid_tenant_ids() {
        let valid = [
            "primary",
            "acme-corp",
            "tenant_123",
            "my-tenant-42",
            "a",    // Single char
            "ab",   // Two chars
            "ABC",  // Uppercase OK
            "ACME", // All uppercase OK
            "a1b2c3",
            "tenant-with-many-hyphens",
            "tenant_with_underscores",
            "MixedCase123",
        ];

        for id in &valid {
            let result = TenantId::new(*id);
            assert!(result.is_ok(), "Should accept valid tenant ID '{}'", id);
        }
    }

    #[test]
    fn test_invalid_tenant_ids() {
        let invalid = [
            ("", "empty"),
            ("-invalid", "starts with hyphen"),
            ("invalid-", "ends with hyphen"),
            ("_invalid", "starts with underscore"),
            ("invalid_", "ends with underscore"),
            ("../etc", "path traversal with .."),
            ("tenant/id", "contains forward slash"),
            ("tenant\\id", "contains backslash"),
            ("tenant..id", "contains double dots"),
            (&"a".repeat(65), "exceeds 64 chars"),
        ];

        for (id, reason) in &invalid {
            let result = TenantId::new(*id);
            assert!(result.is_err(), "Should reject '{}' ({})", id, reason);
        }
    }

    #[test]
    fn test_tenant_id_max_length() {
        // Exactly 64 chars should work
        let max_len = "a".repeat(64);
        assert!(TenantId::new(&max_len).is_ok());

        // 65 chars should fail
        let too_long = "a".repeat(65);
        assert!(TenantId::new(&too_long).is_err());
    }

    #[test]
    fn test_tenant_id_special_values() {
        let default = TenantId::single_tenant_default();
        assert_eq!(default.as_str(), "primary");

        let system = TenantId::system();
        assert_eq!(system.as_str(), "system");
    }

    #[test]
    fn test_tenant_id_display() {
        let id = TenantId::new("acme-corp").unwrap();
        assert_eq!(format!("{}", id), "acme-corp");
        assert_eq!(format!("{:?}", id), "TenantId(acme-corp)");
    }

    #[test]
    fn test_tenant_id_serde_roundtrip() {
        let id = TenantId::new("test-tenant").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"test-tenant\"");

        let parsed: TenantId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_tenant_id_serde_invalid() {
        // Should fail deserialization for invalid ID
        let json = "\"../invalid\"";
        let result: std::result::Result<TenantId, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_tenant_id_equality() {
        let a = TenantId::new("tenant").unwrap();
        let b = TenantId::new("tenant").unwrap();
        let c = TenantId::new("other").unwrap();

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_tenant_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(TenantId::new("tenant-a").unwrap());
        set.insert(TenantId::new("tenant-b").unwrap());
        set.insert(TenantId::new("tenant-a").unwrap()); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_tenant_id_from_str() {
        let id: TenantId = "acme-corp".parse().unwrap();
        assert_eq!(id.as_str(), "acme-corp");

        let invalid: std::result::Result<TenantId, _> = "../bad".parse();
        assert!(invalid.is_err());
    }

    // WorkspaceId tests

    #[test]
    fn test_valid_workspace_ids() {
        let valid = ["default", "production", "dev-workspace", "ws_123"];

        for id in &valid {
            let result = WorkspaceId::new(*id);
            assert!(result.is_ok(), "Should accept valid workspace ID '{}'", id);
        }
    }

    #[test]
    fn test_invalid_workspace_ids() {
        let invalid = ["", "-invalid", "../etc", "ws/bad"];

        for id in &invalid {
            let result = WorkspaceId::new(*id);
            assert!(
                result.is_err(),
                "Should reject invalid workspace ID '{}'",
                id
            );
        }
    }

    #[test]
    fn test_workspace_id_default() {
        let default = WorkspaceId::default_workspace();
        assert_eq!(default.as_str(), "default");
    }

    // TenantContext tests

    #[test]
    fn test_tenant_context_new() {
        let ctx = TenantContext::new(TenantId::new("acme").unwrap());
        assert_eq!(ctx.tenant.as_str(), "acme");
        assert!(ctx.workspace.is_none());
    }

    #[test]
    fn test_tenant_context_with_workspace() {
        let ctx = TenantContext::with_workspace(
            TenantId::new("acme").unwrap(),
            WorkspaceId::new("prod").unwrap(),
        );
        assert_eq!(ctx.tenant.as_str(), "acme");
        assert_eq!(ctx.workspace.as_ref().unwrap().as_str(), "prod");
    }

    #[test]
    fn test_tenant_context_single_tenant() {
        let ctx = TenantContext::single_tenant();
        assert_eq!(ctx.tenant.as_str(), "primary");
        assert!(ctx.workspace.is_none());
    }

    #[test]
    fn test_tenant_context_system() {
        let ctx = TenantContext::system();
        assert_eq!(ctx.tenant.as_str(), "system");
    }

    #[test]
    fn test_tenant_context_convenience_methods() {
        let ctx = TenantContext::with_workspace(
            TenantId::new("tenant").unwrap(),
            WorkspaceId::new("workspace").unwrap(),
        );
        assert_eq!(ctx.tenant_str(), "tenant");
        assert_eq!(ctx.workspace_str(), Some("workspace"));

        let ctx_no_ws = TenantContext::new(TenantId::new("tenant").unwrap());
        assert_eq!(ctx_no_ws.workspace_str(), None);
    }
}
