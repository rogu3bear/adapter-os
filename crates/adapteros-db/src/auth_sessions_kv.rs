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
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub rot_id: Option<String>,
    #[serde(default)]
    pub refresh_expires_at: Option<i64>,
    #[serde(default)]
    pub refresh_hash: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub expires_at: i64,
    #[serde(default)]
    pub locked: bool,
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

    /// Idempotent upsert of a session (used by migration/repair).
    pub async fn put_session(&self, session: AuthSessionKv) -> Result<()> {
        let bytes = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::session_key(&session.jti), bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store auth session: {}", e)))?;

        // Keep per-user membership deterministic
        self.backend
            .set_add(&Self::user_sessions_set(&session.user_id), &session.jti)
            .await
            .map_err(|e| AosError::Database(format!("Failed to index auth session: {}", e)))?;
        Ok(())
    }

    pub async fn revoke_token(
        &self,
        token_hash: &str,
        revoked_by: &str,
        reason: Option<&str>,
    ) -> Result<()> {
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
                let _ = self.backend.set_remove(&user_set, jti).await.map_err(|e| {
                    AosError::Database(format!("Failed to update user session set: {}", e))
                });
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
            tenant_id: None,
            session_id: None,
            device_id: None,
            rot_id: None,
            refresh_expires_at: None,
            refresh_hash: None,
            ip_address: ip_address.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            created_at: now,
            last_activity: now,
            expires_at,
            locked: false,
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

    /// Create or update a session with device and rotation metadata.
    pub async fn create_session_with_device(
        &self,
        session_id: &str,
        user_id: &str,
        tenant_id: &str,
        device_id: Option<&str>,
        rot_id: Option<&str>,
        refresh_expires_at: i64,
        refresh_hash: Option<&str>,
        locked: bool,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        let session = AuthSessionKv {
            jti: session_id.to_string(),
            user_id: user_id.to_string(),
            tenant_id: Some(tenant_id.to_string()),
            session_id: Some(session_id.to_string()),
            device_id: device_id.map(|s| s.to_string()),
            rot_id: rot_id.map(|s| s.to_string()),
            refresh_expires_at: Some(refresh_expires_at),
            refresh_hash: refresh_hash.map(|s| s.to_string()),
            ip_address: ip_address.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            created_at: now,
            last_activity: now,
            expires_at: refresh_expires_at,
            locked,
        };

        let bytes = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::session_key(session_id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store auth session: {}", e)))?;

        self.backend
            .set_add(&Self::user_sessions_set(user_id), session_id)
            .await
            .map_err(|e| AosError::Database(format!("Failed to index user session: {}", e)))?;

        Ok(())
    }

    /// Rotate refresh metadata for a session.
    pub async fn rotate_session(
        &self,
        session_id: &str,
        rot_id: &str,
        refresh_hash: Option<&str>,
        refresh_expires_at: i64,
    ) -> Result<()> {
        let key = Self::session_key(session_id);
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
        session.rot_id = Some(rot_id.to_string());
        session.refresh_hash = refresh_hash.map(|s| s.to_string());
        session.refresh_expires_at = Some(refresh_expires_at);
        session.expires_at = refresh_expires_at;
        session.last_activity = Utc::now();

        let updated = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&key, updated)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update auth session: {}", e)))
    }

    /// Mark a session as locked (e.g., on logout/revoke).
    pub async fn lock_session(&self, session_id: &str) -> Result<()> {
        let key = Self::session_key(session_id);
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
        session.locked = true;
        session.refresh_hash = None;
        session.last_activity = Utc::now();

        let updated = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&key, updated)
            .await
            .map_err(|e| AosError::Database(format!("Failed to lock auth session: {}", e)))
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

    /// Fetch a session by JTI.
    pub async fn get_session(&self, jti: &str) -> Result<Option<AuthSessionKv>> {
        let key = Self::session_key(jti);
        let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get auth session: {}", e)))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn list_user_sessions(&self, user_id: &str) -> Result<Vec<AuthSessionKv>> {
        let set_key = Self::user_sessions_set(user_id);
        let session_ids =
            self.backend.set_members(&set_key).await.map_err(|e| {
                AosError::Database(format!("Failed to read user session set: {}", e))
            })?;

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
                    Err(e) => {
                        warn!(session_id = %sid, error = %e, "Failed to deserialize auth session")
                    }
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
                    let expiry = session.refresh_expires_at.unwrap_or(session.expires_at);
                    if expiry < now_ts {
                        self.delete_session(&session.jti).await?;
                        deleted += 1;
                    }
                }
            }
        }

        Ok(deleted)
    }
}
