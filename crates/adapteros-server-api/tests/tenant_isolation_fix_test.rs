//! Test for tenant isolation bypass fix
//!
//! This test verifies that the tenant isolation vulnerability has been fixed.
//!
//! **Vulnerability:** Admin users could access any tenant without restriction
//! **Fix:** Admin users can only access tenants listed in their `admin_tenants` claim

use adapteros_server_api::auth::Claims;
use adapteros_server_api::security::validate_tenant_isolation;

#[test]
fn test_non_admin_cannot_cross_tenant_access() {
    let claims = Claims {
        sub: "user-1".to_string(),
        email: "user@tenant-a.com".to_string(),
        role: "operator".to_string(),
        roles: vec!["operator".to_string()],
        tenant_id: "tenant-a".to_string(),
        admin_tenants: vec![],
        exp: 0,
        iat: 0,
        jti: "jti-1".to_string(),
        nbf: 0,
    };

    // User can access their own tenant
    assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok());

    // User cannot access other tenants
    assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
    assert!(validate_tenant_isolation(&claims, "tenant-c").is_err());
}

#[test]
fn test_admin_without_grants_cannot_cross_tenant_access() {
    // This is the key fix: admins without explicit grants are isolated to their own tenant
    let claims = Claims {
        sub: "admin-1".to_string(),
        email: "admin@system.com".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        tenant_id: "system".to_string(),
        admin_tenants: vec![], // Empty = no cross-tenant access
        exp: 0,
        iat: 0,
        jti: "jti-1".to_string(),
        nbf: 0,
    };

    // Admin can access their own tenant
    assert!(validate_tenant_isolation(&claims, "system").is_ok());

    // Admin CANNOT access other tenants without explicit grants
    assert!(validate_tenant_isolation(&claims, "tenant-a").is_err());
    assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
    assert!(validate_tenant_isolation(&claims, "tenant-c").is_err());
}

#[test]
fn test_admin_with_specific_grants_can_access_only_granted_tenants() {
    let claims = Claims {
        sub: "admin-1".to_string(),
        email: "admin@system.com".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        tenant_id: "system".to_string(),
        admin_tenants: vec!["tenant-a".to_string(), "tenant-b".to_string()],
        exp: 0,
        iat: 0,
        jti: "jti-1".to_string(),
        nbf: 0,
    };

    // Admin can access their own tenant
    assert!(validate_tenant_isolation(&claims, "system").is_ok());

    // Admin can access explicitly granted tenants
    assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok());
    assert!(validate_tenant_isolation(&claims, "tenant-b").is_ok());

    // Admin CANNOT access tenants not in the grant list
    assert!(validate_tenant_isolation(&claims, "tenant-c").is_err());
    assert!(validate_tenant_isolation(&claims, "tenant-d").is_err());
}

#[test]
fn test_sre_role_cannot_cross_tenant_access() {
    let claims = Claims {
        sub: "sre-1".to_string(),
        email: "sre@tenant-a.com".to_string(),
        role: "sre".to_string(),
        roles: vec!["sre".to_string()],
        tenant_id: "tenant-a".to_string(),
        admin_tenants: vec![], // SRE shouldn't have this anyway
        exp: 0,
        iat: 0,
        jti: "jti-1".to_string(),
        nbf: 0,
    };

    // SRE can access their own tenant
    assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok());

    // SRE cannot access other tenants
    assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
}

#[test]
fn test_viewer_role_cannot_cross_tenant_access() {
    let claims = Claims {
        sub: "viewer-1".to_string(),
        email: "viewer@tenant-a.com".to_string(),
        role: "viewer".to_string(),
        roles: vec!["viewer".to_string()],
        tenant_id: "tenant-a".to_string(),
        admin_tenants: vec![],
        exp: 0,
        iat: 0,
        jti: "jti-1".to_string(),
        nbf: 0,
    };

    // Viewer can access their own tenant
    assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok());

    // Viewer cannot access other tenants
    assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
}

#[test]
fn test_admin_tenants_ignored_for_non_admin_roles() {
    // Even if a non-admin user has admin_tenants populated (shouldn't happen),
    // they should not get cross-tenant access
    let claims = Claims {
        sub: "operator-1".to_string(),
        email: "operator@tenant-a.com".to_string(),
        role: "operator".to_string(),
        roles: vec!["operator".to_string()],
        tenant_id: "tenant-a".to_string(),
        admin_tenants: vec!["tenant-b".to_string()], // Should be ignored
        exp: 0,
        iat: 0,
        jti: "jti-1".to_string(),
        nbf: 0,
    };

    // Can access own tenant
    assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok());

    // Cannot access other tenants even if in admin_tenants (not admin role)
    assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
}

#[test]
fn test_backwards_compatibility_empty_admin_tenants() {
    // Test that existing JWTs with missing admin_tenants field (defaults to empty)
    // work correctly (serde(default) should handle this)
    let claims = Claims {
        sub: "admin-1".to_string(),
        email: "admin@system.com".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        tenant_id: "system".to_string(),
        admin_tenants: vec![], // This will be the default for old tokens
        exp: 0,
        iat: 0,
        jti: "jti-1".to_string(),
        nbf: 0,
    };

    // Should work like normal isolated admin
    assert!(validate_tenant_isolation(&claims, "system").is_ok());
    assert!(validate_tenant_isolation(&claims, "tenant-a").is_err());
}
