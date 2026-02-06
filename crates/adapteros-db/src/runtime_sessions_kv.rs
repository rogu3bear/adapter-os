//! KV storage for runtime sessions (control-plane instances).
//!
//! Mirrors the SQL `runtime_sessions` table to support KV-primary and
//! dual-write modes. Provides basic queries used by the runtime sessions
//! API: insert, get by id, most recent ended per host, end, and cleanup.

use crate::new_id;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use adapteros_storage::KvBackend;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSessionKv {
    pub id: String,
    pub session_id: String,
    pub config_hash: String,
    pub binary_version: String,
    pub binary_commit: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub end_reason: Option<String>,
    pub hostname: String,
    pub runtime_mode: String,
    pub config_snapshot: String,
    pub drift_detected: bool,
    pub drift_summary: Option<String>,
    pub previous_session_id: Option<String>,
    pub model_path: Option<String>,
    pub adapters_root: Option<String>,
    pub database_path: Option<String>,
    pub var_dir: Option<String>,
}

pub struct RuntimeSessionKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl RuntimeSessionKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn key(id: &str) -> String {
        format!("runtime_session:{id}")
    }

    fn now_ts() -> String {
        Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    pub fn new_session(&self, base: RuntimeSessionKv) -> RuntimeSessionKv {
        RuntimeSessionKv {
            id: if base.id.is_empty() {
                new_id(IdPrefix::Ses)
            } else {
                base.id
            },
            started_at: if base.started_at.is_empty() {
                Self::now_ts()
            } else {
                base.started_at
            },
            ..base
        }
    }

    pub async fn put(&self, session: &RuntimeSessionKv) -> Result<()> {
        let bytes = serde_json::to_vec(session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::key(&session.id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV store runtime session failed: {e}")))
    }

    pub async fn get(&self, id: &str) -> Result<Option<RuntimeSessionKv>> {
        let Some(bytes) = self
            .backend
            .get(&Self::key(id))
            .await
            .map_err(|e| AosError::Database(format!("KV get runtime session failed: {e}")))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    async fn scan_all(&self) -> Result<Vec<RuntimeSessionKv>> {
        let keys = self
            .backend
            .scan_prefix("runtime_session:")
            .await
            .map_err(|e| AosError::Database(format!("KV scan runtime sessions failed: {e}")))?;

        let mut sessions = Vec::new();
        for key in keys {
            if let Some(bytes) =
                self.backend.get(&key).await.map_err(|e| {
                    AosError::Database(format!("KV load runtime session failed: {e}"))
                })?
            {
                if let Ok(sess) = serde_json::from_slice::<RuntimeSessionKv>(&bytes) {
                    sessions.push(sess);
                }
            }
        }
        Ok(sessions)
    }

    pub async fn most_recent_ended_for_host(
        &self,
        hostname: &str,
    ) -> Result<Option<RuntimeSessionKv>> {
        let mut sessions: Vec<RuntimeSessionKv> = self
            .scan_all()
            .await?
            .into_iter()
            .filter(|s| s.hostname == hostname && s.ended_at.is_some())
            .collect();

        sessions.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.id.cmp(&a.id))
        });

        Ok(sessions.into_iter().next())
    }

    pub async fn end_session(&self, id: &str, reason: &str) -> Result<()> {
        let Some(mut session) = self.get(id).await? else {
            return Ok(());
        };

        session.ended_at = Some(Self::now_ts());
        session.end_reason = Some(reason.to_string());

        self.put(&session).await
    }

    pub async fn cleanup_old(&self, retention_days: i64, max_per_host: i64) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(retention_days);
        let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

        let mut by_host: HashMap<String, Vec<RuntimeSessionKv>> = HashMap::new();
        for sess in self.scan_all().await? {
            by_host.entry(sess.hostname.clone()).or_default().push(sess);
        }

        let mut to_delete = Vec::new();
        for sessions in by_host.values_mut() {
            // Sort newest first
            sessions.sort_by(|a, b| {
                b.started_at
                    .cmp(&a.started_at)
                    .then_with(|| b.id.cmp(&a.id))
            });

            for (idx, sess) in sessions.iter().enumerate() {
                // Keep if within retention or among most recent per host
                let keep_recent = (idx as i64) < max_per_host;
                let keep_newer = sess.started_at >= cutoff_str;
                if keep_recent || keep_newer {
                    continue;
                }
                to_delete.push(sess.id.clone());
            }
        }

        if !to_delete.is_empty() {
            self.backend.batch_delete(&to_delete).await.map_err(|e| {
                AosError::Database(format!("KV delete runtime sessions failed: {e}"))
            })?;
        }

        Ok(to_delete.len() as u64)
    }
}
