//! Authentication context types.
//!
//! These types are injected into request extensions by the auth middleware
//! and can be extracted by handlers.

use crate::mode::AuthMode;
use serde::{Deserialize, Serialize};

/// Type of principal making the request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    /// Regular user authenticated via JWT or session
    #[default]
    User,
    /// API key-based access
    ApiKey,
    /// Internal service-to-service call
    InternalService,
    /// Development bypass (debug builds only)
    DevBypass,
}

impl PrincipalType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrincipalType::User => "user",
            PrincipalType::ApiKey => "api_key",
            PrincipalType::InternalService => "internal_service",
            PrincipalType::DevBypass => "dev_bypass",
        }
    }
}

impl std::fmt::Display for PrincipalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Normalized principal identity.
///
/// This struct provides a unified view of the authenticated caller,
/// regardless of the authentication method used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    /// Type of principal
    pub principal_type: PrincipalType,

    /// Unique identifier for the principal (user_id or api-key:{id})
    pub principal_id: String,

    /// Tenant ID the principal belongs to
    pub tenant_id: String,

    /// List of tenant IDs the principal can administer
    /// Special value "*" means all tenants (dev mode only)
    pub admin_tenants: Vec<String>,

    /// Session ID (if session-based auth)
    pub session_id: Option<String>,

    /// Device ID for device binding
    pub device_id: Option<String>,

    /// MFA level achieved
    pub mfa_level: Option<String>,

    /// JWT ID for revocation tracking
    pub jti: String,

    /// How the principal was authenticated
    pub auth_mode: AuthMode,
}

impl Principal {
    /// Create a Principal from Claims.
    pub fn from_claims(
        claims: &Claims,
        principal_type: PrincipalType,
        auth_mode: AuthMode,
    ) -> Self {
        Self {
            principal_type,
            principal_id: claims.sub.clone(),
            tenant_id: claims.tenant_id.clone(),
            admin_tenants: claims.admin_tenants.clone(),
            session_id: claims.session_id.clone(),
            device_id: claims.device_id.clone(),
            mfa_level: claims.mfa_level.clone(),
            jti: claims.jti.clone(),
            auth_mode,
        }
    }

    /// Check if the principal can access a specific tenant.
    ///
    /// Returns true if:
    /// - The principal's tenant_id matches the target
    /// - The target tenant is in admin_tenants
    /// - admin_tenants contains "*" (wildcard, dev mode only)
    pub fn can_access_tenant(&self, target_tenant: &str) -> bool {
        if self.tenant_id == target_tenant {
            return true;
        }

        // Check admin_tenants list
        self.admin_tenants
            .iter()
            .any(|t| t == "*" || t == target_tenant)
    }

    /// Check if the principal is a cross-tenant admin.
    pub fn is_cross_tenant_admin(&self) -> bool {
        !self.admin_tenants.is_empty() && self.admin_tenants.iter().any(|t| t != &self.tenant_id)
    }

    /// Check if the principal has wildcard tenant access (dev mode).
    pub fn has_wildcard_access(&self) -> bool {
        self.admin_tenants.iter().any(|t| t == "*")
    }
}

/// JWT claims structure.
///
/// This struct represents the payload of both access tokens and session tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,

    /// User email
    pub email: String,

    /// Primary role
    pub role: String,

    /// All assigned roles
    pub roles: Vec<String>,

    /// Tenant ID the token is scoped to
    pub tenant_id: String,

    /// Tenants the user can administer
    pub admin_tenants: Vec<String>,

    /// Device ID for device binding
    pub device_id: Option<String>,

    /// Session ID linking to auth_sessions
    pub session_id: Option<String>,

    /// MFA authentication level
    pub mfa_level: Option<String>,

    /// Key rotation ID
    pub rot_id: Option<String>,

    /// Expiration timestamp (Unix epoch)
    pub exp: i64,

    /// Issued at timestamp (Unix epoch)
    pub iat: i64,

    /// JWT ID (unique token identifier for revocation)
    pub jti: String,

    /// Not before timestamp (Unix epoch)
    pub nbf: i64,

    /// Issuer
    pub iss: String,

    /// Authentication mode used (populated by middleware)
    #[serde(default)]
    pub auth_mode: AuthMode,

    /// Principal type (populated by middleware)
    #[serde(default)]
    pub principal_type: Option<PrincipalType>,
}

impl Claims {
    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now >= self.exp
    }

    /// Check if the token is not yet valid.
    pub fn is_not_yet_valid(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now < self.nbf
    }

    /// Get the remaining TTL in seconds.
    pub fn remaining_ttl(&self) -> i64 {
        let now = chrono::Utc::now().timestamp();
        self.exp - now
    }

    /// Check if the user has a specific role.
    pub fn has_role(&self, role: &str) -> bool {
        self.role == role || self.roles.iter().any(|r| r == role)
    }

    /// Check if the user has admin role.
    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }
}

/// Authentication context injected into request extensions.
///
/// This is the primary type handlers should extract to access auth information.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// The authenticated principal
    pub principal: Principal,

    /// The parsed claims
    pub claims: Claims,

    /// How authentication was performed
    pub auth_mode: AuthMode,
}

impl AuthContext {
    /// Create a new AuthContext from claims and auth mode.
    pub fn new(claims: Claims, auth_mode: AuthMode, principal_type: PrincipalType) -> Self {
        let principal = Principal::from_claims(&claims, principal_type, auth_mode.clone());
        Self {
            principal,
            claims,
            auth_mode,
        }
    }

    /// Get the user ID.
    pub fn user_id(&self) -> &str {
        &self.claims.sub
    }

    /// Get the tenant ID.
    pub fn tenant_id(&self) -> &str {
        &self.claims.tenant_id
    }

    /// Get the primary role.
    pub fn role(&self) -> &str {
        &self.claims.role
    }

    /// Check if the user is an admin.
    pub fn is_admin(&self) -> bool {
        self.claims.is_admin()
    }

    /// Check if the context is from dev bypass.
    pub fn is_dev_bypass(&self) -> bool {
        self.auth_mode.is_dev_bypass()
    }

    /// Check if the user can access a specific tenant.
    pub fn can_access_tenant(&self, tenant_id: &str) -> bool {
        self.principal.can_access_tenant(tenant_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_claims() -> Claims {
        Claims {
            sub: "user-123".to_string(),
            email: "test@example.com".to_string(),
            role: "admin".to_string(),
            roles: vec!["admin".to_string(), "viewer".to_string()],
            tenant_id: "tenant-1".to_string(),
            admin_tenants: vec!["tenant-1".to_string(), "tenant-2".to_string()],
            device_id: None,
            session_id: Some("session-123".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            jti: "jti-123".to_string(),
            nbf: chrono::Utc::now().timestamp(),
            iss: "adapteros-server".to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        }
    }

    #[test]
    fn test_claims_has_role() {
        let claims = test_claims();
        assert!(claims.has_role("admin"));
        assert!(claims.has_role("viewer"));
        assert!(!claims.has_role("nonexistent"));
    }

    #[test]
    fn test_claims_is_admin() {
        let claims = test_claims();
        assert!(claims.is_admin());
    }

    #[test]
    fn test_principal_can_access_tenant() {
        let claims = test_claims();
        let principal = Principal::from_claims(&claims, PrincipalType::User, AuthMode::BearerToken);

        assert!(principal.can_access_tenant("tenant-1"));
        assert!(principal.can_access_tenant("tenant-2"));
        assert!(!principal.can_access_tenant("tenant-3"));
    }

    #[test]
    fn test_principal_wildcard_access() {
        let mut claims = test_claims();
        claims.admin_tenants = vec!["*".to_string()];
        let principal =
            Principal::from_claims(&claims, PrincipalType::DevBypass, AuthMode::DevBypass);

        assert!(principal.can_access_tenant("any-tenant"));
        assert!(principal.has_wildcard_access());
    }

    #[test]
    fn test_auth_context() {
        let claims = test_claims();
        let ctx = AuthContext::new(claims, AuthMode::BearerToken, PrincipalType::User);

        assert_eq!(ctx.user_id(), "user-123");
        assert_eq!(ctx.tenant_id(), "tenant-1");
        assert!(ctx.is_admin());
        assert!(!ctx.is_dev_bypass());
    }
}
