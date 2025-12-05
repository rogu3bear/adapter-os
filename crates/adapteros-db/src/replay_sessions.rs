//! Replay session storage and retrieval
//!
//! Manages deterministic replay sessions with full system state snapshots.

use crate::rag_retrieval_audit::RagReplayState;
use crate::replay_kv::record_replay_drift;
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReplaySession {
    pub id: String,
    pub tenant_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub snapshot_at: String,
    pub seed_global_b3: String,
    pub manifest_hash_b3: String,
    pub policy_hash_b3: String,
    pub kernel_hash_b3: Option<String>,
    pub telemetry_bundle_ids_json: String,
    pub adapter_state_json: String,
    pub routing_decisions_json: String,
    pub inference_traces_json: Option<String>,
    pub rng_state_json: String,
    pub signature: String,
    pub created_at: String,
    /// RAG state for deterministic replay with original documents
    /// JSON: {"doc_ids": [...], "scores": [...], "collection_id": "...", "embedding_model_hash": "..."}
    pub rag_state_json: Option<String>,
}

impl ReplaySession {
    /// Restore RNG state from JSON
    pub fn restore_rng_state(&self) -> Result<serde_json::Value> {
        serde_json::from_str(&self.rng_state_json)
            .map_err(|e| AosError::Validation(format!("Failed to parse RNG state: {}", e)))
    }

    /// Get global nonce from RNG state
    pub fn get_global_nonce(&self) -> Result<u64> {
        let state: serde_json::Value = self.restore_rng_state()?;
        state
            .get("global_nonce")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| AosError::Validation("Missing global_nonce in RNG state".into()))
    }

    /// Restore RAG state from JSON for deterministic replay with original documents
    ///
    /// Returns None if no RAG state was stored (inference didn't use RAG),
    /// or the deserialized RagReplayState if available.
    pub fn restore_rag_state(&self) -> Result<Option<RagReplayState>> {
        self.rag_state_json
            .as_ref()
            .map(|json| {
                serde_json::from_str(json)
                    .map_err(|e| AosError::Validation(format!("Failed to parse RAG state: {}", e)))
            })
            .transpose()
    }
}

impl Db {
    /// List replay sessions, optionally filtered by tenant
    pub async fn list_replay_sessions(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<Vec<ReplaySession>> {
        if let Some(repo) = self.replay_repo_if_read() {
            if let Some(tid) = tenant_id {
                match repo.list_sessions_by_tenant(tid).await {
                    Ok(sessions) => {
                        let mut mapped = Vec::new();
                        for sess in sessions {
                            mapped.push(self.kv_replay_session_to_record(sess)?);
                        }
                        return Ok(mapped);
                    }
                    Err(e) => {
                        if !self.storage_mode().sql_fallback_enabled() {
                            return Err(AosError::Database(format!(
                                "KV read failed for replay sessions: {}",
                                e
                            )));
                        }
                        warn!(
                            tenant_id = %tid,
                            error = %e,
                            "KV replay session list failed, falling back to SQL"
                        );
                    }
                }
            }
        }

        let query = if tenant_id.is_some() {
            "SELECT * FROM replay_sessions WHERE tenant_id = ? ORDER BY snapshot_at DESC"
        } else {
            "SELECT * FROM replay_sessions ORDER BY snapshot_at DESC"
        };

        let sessions = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, ReplaySession>(query)
                .bind(tid)
                .fetch_all(&*self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to list replay sessions: {}", e)))?
        } else {
            sqlx::query_as::<_, ReplaySession>(query)
                .fetch_all(&*self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to list replay sessions: {}", e)))?
        };

        Ok(sessions)
    }

    /// Get a single replay session by ID
    pub async fn get_replay_session(&self, session_id: &str) -> Result<Option<ReplaySession>> {
        if let Some(repo) = self.replay_repo_if_read() {
            match repo.get_session_by_id(session_id).await {
                Ok(Some(session)) => return self.kv_replay_session_to_record(session).map(Some),
                Ok(None) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for replay session: {}",
                            e
                        )));
                    }
                    warn!(session_id = %session_id, error = %e, "KV replay session read failed, falling back to SQL");
                }
            }
        }

        let session =
            sqlx::query_as::<_, ReplaySession>("SELECT * FROM replay_sessions WHERE id = ?")
                .bind(session_id)
                .fetch_optional(&*self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to get replay session: {}", e)))?;

        Ok(session)
    }

    /// Create a new replay session
    pub async fn create_replay_session(&self, session: &ReplaySession) -> Result<()> {
        sqlx::query(
            "INSERT INTO replay_sessions (
                id, tenant_id, cpid, plan_id, snapshot_at, seed_global_b3,
                manifest_hash_b3, policy_hash_b3, kernel_hash_b3,
                telemetry_bundle_ids_json, adapter_state_json,
                routing_decisions_json, inference_traces_json, rng_state_json, signature,
                rag_state_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(&session.tenant_id)
        .bind(&session.cpid)
        .bind(&session.plan_id)
        .bind(&session.snapshot_at)
        .bind(&session.seed_global_b3)
        .bind(&session.manifest_hash_b3)
        .bind(&session.policy_hash_b3)
        .bind(&session.kernel_hash_b3)
        .bind(&session.telemetry_bundle_ids_json)
        .bind(&session.adapter_state_json)
        .bind(&session.routing_decisions_json)
        .bind(&session.inference_traces_json)
        .bind(&session.rng_state_json)
        .bind(&session.signature)
        .bind(&session.rag_state_json)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create replay session: {}", e)))?;

        if let Some(repo) = self.replay_repo_if_write() {
            let kv_session = Db::kv_replay_session_from_record(session);
            if let Err(e) = repo.store_session(kv_session).await {
                warn!(
                    tenant_id = %session.tenant_id,
                    session_id = %session.id,
                    error = %e,
                    "Failed to dual-write replay session to KV"
                );
                record_replay_drift("replay_session_dual_write_failed");
            }
        }

        Ok(())
    }

    /// Delete a replay session
    pub async fn delete_replay_session(&self, session_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM replay_sessions WHERE id = ?")
            .bind(session_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete replay session: {}", e)))?;

        if let Some(repo) = self.replay_repo_if_write() {
            if let Err(e) = repo.delete_session(session_id).await {
                warn!(
                    session_id = %session_id,
                    error = %e,
                    "Failed to delete replay session from KV"
                );
                record_replay_drift("replay_session_delete_failed");
            }
        }
        Ok(())
    }
}
