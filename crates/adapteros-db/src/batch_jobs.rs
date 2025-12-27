use crate::Db;
use adapteros_core::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Batch job record stored in database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BatchJobRecord {
    pub id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub status: String,
    pub total_items: i64,
    pub completed_items: i64,
    pub failed_items: i64,
    pub timeout_secs: i64,
    pub max_concurrent: i64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub metadata: Option<String>,
}

impl BatchJobRecord {
    /// Convert created_at to DateTime<Utc>
    pub fn created_at_datetime(&self) -> Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| adapteros_core::AosError::Validation(format!("Invalid created_at: {}", e)))
    }

    /// Convert started_at to DateTime<Utc>
    pub fn started_at_datetime(&self) -> Result<Option<DateTime<Utc>>> {
        match &self.started_at {
            Some(ts) => DateTime::parse_from_rfc3339(ts)
                .map(|dt| Some(dt.with_timezone(&Utc)))
                .map_err(|e| {
                    adapteros_core::AosError::Validation(format!("Invalid started_at: {}", e))
                }),
            None => Ok(None),
        }
    }

    /// Convert completed_at to DateTime<Utc>
    pub fn completed_at_datetime(&self) -> Result<Option<DateTime<Utc>>> {
        match &self.completed_at {
            Some(ts) => DateTime::parse_from_rfc3339(ts)
                .map(|dt| Some(dt.with_timezone(&Utc)))
                .map_err(|e| {
                    adapteros_core::AosError::Validation(format!("Invalid completed_at: {}", e))
                }),
            None => Ok(None),
        }
    }
}

/// Batch item record stored in database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BatchItemRecord {
    pub id: String,
    pub batch_job_id: String,
    pub item_id: String,
    pub status: String,
    pub request_json: String,
    pub response_json: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub latency_ms: Option<i64>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

impl BatchItemRecord {
    /// Convert created_at to DateTime<Utc>
    pub fn created_at_datetime(&self) -> Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| adapteros_core::AosError::Validation(format!("Invalid created_at: {}", e)))
    }

    /// Convert completed_at to DateTime<Utc>
    pub fn completed_at_datetime(&self) -> Result<Option<DateTime<Utc>>> {
        match &self.completed_at {
            Some(ts) => DateTime::parse_from_rfc3339(ts)
                .map(|dt| Some(dt.with_timezone(&Utc)))
                .map_err(|e| {
                    adapteros_core::AosError::Validation(format!("Invalid completed_at: {}", e))
                }),
            None => Ok(None),
        }
    }
}

/// Parameters for creating a batch job
#[derive(Debug, Clone)]
pub struct CreateBatchJobParams {
    pub tenant_id: String,
    pub user_id: String,
    pub total_items: i64,
    pub timeout_secs: i64,
    pub max_concurrent: i64,
}

/// Parameters for creating a batch item
#[derive(Debug, Clone)]
pub struct CreateBatchItemParams {
    pub batch_job_id: String,
    pub item_id: String,
    pub request_json: String,
}

impl Db {
    /// Create a new batch job
    ///
    /// ## Security: Tenant Isolation
    /// The tenant_id is bound as a parameter to ensure proper tenant isolation.
    pub async fn create_batch_job(&self, params: CreateBatchJobParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO batch_jobs (
                id, tenant_id, user_id, status, total_items, completed_items,
                failed_items, timeout_secs, max_concurrent, created_at
            ) VALUES (?, ?, ?, 'pending', ?, 0, 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.user_id)
        .bind(params.total_items)
        .bind(params.timeout_secs)
        .bind(params.max_concurrent)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create batch job: {}", e))
        })?;

        Ok(id)
    }

    /// Get batch job by ID and tenant
    ///
    /// ## Security: Tenant Isolation
    /// Both tenant_id and id are bound as parameters to ensure the job belongs to the tenant.
    pub async fn get_batch_job(&self, id: &str, tenant_id: &str) -> Result<Option<BatchJobRecord>> {
        let row = sqlx::query_as::<_, BatchJobRecord>(
            r#"
            SELECT id, tenant_id, user_id, status, total_items, completed_items,
                   failed_items, timeout_secs, max_concurrent, created_at,
                   started_at, completed_at, error_message, metadata
            FROM batch_jobs
            WHERE id = ? AND tenant_id = ?
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to fetch batch job: {}", e))
        })?;

        Ok(row)
    }

    /// Update batch job status and completion timestamp
    ///
    /// ## Security: Parameterized Query
    /// All parameters are properly bound to prevent SQL injection.
    pub async fn update_batch_job_status(
        &self,
        id: &str,
        status: &str,
        completed_at: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE batch_jobs
             SET status = ?, completed_at = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(completed_at.unwrap_or(&now))
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to update batch job status: {}", e))
        })?;

        Ok(())
    }

    /// Increment the completed items counter for a batch job
    ///
    /// ## Security: Parameterized Query
    /// The job ID is properly bound to prevent SQL injection.
    pub async fn increment_batch_completed(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE batch_jobs
             SET completed_items = completed_items + 1
             WHERE id = ?",
        )
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to increment batch completed items: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Increment the failed items counter for a batch job
    ///
    /// ## Security: Parameterized Query
    /// The job ID is properly bound to prevent SQL injection.
    pub async fn increment_batch_failed(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE batch_jobs
             SET failed_items = failed_items + 1
             WHERE id = ?",
        )
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!(
                "Failed to increment batch failed items: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Create batch items in bulk
    ///
    /// ## Security: Parameterized Query
    /// All values are properly bound using sqlx parameters to prevent SQL injection.
    pub async fn create_batch_items(&self, items: Vec<CreateBatchItemParams>) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        // Begin transaction for atomic bulk insert
        let mut tx = self.begin_write_tx().await?;

        let now = Utc::now().to_rfc3339();

        for item in items {
            let id = Uuid::now_v7().to_string();

            sqlx::query(
                "INSERT INTO batch_items (
                    id, batch_job_id, item_id, status, request_json, created_at
                ) VALUES (?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&id)
            .bind(&item.batch_job_id)
            .bind(&item.item_id)
            .bind(&item.request_json)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to create batch item: {}", e))
            })?;
        }

        // Commit transaction
        tx.commit().await.map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to commit batch items: {}", e))
        })?;

        Ok(())
    }

    /// Get all batch items for a job
    ///
    /// ## Security: Parameterized Query
    /// The batch_job_id is properly bound to prevent SQL injection.
    pub async fn get_batch_items(&self, batch_job_id: &str) -> Result<Vec<BatchItemRecord>> {
        let rows = sqlx::query_as::<_, BatchItemRecord>(
            r#"
            SELECT id, batch_job_id, item_id, status, request_json,
                   response_json, error_code, error_message, latency_ms,
                   created_at, completed_at
            FROM batch_items
            WHERE batch_job_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(batch_job_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to fetch batch items: {}", e))
        })?;

        Ok(rows)
    }

    /// Update batch item with result data
    ///
    /// ## Security: Parameterized Query
    /// All parameters are properly bound to prevent SQL injection.
    pub async fn update_batch_item_result(
        &self,
        id: &str,
        status: &str,
        response_json: Option<&str>,
        error_code: Option<&str>,
        error_message: Option<&str>,
        latency_ms: Option<i32>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE batch_items
             SET status = ?, response_json = ?, error_code = ?,
                 error_message = ?, latency_ms = ?, completed_at = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(response_json)
        .bind(error_code)
        .bind(error_message)
        .bind(latency_ms)
        .bind(&now)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to update batch item result: {}", e))
        })?;

        Ok(())
    }

    /// Update batch item status only (without result data)
    ///
    /// ## Security: Parameterized Query
    /// All parameters are properly bound to prevent SQL injection.
    pub async fn update_batch_item_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE batch_items
             SET status = ?
             WHERE id = ?",
        )
        .bind(status)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to update batch item status: {}", e))
        })?;

        Ok(())
    }
}
