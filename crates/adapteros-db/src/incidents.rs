//! Worker incident management for tracking failures and anomalies.
//!
//! This module provides additional CRUD operations for the `worker_incidents` table.
//! Core types and methods are defined in the `workers` module and re-exported here.

use crate::Db;
use adapteros_core::{AosError, Result};
use sqlx::QueryBuilder;

// Re-export types from workers for convenience
pub use crate::workers::{WorkerIncident, WorkerIncidentType};

#[derive(Debug, Clone, Default)]
pub struct IncidentFilters {
    pub tenant_id: Option<String>,
    pub worker_id: Option<String>,
    pub incident_type: Option<WorkerIncidentType>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub fn parse_incident_type(value: &str) -> Result<WorkerIncidentType> {
    value.parse::<WorkerIncidentType>()
}

impl Db {
    /// Create a new worker incident record.
    pub async fn create_incident(
        &self,
        worker_id: &str,
        tenant_id: &str,
        incident_type: WorkerIncidentType,
        reason: &str,
        backtrace_snippet: Option<&str>,
        latency_at_incident_ms: Option<f64>,
    ) -> Result<String> {
        self.insert_worker_incident(
            worker_id,
            tenant_id,
            incident_type,
            reason,
            backtrace_snippet,
            latency_at_incident_ms,
        )
        .await
    }

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

    /// Get a single incident by ID (alias).
    pub async fn get_incident(&self, id: &str) -> Result<Option<WorkerIncident>> {
        self.get_worker_incident(id).await
    }

    /// List incidents with optional filters.
    pub async fn list_incidents(&self, filters: IncidentFilters) -> Result<Vec<WorkerIncident>> {
        let mut query = QueryBuilder::new(
            "SELECT id, worker_id, tenant_id, incident_type, reason, \
             backtrace_snippet, latency_at_incident_ms, created_at \
             FROM worker_incidents WHERE 1=1",
        );

        if let Some(tenant_id) = filters.tenant_id {
            query.push(" AND tenant_id = ").push_bind(tenant_id);
        }

        if let Some(worker_id) = filters.worker_id {
            query.push(" AND worker_id = ").push_bind(worker_id);
        }

        if let Some(incident_type) = filters.incident_type {
            query
                .push(" AND incident_type = ")
                .push_bind(incident_type.as_str());
        }

        query.push(" ORDER BY created_at DESC");

        if let Some(limit) = filters.limit {
            query.push(" LIMIT ").push_bind(limit);
        }

        if let Some(offset) = filters.offset {
            query.push(" OFFSET ").push_bind(offset);
        }

        let incidents = query
            .build_query_as::<WorkerIncident>()
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list incidents: {}", e)))?;

        Ok(incidents)
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
