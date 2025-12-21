//! Authentication sessions database operations
//!
//! Provides methods for managing authentication sessions, token revocation,
//! and session lifecycle.

use crate::auth_sessions_kv::{AuthSessionKv, AuthSessionKvRepository};
use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::{AosError, Result};

impl Db {
    fn get_auth_kv_repo(&self) -> Option<AuthSessionKvRepository> {
        if (self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv())
            && self.has_kv_backend()
        {
            self.kv_backend().map(|kv| {
                let backend: std::sync::Arc<dyn crate::kv_backend::KvBackend> = kv.clone();
                AuthSessionKvRepository::new(backend)
            })
        } else {
            None
        }
    }

    fn kv_to_auth_session(kv: AuthSessionKv) -> AuthSession {
        AuthSession {
            jti: kv.jti.clone(),
            session_id: kv.session_id.or(Some(kv.jti)),
            user_id: kv.user_id,
            tenant_id: kv.tenant_id.unwrap_or_default(),
            device_id: kv.device_id,
            rot_id: kv.rot_id,
            refresh_hash: kv.refresh_hash,
            refresh_expires_at: kv.refresh_expires_at.map(|ts| ts.to_string()),
            ip_address: kv.ip_address,
            user_agent: kv.user_agent,
            created_at: kv.created_at.to_rfc3339(),
            last_activity: kv.last_activity.to_rfc3339(),
            expires_at: kv.expires_at,
            locked: kv.locked,
        }
    }

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
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "INSERT INTO revoked_tokens (token_hash, revoked_by, reason, revoked_at)
                     VALUES (?, ?, ?, datetime('now'))",
                )
                .bind(token_hash)
                .bind(revoked_by)
                .bind(reason)
                .execute(pool)
                .await
                .db_err("revoke token")?;
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                repo.revoke_token(token_hash, revoked_by, reason).await?;
            }
        }

        Ok(())
    }

    /// Delete an authentication session by JTI (JWT ID)
    ///
    /// # Arguments
    /// * `jti` - The JWT ID to delete
    pub async fn delete_auth_session(&self, jti: &str) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query("DELETE FROM auth_sessions WHERE jti = ?")
                    .bind(jti)
                    .execute(pool)
                    .await
                    .db_err("delete auth session")?;
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                repo.delete_session(jti).await?;
            }
        }

        Ok(())
    }

    /// Create a new authentication session
    ///
    /// # Arguments
    /// * `jti` - JWT ID (unique identifier for the session)
    /// * `tenant_id` - Tenant associated with the session
    /// * `user_id` - User ID associated with the session
    /// * `ip_address` - IP address of the client
    /// * `user_agent` - User agent string
    /// * `expires_at` - Session expiration timestamp
    pub async fn create_auth_session(
        &self,
        jti: &str,
        tenant_id: &str,
        user_id: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
        expires_at: i64,
    ) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "INSERT INTO auth_sessions (
                        jti, session_id, user_id, tenant_id, device_id, rot_id, refresh_hash,
                        refresh_expires_at, ip_address, user_agent, created_at, last_activity,
                        expires_at, locked
                    )
                    VALUES (?, ?, ?, ?, NULL, NULL, NULL, NULL, ?, ?, datetime('now'), datetime('now'), ?, 0)",
                )
                .bind(jti)
                .bind(jti) // align session_id with jti for compatibility
                .bind(user_id)
                .bind(tenant_id)
                .bind(ip_address)
                .bind(user_agent)
                .bind(expires_at)
                .execute(pool)
                .await
                .db_err("create auth session")?;
            } else {
                return Err(AosError::Database(
                    "SQL backend unavailable for auth session creation".to_string(),
                ));
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                repo.create_session(jti, user_id, ip_address, user_agent, expires_at)
                    .await?;
            }
        }

        Ok(())
    }

    /// Update auth session last activity timestamp
    pub async fn update_auth_session_activity(&self, jti: &str) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "UPDATE auth_sessions SET last_activity = datetime('now') WHERE jti = ?",
                )
                .bind(jti)
                .execute(pool)
                .await
                .db_err("update session activity")?;
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                repo.update_activity(jti).await?;
            }
        }

        Ok(())
    }

    /// Check if a token has been revoked
    pub async fn is_token_revoked(&self, token_hash: &str) -> Result<bool> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                return repo.is_revoked(token_hash).await;
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(false);
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(false),
        };
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM revoked_tokens WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_one(pool)
        .await
        .db_err("check token revocation")?;

        Ok(count > 0)
    }

    /// Get all active sessions for a user
    pub async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<AuthSession>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                let sessions = repo
                    .list_user_sessions(user_id)
                    .await?
                    .into_iter()
                    .map(Self::kv_to_auth_session)
                    .collect();
                return Ok(sessions);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        let sessions = sqlx::query_as::<_, AuthSession>(
            "SELECT
                 jti,
                 session_id,
                 user_id,
                 tenant_id,
                 device_id,
                 rot_id,
                 refresh_hash,
                 refresh_expires_at,
                 ip_address,
                 user_agent,
                 created_at,
                 last_activity,
                 expires_at,
                 locked
             FROM auth_sessions
             WHERE user_id = ? AND expires_at > ?
             ORDER BY last_activity DESC",
        )
        .bind(user_id)
        .bind(chrono::Utc::now().timestamp())
        .fetch_all(pool)
        .await
        .db_err("get user sessions")?;

        Ok(sessions)
    }

    /// Delete all sessions for a user (logout all devices)
    pub async fn delete_all_user_sessions(&self, user_id: &str) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query("DELETE FROM auth_sessions WHERE user_id = ?")
                    .bind(user_id)
                    .execute(pool)
                    .await
                    .db_err("delete all user sessions")?;
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                // delete per-session keys tracked via set
                for session in repo.list_user_sessions(user_id).await? {
                    repo.delete_session(&session.jti).await?;
                }
            }
        }

        Ok(())
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let mut total_deleted = 0u64;

        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                let result = sqlx::query("DELETE FROM auth_sessions WHERE expires_at < ?")
                    .bind(chrono::Utc::now().timestamp())
                    .execute(pool)
                    .await
                    .db_err("cleanup expired sessions")?;
                total_deleted += result.rows_affected();
            }
        }

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_auth_kv_repo() {
                total_deleted += repo.cleanup_expired(chrono::Utc::now().timestamp()).await?;
            }
        }

        Ok(total_deleted)
    }
}

/// Authentication session record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuthSession {
    pub jti: String,
    pub session_id: Option<String>,
    pub user_id: String,
    pub tenant_id: String,
    pub device_id: Option<String>,
    pub rot_id: Option<String>,
    pub refresh_hash: Option<String>,
    pub refresh_expires_at: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: String,
    pub last_activity: String,
    pub expires_at: i64,
    pub locked: bool,
}
