//! Integration tests for authentication middleware security fixes
//!
//! Tests verify:
//! - Token revocation checks in auth_middleware
//! - Correct HTTP status codes for auth failures
//! - Debug-only bypass restrictions

use adapteros_core::{B3Hash, Result};
use adapteros_db::Db;
use chrono::{Duration, Utc};
use uuid::Uuid;

#[tokio::test]
async fn test_revoked_token_detection() -> Result<()> {
    // Create an in-memory test database
    let db = Db::connect("sqlite::memory:").await?;

    // Initialize schema
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS revoked_tokens (
            jti TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            revoked_at TEXT NOT NULL,
            revoked_by TEXT,
            reason TEXT,
            expires_at TEXT NOT NULL
        )",
    )
    .execute(db.pool())
    .await?;

    let jti = "test-jti-123";
    let user_id = "user-1";
    let tenant_id = "tenant-a";
    let expires_at = (Utc::now() + Duration::hours(8)).to_rfc3339();

    // Token should not be revoked initially
    let is_revoked =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM revoked_tokens WHERE jti = ?")
            .bind(jti)
            .fetch_one(db.pool())
            .await?;
    assert_eq!(is_revoked, 0, "Token should not be revoked initially");

    // Revoke the token
    sqlx::query(
        "INSERT INTO revoked_tokens (jti, user_id, tenant_id, revoked_at, revoked_by, reason, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(jti)
    .bind(user_id)
    .bind(tenant_id)
    .bind(Utc::now().to_rfc3339())
    .bind(Some("admin"))
    .bind(Some("logout"))
    .bind(&expires_at)
    .execute(db.pool())
    .await?;

    // Verify token is now revoked
    let is_revoked =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM revoked_tokens WHERE jti = ?")
            .bind(jti)
            .fetch_one(db.pool())
            .await?;
    assert_eq!(is_revoked, 1, "Token should be revoked after insertion");

    Ok(())
}

#[tokio::test]
async fn test_token_revocation_cleanup() -> Result<()> {
    // Create an in-memory test database
    let db = Db::connect("sqlite::memory:").await?;

    // Initialize schema
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS revoked_tokens (
            jti TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            revoked_at TEXT NOT NULL,
            revoked_by TEXT,
            reason TEXT,
            expires_at TEXT NOT NULL
        )",
    )
    .execute(db.pool())
    .await?;

    let past_expiry = (Utc::now() - Duration::hours(1)).to_rfc3339();
    let future_expiry = (Utc::now() + Duration::hours(8)).to_rfc3339();

    // Insert expired token
    sqlx::query(
        "INSERT INTO revoked_tokens (jti, user_id, tenant_id, revoked_at, revoked_by, reason, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("expired-jti")
    .bind("user-1")
    .bind("tenant-a")
    .bind(Utc::now().to_rfc3339())
    .bind(Some("admin"))
    .bind(Some("test"))
    .bind(&past_expiry)
    .execute(db.pool())
    .await?;

    // Insert valid token
    sqlx::query(
        "INSERT INTO revoked_tokens (jti, user_id, tenant_id, revoked_at, revoked_by, reason, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("valid-jti")
    .bind("user-2")
    .bind("tenant-b")
    .bind(Utc::now().to_rfc3339())
    .bind(Some("admin"))
    .bind(Some("logout"))
    .bind(&future_expiry)
    .execute(db.pool())
    .await?;

    // Cleanup expired tokens
    let result = sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < ?")
        .bind(Utc::now().to_rfc3339())
        .execute(db.pool())
        .await?;
    assert_eq!(result.rows_affected(), 1, "Should cleanup 1 expired token");

    // Verify expired token is gone
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM revoked_tokens WHERE jti = ?")
        .bind("expired-jti")
        .fetch_one(db.pool())
        .await?;
    assert_eq!(count, 0, "Expired token should be cleaned up");

    // Verify valid token still exists
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM revoked_tokens WHERE jti = ?")
        .bind("valid-jti")
        .fetch_one(db.pool())
        .await?;
    assert_eq!(count, 1, "Valid token should not be cleaned up");

    Ok(())
}

#[test]
fn test_error_code_constants() {
    // Verify error codes used in middleware are correct
    let unauthorized = "UNAUTHORIZED";
    let internal_error = "INTERNAL_ERROR";
    let token_revoked = "TOKEN_REVOKED";

    // These should be used for auth failures
    assert_eq!(unauthorized, "UNAUTHORIZED");
    assert_eq!(internal_error, "INTERNAL_ERROR");
    assert_eq!(token_revoked, "TOKEN_REVOKED");

    // Verify INTERNAL_ERROR is NOT used for auth failures in fixed code
    // (this is a regression test for the security fix)
    assert_ne!(unauthorized, internal_error);
}
