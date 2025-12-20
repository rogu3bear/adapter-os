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
use adapteros_core::tenant_isolation::{
    TenantIsolationConfig, TenantIsolationEngine, TenantIsolationRequest, TenantPrincipal,
};
use adapteros_core::Result;
use adapteros_db::Db;
use axum::{http::StatusCode, Json};
use chrono::{DateTime, Duration, Utc};
use sqlx::{FromRow, Row};
use std::sync::OnceLock;
use tracing::{info, warn};
use uuid::Uuid;

const LOCKOUT_THRESHOLD: i64 = 5;
const LOCKOUT_WINDOW_MINUTES: i64 = 15;
const LOCKOUT_COOLDOWN_MINUTES: i64 = 15;

fn lockout_columns_missing(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db_err) if db_err.message().contains("failed_attempts"))
}

/// Check if dev no-auth bypass is enabled (compile-time restricted to debug builds)
fn dev_no_auth_enabled() -> bool {
    crate::auth::dev_no_auth_enabled()
}

fn tenant_isolation_engine() -> &'static TenantIsolationEngine {
    static ENGINE: OnceLock<TenantIsolationEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut cfg = TenantIsolationConfig::default();
        cfg.set_dev_mode_admin_all_tenants(dev_no_auth_enabled());
        TenantIsolationEngine::new(cfg)
    })
}

/// Core tenant access check logic (shared by all validation functions)
///
/// This is the single source of truth for tenant isolation logic.
/// Returns `true` if access is allowed, `false` otherwise.
fn check_tenant_access_core(claims: &Claims, resource_tenant_id: &str) -> bool {
    let principal = TenantPrincipal::new(
        claims.tenant_id.as_str(),
        claims.role.as_str(),
        &claims.admin_tenants,
    )
    .with_roles(&claims.roles)
    .with_subject(Some(claims.sub.as_str()), Some(claims.email.as_str()));

    let request = TenantIsolationRequest::new(principal, resource_tenant_id);
    tenant_isolation_engine().check(&request)
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

fn parse_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S")
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        })
}

#[derive(Debug, Clone)]
pub struct LockoutState {
    pub until: DateTime<Utc>,
    pub reason: &'static str,
}

/// Track authentication attempt (for brute force protection)
pub async fn track_auth_attempt(
    db: &Db,
    email: &str,
    ip_address: &str,
    success: bool,
    failure_reason: Option<&str>,
) -> Result<()> {
    if !db.storage_mode().read_from_sql() {
        return Ok(());
    }
    let id = Uuid::now_v7().to_string();
    let now = Utc::now();
    let attempted_at = now.to_rfc3339();

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

    if success {
        // Reset per-user counters on successful login (best effort)
        sqlx::query(
            "UPDATE users SET failed_attempts = 0, last_failed_at = NULL, lockout_until = NULL WHERE email = ?",
        )
        .bind(email)
        .execute(db.pool())
        .await
        .ok();

        return Ok(());
    }

    // Failed attempt: update counters and lockout metadata when possible
    let existing = match sqlx::query(
        "SELECT failed_attempts, last_failed_at, lockout_until FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(db.pool())
    .await
    {
        Ok(row) => row,
        Err(e) if lockout_columns_missing(&e) => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    let mut lockout_until = existing
        .as_ref()
        .and_then(|row| row.try_get::<Option<String>, _>("lockout_until").ok())
        .and_then(|s| s.as_deref().and_then(parse_timestamp));

    if let Some(row) = existing {
        let failed_attempts: i64 = row.try_get("failed_attempts").unwrap_or(0);
        let last_failed = row
            .try_get::<Option<String>, _>("last_failed_at")
            .ok()
            .and_then(|s| s.as_deref().and_then(parse_timestamp));
        let window_start = now - Duration::minutes(LOCKOUT_WINDOW_MINUTES);
        let within_window = last_failed.map(|ts| ts > window_start).unwrap_or(false);

        let mut attempts = if within_window { failed_attempts } else { 0 };
        attempts += 1;

        if attempts >= LOCKOUT_THRESHOLD && within_window {
            let candidate = now + Duration::minutes(LOCKOUT_COOLDOWN_MINUTES);
            lockout_until =
                Some(lockout_until.map_or(candidate, |existing| existing.max(candidate)));
        }

        // IP+user rate limiting
        let window_cutoff = (now - Duration::minutes(LOCKOUT_WINDOW_MINUTES)).to_rfc3339();
        let ip_failures: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM auth_attempts
             WHERE email = ? AND ip_address = ? AND success = 0 AND attempted_at > ?",
        )
        .bind(email)
        .bind(ip_address)
        .bind(&window_cutoff)
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);

        if ip_failures >= LOCKOUT_THRESHOLD {
            let candidate = now + Duration::minutes(LOCKOUT_COOLDOWN_MINUTES);
            lockout_until =
                Some(lockout_until.map_or(candidate, |existing| existing.max(candidate)));
        }

        sqlx::query(
            "UPDATE users
             SET failed_attempts = ?, last_failed_at = ?, lockout_until = ?
             WHERE email = ?",
        )
        .bind(attempts)
        .bind(&attempted_at)
        .bind(lockout_until.map(|ts| ts.to_rfc3339()))
        .bind(email)
        .execute(db.pool())
        .await?;
    }

    info!(
        email = %email,
        ip_address = %ip_address,
        reason = ?failure_reason,
        "Failed authentication attempt"
    );

    Ok(())
}

/// Evaluate lockout/rate-limit state for a user+IP pair
pub async fn check_login_lockout(
    db: &Db,
    email: &str,
    ip_address: &str,
) -> Result<Option<LockoutState>> {
    if !db.storage_mode().read_from_sql() {
        return Ok(None);
    }
    let now = Utc::now();

    if let Some(row) = match sqlx::query(
        "SELECT failed_attempts, last_failed_at, lockout_until FROM users WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(db.pool())
    .await
    {
        Ok(row) => row,
        Err(e) if lockout_columns_missing(&e) => None,
        Err(e) => return Err(e.into()),
    } {
        let lockout_until = row
            .try_get::<Option<String>, _>("lockout_until")
            .ok()
            .and_then(|s| s.as_deref().and_then(parse_timestamp));
        if let Some(until) = lockout_until.filter(|ts| *ts > now) {
            return Ok(Some(LockoutState {
                until,
                reason: "user_lockout",
            }));
        }

        let failed_attempts: i64 = row.try_get("failed_attempts").unwrap_or(0);

        if failed_attempts >= LOCKOUT_THRESHOLD {
            let last_failed = row
                .try_get::<Option<String>, _>("last_failed_at")
                .ok()
                .and_then(|s| s.as_deref().and_then(parse_timestamp));
            if let Some(last_failed) = last_failed {
                if now - last_failed < Duration::minutes(LOCKOUT_COOLDOWN_MINUTES) {
                    let until = last_failed + Duration::minutes(LOCKOUT_COOLDOWN_MINUTES);
                    return Ok(Some(LockoutState {
                        until,
                        reason: "user_lockout",
                    }));
                }
            }
        }
    }

    let window_cutoff = (now - Duration::minutes(LOCKOUT_WINDOW_MINUTES)).to_rfc3339();
    let ip_failures: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM auth_attempts
         WHERE email = ?
           AND ip_address = ?
           AND success = 0
           AND attempted_at > ?",
    )
    .bind(email)
    .bind(ip_address)
    .bind(&window_cutoff)
    .fetch_one(db.pool())
    .await
    .unwrap_or(0);

    if ip_failures >= LOCKOUT_THRESHOLD {
        let until = now + Duration::minutes(LOCKOUT_COOLDOWN_MINUTES);
        return Ok(Some(LockoutState {
            until,
            reason: "ip_rate_limit",
        }));
    }

    Ok(None)
}

/// Check if account is locked due to too many failed attempts
pub async fn is_account_locked(db: &Db, email: &str, ip_address: &str) -> Result<bool> {
    Ok(check_login_lockout(db, email, ip_address).await?.is_some())
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

/// Session record with device binding metadata (SQL).
#[derive(Debug, Clone, FromRow)]
pub struct SessionRecord {
    pub session_id: String,
    pub user_id: String,
    pub tenant_id: String,
    pub device_id: Option<String>,
    pub rot_id: Option<String>,
    pub refresh_hash: Option<String>,
    pub refresh_expires_at: Option<String>,
    pub expires_at: String,
    pub locked: i64,
}

/// Insert or update a session row with device binding and rotation metadata.
pub async fn upsert_user_session(
    db: &Db,
    session_id: &str,
    user_id: &str,
    tenant_id: &str,
    device_id: Option<&str>,
    rot_id: Option<&str>,
    refresh_hash: Option<&str>,
    refresh_expires_at: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    locked: bool,
) -> Result<()> {
    let created_at = Utc::now().to_rfc3339();
    let session_table = db.resolve_session_table().await?;

    let query = format!(
        "INSERT INTO {session_table} (
            session_id, jti, user_id, tenant_id, created_at, expires_at, refresh_expires_at,
            device_id, rot_id, refresh_hash, locked, ip_address, user_agent, last_activity
         )
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(jti) DO UPDATE SET
            device_id=excluded.device_id,
            rot_id=excluded.rot_id,
            refresh_hash=excluded.refresh_hash,
            refresh_expires_at=excluded.refresh_expires_at,
            expires_at=excluded.expires_at,
            locked=excluded.locked,
            ip_address=excluded.ip_address,
            user_agent=excluded.user_agent,
            last_activity=datetime('now')"
    );

    sqlx::query(&query)
        .bind(session_id)
        .bind(session_id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(&created_at)
        .bind(refresh_expires_at)
        .bind(refresh_expires_at)
        .bind(device_id)
        .bind(rot_id)
        .bind(refresh_hash)
        .bind(if locked { 1 } else { 0 })
        .bind(ip_address)
        .bind(user_agent)
        .execute(db.pool())
        .await?;

    Ok(())
}

/// Fetch a session by ID (session_id preferred, falls back to jti for legacy rows).
pub async fn get_session_by_id(db: &Db, session_id: &str) -> Result<Option<SessionRecord>> {
    let session_table = db.resolve_session_table().await?;
    let query = format!(
        "SELECT
            COALESCE(session_id, jti) as session_id,
            user_id,
            tenant_id,
            device_id,
            rot_id,
            refresh_hash,
            refresh_expires_at,
            expires_at,
            locked
         FROM {session_table}
         WHERE session_id = ? OR jti = ?
         LIMIT 1"
    );

    let row = sqlx::query_as::<_, SessionRecord>(&query)
        .bind(session_id)
        .bind(session_id)
        .fetch_optional(db.pool())
        .await?;

    Ok(row)
}

/// Rotate session metadata for refresh tokens.
pub async fn update_session_rotation(
    db: &Db,
    session_id: &str,
    rot_id: &str,
    refresh_hash: Option<&str>,
    refresh_expires_at: &str,
) -> Result<()> {
    let session_table = db.resolve_session_table().await?;
    let query = format!(
        "UPDATE {session_table}
         SET rot_id = ?, refresh_hash = ?, refresh_expires_at = ?, expires_at = ?, last_activity = datetime('now')
         WHERE session_id = ? OR jti = ?"
    );

    sqlx::query(&query)
        .bind(rot_id)
        .bind(refresh_hash)
        .bind(refresh_expires_at)
        .bind(refresh_expires_at)
        .bind(session_id)
        .bind(session_id)
        .execute(db.pool())
        .await?;
    Ok(())
}

/// Lock a session to prevent further use.
pub async fn lock_session(db: &Db, session_id: &str) -> Result<()> {
    let session_table = db.resolve_session_table().await?;
    let query = format!(
        "UPDATE {session_table}
         SET locked = 1, refresh_hash = NULL, last_activity = datetime('now')
         WHERE session_id = ? OR jti = ?"
    );

    sqlx::query(&query)
        .bind(session_id)
        .bind(session_id)
        .execute(db.pool())
        .await?;
    Ok(())
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
    let session_table = db.resolve_session_table().await?;
    let query = format!(
        "INSERT INTO {session_table} (jti, session_id, user_id, tenant_id, created_at, expires_at, ip_address, user_agent, last_activity, locked)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)"
    );

    sqlx::query(&query)
        .bind(jti)
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
    let session_table = db.resolve_session_table().await?;
    let query = format!("UPDATE {session_table} SET last_activity = ? WHERE jti = ?");

    sqlx::query(&query)
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
    let session_table = db.resolve_session_table().await?;
    let query = format!(
        "SELECT jti, created_at, ip_address, last_activity
         FROM {session_table}
         WHERE user_id = ?
           AND expires_at > ?
         ORDER BY last_activity DESC"
    );

    let sessions = sqlx::query_as::<_, (String, String, Option<String>, String)>(&query)
        .bind(user_id)
        .bind(&now)
        .fetch_all(db.pool())
        .await?;

    Ok(sessions)
}

/// Cleanup expired sessions
pub async fn cleanup_expired_sessions(db: &Db) -> Result<usize> {
    let now = Utc::now().to_rfc3339();
    let session_table = db.resolve_session_table().await?;
    let query = format!("DELETE FROM {session_table} WHERE expires_at < ?");

    let result = sqlx::query(&query).bind(&now).execute(db.pool()).await?;

    let count = result.rows_affected() as usize;

    if count > 0 {
        info!(count = %count, "Cleaned up expired sessions");
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthMode, PrincipalType};
    use crate::middleware::tenant_route_guard_middleware;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt;

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

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                display_name TEXT NOT NULL,
                pw_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                disabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                tenant_id TEXT DEFAULT 'default',
                failed_attempts INTEGER NOT NULL DEFAULT 0,
                last_failed_at TEXT,
                lockout_until TEXT
            )",
        )
        .execute(db.pool())
        .await
        .expect("Failed to create users table");
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
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: "jti-1".to_string(),
            nbf: 0,
            iss: "adapteros".to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
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
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: "jti-1".to_string(),
            nbf: 0,
            iss: "adapteros".to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
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
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: "jti-2".to_string(),
            nbf: 0,
            iss: "adapteros".to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
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
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: "jti-2".to_string(),
            nbf: 0,
            iss: "adapteros".to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        // Admin can access tenants in admin_tenants list
        assert!(validate_tenant_isolation(&claims, "system").is_ok()); // Own tenant
        assert!(validate_tenant_isolation(&claims, "tenant-a").is_ok()); // Granted access
        assert!(validate_tenant_isolation(&claims, "tenant-b").is_ok()); // Granted access
        assert!(validate_tenant_isolation(&claims, "tenant-c").is_err()); // No access
    }

    #[tokio::test]
    async fn test_auth_attempt_tracking() {
        let db = Db::connect("sqlite::memory:?cache=shared")
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

        let is_locked = is_account_locked(&db, "user@example.com", "192.168.1.1")
            .await
            .expect("Security operation failed");
        assert!(!is_locked); // Only 1 attempt, threshold is 5
    }

    #[tokio::test]
    async fn test_account_lockout() {
        let db = Db::connect("sqlite::memory:?cache=shared")
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

        let is_locked = is_account_locked(&db, "user@example.com", "192.168.1.1")
            .await
            .expect("Security operation failed");
        assert!(is_locked);
    }

    #[tokio::test]
    async fn test_failed_attempt_counters_and_reset() {
        let db = Db::connect("sqlite::memory:?cache=shared")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        sqlx::query("INSERT INTO users (id, email, display_name, pw_hash, role, disabled, tenant_id) VALUES (?, ?, ?, ?, ?, 0, 'default')")
            .bind("user-1")
            .bind("user@example.com")
            .bind("Test User")
            .bind("hash")
            .bind("admin")
            .execute(db.pool())
            .await
            .expect("insert user");

        for _ in 0..LOCKOUT_THRESHOLD {
            track_auth_attempt(
                &db,
                "user@example.com",
                "10.0.0.1",
                false,
                Some("invalid password"),
            )
            .await
            .expect("Security operation failed");
        }

        let locked_row =
            sqlx::query("SELECT failed_attempts, lockout_until FROM users WHERE email = ?")
                .bind("user@example.com")
                .fetch_one(db.pool())
                .await
                .expect("fetch user");

        let failed_attempts: i64 = locked_row.try_get("failed_attempts").unwrap_or(0);
        let lockout_until = locked_row
            .try_get::<Option<String>, _>("lockout_until")
            .ok()
            .flatten();

        assert!(
            failed_attempts >= LOCKOUT_THRESHOLD,
            "failed_attempts should be incremented"
        );
        assert!(
            lockout_until.is_some(),
            "lockout_until should be set after threshold is hit"
        );

        track_auth_attempt(&db, "user@example.com", "10.0.0.1", true, None)
            .await
            .expect("reset counters");

        let reset_row =
            sqlx::query("SELECT failed_attempts, lockout_until FROM users WHERE email = ?")
                .bind("user@example.com")
                .fetch_one(db.pool())
                .await
                .expect("fetch user");

        let reset_failed: i64 = reset_row.try_get("failed_attempts").unwrap_or(0);
        let reset_lockout = reset_row
            .try_get::<Option<String>, _>("lockout_until")
            .ok()
            .flatten();

        assert_eq!(reset_failed, 0);
        assert!(reset_lockout.is_none());
    }

    fn make_claims(tenant_id: &str, role: &str, admin_tenants: Vec<&str>) -> Claims {
        let now = chrono::Utc::now().timestamp();
        Claims {
            sub: "user-tenant-guard".to_string(),
            email: "user@example.com".to_string(),
            role: role.to_string(),
            roles: vec![role.to_string()],
            tenant_id: tenant_id.to_string(),
            admin_tenants: admin_tenants.into_iter().map(|s| s.to_string()).collect(),
            device_id: None,
            session_id: Some("session".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: now + 3600,
            iat: now,
            jti: "jti-tenant-guard".to_string(),
            nbf: now,
            iss: crate::auth::JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        }
    }

    fn tenant_guard_app() -> Router {
        Router::new()
            .route(
                "/v1/tenants/{tenant_id}/resource",
                get(|| async { StatusCode::OK }),
            )
            .layer(axum::middleware::from_fn(tenant_route_guard_middleware))
    }

    #[tokio::test]
    async fn tenant_guard_allows_same_tenant() {
        let mut req = Request::builder()
            .uri("/v1/tenants/tenant-a/resource")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(make_claims("tenant-a", "operator", vec![]));

        let response = tenant_guard_app().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn tenant_guard_rejects_cross_tenant_non_admin() {
        let mut req = Request::builder()
            .uri("/v1/tenants/tenant-b/resource")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(make_claims("tenant-a", "operator", vec![]));

        let resp = tenant_guard_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn tenant_guard_allows_admin_with_grant() {
        let mut req = Request::builder()
            .uri("/v1/tenants/tenant-b/resource")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(make_claims("system", "admin", vec!["tenant-b"]));

        let response = tenant_guard_app().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn tenant_guard_allows_admin_wildcard() {
        let mut req = Request::builder()
            .uri("/v1/tenants/tenant-c/resource")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(make_claims("system", "admin", vec!["*"]));

        let response = tenant_guard_app().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
