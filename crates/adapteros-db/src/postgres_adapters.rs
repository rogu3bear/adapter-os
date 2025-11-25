//! PostgreSQL adapter management
//!
//! Implements adapter CRUD operations for PostgreSQL backend.

use crate::postgres::PostgresDb;
use adapteros_core::Result;
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
        .execute(&*self.pool())
        .await?;
        
        Ok(id)
    }

    /// Get adapter by ID
    pub async fn get_adapter(&self, id: &str) -> Result<Option<AdapterRow>> {
        let adapter = sqlx::query_as::<_, AdapterRow>(
            "SELECT id, tenant_id, name, rank, version, base_model, lora_config, weights_hash, status, created_at
             FROM adapters WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await?;
        
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
        .fetch_all(&*self.pool())
        .await?;
        
        Ok(adapters)
    }

    /// Update adapter status
    pub async fn update_adapter_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET status = $1 WHERE id = $2"
        )
        .bind(status)
        .bind(id)
        .execute(&*self.pool())
        .await?;
        
        Ok(())
    }

    /// Delete adapter (soft delete)
    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET status = 'deleted' WHERE id = $1"
        )
        .bind(id)
        .execute(&*self.pool())
        .await?;
        
        Ok(())
    }

    /// Get adapters by rank range
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
        .fetch_all(&*self.pool())
        .await?;
        
        Ok(adapters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_adapter_crud() {
        let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect");

        db.migrate().await.expect("Migration failed");

        // Create adapter
        let adapter_id = db
            .create_adapter(
                "default",
                "test-adapter",
                16,
                "qwen2.5-7b",
                r#"{"alpha": 0.5}"#,
                "abc123",
            )
            .await
            .expect("Failed to create adapter");

        // Get adapter
        let adapter = db
            .get_adapter(&adapter_id)
            .await
            .expect("Failed to get adapter")
            .expect("Adapter not found");

        assert_eq!(adapter.name, "test-adapter");
        assert_eq!(adapter.rank, 16);

        // List adapters
        let adapters = db
            .list_adapters("default")
            .await
            .expect("Failed to list adapters");

        assert!(!adapters.is_empty());

        // Update status
        db.update_adapter_status(&adapter_id, "inactive")
            .await
            .expect("Failed to update status");

        // Delete adapter
        db.delete_adapter(&adapter_id)
            .await
            .expect("Failed to delete adapter");

        db.close().await;
    }
}

