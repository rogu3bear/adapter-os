use crate::Db;
use adapteros_core::{AosError, Result};
use sqlx::Row;

impl Db {
    pub async fn set_plugin_enable(
        &self,
        tenant_id: &str,
        plugin_name: &str,
        enabled: bool,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO plugin_tenant_enables 
            (tenant_id, plugin_name, enabled, updated_at) 
            VALUES (?, ?, ?, datetime('now'))
            "#,
        )
        .bind(tenant_id)
        .bind(plugin_name)
        .bind(enabled)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn get_plugin_enable(
        &self,
        tenant_id: &str,
        plugin_name: &str,
    ) -> Result<Option<bool>> {
        let row = sqlx::query(
            "SELECT enabled FROM plugin_tenant_enables WHERE tenant_id = ? AND plugin_name = ?",
        )
        .bind(tenant_id)
        .bind(plugin_name)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(row.map(|r| r.get("enabled")))
    }
}

