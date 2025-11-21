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
    check_rate_limit, get_rate_limit_status, reset_rate_limit, update_rate_limit,
    RateLimitResult,
};
pub use token_revocation::{
    cleanup_expired_revocations, get_user_revocations, is_token_revoked, revoke_all_user_tokens,
    revoke_token, RevokedToken,
};

use crate::auth::Claims;
use crate::types::ErrorResponse;
use adapteros_core::Result;
use adapteros_db::Db;
use axum::{http::StatusCode, Json};
use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

/// Validate that the tenant_id in JWT claims matches the requested resource
///
/// This enforces tenant isolation at the request level.
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
    // Admin users with "admin" role can access all tenants
    if claims.role == "admin" {
        return Ok(());
    }

    if claims.tenant_id != resource_tenant_id {
        warn!(
            user_id = %claims.sub,
            user_tenant = %claims.tenant_id,
            resource_tenant = %resource_tenant_id,
            "Tenant isolation violation attempt"
        );

        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("tenant isolation violation")
                    .with_code("TENANT_ISOLATION_ERROR")
                    .with_string_details(format!(
                        "user tenant '{}' cannot access resource in tenant '{}'",
                        claims.tenant_id, resource_tenant_id
                    )),
            ),
        ));
    }

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
         VALUES (?, ?, ?, ?, ?, ?)"
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
           AND attempted_at > ?"
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
         LIMIT ?"
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

    sqlx::query(
        "UPDATE user_sessions SET last_activity = ? WHERE jti = ?"
    )
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
         ORDER BY last_activity DESC"
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

    let result = sqlx::query(
        "DELETE FROM user_sessions WHERE expires_at < ?"
    )
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

    #[test]
    fn test_tenant_isolation_same_tenant() {
        let claims = Claims {
            sub: "user-1".to_string(),
            email: "user@tenant-a.com".to_string(),
            role: "operator".to_string(),
            tenant_id: "tenant-a".to_string(),
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
            tenant_id: "tenant-a".to_string(),
            exp: 0,
            iat: 0,
            jti: "jti-1".to_string(),
            nbf: 0,
        };

        assert!(validate_tenant_isolation(&claims, "tenant-b").is_err());
    }

    #[test]
    fn test_tenant_isolation_admin_bypass() {
        let claims = Claims {
            sub: "admin-1".to_string(),
            email: "admin@system.com".to_string(),
            role: "admin".to_string(),
            tenant_id: "system".to_string(),
            exp: 0,
            iat: 0,
            jti: "jti-2".to_string(),
            nbf: 0,
        };

        // Admin can access any tenant
        assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok());
        assert!(validate_tenant_isolation(&claims, "tenant-b").is_ok());
    }

    #[tokio::test]
    async fn test_auth_attempt_tracking() {
        let db = Db::connect("sqlite::memory:").await.expect("Failed to create test database");

        track_auth_attempt(&db, "user@example.com", "192.168.1.1", false, Some("invalid password"))
            .await
            .expect("Security operation failed");

        let is_locked = is_account_locked(&db, "user@example.com", 15)
            .await
            .expect("Security operation failed");
        assert!(!is_locked); // Only 1 attempt, threshold is 5
    }

    #[tokio::test]
    async fn test_account_lockout() {
        let db = Db::connect("sqlite::memory:").await.expect("Failed to create test database");

        // Simulate 5 failed attempts
        for _ in 0..5 {
            track_auth_attempt(&db, "user@example.com", "192.168.1.1", false, Some("invalid password"))
                .await
                .expect("Security operation failed");
        }

        let is_locked = is_account_locked(&db, "user@example.com", 15)
            .await
            .expect("Security operation failed");
        assert!(is_locked);
    }
}
