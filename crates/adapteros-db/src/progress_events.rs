//! Progress event database operations
//!
//! Implements CRUD operations for progress tracking events.
//! Stores historical progress data with configurable retention.

use crate::Db;
use adapteros_core::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Progress event record stored in database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProgressEventRecord {
    pub id: String,
    pub operation_id: String,
    pub tenant_id: String,
    pub event_type: String,
    pub progress_pct: f64,
    pub status: String,
    pub message: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String, // TEXT datetime in SQLite
    pub updated_at: String, // TEXT datetime in SQLite
}

impl ProgressEventRecord {
    /// Convert created_at to DateTime<Utc>
    pub fn created_at_datetime(&self) -> Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| adapteros_core::AosError::Validation(format!("Invalid created_at: {}", e)))
    }

    /// Convert updated_at to DateTime<Utc>
    pub fn updated_at_datetime(&self) -> Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| adapteros_core::AosError::Validation(format!("Invalid updated_at: {}", e)))
    }
}

/// Query parameters for progress events
#[derive(Debug, Clone, Default)]
pub struct ProgressEventQuery {
    pub tenant_id: Option<String>,
    pub operation_id: Option<String>,
    pub event_type: Option<String>,
    pub status: Option<String>,
    pub min_progress: Option<f64>,
    pub max_progress: Option<f64>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Progress event statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressStats {
    pub total_events: i64,
    pub active_operations: i64,
    pub completed_operations: i64,
    pub failed_operations: i64,
    pub avg_completion_time_secs: Option<f64>,
}

impl Db {
    /// Create a new progress event
    pub async fn create_progress_event(
        &self,
        operation_id: &str,
        tenant_id: &str,
        event_type: &str,
        progress_pct: f64,
        status: &str,
        message: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO progress_events (
                id, operation_id, tenant_id, event_type, progress_pct,
                status, message, metadata, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(operation_id)
        .bind(tenant_id)
        .bind(event_type)
        .bind(progress_pct)
        .bind(status)
        .bind(message)
        .bind(metadata)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create progress event: {}", e))
        })?;

        Ok(id)
    }

    /// Get progress events with optional filtering
    ///
    /// ## Security: Parameterized Queries
    /// All filter values are properly bound using sqlx parameters (not string interpolation)
    /// to prevent SQL injection vulnerabilities.
    pub async fn get_progress_events(
        &self,
        query: ProgressEventQuery,
    ) -> Result<Vec<ProgressEventRecord>> {
        let mut sql = "SELECT id, operation_id, tenant_id, event_type, progress_pct, status, message, metadata, created_at, updated_at FROM progress_events WHERE 1=1".to_string();

        // Build dynamic query - all values will be bound as parameters
        if query.tenant_id.is_some() {
            sql.push_str(" AND tenant_id = ?");
        }
        if query.operation_id.is_some() {
            sql.push_str(" AND operation_id = ?");
        }
        if query.event_type.is_some() {
            sql.push_str(" AND event_type = ?");
        }
        if query.status.is_some() {
            sql.push_str(" AND status = ?");
        }
        if query.min_progress.is_some() {
            sql.push_str(" AND progress_pct >= ?");
        }
        if query.max_progress.is_some() {
            sql.push_str(" AND progress_pct <= ?");
        }
        if query.since.is_some() {
            sql.push_str(" AND created_at >= ?");
        }
        if query.until.is_some() {
            sql.push_str(" AND created_at <= ?");
        }

        sql.push_str(" ORDER BY updated_at DESC");

        if query.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        if query.offset.is_some() {
            sql.push_str(" OFFSET ?");
        }

        // Bind parameters in the exact order they appear in the SQL
        let mut q = sqlx::query_as::<_, ProgressEventRecord>(&sql);

        if let Some(tenant_id) = query.tenant_id {
            q = q.bind(tenant_id);
        }
        if let Some(operation_id) = query.operation_id {
            q = q.bind(operation_id);
        }
        if let Some(event_type) = query.event_type {
            q = q.bind(event_type);
        }
        if let Some(status) = query.status {
            q = q.bind(status);
        }
        if let Some(min_progress) = query.min_progress {
            q = q.bind(min_progress);
        }
        if let Some(max_progress) = query.max_progress {
            q = q.bind(max_progress);
        }
        if let Some(since) = query.since {
            q = q.bind(since.to_rfc3339());
        }
        if let Some(until) = query.until {
            q = q.bind(until.to_rfc3339());
        }
        if let Some(limit) = query.limit {
            q = q.bind(limit);
        }
        if let Some(offset) = query.offset {
            q = q.bind(offset);
        }

        let records = q.fetch_all(self.pool()).await.map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to query progress events: {}", e))
        })?;

        Ok(records)
    }

    /// Get progress statistics
    ///
    /// ## Security: Parameterized Query
    /// tenant_id is properly bound to prevent SQL injection.
    pub async fn get_progress_stats(&self, tenant_id: Option<&str>) -> Result<ProgressStats> {
        let row: (i64, i64, i64, i64) = if let Some(tid) = tenant_id {
            // Query with tenant filter
            let query_sql = "SELECT
                COUNT(*) as total_events,
                COUNT(CASE WHEN status = 'running' THEN 1 END) as active_operations,
                COUNT(CASE WHEN status = 'completed' THEN 1 END) as completed_operations,
                COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_operations
                FROM progress_events
                WHERE tenant_id = ?";

            sqlx::query_as(query_sql)
                .bind(tid)
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    adapteros_core::AosError::Database(format!("Failed to get progress stats: {}", e))
                })?
        } else {
            // Query without tenant filter
            let query_sql = "SELECT
                COUNT(*) as total_events,
                COUNT(CASE WHEN status = 'running' THEN 1 END) as active_operations,
                COUNT(CASE WHEN status = 'completed' THEN 1 END) as completed_operations,
                COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_operations
                FROM progress_events";

            sqlx::query_as(query_sql)
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    adapteros_core::AosError::Database(format!("Failed to get progress stats: {}", e))
                })?
        };

        // Calculate average completion time from timestamps
        // We calculate the difference between created_at and updated_at for completed operations
        let avg_completion_time_secs: Option<f64> = if let Some(tid) = tenant_id {
            let avg_result: (Option<f64>,) = sqlx::query_as(
                "SELECT AVG((julianday(updated_at) - julianday(created_at)) * 86400.0)
                 FROM progress_events
                 WHERE tenant_id = ? AND status = 'completed'"
            )
            .bind(tid)
            .fetch_one(self.pool())
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to get avg completion time: {}", e))
            })?;
            avg_result.0
        } else {
            let avg_result: (Option<f64>,) = sqlx::query_as(
                "SELECT AVG((julianday(updated_at) - julianday(created_at)) * 86400.0)
                 FROM progress_events
                 WHERE status = 'completed'"
            )
            .fetch_one(self.pool())
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to get avg completion time: {}", e))
            })?;
            avg_result.0
        };

        Ok(ProgressStats {
            total_events: row.0,
            active_operations: row.1,
            completed_operations: row.2,
            failed_operations: row.3,
            avg_completion_time_secs,
        })
    }

    /// Count active operations
    ///
    /// ## Security: Parameterized Query
    /// tenant_id is properly bound to prevent SQL injection.
    pub async fn count_active_operations(&self, tenant_id: Option<&str>) -> Result<i64> {
        let count: (i64,) = if let Some(tid) = tenant_id {
            sqlx::query_as("SELECT COUNT(*) FROM progress_events WHERE status = 'running' AND tenant_id = ?")
                .bind(tid)
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    adapteros_core::AosError::Database(format!(
                        "Failed to count active operations: {}",
                        e
                    ))
                })?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM progress_events WHERE status = 'running'")
                .fetch_one(self.pool())
                .await
                .map_err(|e| {
                    adapteros_core::AosError::Database(format!(
                        "Failed to count active operations: {}",
                        e
                    ))
                })?
        };

        Ok(count.0)
    }

    /// Update progress event status and progress
    pub async fn update_progress_event(
        &self,
        operation_id: &str,
        tenant_id: &str,
        progress_pct: f64,
        status: &str,
        message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE progress_events
             SET progress_pct = ?, status = ?, message = ?, updated_at = ?
             WHERE operation_id = ? AND tenant_id = ? AND status = 'running'",
        )
        .bind(progress_pct)
        .bind(status)
        .bind(message)
        .bind(&now)
        .bind(operation_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to update progress event: {}", e))
        })?;

        Ok(())
    }

    /// Delete old progress events (for cleanup)
    pub async fn delete_old_progress_events(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        let cutoff_str = cutoff.to_rfc3339();

        let result = sqlx::query("DELETE FROM progress_events WHERE updated_at < ?")
            .bind(&cutoff_str)
            .execute(self.pool())
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!(
                    "Failed to delete old progress events: {}",
                    e
                ))
            })?;

        Ok(result.rows_affected())
    }
}
