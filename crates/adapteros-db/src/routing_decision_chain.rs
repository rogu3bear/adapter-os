use crate::{Db, KvBackend};
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use serde_json;
use sqlx::FromRow;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

/// Routing decision chain entry (per token)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoutingDecisionChainRecord {
    pub id: String,
    pub tenant_id: String,
    pub inference_id: String,
    pub request_id: Option<String>,
    pub step: i64,
    pub input_token_id: Option<i64>,
    pub adapter_indices: String,
    pub adapter_ids: String,
    pub gates_q15: String,
    pub entropy: f64,
    pub decision_hash_json: Option<String>,
    pub previous_hash: Option<String>,
    pub entry_hash: String,
    pub created_at: String,
}

impl Db {
    /// Insert a batch of routing decision chain records with optional KV dual-write.
    ///
    /// Records are keyed by tenant + inference_id + step for uniqueness.
    pub async fn insert_routing_decision_chain_batch(
        &self,
        entries: &[RoutingDecisionChainRecord],
    ) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        if !self.storage_mode().write_to_sql() {
            return Err(AosError::Validation(
                "SQL write disabled - cannot persist routing decision chain".to_string(),
            ));
        }

        let kv_backend: Option<Arc<dyn KvBackend>> = if self.storage_mode().write_to_kv() {
            Some(
                self.kv_backend()
                    .ok_or_else(|| {
                        AosError::Validation(
                            "KV backend unavailable - cannot dual-write routing decision chain"
                                .to_string(),
                        )
                    })?
                    .backend()
                    .clone(),
            )
        } else {
            None
        };

        let mut tx = self.pool().begin().await.map_err(|e| {
            AosError::Database(format!("Failed to begin routing chain transaction: {e}"))
        })?;

        // Clear any prior chain for this inference to avoid unique constraint churn
        let tenant_id = &entries[0].tenant_id;
        let inference_id = &entries[0].inference_id;
        sqlx::query(
            r#"
            DELETE FROM routing_decision_chain
            WHERE tenant_id = ? AND inference_id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(inference_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to clear routing chain: {e}")))?;

        for entry in entries {
            sqlx::query(
                r#"
                INSERT INTO routing_decision_chain (
                    id, tenant_id, inference_id, request_id, step, input_token_id,
                    adapter_indices, adapter_ids, gates_q15, entropy, decision_hash_json,
                    previous_hash, entry_hash, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                "#,
            )
            .bind(&entry.id)
            .bind(&entry.tenant_id)
            .bind(&entry.inference_id)
            .bind(&entry.request_id)
            .bind(entry.step)
            .bind(entry.input_token_id)
            .bind(&entry.adapter_indices)
            .bind(&entry.adapter_ids)
            .bind(&entry.gates_q15)
            .bind(entry.entropy)
            .bind(&entry.decision_hash_json)
            .bind(&entry.previous_hash)
            .bind(&entry.entry_hash)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to insert routing decision chain: {e}"))
            })?;

            if let Some(kv) = kv_backend.as_ref() {
                let kv_key = format!(
                    "routing_chain:{}:{}:{}",
                    entry.tenant_id, entry.inference_id, entry.step
                );
                match serde_json::to_vec(entry) {
                    Ok(bytes) => {
                        if let Err(e) = kv.set(&kv_key, bytes).await {
                            warn!(error = %e, key = %kv_key, "KV dual-write failed for routing chain entry");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, key = %kv_key, "Failed to serialize routing chain entry for KV");
                    }
                }
            }
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit routing chain: {e}")))?;

        Ok(entries.len())
    }

    /// Fetch routing decision chain entries for an inference.
    pub async fn get_routing_decision_chain(
        &self,
        tenant_id: &str,
        inference_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<RoutingDecisionChainRecord>> {
        if !self.storage_mode().read_from_sql() {
            return Err(AosError::Validation(
                "SQL read disabled - cannot fetch routing decision chain".to_string(),
            ));
        }

        let records = sqlx::query_as::<_, RoutingDecisionChainRecord>(
            r#"
            SELECT id, tenant_id, inference_id, request_id, step, input_token_id,
                   adapter_indices, adapter_ids, gates_q15, entropy, decision_hash_json,
                   previous_hash, entry_hash, created_at
            FROM routing_decision_chain
            WHERE tenant_id = ? AND inference_id = ?
            ORDER BY step ASC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tenant_id)
        .bind(inference_id)
        .bind(limit.unwrap_or(500) as i64)
        .bind(offset.unwrap_or(0) as i64)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch routing chain: {e}")))?;

        Ok(records)
    }

    /// Verify chain integrity: previous_hash linkage and entry_hash recomputation.
    pub async fn verify_routing_decision_chain(
        &self,
        tenant_id: &str,
        inference_id: &str,
    ) -> Result<ChainVerification> {
        let records = self
            .get_routing_decision_chain(tenant_id, inference_id, None, None)
            .await?;

        let mut previous: Option<String> = None;
        for (idx, rec) in records.iter().enumerate() {
            if rec.previous_hash.as_ref() != previous.as_ref() {
                return Ok(ChainVerification {
                    is_valid: false,
                    entries_checked: idx,
                    first_invalid_step: Some(rec.step),
                    error: Some("previous_hash mismatch".to_string()),
                });
            }

            let indices_joined = join_json_slice::<u16>(&rec.adapter_indices)?;
            let gates_joined = join_json_slice::<i16>(&rec.gates_q15)?;
            let decision_hash = rec
                .decision_hash_json
                .as_ref()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                .and_then(|v| {
                    v.get("combined_hash")
                        .and_then(|c| c.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_default();

            let material = format!(
                "{}|{}|{}|{}|{}|{}",
                rec.step,
                rec.input_token_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(String::new),
                indices_joined,
                gates_joined,
                decision_hash,
                previous.as_deref().unwrap_or("")
            );

            let recomputed = adapteros_core::B3Hash::hash(material.as_bytes()).to_hex();
            if recomputed != rec.entry_hash {
                return Ok(ChainVerification {
                    is_valid: false,
                    entries_checked: idx + 1,
                    first_invalid_step: Some(rec.step),
                    error: Some("entry_hash mismatch".to_string()),
                });
            }

            previous = Some(rec.entry_hash.clone());
        }

        Ok(ChainVerification {
            is_valid: true,
            entries_checked: records.len(),
            first_invalid_step: None,
            error: None,
        })
    }
}

/// Helper to build a record from API-layer chain entry
pub fn make_chain_record_from_api(
    tenant_id: &str,
    inference_id: &str,
    request_id: Option<&str>,
    entry: &adapteros_api_types::inference::RouterDecisionChainEntry,
    decision_hash_json: Option<String>,
) -> RoutingDecisionChainRecord {
    RoutingDecisionChainRecord {
        id: Uuid::now_v7().to_string(),
        tenant_id: tenant_id.to_string(),
        inference_id: inference_id.to_string(),
        request_id: request_id.map(|s| s.to_string()),
        step: entry.step as i64,
        input_token_id: entry.input_token_id.map(|v| v as i64),
        adapter_indices: serde_json::to_string(&entry.adapter_indices).unwrap_or_default(),
        adapter_ids: serde_json::to_string(&entry.adapter_ids).unwrap_or_default(),
        gates_q15: serde_json::to_string(&entry.gates_q15).unwrap_or_default(),
        entropy: entry.entropy as f64,
        decision_hash_json,
        previous_hash: entry.previous_hash.clone(),
        entry_hash: entry.entry_hash.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Chain verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerification {
    pub is_valid: bool,
    pub entries_checked: usize,
    pub first_invalid_step: Option<i64>,
    pub error: Option<String>,
}

fn join_json_slice<T>(raw: &str) -> Result<String>
where
    T: serde::de::DeserializeOwned + ToString,
{
    let vals: Vec<T> = serde_json::from_str(raw)
        .map_err(|e| AosError::Validation(format!("Failed to parse chain field: {e}")))?;
    Ok(vals
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_and_fetch_chain_entries() {
        let db = Db::connect(":memory:").await.expect("create db");

        // Minimal schema setup (avoid full migration set to sidestep duplicate version conflicts in tests)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                allow_unrestricted_egress INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            "#,
        )
        .execute(db.pool())
        .await
        .expect("create tenants");

        // Apply routing decision chain schema directly
        sqlx::query(include_str!(
            "../../../migrations/0158_routing_decision_chain.sql"
        ))
        .execute(db.pool())
        .await
        .expect("create routing_decision_chain");

        let tenant_id = "routing-chain-tenant".to_string();
        sqlx::query("INSERT INTO tenants (id, name, allow_unrestricted_egress) VALUES (?, ?, 0)")
            .bind(&tenant_id)
            .bind("Routing Chain Tenant")
            .execute(db.pool())
            .await
            .expect("insert tenant");

        let entry = RoutingDecisionChainRecord {
            id: Uuid::now_v7().to_string(),
            tenant_id: tenant_id.clone(),
            inference_id: "req-123".to_string(),
            request_id: Some("req-123".to_string()),
            step: 0,
            input_token_id: Some(42),
            adapter_indices: "[0,1]".to_string(),
            adapter_ids: r#"["a","b"]"#.to_string(),
            gates_q15: "[100,200]".to_string(),
            entropy: 0.5,
            decision_hash_json: Some(r#"{"combined_hash":"abc"}"#.to_string()),
            previous_hash: None,
            entry_hash: "deadbeef".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        db.insert_routing_decision_chain_batch(&[entry.clone()])
            .await
            .expect("insert chain");

        let fetched = db
            .get_routing_decision_chain(&tenant_id, "req-123", None, None)
            .await
            .expect("fetch chain");

        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].step, entry.step);
        assert_eq!(fetched[0].entry_hash, entry.entry_hash);
    }
}
