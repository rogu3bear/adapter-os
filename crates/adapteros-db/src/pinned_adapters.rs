use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PinnedAdapter {
    pub id: String,
    pub tenant_id: String,
    pub adapter_id: String,
    pub pinned_until: Option<String>,
    pub reason: String,
    pub pinned_at: String,
    pub pinned_by: Option<String>,
}

impl Db {
    /// Pin an adapter to prevent eviction
    pub async fn pin_adapter(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        pinned_until: Option<&str>,
        reason: &str,
        pinned_by: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO pinned_adapters (id, tenant_id, adapter_id, pinned_until, reason, pinned_by)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(tenant_id, adapter_id) DO UPDATE SET
                pinned_until = excluded.pinned_until,
                reason = excluded.reason,
                pinned_by = excluded.pinned_by,
                pinned_at = datetime('now')"
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(pinned_until)
        .bind(reason)
        .bind(pinned_by)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    /// Unpin an adapter to allow eviction
    pub async fn unpin_adapter(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM pinned_adapters WHERE tenant_id = ? AND adapter_id = ?")
            .bind(tenant_id)
            .bind(adapter_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Check if an adapter is currently pinned
    pub async fn is_pinned(&self, tenant_id: &str, adapter_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pinned_adapters 
             WHERE tenant_id = ? AND adapter_id = ? 
             AND (pinned_until IS NULL OR pinned_until > datetime('now'))",
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .fetch_one(self.pool())
        .await?;
        Ok(count > 0)
    }

    /// List all pinned adapters for a tenant
    pub async fn list_pinned_adapters(&self, tenant_id: &str) -> Result<Vec<PinnedAdapter>> {
        let adapters = sqlx::query_as::<_, PinnedAdapter>(
            "SELECT id, tenant_id, adapter_id, pinned_until, reason, pinned_at, pinned_by 
             FROM pinned_adapters 
             WHERE tenant_id = ? 
             AND (pinned_until IS NULL OR pinned_until > datetime('now'))
             ORDER BY pinned_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await?;
        Ok(adapters)
    }

    /// Clean up expired pins
    pub async fn cleanup_expired_pins(&self) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM pinned_adapters 
             WHERE pinned_until IS NOT NULL AND pinned_until <= datetime('now')",
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }
}
