//! Enclave operation audit trail

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use crate::new_id;
use adapteros_id::IdPrefix;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnclaveOperation {
    pub id: String,
    pub timestamp: i64,
    pub operation: String,
    pub requester: Option<String>,
    pub artifact_hash: Option<String>,
    pub result: String,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationStats {
    pub operation: String,
    pub count: i64,
    pub success_count: i64,
    pub error_count: i64,
}

impl Db {
    /// Log an enclave operation to the audit trail
    pub async fn log_enclave_operation(
        &self,
        operation: &str,
        requester: Option<&str>,
        artifact_hash: Option<&str>,
        result: &str,
        error_message: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Evt);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO enclave_operations
             (id, timestamp, operation, requester, artifact_hash, result, error_message)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(timestamp)
        .bind(operation)
        .bind(requester)
        .bind(artifact_hash)
        .bind(result)
        .bind(error_message)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to log enclave operation: {}", e)))?;

        Ok(id)
    }

    /// List last N enclave operations
    pub async fn list_enclave_operations(&self, limit: i64) -> Result<Vec<EnclaveOperation>> {
        let operations = sqlx::query_as::<_, EnclaveOperation>(
            "SELECT id, timestamp, operation, requester, artifact_hash, result, error_message, created_at
             FROM enclave_operations
             ORDER BY timestamp DESC
             LIMIT ?"
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list enclave operations: {}", e)))?;

        Ok(operations)
    }

    /// List enclave operations filtered by operation type
    pub async fn list_enclave_operations_by_type(
        &self,
        operation: &str,
        limit: i64,
    ) -> Result<Vec<EnclaveOperation>> {
        let operations = sqlx::query_as::<_, EnclaveOperation>(
            "SELECT id, timestamp, operation, requester, artifact_hash, result, error_message, created_at
             FROM enclave_operations
             WHERE operation = ?
             ORDER BY timestamp DESC
             LIMIT ?"
        )
        .bind(operation)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list enclave operations by type: {}", e)))?;

        Ok(operations)
    }

    /// Get operation statistics by type
    pub async fn get_operation_stats(&self) -> Result<Vec<OperationStats>> {
        let rows = sqlx::query(
            "SELECT
                operation,
                COUNT(*) as count,
                SUM(CASE WHEN result = 'success' THEN 1 ELSE 0 END) as success_count,
                SUM(CASE WHEN result = 'error' THEN 1 ELSE 0 END) as error_count
             FROM enclave_operations
             GROUP BY operation
             ORDER BY count DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get operation stats: {}", e)))?;

        let stats = rows
            .into_iter()
            .map(|row| OperationStats {
                operation: row.get("operation"),
                count: row.get("count"),
                success_count: row.get("success_count"),
                error_count: row.get("error_count"),
            })
            .collect();

        Ok(stats)
    }

    /// Get total count of enclave operations
    pub async fn count_enclave_operations(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM enclave_operations")
            .fetch_one(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to count enclave operations: {}", e))
            })?;

        Ok(count)
    }
}
