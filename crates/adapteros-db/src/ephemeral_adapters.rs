use crate::Db;
use adapteros_core::{AosError, Result};

/// Database row for ephemeral adapter
#[derive(Debug, sqlx::FromRow)]
pub struct EphemeralAdapterRow {
    pub id: String,
    pub adapter_data: String,
    pub created_at: String,
}

impl Db {
    /// Save an ephemeral adapter to the database
    pub async fn save_ephemeral_adapter(&self, id: &str, adapter_json: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO ephemeral_adapters (id, adapter_data) VALUES (?, ?)
             ON CONFLICT(id) DO UPDATE SET adapter_data = excluded.adapter_data",
        )
        .bind(id)
        .bind(adapter_json)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to save ephemeral adapter: {}", e)))?;
        Ok(())
    }

    /// Get an ephemeral adapter by ID
    pub async fn get_ephemeral_adapter(&self, id: &str) -> Result<Option<EphemeralAdapterRow>> {
        let row = sqlx::query_as::<_, EphemeralAdapterRow>(
            "SELECT id, adapter_data, created_at FROM ephemeral_adapters WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get ephemeral adapter: {}", e)))?;
        Ok(row)
    }

    /// List all ephemeral adapters
    pub async fn list_ephemeral_adapters(&self) -> Result<Vec<EphemeralAdapterRow>> {
        let rows = sqlx::query_as::<_, EphemeralAdapterRow>(
            "SELECT id, adapter_data, created_at FROM ephemeral_adapters ORDER BY created_at DESC",
        )
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list ephemeral adapters: {}", e)))?;
        Ok(rows)
    }

    /// Delete an ephemeral adapter by ID
    pub async fn delete_ephemeral_adapter(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM ephemeral_adapters WHERE id = ?")
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to delete ephemeral adapter: {}", e))
            })?;
        Ok(())
    }
}
