use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Persisted workspace active state keyed by tenant/workspace id.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceActiveState {
    pub tenant_id: String,
    pub active_base_model_id: Option<String>,
    pub active_plan_id: Option<String>,
    pub active_adapter_ids: Option<String>,
    pub manifest_hash_b3: Option<String>,
    pub updated_at: String,
}

impl Db {
    /// Fetch the active state for a tenant/workspace.
    pub async fn get_workspace_active_state(
        &self,
        tenant_id: &str,
    ) -> Result<Option<WorkspaceActiveState>> {
        let state = sqlx::query_as::<_, WorkspaceActiveState>(
            r#"SELECT tenant_id, active_base_model_id, active_plan_id, active_adapter_ids, manifest_hash_b3, updated_at
               FROM workspace_active_state WHERE tenant_id = ?"#,
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(state)
    }

    /// Upsert the active state for a tenant/workspace.
    ///
    /// All provided fields will be written. Passing `None` clears the field.
    pub async fn upsert_workspace_active_state(
        &self,
        tenant_id: &str,
        active_base_model_id: Option<&str>,
        active_plan_id: Option<&str>,
        active_adapter_ids: Option<&[String]>,
        manifest_hash_b3: Option<&str>,
    ) -> Result<WorkspaceActiveState> {
        let adapters_json = if let Some(ids) = active_adapter_ids {
            Some(
                serde_json::to_string(ids)
                    .map_err(|e| AosError::Validation(format!("invalid adapter ids: {}", e)))?,
            )
        } else {
            None
        };

        sqlx::query(
            r#"
            INSERT INTO workspace_active_state (tenant_id, active_base_model_id, active_plan_id, active_adapter_ids, manifest_hash_b3, updated_at)
            VALUES (?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(tenant_id) DO UPDATE SET
                active_base_model_id = excluded.active_base_model_id,
                active_plan_id = excluded.active_plan_id,
                active_adapter_ids = excluded.active_adapter_ids,
                manifest_hash_b3 = excluded.manifest_hash_b3,
                updated_at = datetime('now')
            "#,
        )
        .bind(tenant_id)
        .bind(active_base_model_id)
        .bind(active_plan_id)
        .bind(adapters_json.as_deref())
        .bind(manifest_hash_b3)
        .execute(self.pool())
        .await?;

        self.get_workspace_active_state(tenant_id)
            .await?
            .ok_or_else(|| {
                AosError::Database("Failed to read workspace active state after upsert".into())
            })
    }

    /// Set the active base model only if none is currently set.
    ///
    /// Returns true if the active base model was set by this call.
    pub async fn set_active_base_model_if_empty(
        &self,
        tenant_id: &str,
        model_id: &str,
        manifest_hash_b3: Option<&str>,
    ) -> Result<bool> {
        if let Some(current) = self.get_workspace_active_state(tenant_id).await? {
            if current.active_base_model_id.is_some() {
                return Ok(false);
            }

            let adapters: Option<Vec<String>> = current
                .active_adapter_ids
                .as_deref()
                .map(|s| serde_json::from_str(s))
                .transpose()
                .map_err(|e| AosError::Validation(format!("invalid adapter ids json: {}", e)))?;

            let plan = current.active_plan_id.as_deref();
            let manifest = manifest_hash_b3.or(current.manifest_hash_b3.as_deref());
            self.upsert_workspace_active_state(
                tenant_id,
                Some(model_id),
                plan,
                adapters.as_deref(),
                manifest,
            )
            .await?;
            return Ok(true);
        }

        self.upsert_workspace_active_state(tenant_id, Some(model_id), None, None, manifest_hash_b3)
            .await?;
        Ok(true)
    }

    /// Clear the active base model if it matches the provided model id.
    ///
    /// Returns true if an active model was cleared.
    pub async fn clear_active_base_model_if_matches(
        &self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE workspace_active_state
            SET active_base_model_id = NULL, updated_at = datetime('now')
            WHERE tenant_id = ? AND active_base_model_id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(model_id)
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// List all active workspace states.
    pub async fn list_workspace_active_states(&self) -> Result<Vec<WorkspaceActiveState>> {
        let records = sqlx::query_as::<_, WorkspaceActiveState>(
            r#"SELECT tenant_id, active_base_model_id, active_plan_id, active_adapter_ids, manifest_hash_b3, updated_at
               FROM workspace_active_state ORDER BY updated_at DESC"#,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(records)
    }
}
