use crate::new_id;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PinnedAdapter {
    pub id: String,
    pub tenant_id: String,
    pub adapter_pk: String,         // Stores adapters.id (PK)
    pub adapter_id: Option<String>, // External adapter_id from joined adapters table
    pub pinned_until: Option<String>,
    pub reason: String,
    pub pinned_at: String,
    pub pinned_by: Option<String>,
}

impl Db {
    /// Look up adapter PK (id) by (tenant_id, adapter_id) tuple
    ///
    /// CRITICAL: Must filter by tenant_id to prevent cross-tenant data leakage.
    /// The adapter_id is NOT globally unique - only unique per tenant.
    async fn get_adapter_pk(&self, tenant_id: &str, adapter_id: &str) -> Result<String> {
        let row = sqlx::query("SELECT id FROM adapters WHERE tenant_id = ? AND adapter_id = ?")
            .bind(tenant_id)
            .bind(adapter_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?
            .ok_or_else(|| {
                AosError::NotFound(format!("Adapter not found: {}:{}", tenant_id, adapter_id))
            })?;
        Ok(row.get(0))
    }

    /// Pin an adapter to prevent eviction
    pub async fn pin_adapter(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        pinned_until: Option<&str>,
        reason: &str,
        pinned_by: Option<&str>,
    ) -> Result<String> {
        // Look up adapter PK by (tenant_id, adapter_id) tuple
        let adapter_pk = self.get_adapter_pk(tenant_id, adapter_id).await?;

        // === ISSUE 3: Validate pin TTL is not in the past ===
        if let Some(until_str) = pinned_until {
            let expires_at = DateTime::parse_from_rfc3339(until_str)
                .map_err(|e| {
                    AosError::validation(format!(
                        "Invalid pinned_until timestamp format: {}. Expected RFC3339 (e.g., 2099-12-31T23:59:59Z)",
                        e
                    ))
                })?
                .with_timezone(&Utc);

            if expires_at <= Utc::now() {
                return Err(AosError::validation(
                    "Adapter pin TTL is in the past".to_string(),
                ));
            }
        }

        let id = new_id(IdPrefix::Adp);
        sqlx::query(
            "INSERT INTO pinned_adapters (id, tenant_id, adapter_pk, pinned_until, reason, pinned_by)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(tenant_id, adapter_pk) DO UPDATE SET
                pinned_until = excluded.pinned_until,
                reason = excluded.reason,
                pinned_by = excluded.pinned_by,
                pinned_at = datetime('now')"
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(&adapter_pk)
        .bind(pinned_until)
        .bind(reason)
        .bind(pinned_by)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(id)
    }

    /// Unpin an adapter to allow eviction
    pub async fn unpin_adapter(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        // Look up adapter PK by (tenant_id, adapter_id) tuple
        let adapter_pk = self.get_adapter_pk(tenant_id, adapter_id).await?;

        sqlx::query("DELETE FROM pinned_adapters WHERE tenant_id = ? AND adapter_pk = ?")
            .bind(tenant_id)
            .bind(&adapter_pk)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Check if an adapter is currently pinned
    pub async fn is_pinned(&self, tenant_id: &str, adapter_id: &str) -> Result<bool> {
        // Look up adapter PK by (tenant_id, adapter_id) tuple
        let adapter_pk = self.get_adapter_pk(tenant_id, adapter_id).await?;

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pinned_adapters
             WHERE tenant_id = ? AND adapter_pk = ?
             AND (pinned_until IS NULL OR pinned_until > datetime('now'))",
        )
        .bind(tenant_id)
        .bind(&adapter_pk)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(count > 0)
    }

    /// List all pinned adapters for a tenant
    ///
    /// Joins with adapters table to include external adapter_id for lookups.
    pub async fn list_pinned_adapters(&self, tenant_id: &str) -> Result<Vec<PinnedAdapter>> {
        let adapters = sqlx::query_as::<_, PinnedAdapter>(
            "SELECT p.id, p.tenant_id, p.adapter_pk, a.adapter_id, p.pinned_until, p.reason, p.pinned_at, p.pinned_by
             FROM pinned_adapters p
             LEFT JOIN adapters a ON p.adapter_pk = a.id
             WHERE p.tenant_id = ?
             AND (p.pinned_until IS NULL OR p.pinned_until > datetime('now'))
             ORDER BY p.pinned_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(adapters)
    }

    /// Clean up expired pins
    pub async fn cleanup_expired_pins(&self) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM pinned_adapters
             WHERE pinned_until IS NOT NULL AND pinned_until <= datetime('now')",
        )
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(result.rows_affected())
    }
}
