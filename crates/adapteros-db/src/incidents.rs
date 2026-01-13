//! Worker incident management for tracking failures and anomalies.
//!
//! This module provides additional CRUD operations for the `worker_incidents` table.
//! Core types and methods are defined in the `workers` module and re-exported here.

use crate::Db;
use adapteros_core::{AosError, Result};

// Re-export types from workers for convenience
pub use crate::workers::{WorkerIncident, WorkerIncidentType};

impl Db {
    /// Get a single incident by ID.
    pub async fn get_worker_incident(&self, id: &str) -> Result<Option<WorkerIncident>> {
        let incident = sqlx::query_as::<_, WorkerIncident>(
            "SELECT id, worker_id, tenant_id, incident_type, reason,
                    backtrace_snippet, latency_at_incident_ms, created_at
             FROM worker_incidents
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker incident: {}", e)))?;

        Ok(incident)
    }

    /// List incidents by type across all workers.
    pub async fn list_incidents_by_type(
        &self,
        incident_type: WorkerIncidentType,
        limit: Option<i32>,
    ) -> Result<Vec<WorkerIncident>> {
        let limit = limit.unwrap_or(100);

        let incidents = sqlx::query_as::<_, WorkerIncident>(
            "SELECT id, worker_id, tenant_id, incident_type, reason,
                    backtrace_snippet, latency_at_incident_ms, created_at
             FROM worker_incidents
             WHERE incident_type = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(incident_type.as_str())
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list incidents by type: {}", e)))?;

        Ok(incidents)
    }

    /// Delete incidents older than the specified date (for retention cleanup).
    ///
    /// # Arguments
    /// * `before_date` - ISO 8601 datetime string; incidents created before this are deleted
    ///
    /// # Returns
    /// The number of deleted incidents.
    pub async fn delete_old_incidents(&self, before_date: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM worker_incidents WHERE created_at < ?")
            .bind(before_date)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete old incidents: {}", e)))?;

        Ok(result.rows_affected())
    }
}
