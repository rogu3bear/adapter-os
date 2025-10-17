//! PostgreSQL adapter management
//!
//! Implements adapter CRUD operations for PostgreSQL backend.

use super::PostgresDb;
use adapteros_core::{AosContext, AosError, Result};
use uuid::Uuid;

/// Adapter row from PostgreSQL
#[derive(Debug, sqlx::FromRow)]
pub struct AdapterRow {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub rank: i32,
    pub version: i32,
    pub base_model: String,
    pub lora_config: String,
    pub weights_hash: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl PostgresDb {
    /// Create a new adapter
    pub async fn create_adapter(
        &self,
        tenant_id: &str,
        name: &str,
        rank: i32,
        base_model: &str,
        lora_config: &str,
        weights_hash: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        sqlx::query(
            "INSERT INTO adapters (id, tenant_id, name, rank, version, base_model, lora_config, weights_hash, status, created_at)
             VALUES ($1, $2, $3, $4, 1, $5, $6, $7, 'active', NOW())"
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(name)
        .bind(rank)
        .bind(base_model)
        .bind(lora_config)
        .bind(weights_hash)
        .execute(self.pool())
        .await
        .context("Failed to create adapter")
        .with_context(|| format!("tenant_id={tenant_id}, adapter_id={id}"))?;

        Ok(id)
    }

    /// Get adapter by ID
    pub async fn get_adapter(&self, id: &str) -> Result<Option<AdapterRow>> {
        let adapter = sqlx::query_as::<_, AdapterRow>(
            "SELECT id, tenant_id, name, rank, version, base_model, lora_config, weights_hash, status, created_at
             FROM adapters WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .context("Failed to fetch adapter by id")
        .with_context(|| format!("adapter_id={id}"))?;

        Ok(adapter)
    }

    /// List adapters for a tenant
    pub async fn list_adapters(&self, tenant_id: &str) -> Result<Vec<AdapterRow>> {
        let adapters = sqlx::query_as::<_, AdapterRow>(
            "SELECT id, tenant_id, name, rank, version, base_model, lora_config, weights_hash, status, created_at
             FROM adapters WHERE tenant_id = $1 AND status = 'active'
             ORDER BY rank DESC, created_at DESC"
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .context("Failed to list tenant adapters")
        .with_context(|| format!("tenant_id={tenant_id}"))?;

        Ok(adapters)
    }

    /// Update adapter status
    pub async fn update_adapter_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE adapters SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to update adapter status")
            .with_context(|| format!("adapter_id={id}, status={status}"))?;

        Ok(())
    }

    /// Delete adapter (soft delete)
    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE adapters SET status = 'deleted' WHERE id = $1")
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to mark adapter as deleted")
            .with_context(|| format!("adapter_id={id}"))?;

        Ok(())
    }

    /// Get adapters by rank range (5-tier hierarchy support)
    pub async fn get_adapters_by_rank(
        &self,
        tenant_id: &str,
        min_rank: i32,
        max_rank: i32,
    ) -> Result<Vec<AdapterRow>> {
        let adapters = sqlx::query_as::<_, AdapterRow>(
            "SELECT id, tenant_id, name, rank, version, base_model, lora_config, weights_hash, status, created_at
             FROM adapters 
             WHERE tenant_id = $1 AND rank BETWEEN $2 AND $3 AND status = 'active'
             ORDER BY rank DESC"
        )
        .bind(tenant_id)
        .bind(min_rank)
        .bind(max_rank)
        .fetch_all(self.pool())
        .await
        .context("Failed to fetch adapters by rank range")
        .with_context(|| format!("tenant_id={tenant_id}, min_rank={min_rank}, max_rank={max_rank}"))?;

        Ok(adapters)
    }
}
