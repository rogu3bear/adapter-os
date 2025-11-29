//! Authentication sessions database operations
//!
//! Provides methods for managing authentication sessions, token revocation,
//! and session lifecycle.

use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::{AosError, Result};

impl Db {
    /// Revoke a token by inserting into revoked_tokens table
    ///
    /// # Arguments
    /// * `token_hash` - SHA-256 hash of the token to revoke
    /// * `revoked_by` - User ID of the user revoking the token
    /// * `reason` - Optional reason for revocation
    pub async fn revoke_token(
        &self,
        token_hash: &str,
        revoked_by: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO revoked_tokens (token_hash, revoked_by, reason, revoked_at)
             VALUES (?, ?, ?, datetime('now'))",
        )
        .bind(token_hash)
        .bind(revoked_by)
        .bind(reason)
        .execute(self.pool())
        .await
        .db_err("revoke token")?;

        Ok(())
    }

    /// Delete an authentication session by JTI (JWT ID)
    ///
    /// # Arguments
    /// * `jti` - The JWT ID to delete
    pub async fn delete_auth_session(&self, jti: &str) -> Result<()> {
        sqlx::query("DELETE FROM auth_sessions WHERE jti = ?")
            .bind(jti)
            .execute(self.pool())
            .await
            .db_err("delete auth session")?;

        Ok(())
    }

    /// Create a new authentication session
    ///
    /// # Arguments
    /// * `jti` - JWT ID (unique identifier for the session)
    /// * `user_id` - User ID associated with the session
    /// * `ip_address` - IP address of the client
    /// * `user_agent` - User agent string
    /// * `expires_at` - Session expiration timestamp
    pub async fn create_auth_session(
        &self,
        jti: &str,
        user_id: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
        expires_at: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO auth_sessions (jti, user_id, ip_address, user_agent, created_at, last_activity, expires_at)
             VALUES (?, ?, ?, ?, datetime('now'), datetime('now'), ?)",
        )
        .bind(jti)
        .bind(user_id)
        .bind(ip_address)
        .bind(user_agent)
        .bind(expires_at)
        .execute(self.pool())
        .await
        .db_err("create auth session")?;

        Ok(())
    }

    /// Update auth session last activity timestamp
    pub async fn update_auth_session_activity(&self, jti: &str) -> Result<()> {
        sqlx::query("UPDATE auth_sessions SET last_activity = datetime('now') WHERE jti = ?")
            .bind(jti)
            .execute(self.pool())
            .await
            .db_err("update session activity")?;

        Ok(())
    }

    /// Check if a token has been revoked
    pub async fn is_token_revoked(&self, token_hash: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM revoked_tokens WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_one(self.pool())
        .await
        .db_err("check token revocation")?;

        Ok(count > 0)
    }

    /// Get all active sessions for a user
    pub async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<AuthSession>> {
        let sessions = sqlx::query_as::<_, AuthSession>(
            "SELECT jti, user_id, ip_address, user_agent, created_at, last_activity, expires_at
             FROM auth_sessions
             WHERE user_id = ? AND expires_at > ?
             ORDER BY last_activity DESC",
        )
        .bind(user_id)
        .bind(chrono::Utc::now().timestamp())
        .fetch_all(self.pool())
        .await
        .db_err("get user sessions")?;

        Ok(sessions)
    }

    /// Delete all sessions for a user (logout all devices)
    pub async fn delete_all_user_sessions(&self, user_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM auth_sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(self.pool())
            .await
            .db_err("delete all user sessions")?;

        Ok(())
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM auth_sessions WHERE expires_at < ?")
            .bind(chrono::Utc::now().timestamp())
            .execute(self.pool())
            .await
            .db_err("cleanup expired sessions")?;

        Ok(result.rows_affected())
    }
}

/// Authentication session record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuthSession {
    pub jti: String,
    pub user_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_activity: String,
    pub expires_at: i64,
}
