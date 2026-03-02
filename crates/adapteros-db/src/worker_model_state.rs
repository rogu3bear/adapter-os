//! Worker-scoped model lifecycle state and history.

use crate::{new_id, Db};
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerModelState {
    pub worker_id: String,
    pub tenant_id: String,
    pub active_model_id: Option<String>,
    pub active_model_hash_b3: Option<String>,
    pub desired_model_id: Option<String>,
    pub status: String,
    pub generation: i64,
    pub last_error: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerModelStatusHistory {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub model_id: Option<String>,
    pub from_status: Option<String>,
    pub to_status: String,
    pub reason: String,
    pub actor: Option<String>,
    pub valid_transition: i64,
    pub generation: i64,
    pub created_at: String,
}

fn normalize_model_status(status: &str) -> &'static str {
    match status {
        "loaded" | "ready" => "ready",
        "unloaded" | "no-model" | "none" => "no-model",
        "loading" | "checking" => "loading",
        "unloading" => "unloading",
        "error" => "error",
        _ => "no-model",
    }
}

/// Normalize model lifecycle status to canonical storage form.
pub fn normalize_worker_model_status(status: &str) -> &'static str {
    normalize_model_status(status)
}

fn is_valid_transition(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }

    matches!(
        (from, to),
        ("no-model", "loading")
            | ("no-model", "error")
            | ("loading", "ready")
            | ("loading", "error")
            | ("loading", "no-model")
            | ("loading", "unloading")
            | ("ready", "unloading")
            | ("ready", "loading")
            | ("ready", "error")
            | ("unloading", "no-model")
            | ("unloading", "error")
            | ("unloading", "loading")
            | ("error", "loading")
            | ("error", "no-model")
    )
}

impl Db {
    pub async fn get_worker_model_state(
        &self,
        worker_id: &str,
    ) -> Result<Option<WorkerModelState>> {
        let state = sqlx::query_as::<_, WorkerModelState>(
            "SELECT worker_id, tenant_id, active_model_id, active_model_hash_b3, desired_model_id, status, generation, last_error, memory_usage_mb, created_at, updated_at
             FROM worker_model_state
             WHERE worker_id = ?",
        )
        .bind(worker_id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker model state: {}", e)))?;

        Ok(state)
    }

    pub async fn list_worker_model_states_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<WorkerModelState>> {
        let states = sqlx::query_as::<_, WorkerModelState>(
            "SELECT worker_id, tenant_id, active_model_id, active_model_hash_b3, desired_model_id, status, generation, last_error, memory_usage_mb, created_at, updated_at
             FROM worker_model_state
             WHERE tenant_id = ?
             ORDER BY updated_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list worker model states: {}", e)))?;

        Ok(states)
    }

    pub async fn list_worker_model_states_for_model(
        &self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<Vec<WorkerModelState>> {
        let states = sqlx::query_as::<_, WorkerModelState>(
            "SELECT worker_id, tenant_id, active_model_id, active_model_hash_b3, desired_model_id, status, generation, last_error, memory_usage_mb, created_at, updated_at
             FROM worker_model_state
             WHERE tenant_id = ?
               AND (active_model_id = ? OR desired_model_id = ?)
             ORDER BY updated_at DESC",
        )
        .bind(tenant_id)
        .bind(model_id)
        .bind(model_id)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list worker model states for model: {}",
                e
            ))
        })?;

        Ok(states)
    }

    pub async fn list_ready_worker_ids_for_model(
        &self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT worker_id
             FROM worker_model_state
             WHERE tenant_id = ?
               AND active_model_id = ?
               AND status IN ('ready', 'loaded')
             ORDER BY worker_id ASC",
        )
        .bind(tenant_id)
        .bind(model_id)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to list ready worker IDs for model: {}", e))
        })?;

        Ok(rows.into_iter().map(|(worker_id,)| worker_id).collect())
    }

    pub async fn upsert_worker_model_state(
        &self,
        worker_id: &str,
        tenant_id: &str,
        active_model_id: Option<&str>,
        active_model_hash_b3: Option<&str>,
        desired_model_id: Option<&str>,
        status: &str,
        generation: i64,
        last_error: Option<&str>,
        memory_usage_mb: Option<i32>,
    ) -> Result<()> {
        let status = normalize_model_status(status);

        sqlx::query(
            "INSERT INTO worker_model_state (
                worker_id, tenant_id, active_model_id, active_model_hash_b3, desired_model_id,
                status, generation, last_error, memory_usage_mb, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(worker_id) DO UPDATE SET
                tenant_id = excluded.tenant_id,
                active_model_id = excluded.active_model_id,
                active_model_hash_b3 = excluded.active_model_hash_b3,
                desired_model_id = excluded.desired_model_id,
                status = excluded.status,
                generation = excluded.generation,
                last_error = excluded.last_error,
                memory_usage_mb = excluded.memory_usage_mb,
                updated_at = datetime('now')",
        )
        .bind(worker_id)
        .bind(tenant_id)
        .bind(active_model_id)
        .bind(active_model_hash_b3)
        .bind(desired_model_id)
        .bind(status)
        .bind(generation)
        .bind(last_error)
        .bind(memory_usage_mb)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to upsert worker model state: {}", e)))?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transition_worker_model_state(
        &self,
        worker_id: &str,
        to_status_raw: &str,
        reason: &str,
        actor: Option<&str>,
        active_model_id: Option<&str>,
        active_model_hash_b3: Option<&str>,
        desired_model_id: Option<&str>,
        last_error: Option<&str>,
        memory_usage_mb: Option<i32>,
    ) -> Result<()> {
        let mut tx = self.begin_write_tx().await?;

        let row = sqlx::query_as::<_, (String, Option<String>, i64)>(
            "SELECT tenant_id, status, generation FROM worker_model_state WHERE worker_id = ?",
        )
        .bind(worker_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch worker model state: {}", e)))?;

        let (tenant_id, from_status, generation) = if let Some(row) = row {
            row
        } else {
            let worker_row =
                sqlx::query_as::<_, (String,)>("SELECT tenant_id FROM workers WHERE id = ?")
                    .bind(worker_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to fetch worker tenant: {}", e))
                    })?
                    .ok_or_else(|| {
                        AosError::NotFound(format!("Worker not found: {}", worker_id))
                    })?;
            (worker_row.0, Some("no-model".to_string()), 0)
        };

        let from_status_norm = normalize_model_status(from_status.as_deref().unwrap_or("no-model"));
        let to_status = normalize_model_status(to_status_raw);
        let valid = is_valid_transition(from_status_norm, to_status);

        let next_generation = if from_status_norm == to_status {
            generation
        } else {
            generation.saturating_add(1)
        };

        let history_id = new_id(IdPrefix::Mdl);
        sqlx::query(
            "INSERT INTO worker_model_status_history (
                id, worker_id, tenant_id, model_id, from_status, to_status, reason,
                actor, valid_transition, generation, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&history_id)
        .bind(worker_id)
        .bind(&tenant_id)
        .bind(active_model_id.or(desired_model_id))
        .bind(from_status_norm)
        .bind(to_status)
        .bind(reason)
        .bind(actor)
        .bind(if valid { 1 } else { 0 })
        .bind(next_generation)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert worker model history: {}", e)))?;

        if !valid {
            tx.commit().await.map_err(|e| {
                AosError::Database(format!(
                    "Failed to commit worker model invalid transition: {}",
                    e
                ))
            })?;
            return Err(AosError::Lifecycle(format!(
                "Invalid worker model transition: {} -> {}",
                from_status_norm, to_status
            )));
        }

        sqlx::query(
            "INSERT INTO worker_model_state (
                worker_id, tenant_id, active_model_id, active_model_hash_b3,
                desired_model_id, status, generation, last_error, memory_usage_mb,
                created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(worker_id) DO UPDATE SET
                tenant_id = excluded.tenant_id,
                active_model_id = excluded.active_model_id,
                active_model_hash_b3 = excluded.active_model_hash_b3,
                desired_model_id = excluded.desired_model_id,
                status = excluded.status,
                generation = excluded.generation,
                last_error = excluded.last_error,
                memory_usage_mb = excluded.memory_usage_mb,
                updated_at = datetime('now')",
        )
        .bind(worker_id)
        .bind(&tenant_id)
        .bind(active_model_id)
        .bind(active_model_hash_b3)
        .bind(desired_model_id)
        .bind(to_status)
        .bind(next_generation)
        .bind(last_error)
        .bind(memory_usage_mb)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to upsert worker model state transition: {}",
                e
            ))
        })?;

        tx.commit().await.map_err(|e| {
            AosError::Database(format!("Failed to commit worker model transition: {}", e))
        })?;

        Ok(())
    }

    /// Reconcile stale `worker_model_state` rows at startup.
    ///
    /// Finds rows where a worker is missing or in a terminal status but the
    /// model state still claims an active model. Resets those to `no-model`
    /// with reason `"startup_reconciliation"`. Returns the count of reconciled rows.
    pub async fn reconcile_worker_model_states_at_startup(&self) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE worker_model_state
             SET status = 'no-model',
                 last_error = 'startup_reconciliation',
                 active_model_id = NULL,
                 active_model_hash_b3 = NULL,
                 desired_model_id = NULL,
                 updated_at = datetime('now')
             WHERE status NOT IN ('no-model', 'error')
               AND worker_id NOT IN (
                   SELECT id FROM workers
                   WHERE status NOT IN ('stopped', 'error', 'crashed', 'failed')
               )",
        )
        .execute(self.pool_result()?)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to reconcile worker model states at startup: {}",
                e
            ))
        })?;

        Ok(result.rows_affected())
    }

    pub async fn recompute_base_model_status_projection(
        &self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<String> {
        let rows = sqlx::query_as::<_, (String, Option<String>, Option<i32>)>(
            "SELECT status, last_error, memory_usage_mb
             FROM worker_model_state
             WHERE tenant_id = ?
               AND (active_model_id = ? OR desired_model_id = ?)",
        )
        .bind(tenant_id)
        .bind(model_id)
        .bind(model_id)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to read worker model states for projection: {}",
                e
            ))
        })?;

        let mut has_ready = false;
        let mut has_loading = false;
        let mut has_unloading = false;
        let mut has_error = false;
        let mut latest_error: Option<String> = None;
        let mut memory_usage_mb: Option<i32> = None;

        for (status, err, mem) in rows {
            match normalize_model_status(&status) {
                "ready" => has_ready = true,
                "loading" => has_loading = true,
                "unloading" => has_unloading = true,
                "error" => has_error = true,
                _ => {}
            }
            if latest_error.is_none() {
                latest_error = err;
            }
            if memory_usage_mb.is_none() {
                memory_usage_mb = mem;
            }
        }

        let projected_status = if has_ready {
            "ready"
        } else if has_loading {
            "loading"
        } else if has_unloading {
            "unloading"
        } else if has_error {
            "error"
        } else {
            "no-model"
        };

        // Enforce single-active loaded model per tenant before projection upsert.
        // Doing this first avoids unique-index conflicts when a different model is
        // currently marked ready for the same tenant.
        if projected_status == "ready" {
            sqlx::query(
                "UPDATE base_model_status
                 SET status = 'no-model',
                     error_message = NULL,
                     memory_usage_mb = NULL,
                     unloaded_at = datetime('now'),
                     updated_at = datetime('now')
                 WHERE tenant_id = ?
                   AND model_id != ?
                   AND status IN ('ready', 'loaded', 'loading', 'checking', 'unloading')",
            )
            .bind(tenant_id)
            .bind(model_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to demote other base model projections to no-model: {}",
                    e
                ))
            })?;
        }

        // Upsert compatibility projection row.
        let existing = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM base_model_status WHERE tenant_id = ? AND model_id = ?",
        )
        .bind(tenant_id)
        .bind(model_id)
        .fetch_one(self.pool_result()?)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to check base model projection existence: {}",
                e
            ))
        })?;

        if existing > 0 {
            sqlx::query(
                "UPDATE base_model_status
                 SET status = ?,
                     error_message = ?,
                     memory_usage_mb = ?,
                     loaded_at = CASE WHEN ? = 'ready' THEN datetime('now') ELSE loaded_at END,
                     unloaded_at = CASE WHEN ? = 'no-model' THEN datetime('now') ELSE unloaded_at END,
                     updated_at = datetime('now')
                 WHERE tenant_id = ? AND model_id = ?",
            )
            .bind(projected_status)
            .bind(latest_error)
            .bind(memory_usage_mb)
            .bind(projected_status)
            .bind(projected_status)
            .bind(tenant_id)
            .bind(model_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update base model projection: {}", e)))?;
        } else {
            let id = new_id(IdPrefix::Mdl);
            sqlx::query(
                "INSERT INTO base_model_status (
                    id, tenant_id, model_id, status, loaded_at, unloaded_at,
                    error_message, memory_usage_mb, created_at, updated_at
                 ) VALUES (
                    ?, ?, ?, ?,
                    CASE WHEN ? = 'ready' THEN datetime('now') ELSE NULL END,
                    CASE WHEN ? = 'no-model' THEN datetime('now') ELSE NULL END,
                    ?, ?, datetime('now'), datetime('now')
                 )",
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(model_id)
            .bind(projected_status)
            .bind(projected_status)
            .bind(projected_status)
            .bind(latest_error)
            .bind(memory_usage_mb)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to insert base model projection: {}", e))
            })?;
        }

        if projected_status == "ready" {
            // Ensure tenant/model ready row remains the most recent status record so
            // tenant-level lookups prefer the active ready model deterministically.
            sqlx::query(
                "UPDATE base_model_status
                 SET loaded_at = COALESCE(loaded_at, datetime('now')),
                     updated_at = datetime('now')
                 WHERE tenant_id = ?
                   AND model_id = ?
                   AND status IN ('ready', 'loaded')",
            )
            .bind(tenant_id)
            .bind(model_id)
            .execute(self.pool_result()?)
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to refresh active ready model projection timestamp: {}",
                    e
                ))
            })?;
        }

        Ok(projected_status.to_string())
    }
}
