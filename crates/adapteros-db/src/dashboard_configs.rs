use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DashboardWidgetConfig {
    pub id: String,
    pub user_id: String,
    pub widget_id: String,
    pub enabled: bool,
    pub position: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl Db {
    /// Get all dashboard widget configurations for a user
    pub async fn get_dashboard_config(&self, user_id: &str) -> Result<Vec<DashboardWidgetConfig>> {
        let widgets = sqlx::query_as::<_, DashboardWidgetConfig>(
            "SELECT id, user_id, widget_id, enabled, position, created_at, updated_at
             FROM dashboard_configs
             WHERE user_id = ?
             ORDER BY position ASC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get dashboard config: {}", e)))?;

        Ok(widgets)
    }

    /// Upsert a single widget configuration (insert or update)
    pub async fn upsert_widget_config(
        &self,
        user_id: &str,
        widget_id: &str,
        enabled: bool,
        position: i32,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO dashboard_configs (id, user_id, widget_id, enabled, position, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, widget_id) DO UPDATE SET
                enabled = excluded.enabled,
                position = excluded.position,
                updated_at = excluded.updated_at"
        )
        .bind(&id)
        .bind(user_id)
        .bind(widget_id)
        .bind(enabled)
        .bind(position)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to upsert widget config: {}", e)))?;

        Ok(())
    }

    /// Update multiple widget configurations in a single transaction
    pub async fn update_dashboard_config(
        &self,
        user_id: &str,
        widgets: Vec<(String, bool, i32)>, // (widget_id, enabled, position)
    ) -> Result<usize> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("Failed to begin transaction: {}", e)))?;

        let mut updated_count = 0;

        for (widget_id, enabled, position) in widgets {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT INTO dashboard_configs (id, user_id, widget_id, enabled, position, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(user_id, widget_id) DO UPDATE SET
                    enabled = excluded.enabled,
                    position = excluded.position,
                    updated_at = excluded.updated_at"
            )
            .bind(&id)
            .bind(user_id)
            .bind(&widget_id)
            .bind(enabled)
            .bind(position)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update widget config: {}", e)))?;

            updated_count += 1;
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(updated_count)
    }

    /// Delete all dashboard configurations for a user (reset to defaults)
    pub async fn reset_dashboard_config(&self, user_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM dashboard_configs WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to reset dashboard config: {}", e)))?;

        Ok(())
    }

    /// Check if a user has any dashboard configuration
    pub async fn has_dashboard_config(&self, user_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dashboard_configs WHERE user_id = ?"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to check dashboard config: {}", e)))?;

        Ok(count > 0)
    }
}
