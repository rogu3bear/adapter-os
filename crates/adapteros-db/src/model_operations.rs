//! Model operations audit trail

use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModelOperation {
    pub id: String,
    pub tenant_id: String,
    pub model_id: String,
    pub operation: String,
    pub initiated_by: String,
    pub status: String,
    pub error_message: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
}

impl Db {
    /// Log a model operation to the audit trail
    pub async fn log_model_operation(
        &self,
        tenant_id: &str,
        model_id: &str,
        operation: &str,
        initiated_by: &str,
        status: &str,
        error_message: Option<&str>,
        started_at: &str,
        completed_at: Option<&str>,
        duration_ms: Option<i64>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        sqlx::query(
            "INSERT INTO model_operations 
             (id, tenant_id, model_id, operation, initiated_by, status, error_message, started_at, completed_at, duration_ms) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(model_id)
        .bind(operation)
        .bind(initiated_by)
        .bind(status)
        .bind(error_message)
        .bind(started_at)
        .bind(completed_at)
        .bind(duration_ms)
        .execute(self.pool())
        .await?;

        Ok(id)
    }

    /// Update an existing model operation status
    pub async fn update_model_operation(
        &self,
        operation_id: &str,
        status: &str,
        error_message: Option<&str>,
        completed_at: Option<&str>,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE model_operations 
             SET status = ?, error_message = ?, completed_at = ?, duration_ms = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(error_message)
        .bind(completed_at)
        .bind(duration_ms)
        .bind(operation_id)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    /// List last N model operations for a tenant
    pub async fn list_model_operations(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<ModelOperation>> {
        let operations = sqlx::query_as::<_, ModelOperation>(
            "SELECT id, tenant_id, model_id, operation, initiated_by, status, error_message, started_at, completed_at, duration_ms 
             FROM model_operations 
             WHERE tenant_id = ?
             ORDER BY started_at DESC 
             LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(operations)
    }

    /// List model operations for a specific model
    pub async fn list_model_operations_by_model(
        &self,
        tenant_id: &str,
        model_id: &str,
        limit: i64,
    ) -> Result<Vec<ModelOperation>> {
        let operations = sqlx::query_as::<_, ModelOperation>(
            "SELECT id, tenant_id, model_id, operation, initiated_by, status, error_message, started_at, completed_at, duration_ms 
             FROM model_operations 
             WHERE tenant_id = ? AND model_id = ?
             ORDER BY started_at DESC 
             LIMIT ?",
        )
        .bind(tenant_id)
        .bind(model_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(operations)
    }

    /// Get in-progress operations for a model
    pub async fn get_in_progress_operation(
        &self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<Option<ModelOperation>> {
        let operation = sqlx::query_as::<_, ModelOperation>(
            "SELECT id, tenant_id, model_id, operation, initiated_by, status, error_message, started_at, completed_at, duration_ms 
             FROM model_operations 
             WHERE tenant_id = ? AND model_id = ? AND status = 'in_progress'
             ORDER BY started_at DESC 
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(model_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(operation)
    }
}
