//! KV storage for auth sessions and revoked tokens
//!
//! Keys:
//! - `auth/session/{jti}` -> AuthSessionKv (JSON)
//! - `auth/user/{user_id}/sessions` -> Set<jti>
//! - `auth/revoked/{token_hash}` -> Revocation record

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthSessionKv {
    pub jti: String,
    pub user_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevokedTokenKv {
    pub token_hash: String,
    pub revoked_by: String,
    pub reason: Option<String>,
    pub revoked_at: DateTime<Utc>,
}

pub struct AuthSessionKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl AuthSessionKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn session_key(jti: &str) -> String {
        format!("auth/session/{}", jti)
    }

    fn user_sessions_set(user_id: &str) -> String {
        format!("auth/user/{}/sessions", user_id)
    }

    fn revoked_key(token_hash: &str) -> String {
        format!("auth/revoked/{}", token_hash)
    }

    pub async fn revoke_token(&self, token_hash: &str, revoked_by: &str, reason: Option<&str>) -> Result<()> {
        let record = RevokedTokenKv {
            token_hash: token_hash.to_string(),
            revoked_by: revoked_by.to_string(),
            reason: reason.map(|s| s.to_string()),
            revoked_at: Utc::now(),
        };
        let bytes = serde_json::to_vec(&record).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::revoked_key(token_hash), bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store revoked token: {}", e)))
    }

    pub async fn is_revoked(&self, token_hash: &str) -> Result<bool> {
        self.backend
            .exists(&Self::revoked_key(token_hash))
            .await
            .map_err(|e| AosError::Database(format!("Failed to check revoked token: {}", e)))
    }

    pub async fn delete_session(&self, jti: &str) -> Result<()> {
        let session_key = Self::session_key(jti);
        if let Some(bytes) = self
            .backend
            .get(&session_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to fetch session: {}", e)))?
        {
            if let Ok(session) = serde_json::from_slice::<AuthSessionKv>(&bytes) {
                let user_set = Self::user_sessions_set(&session.user_id);
                let _ = self
                    .backend
                    .set_remove(&user_set, jti)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to update user session set: {}", e)));
            }
        }

        self.backend
            .delete(&session_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete session: {}", e)))?;

        Ok(())
    }

    pub async fn create_session(
        &self,
        jti: &str,
        user_id: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
        expires_at: i64,
    ) -> Result<()> {
        let now = Utc::now();
        let session = AuthSessionKv {
            jti: jti.to_string(),
            user_id: user_id.to_string(),
            ip_address: ip_address.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            created_at: now,
            last_activity: now,
            expires_at,
        };

        let bytes = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::session_key(jti), bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store auth session: {}", e)))?;

        // Track per-user membership for lookups
        self.backend
            .set_add(&Self::user_sessions_set(user_id), jti)
            .await
            .map_err(|e| AosError::Database(format!("Failed to index user session: {}", e)))?;

        Ok(())
    }

    pub async fn update_activity(&self, jti: &str) -> Result<()> {
        let key = Self::session_key(jti);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get auth session: {}", e)))?
        else {
            return Ok(());
        };

        let mut session: AuthSessionKv =
            serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
        session.last_activity = Utc::now();

        let updated = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&key, updated)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update auth session: {}", e)))
    }

    pub async fn list_user_sessions(&self, user_id: &str) -> Result<Vec<AuthSessionKv>> {
        let set_key = Self::user_sessions_set(user_id);
        let session_ids = self
            .backend
            .set_members(&set_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read user session set: {}", e)))?;

        let mut sessions = Vec::new();
        for sid in session_ids {
            if let Some(bytes) = self
                .backend
                .get(&Self::session_key(&sid))
                .await
                .map_err(|e| AosError::Database(format!("Failed to read session: {}", e)))?
            {
                match serde_json::from_slice::<AuthSessionKv>(&bytes) {
                    Ok(session) => sessions.push(session),
                    Err(e) => warn!(session_id = %sid, error = %e, "Failed to deserialize auth session"),
                }
            }
        }

        // Deterministic ordering: last_activity DESC then jti ASC
        sessions.sort_by(|a, b| {
            b.last_activity
                .cmp(&a.last_activity)
                .then_with(|| a.jti.cmp(&b.jti))
        });

        Ok(sessions)
    }

    pub async fn cleanup_expired(&self, now_ts: i64) -> Result<u64> {
        let keys = self
            .backend
            .scan_prefix("auth/session/")
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan sessions: {}", e)))?;

        let mut deleted = 0u64;
        for key in keys {
            if let Some(bytes) = self
                .backend
                .get(&key)
                .await
                .map_err(|e| AosError::Database(format!("Failed to fetch session: {}", e)))?
            {
                if let Ok(session) = serde_json::from_slice::<AuthSessionKv>(&bytes) {
                    if session.expires_at < now_ts {
                        self.delete_session(&session.jti).await?;
                        deleted += 1;
                    }
                }
            }
        }

        Ok(deleted)
    }
}

