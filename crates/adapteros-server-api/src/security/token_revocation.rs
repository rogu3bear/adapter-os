///! Token revocation management for JWT security
///!
///! Provides blacklist functionality for revoked tokens to prevent reuse.
use adapteros_core::Result;
use adapteros_db::Db;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RevokedToken {
    pub jti: String,
    pub user_id: String,
    pub tenant_id: String,
    pub revoked_at: String,
    pub revoked_by: Option<String>,
    pub reason: Option<String>,
    pub expires_at: String,
}

/// Check if a token has been revoked
pub async fn is_token_revoked(db: &Db, jti: &str) -> Result<bool> {
    let result = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM revoked_tokens WHERE jti = ?")
        .bind(jti)
        .fetch_one(db.pool())
        .await?;

    Ok(result > 0)
}

/// Revoke a token by JTI
pub async fn revoke_token(
    db: &Db,
    jti: &str,
    user_id: &str,
    tenant_id: &str,
    expires_at: &str,
    revoked_by: Option<&str>,
    reason: Option<&str>,
) -> Result<()> {
    let revoked_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO revoked_tokens (jti, user_id, tenant_id, revoked_at, revoked_by, reason, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(jti) DO NOTHING"
    )
    .bind(jti)
    .bind(user_id)
    .bind(tenant_id)
    .bind(&revoked_at)
    .bind(revoked_by)
    .bind(reason)
    .bind(expires_at)
    .execute(db.pool())
    .await?;

    info!(
        jti = %jti,
        user_id = %user_id,
        tenant_id = %tenant_id,
        reason = ?reason,
        "Token revoked"
    );

    Ok(())
}

/// Revoke all tokens for a user (e.g., on password change or account compromise)
pub async fn revoke_all_user_tokens(
    db: &Db,
    user_id: &str,
    tenant_id: &str,
    revoked_by: &str,
    reason: &str,
) -> Result<usize> {
    // Get all active sessions for the user
    let session_table = db.resolve_session_table().await?;
    let query =
        format!("SELECT jti, expires_at FROM {session_table} WHERE user_id = ? AND tenant_id = ?");
    let sessions = sqlx::query_as::<_, (String, String)>(&query)
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(db.pool())
        .await?;

    let count = sessions.len();

    for (jti, expires_at) in sessions {
        revoke_token(
            db,
            &jti,
            user_id,
            tenant_id,
            &expires_at,
            Some(revoked_by),
            Some(reason),
        )
        .await?;
    }

    info!(
        user_id = %user_id,
        tenant_id = %tenant_id,
        count = %count,
        reason = %reason,
        "Revoked all user tokens"
    );

    Ok(count)
}

/// Clean up expired revoked tokens (should run periodically)
pub async fn cleanup_expired_revocations(db: &Db) -> Result<usize> {
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query("DELETE FROM revoked_tokens WHERE expires_at < ?")
        .bind(&now)
        .execute(db.pool())
        .await?;

    let count = result.rows_affected() as usize;

    if count > 0 {
        info!(count = %count, "Cleaned up expired revoked tokens");
    }

    Ok(count)
}

/// Get revocation history for a user
pub async fn get_user_revocations(db: &Db, user_id: &str, limit: i64) -> Result<Vec<RevokedToken>> {
    let tokens = sqlx::query_as::<_, RevokedToken>(
        "SELECT jti, user_id, tenant_id, revoked_at, revoked_by, reason, expires_at
         FROM revoked_tokens
         WHERE user_id = ?
         ORDER BY revoked_at DESC
         LIMIT ?",
    )
    .bind(user_id)
    .bind(limit.min(100))
    .fetch_all(db.pool())
    .await?;

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    async fn init_test_schema(db: &Db) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS revoked_tokens (
                jti TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                tenant_id TEXT NOT NULL,
                revoked_at TEXT NOT NULL DEFAULT (datetime('now')),
                revoked_by TEXT,
                reason TEXT,
                expires_at TEXT NOT NULL
            )",
        )
        .execute(db.pool())
        .await
        .expect("Failed to create revoked_tokens table");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS user_sessions (
                jti TEXT PRIMARY KEY,
                session_id TEXT,
                user_id TEXT NOT NULL,
                tenant_id TEXT NOT NULL,
                device_id TEXT,
                rot_id TEXT,
                refresh_hash TEXT,
                refresh_expires_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                expires_at TEXT NOT NULL,
                ip_address TEXT,
                user_agent TEXT,
                last_activity TEXT NOT NULL DEFAULT (datetime('now')),
                locked INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(db.pool())
        .await
        .expect("Failed to create user_sessions table");
    }

    #[tokio::test]
    async fn test_token_revocation() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        let jti = "test-jti-123";
        let user_id = "user-1";
        let tenant_id = "tenant-a";
        let expires_at = (Utc::now() + Duration::hours(8)).to_rfc3339();

        // Initially not revoked
        assert!(!is_token_revoked(&db, jti)
            .await
            .expect("Failed to check token revocation"));

        // Revoke token
        revoke_token(
            &db,
            jti,
            user_id,
            tenant_id,
            &expires_at,
            Some("admin"),
            Some("logout"),
        )
        .await
        .expect("Failed to revoke token");

        // Now revoked
        assert!(is_token_revoked(&db, jti)
            .await
            .expect("Failed to check token revocation"));
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let db = Db::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");
        init_test_schema(&db).await;

        let past_expiry = (Utc::now() - Duration::hours(1)).to_rfc3339();

        // Add expired revocation
        revoke_token(
            &db,
            "expired-jti",
            "user-1",
            "tenant-a",
            &past_expiry,
            None,
            Some("test"),
        )
        .await
        .expect("Failed to add revocation");

        // Cleanup
        let count = cleanup_expired_revocations(&db)
            .await
            .expect("Failed to cleanup expired revocations");
        assert_eq!(count, 1);

        // Verify cleaned up
        assert!(!is_token_revoked(&db, "expired-jti")
            .await
            .expect("Failed to check expired token revocation"));
    }
}
