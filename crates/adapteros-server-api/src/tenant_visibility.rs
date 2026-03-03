use crate::auth::Claims;

pub const SYSTEM_TENANT_ID: &str = "system";
pub const DEFAULT_TENANT_ID: &str = "default";
pub const RESERVED_INTERNAL_TENANT_IDS: [&str; 2] = [SYSTEM_TENANT_ID, DEFAULT_TENANT_ID];
pub const ADMIN_TENANT_WILDCARD: &str = "*";

#[inline]
pub fn is_reserved_internal_tenant_id(tenant_id: &str) -> bool {
    RESERVED_INTERNAL_TENANT_IDS.contains(&tenant_id)
}

#[inline]
pub fn is_workspace_tenant_id(tenant_id: &str) -> bool {
    !is_reserved_internal_tenant_id(tenant_id)
}

#[inline]
pub fn claim_can_access_tenant(claims: &Claims, tenant_id: &str) -> bool {
    claims.tenant_id == tenant_id
        || claims.admin_tenants.iter().any(|t| t == tenant_id)
        || claims
            .admin_tenants
            .iter()
            .any(|t| t == ADMIN_TENANT_WILDCARD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthMode, PrincipalType};

    fn test_claims(tenant_id: &str, admin_tenants: &[&str]) -> Claims {
        let now = chrono::Utc::now().timestamp();
        Claims {
            sub: "user-test".to_string(),
            email: "test@example.com".to_string(),
            role: "admin".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: tenant_id.to_string(),
            admin_tenants: admin_tenants.iter().map(|s| s.to_string()).collect(),
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: now + 3600,
            iat: now,
            jti: "jti-test".to_string(),
            nbf: now,
            iss: crate::auth::JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        }
    }

    #[test]
    fn reserved_internal_tenants_are_not_workspace_tenants() {
        assert!(is_reserved_internal_tenant_id("system"));
        assert!(is_reserved_internal_tenant_id("default"));
        assert!(!is_workspace_tenant_id("system"));
        assert!(!is_workspace_tenant_id("default"));
        assert!(is_workspace_tenant_id("tenant-acme"));
    }

    #[test]
    fn tenant_access_respects_primary_explicit_and_wildcard_grants() {
        let primary_only = test_claims("tenant-a", &[]);
        assert!(claim_can_access_tenant(&primary_only, "tenant-a"));
        assert!(!claim_can_access_tenant(&primary_only, "tenant-b"));

        let explicit = test_claims("tenant-a", &["tenant-b"]);
        assert!(claim_can_access_tenant(&explicit, "tenant-b"));
        assert!(!claim_can_access_tenant(&explicit, "tenant-c"));

        let wildcard = test_claims("tenant-a", &["*"]);
        assert!(claim_can_access_tenant(&wildcard, "tenant-z"));
    }
}
