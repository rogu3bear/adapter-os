///! Security subsystem for AdapterOS
///!
///! Provides comprehensive security controls:
///! - Token revocation and session management
///! - IP allowlisting/denylisting
///! - Rate limiting per tenant
///! - Authentication attempt tracking
///! - Tenant isolation validation
pub mod ip_access_control;
pub mod rate_limiting;
pub mod token_revocation;

pub use ip_access_control::{
    add_ip_rule, check_ip_access, cleanup_expired_ip_rules, list_ip_rules, remove_ip_rule,
    AccessDecision, IpAccessRule,
};
pub use rate_limiting::{
    check_rate_limit, get_rate_limit_status, reset_rate_limit, update_rate_limit, RateLimitResult,
};
pub use token_revocation::{
    cleanup_expired_revocations, get_user_revocations, is_token_revoked, revoke_all_user_tokens,
    revoke_token, RevokedToken,
};

// PRD-03: Per-tenant token baseline functions are exported directly from this module
// get_tenant_token_baseline, set_tenant_token_baseline

use crate::auth::Claims;
use crate::types::ErrorResponse;
use adapteros_core::Result;
use adapteros_db::Db;
use axum::{http::StatusCode, Json};
use chrono::Utc;
use std::env;
use tracing::{info, warn};
use uuid::Uuid;

/// Check if dev no-auth bypass is enabled (compile-time restricted to debug builds)
///
/// SECURITY: This function is only available in debug builds. Release builds always return false.
#[cfg(debug_assertions)]
fn dev_no_auth_enabled() -> bool {
    env::var("AOS_DEV_NO_AUTH")
        .map(|v| {
            let lower = v.to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// SECURITY: In release builds, dev_no_auth is NEVER enabled
#[cfg(not(debug_assertions))]
fn dev_no_auth_enabled() -> bool {
    // SECURITY: Always return false in release builds, regardless of environment variable
    if env::var("AOS_DEV_NO_AUTH").is_ok() {
        tracing::error!(
            "AOS_DEV_NO_AUTH detected in release build - this flag is ignored in production"
        );
    }
    false
}

/// Core tenant access check logic (shared by all validation functions)
///
/// This is the single source of truth for tenant isolation logic.
/// Returns `true` if access is allowed, `false` otherwise.
fn check_tenant_access_core(claims: &Claims, resource_tenant_id: &str) -> bool {
    // Same tenant - always allowed
    if claims.tenant_id == resource_tenant_id {
        return true;
    }

    // Dev mode bypass: Allow admin role in dev mode to access any tenant
    // SECURITY: This only works in debug builds, release builds ignore AOS_DEV_NO_AUTH
    if dev_no_auth_enabled() && claims.role == "admin" {
        return true;
    }

    // Admin with explicit access
    if claims.role == "admin"
        && claims
            .admin_tenants
            .contains(&resource_tenant_id.to_string())
    {
        return true;
    }

    false
}

/// Check if tenant access is allowed (includes dev mode bypass)
///
/// This is a helper function for direct tenant_id comparisons in handlers.
/// In dev mode with admin role, allows access to any tenant.
///
/// **Best Practice:** Use this for simple boolean checks in handlers.
/// For endpoints that need proper error responses, use `validate_tenant_isolation()` instead.
///
/// # Arguments
/// * `claims` - The user's JWT claims
/// * `resource_tenant_id` - The tenant ID of the resource being accessed
///
/// # Returns
/// `true` if access is allowed, `false` otherwise
pub fn check_tenant_access(claims: &Claims, resource_tenant_id: &str) -> bool {
    check_tenant_access_core(claims, resource_tenant_id)
}

/// Validate that the tenant_id in JWT claims matches the requested resource
///
/// This enforces tenant isolation at the request level.
///
/// **Security Fix (2025-11-27):**
/// - Removed blanket admin bypass vulnerability
/// - Admins can only access tenants listed in their `admin_tenants` claim
/// - Empty `admin_tenants` = can only access their own tenant
/// - All cross-tenant access attempts are logged for audit
///
/// **Dev Mode (2025-12-02):**
/// - In debug builds with `AOS_DEV_NO_AUTH=1`, admin role can access any tenant
/// - This bypass is compile-time restricted to debug builds only
/// - Release builds ignore `AOS_DEV_NO_AUTH` completely for security
///
/// # Example
/// ```no_run
/// use adapteros_server_api::security::validate_tenant_isolation;
/// use crate::auth::Claims;
///
/// async fn my_handler(claims: Claims, resource_tenant_id: &str) -> Result<()> {
///     validate_tenant_isolation(&claims, resource_tenant_id)?;
///     // ... proceed with operation
///     Ok(())
/// }
/// ```
pub fn validate_tenant_isolation(
    claims: &Claims,
    resource_tenant_id: &str,
) -> std::result::Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Use shared core logic for consistency
    if check_tenant_access_core(claims, resource_tenant_id) {
        // Log successful access (only for cross-tenant, not same-tenant)
        if claims.tenant_id != resource_tenant_id {
            if dev_no_auth_enabled() && claims.role == "admin" {
                info!(
                    user_id = %claims.sub,
                    user_email = %claims.email,
                    user_role = %claims.role,
                    user_tenant = %claims.tenant_id,
                    resource_tenant = %resource_tenant_id,
                    "Dev mode: Admin cross-tenant access granted"
                );
            } else {
                info!(
                    user_id = %claims.sub,
                    user_email = %claims.email,
                    user_role = %claims.role,
                    user_tenant = %claims.tenant_id,
                    resource_tenant = %resource_tenant_id,
                    admin_tenants = ?claims.admin_tenants,
                    "Cross-tenant access granted via admin_tenants"
                );
            }
        }
        return Ok(());
    }

    // Access denied - log the violation attempt
    warn!(
        user_id = %claims.sub,
        user_email = %claims.email,
        user_role = %claims.role,
        user_tenant = %claims.tenant_id,
        resource_tenant = %resource_tenant_id,
        admin_tenants = ?claims.admin_tenants,
        "Tenant isolation violation: access denied"
    );

    Err((
        StatusCode::FORBIDDEN,
        Json(
            ErrorResponse::new("tenant isolation violation")
                .with_code("TENANT_ISOLATION_ERROR")
                .with_string_details(format!(
                    "user tenant '{}' cannot access resource in tenant '{}'. User role: {}, Admin tenants: {:?}",
                    claims.tenant_id, resource_tenant_id, claims.role, claims.admin_tenants
                )),
        ),
    ))
}

/// Log cross-tenant access attempt to audit table
///
/// Records all attempts (both successful and denied) for security audit trail
pub async fn log_tenant_access_attempt(
    db: &Db,
    claims: &Claims,
    resource_tenant_id: &str,
    access_granted: bool,
    reason: Option<&str>,
    request_path: Option<&str>,
) -> Result<()> {
    let id = Uuid::now_v7().to_string();
    let timestamp = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO tenant_access_audit
         (id, user_id, user_email, user_role, user_tenant_id, resource_tenant_id,
          access_granted, reason, request_path, timestamp)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&claims.sub)
    .bind(&claims.email)
    .bind(&claims.role)
    .bind(&claims.tenant_id)
    .bind(resource_tenant_id)
    .bind(access_granted as i64)
    .bind(reason)
    .bind(request_path)
    .bind(&timestamp)
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Validate tenant isolation with database audit logging
///
/// Same as validate_tenant_isolation but also logs to database for compliance
pub async fn validate_tenant_isolation_with_audit(
    db: &Db,
    claims: &Claims,
    resource_tenant_id: &str,
    request_path: Option<&str>,
) -> std::result::Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Use shared core logic for consistency
    let access_granted = check_tenant_access_core(claims, resource_tenant_id);

    // Log the attempt (ignore logging errors to not block the request)
    let reason = if access_granted {
        Some("admin with explicit tenant access")
    } else {
        Some("tenant isolation violation")
    };
    let _ = log_tenant_access_attempt(
        db,
        claims,
        resource_tenant_id,
        access_granted,
        reason,
        request_path,
    )
    .await;

    if access_granted {
        info!(
            user_id = %claims.sub,
            user_email = %claims.email,
            user_role = %claims.role,
            user_tenant = %claims.tenant_id,
            resource_tenant = %resource_tenant_id,
            admin_tenants = ?claims.admin_tenants,
            "Cross-tenant access granted via admin_tenants"
        );
        return Ok(());
    }

    // Access denied - log the violation attempt
    warn!(
        user_id = %claims.sub,
        user_email = %claims.email,
        user_role = %claims.role,
        user_tenant = %claims.tenant_id,
        resource_tenant = %resource_tenant_id,
        admin_tenants = ?claims.admin_tenants,
        "Tenant isolation violation: access denied"
    );

    Err((
        StatusCode::FORBIDDEN,
        Json(
            ErrorResponse::new("tenant isolation violation")
                .with_code("TENANT_ISOLATION_ERROR")
                .with_string_details(format!(
                    "user tenant '{}' cannot access resource in tenant '{}'. User role: {}, Admin tenants: {:?}",
                    claims.tenant_id, resource_tenant_id, claims.role, claims.admin_tenants
                )),
        ),
    ))
}

/// Get the per-tenant token revocation baseline timestamp
///
/// Tokens issued before this timestamp are automatically invalidated.
/// Returns None if no baseline is set (all tokens valid regardless of iat).
///
/// Used by auth middleware to enforce tenant-wide token revocation - PRD-03
pub async fn get_tenant_token_baseline(db: &Db, tenant_id: &str) -> Result<Option<String>> {
    let result = sqlx::query_scalar::<_, Option<String>>(
        "SELECT token_issued_at_min FROM tenants WHERE id = ?",
    )
    .bind(tenant_id)
    .fetch_optional(db.pool())
    .await?;

    // flatten: Option<Option<String>> -> Option<String>
    Ok(result.flatten())
}

/// Set the per-tenant token revocation baseline timestamp
///
/// All tokens issued before this timestamp will be rejected.
/// Use this to bulk-revoke all tokens for a tenant during security incidents.
///
/// PRD-03: Tenant token revocation baseline
pub async fn set_tenant_token_baseline(db: &Db, tenant_id: &str, baseline: &str) -> Result<()> {
    sqlx::query("UPDATE tenants SET token_issued_at_min = ? WHERE id = ?")
        .bind(baseline)
        .bind(tenant_id)
        .execute(db.pool())
        .await?;

    info!(
        tenant_id = %tenant_id,
        baseline = %baseline,
        "Tenant token revocation baseline updated"
    );

    Ok(())
}

/// Track authentication attempt (for brute force protection)
pub async fn track_auth_attempt(
    db: &Db,
    email: &str,
    ip_address: &str,
    success: bool,
    failure_reason: Option<&str>,
) -> Result<()> {
    let id = Uuid::now_v7().to_string();
    let attempted_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO auth_attempts (id, email, ip_address, success, attempted_at, failure_reason)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(email)
    .bind(ip_address)
    .bind(success as i64)
    .bind(&attempted_at)
    .bind(failure_reason)
    .execute(db.pool())
    .await?;

    if !success {
        info!(
            email = %email,
            ip_address = %ip_address,
            reason = ?failure_reason,
            "Failed authentication attempt"
        );
    }

    Ok(())
}

/// Check if account is locked due to too many failed attempts
///
/// Returns Ok(true) if locked, Ok(false) if not locked
pub async fn is_account_locked(db: &Db, email: &str, window_minutes: i64) -> Result<bool> {
    let threshold = 5; // 5 failed attempts
    let window_start = (Utc::now() - chrono::Duration::minutes(window_minutes)).to_rfc3339();

    let failed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM auth_attempts
         WHERE email = ?
           AND success = 0
           AND attempted_at > ?",
    )
    .bind(email)
    .bind(&window_start)
    .fetch_one(db.pool())
    .await?;

    Ok(failed_count >= threshold)
}

/// Get recent failed attempts for an account
pub async fn get_failed_attempts(
    db: &Db,
    email: &str,
    limit: i64,
) -> Result<Vec<(String, String, String)>> {
    let attempts = sqlx::query_as::<_, (String, String, String)>(
        "SELECT ip_address, attempted_at, failure_reason
         FROM auth_attempts
         WHERE email = ? AND success = 0
         ORDER BY attempted_at DESC
         LIMIT ?",
    )
    .bind(email)
    .bind(limit.min(100))
    .fetch_all(db.pool())
    .await?;

    Ok(attempts)
}

/// Create a user session
pub async fn create_session(
    db: &Db,
    jti: &str,
    user_id: &str,
    tenant_id: &str,
    expires_at: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<()> {
    let created_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO user_sessions (jti, user_id, tenant_id, created_at, expires_at, ip_address, user_agent, last_activity)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(jti)
    .bind(user_id)
    .bind(tenant_id)
    .bind(&created_at)
    .bind(expires_at)
    .bind(ip_address)
    .bind(user_agent)
    .bind(&created_at)
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Update session activity
pub async fn update_session_activity(db: &Db, jti: &str) -> Result<()> {
    let last_activity = Utc::now().to_rfc3339();

    sqlx::query("UPDATE user_sessions SET last_activity = ? WHERE jti = ?")
        .bind(&last_activity)
        .bind(jti)
        .execute(db.pool())
        .await?;

    Ok(())
}

/// Get active sessions for a user
pub async fn get_user_sessions(
    db: &Db,
    user_id: &str,
) -> Result<Vec<(String, String, Option<String>, String)>> {
    let now = Utc::now().to_rfc3339();

    let sessions = sqlx::query_as::<_, (String, String, Option<String>, String)>(
        "SELECT jti, created_at, ip_address, last_activity
         FROM user_sessions
         WHERE user_id = ?
           AND expires_at > ?
         ORDER BY last_activity DESC",
    )
    .bind(user_id)
    .bind(&now)
    .fetch_all(db.pool())
    .await?;

    Ok(sessions)
}

/// Cleanup expired sessions
pub async fn cleanup_expired_sessions(db: &Db) -> Result<usize> {
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query("DELETE FROM user_sessions WHERE expires_at < ?")
        .bind(&now)
        .execute(db.pool())
        .await?;

    let count = result.rows_affected() as usize;

    if count > 0 {
        info!(count = %count, "Cleaned up expired sessions");
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_test_schema(db: &Db) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS auth_attempts (
                id TEXT PRIMARY KEY,
                email TEXT NOT NULL,
                ip_address TEXT NOT NULL,
                success INTEGER NOT NULL,
                attempted_at TEXT NOT NULL DEFAULT (datetime('now')),
                failure_reason TEXT
            )",
        )
        .execute(db.pool())
        .await
        .expect("Failed to create auth_attempts table");
    }

    #[test]
    fn test_tenant_isolation_same_tenant() {
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

        assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok());
    }

    #[test]
    fn test_tenant_isolation_different_tenant() {
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

        assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
    }

    #[test]
    fn test_tenant_isolation_admin_no_bypass() {
        let claims = Claims {
            sub: "admin-1".to_string(),
            email: "admin@system.com".to_string(),
            role: "admin".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: "system".to_string(),
            admin_tenants: vec![], // Empty = can only access own tenant
            exp: 0,
            iat: 0,
            jti: "jti-2".to_string(),
            nbf: 0,
        };

        // Admin with empty admin_tenants can only access their own tenant
        assert!(validate_tenant_isolation(&claims, "system").is_ok());
        assert!(validate_tenant_isolation(&claims, "tenant-a").is_err());
        assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
    }

    #[test]
    fn test_tenant_isolation_admin_with_access() {
        let claims = Claims {
            sub: "admin-1".to_string(),
            email: "admin@system.com".to_string(),
            role: "admin".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: "system".to_string(),
            admin_tenants: vec!["tenant-a".to_string(), "tenant-b".to_string()],
            exp: 0,
            iat: 0,
            jti: "jti-2".to_string(),
            nbf: 0,
        };

        // Admin can access tenants in admin_tenants list
        assert!(validate_tenant_isolation(&claims, "system").is_ok()); // Own tenant
        assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok()); // Granted access
        assert!(validate_tenant_isolation(&claims, "tenant-b").is_ok()); // Granted access
        assert!(validate_tenant_isolation(&claims, "tenant-c").is_err()); // No access
    }

    #[tokio::test]
    async fn test_auth_attempt_tracking() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        track_auth_attempt(
            &db,
            "user@example.com",
            "192.168.1.1",
            false,
            Some("invalid password"),
        )
        .await
        .expect("Security operation failed");

        let is_locked = is_account_locked(&db, "user@example.com", 15)
            .await
            .expect("Security operation failed");
        assert!(!is_locked); // Only 1 attempt, threshold is 5
    }

    #[tokio::test]
    async fn test_account_lockout() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        // Simulate 5 failed attempts
        for _ in 0..5 {
            track_auth_attempt(
                &db,
                "user@example.com",
                "192.168.1.1",
                false,
                Some("invalid password"),
            )
            .await
            .expect("Security operation failed");
        }

        let is_locked = is_account_locked(&db, "user@example.com", 15)
            .await
            .expect("Security operation failed");
        assert!(is_locked);
    }
}
