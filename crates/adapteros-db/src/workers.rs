use crate::{models::Worker, Db};
use adapteros_core::{AosError, Result, WorkerStatus};
use std::str::FromStr;

/// Builder for creating worker insertion parameters
#[derive(Debug, Default)]
pub struct WorkerInsertBuilder {
    id: Option<String>,
    tenant_id: Option<String>,
    node_id: Option<String>,
    plan_id: Option<String>,
    uds_path: Option<String>,
    pid: Option<i32>,
    status: Option<String>,
}

/// Parameters for worker insertion
#[derive(Debug)]
pub struct WorkerInsertParams {
    pub id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub pid: Option<i32>,
    pub status: String,
}

impl WorkerInsertBuilder {
    /// Create a new worker insertion builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the worker ID (required)
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the node ID (required)
    pub fn node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Set the plan ID (required)
    pub fn plan_id(mut self, plan_id: impl Into<String>) -> Self {
        self.plan_id = Some(plan_id.into());
        self
    }

    /// Set the UDS path (required)
    pub fn uds_path(mut self, uds_path: impl Into<String>) -> Self {
        self.uds_path = Some(uds_path.into());
        self
    }

    /// Set the process ID (optional)
    pub fn pid(mut self, pid: i32) -> Self {
        self.pid = Some(pid);
        self
    }

    /// Set the status (required)
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    /// Build the worker insertion parameters
    pub fn build(self) -> Result<WorkerInsertParams> {
        Ok(WorkerInsertParams {
            id: self
                .id
                .ok_or_else(|| AosError::Validation("id is required".to_string()))?,
            tenant_id: self
                .tenant_id
                .ok_or_else(|| AosError::Validation("tenant_id is required".to_string()))?,
            node_id: self
                .node_id
                .ok_or_else(|| AosError::Validation("node_id is required".to_string()))?,
            plan_id: self
                .plan_id
                .ok_or_else(|| AosError::Validation("plan_id is required".to_string()))?,
            uds_path: self
                .uds_path
                .ok_or_else(|| AosError::Validation("uds_path is required".to_string()))?,
            pid: self.pid,
            status: self
                .status
                .ok_or_else(|| AosError::Validation("status is required".to_string()))?,
        })
    }
}

impl Db {
    pub async fn list_workers_by_tenant(&self, tenant_id: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers WHERE tenant_id = ?"
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(workers)
    }

    pub async fn list_all_workers(&self) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers ORDER BY started_at DESC"
        )
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(workers)
    }

    pub async fn list_workers_by_node(&self, node_id: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers WHERE node_id = ? ORDER BY started_at DESC",
        )
        .bind(node_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(workers)
    }

    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE workers SET status = ?, last_seen_at = datetime('now') WHERE id = ?")
            .bind(status)
            .bind(worker_id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Insert a worker record
    ///
    /// Use [`WorkerInsertBuilder`] to construct worker parameters:
    /// ```no_run
    /// use adapteros_db::workers::WorkerInsertBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = WorkerInsertBuilder::new()
    ///     .id("worker-123")
    ///     .tenant_id("tenant-456")
    ///     .node_id("node-789")
    ///     .plan_id("plan-101")
    ///     .uds_path("./var/run/worker.sock")
    ///     .pid(12345)
    ///     .status("running")
    ///     .build()
    ///     .expect("required fields");
    /// db.insert_worker(params).await.expect("insert succeeds");
    /// # }
    /// ```
    pub async fn insert_worker(&self, params: WorkerInsertParams) -> Result<()> {
        sqlx::query(
            "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.node_id)
        .bind(&params.plan_id)
        .bind(&params.uds_path)
        .bind(params.pid)
        .bind(&params.status)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update worker heartbeat and optionally status
    pub async fn update_worker_heartbeat(&self, id: &str, status: Option<&str>) -> Result<()> {
        if let Some(st) = status {
            sqlx::query(
                "UPDATE workers SET status = ?, last_seen_at = datetime('now') WHERE id = ?",
            )
            .bind(st)
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        } else {
            sqlx::query("UPDATE workers SET last_seen_at = datetime('now') WHERE id = ?")
                .bind(id)
                .execute(&*self.pool())
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
        }
        Ok(())
    }

    /// Get a worker by ID
    pub async fn get_worker(&self, worker_id: &str) -> Result<Option<Worker>> {
        let worker = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(worker)
    }

    /// Check if a worker is currently running a training job
    pub async fn is_worker_training(&self, worker_id: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM training_jobs WHERE worker_id = ? AND status = 'running'",
        )
        .bind(worker_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check worker training status: {}", e)))?;

        Ok(count > 0)
    }

    /// Get count of requests processed by a worker
    ///
    /// Note: This assumes an inference_log or similar table exists.
    /// Returns 0 if the table doesn't exist.
    pub async fn get_worker_requests_count(&self, worker_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM routing_decisions WHERE worker_id = ?",
        )
        .bind(worker_id)
        .fetch_one(&*self.pool())
        .await
        .unwrap_or(0);

        Ok(count)
    }

    /// Get count of errors for a worker
    pub async fn get_worker_errors_count(&self, worker_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM audit_logs WHERE resource_id = ? AND status = 'error'",
        )
        .bind(worker_id)
        .fetch_one(&*self.pool())
        .await
        .unwrap_or(0);

        Ok(count)
    }

    /// Get average latency for a worker in milliseconds
    pub async fn get_worker_avg_latency(&self, worker_id: &str) -> Result<Option<f64>> {
        let avg = sqlx::query_scalar::<_, Option<f64>>(
            "SELECT AVG(latency_ms) FROM routing_decisions WHERE worker_id = ?",
        )
        .bind(worker_id)
        .fetch_one(&*self.pool())
        .await
        .unwrap_or(None);

        Ok(avg)
    }

    /// Get training tasks for a worker
    pub async fn get_worker_training_tasks(&self, worker_id: &str) -> Result<Vec<TrainingTask>> {
        let tasks = sqlx::query_as::<_, TrainingTask>(
            "SELECT id, worker_id, dataset_id, status, progress, created_at, updated_at
             FROM training_jobs
             WHERE worker_id = ?
             ORDER BY created_at DESC",
        )
        .bind(worker_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker training tasks: {}", e)))?;

        Ok(tasks)
    }

    /// Get detailed worker information by ID
    pub async fn get_worker_detail(&self, worker_id: &str) -> Result<Option<WorkerDetail>> {
        let worker = sqlx::query_as::<_, WorkerDetail>(
            "SELECT id, tenant_id, node_id, plan_id, status, pid, uds_path,
                    memory_headroom_pct, k_current, adapters_loaded_json,
                    started_at, last_heartbeat_at
             FROM workers WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker detail: {}", e)))?;

        Ok(worker)
    }

    /// Get count of telemetry events for a worker by event type
    pub async fn get_worker_telemetry_count(&self, worker_id: &str, event_type: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM telemetry_events
             WHERE worker_id = ? AND event_type = ?",
        )
        .bind(worker_id)
        .bind(event_type)
        .fetch_one(&*self.pool())
        .await
        .unwrap_or(0);

        Ok(count)
    }

    /// Get average latency for a worker from recent telemetry events
    pub async fn get_worker_avg_latency_recent(&self, worker_id: &str, minutes: i32) -> Result<Option<f64>> {
        let avg = sqlx::query_scalar::<_, Option<f64>>(
            "SELECT AVG(CAST(json_extract(payload, '$.latency_ms') AS REAL))
             FROM telemetry_events
             WHERE worker_id = ? AND event_type = 'inference_complete'
             AND timestamp > datetime('now', ? || ' minutes')",
        )
        .bind(worker_id)
        .bind(format!("-{}", minutes))
        .fetch_one(&*self.pool())
        .await
        .unwrap_or(None);

        Ok(avg)
    }

    /// Get active training tasks for a worker (running or pending)
    pub async fn get_worker_active_training_tasks(&self, worker_id: &str) -> Result<Vec<ActiveTrainingTask>> {
        let tasks = sqlx::query_as::<_, ActiveTrainingTask>(
            "SELECT id, 'training' as task_type, status, started_at, progress_pct
             FROM training_jobs
             WHERE worker_id = ? AND status IN ('running', 'pending')",
        )
        .bind(worker_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get active training tasks: {}", e)))?;

        Ok(tasks)
    }

    // =========================================================================
    // PRD-09: Worker Health Metrics
    // =========================================================================

    /// Update worker health metrics (called by WorkerHealthMonitor)
    pub async fn update_worker_health_metrics(
        &self,
        worker_id: &str,
        health_status: &str,
        avg_latency_ms: f64,
        latency_samples: i32,
        consecutive_slow_responses: i32,
        consecutive_failures: i32,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE workers SET
                health_status = ?,
                avg_latency_ms = ?,
                latency_samples = ?,
                last_response_at = datetime('now'),
                consecutive_slow_responses = ?,
                consecutive_failures = ?
             WHERE id = ?",
        )
        .bind(health_status)
        .bind(avg_latency_ms)
        .bind(latency_samples)
        .bind(consecutive_slow_responses)
        .bind(consecutive_failures)
        .bind(worker_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update worker health metrics: {}", e)))?;

        Ok(())
    }

    /// Get worker health metrics
    pub async fn get_worker_health(&self, worker_id: &str) -> Result<Option<WorkerHealthRecord>> {
        let record = sqlx::query_as::<_, WorkerHealthRecord>(
            "SELECT id, health_status, avg_latency_ms, latency_samples,
                    last_response_at, consecutive_slow_responses, consecutive_failures
             FROM workers WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker health: {}", e)))?;

        Ok(record)
    }

    /// List workers with health filtering
    pub async fn list_workers_by_health(&self, health_status: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at
             FROM workers WHERE health_status = ? ORDER BY started_at DESC",
        )
        .bind(health_status)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workers by health: {}", e)))?;

        Ok(workers)
    }

    /// List healthy workers for a tenant (for routing)
    pub async fn list_healthy_workers_by_tenant(&self, tenant_id: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at
             FROM workers
             WHERE tenant_id = ? AND health_status IN ('healthy', 'unknown')
             ORDER BY avg_latency_ms ASC NULLS LAST",
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list healthy workers: {}", e)))?;

        Ok(workers)
    }

    // =========================================================================
    // PRD-09: Worker Incidents
    // =========================================================================

    /// Insert a worker incident
    pub async fn insert_worker_incident(
        &self,
        worker_id: &str,
        tenant_id: &str,
        incident_type: &str,
        reason: &str,
        backtrace_snippet: Option<&str>,
        latency_at_incident_ms: Option<f64>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO worker_incidents
             (id, worker_id, tenant_id, incident_type, reason, backtrace_snippet, latency_at_incident_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(worker_id)
        .bind(tenant_id)
        .bind(incident_type)
        .bind(reason)
        .bind(backtrace_snippet)
        .bind(latency_at_incident_ms)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert worker incident: {}", e)))?;

        Ok(id)
    }

    /// List incidents for a worker
    pub async fn list_worker_incidents(
        &self,
        worker_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<WorkerIncident>> {
        let limit = limit.unwrap_or(50);

        let incidents = sqlx::query_as::<_, WorkerIncident>(
            "SELECT id, worker_id, tenant_id, incident_type, reason,
                    backtrace_snippet, latency_at_incident_ms, created_at
             FROM worker_incidents
             WHERE worker_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(worker_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list worker incidents: {}", e)))?;

        Ok(incidents)
    }

    /// List recent incidents for a tenant
    pub async fn list_tenant_worker_incidents(
        &self,
        tenant_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<WorkerIncident>> {
        let limit = limit.unwrap_or(100);

        let incidents = sqlx::query_as::<_, WorkerIncident>(
            "SELECT id, worker_id, tenant_id, incident_type, reason,
                    backtrace_snippet, latency_at_incident_ms, created_at
             FROM worker_incidents
             WHERE tenant_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list tenant incidents: {}", e)))?;

        Ok(incidents)
    }

    /// Get incident count for a worker
    pub async fn get_worker_incident_count(&self, worker_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM worker_incidents WHERE worker_id = ?",
        )
        .bind(worker_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count worker incidents: {}", e)))?;

        Ok(count)
    }

    /// Get recent incident count (last N hours)
    pub async fn get_recent_incident_count(&self, worker_id: &str, hours: i32) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM worker_incidents
             WHERE worker_id = ?
             AND created_at > datetime('now', ? || ' hours')",
        )
        .bind(worker_id)
        .bind(format!("-{}", hours))
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count recent incidents: {}", e)))?;

        Ok(count)
    }
}

/// Training task record for worker detail queries
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TrainingTask {
    pub id: String,
    pub worker_id: String,
    pub dataset_id: String,
    pub status: String,
    pub progress: Option<f64>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Detailed worker record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkerDetail {
    pub id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub status: String,
    pub pid: Option<i32>,
    pub uds_path: String,
    pub memory_headroom_pct: Option<f32>,
    pub k_current: Option<i32>,
    pub adapters_loaded_json: Option<String>,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
}

/// Active training task record
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActiveTrainingTask {
    pub id: String,
    pub task_type: String,
    pub status: String,
    pub started_at: Option<String>,
    pub progress_pct: Option<f32>,
}

/// Worker health metrics for PRD-09
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkerHealthRecord {
    pub id: String,
    pub health_status: String,
    pub avg_latency_ms: Option<f64>,
    pub latency_samples: Option<i32>,
    pub last_response_at: Option<String>,
    pub consecutive_slow_responses: Option<i32>,
    pub consecutive_failures: Option<i32>,
}

/// Worker incident record for PRD-09
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct WorkerIncident {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub incident_type: String,
    pub reason: String,
    pub backtrace_snippet: Option<String>,
    pub latency_at_incident_ms: Option<f64>,
    pub created_at: String,
}

// =========================================================================
// PRD-01: Worker Lifecycle & Manifest Binding
// =========================================================================

/// Worker with manifest binding fields for PRD-01
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkerWithBinding {
    pub id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub pid: Option<i32>,
    pub status: String,
    pub started_at: String,
    pub last_seen_at: Option<String>,
    pub manifest_hash_b3: Option<String>,
    pub schema_version: Option<String>,
    pub api_version: Option<String>,
    pub registered_at: Option<String>,
    pub health_status: Option<String>,
}

/// Worker status history record for PRD-01
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct WorkerStatusHistoryRecord {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub from_status: Option<String>,
    pub to_status: String,
    pub reason: String,
    pub actor: Option<String>,
    pub valid_transition: i32,
    pub created_at: String,
}

/// Parameters for worker registration (PRD-01)
#[derive(Debug, Clone)]
pub struct WorkerRegistrationParams {
    pub worker_id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub pid: i32,
    pub manifest_hash: String,
    pub schema_version: String,
    pub api_version: String,
}

impl Db {
    // =========================================================================
    // PRD-01: Worker Registration & Lifecycle
    // =========================================================================

    /// Register a worker with manifest binding (PRD-01)
    ///
    /// Inserts a new worker record with manifest hash and version information.
    /// Sets initial status to 'starting'.
    pub async fn register_worker(&self, params: WorkerRegistrationParams) -> Result<()> {
        sqlx::query(
            "INSERT INTO workers (
                id, tenant_id, node_id, plan_id, uds_path, pid, status,
                manifest_hash_b3, schema_version, api_version,
                started_at, registered_at
             ) VALUES (?, ?, ?, ?, ?, ?, 'starting', ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&params.worker_id)
        .bind(&params.tenant_id)
        .bind(&params.node_id)
        .bind(&params.plan_id)
        .bind(&params.uds_path)
        .bind(params.pid)
        .bind(&params.manifest_hash)
        .bind(&params.schema_version)
        .bind(&params.api_version)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to register worker: {}", e)))?;

        Ok(())
    }

    /// Transition worker status with validation and history (PRD-01)
    ///
    /// Validates the transition, records history, and optionally logs to audit_logs
    /// if the transition is invalid.
    ///
    /// # Arguments
    /// * `worker_id` - The worker ID
    /// * `new_status` - Target status (starting, serving, draining, stopped, crashed)
    /// * `reason` - Reason for the transition
    /// * `actor` - User or system that initiated (None for system)
    ///
    /// # Returns
    /// * `Ok(())` if transition is valid and applied
    /// * `Err(AosError::Lifecycle)` if transition is invalid
    pub async fn transition_worker_status(
        &self,
        worker_id: &str,
        new_status: &str,
        reason: &str,
        actor: Option<&str>,
    ) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Get current status and tenant_id
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT status, tenant_id FROM workers WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch worker: {}", e)))?;

        let (current_status, tenant_id) = row.ok_or_else(|| {
            AosError::NotFound(format!("Worker not found: {}", worker_id))
        })?;

        // Parse and validate transition
        let from_status = WorkerStatus::from_str(&current_status)
            .map_err(|e| AosError::Validation(format!("Invalid from status: {}", e)))?;
        let to_status = WorkerStatus::from_str(new_status)
            .map_err(|e| AosError::Validation(format!("Invalid to status: {}", e)))?;
        let is_valid = from_status.can_transition_to(to_status);

        // Generate history record ID
        let history_id = uuid::Uuid::now_v7().to_string();

        // Insert history record (regardless of validity)
        sqlx::query(
            "INSERT INTO worker_status_history
             (id, worker_id, tenant_id, from_status, to_status, reason, actor, valid_transition)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&history_id)
        .bind(worker_id)
        .bind(&tenant_id)
        .bind(&current_status)
        .bind(new_status)
        .bind(reason)
        .bind(actor)
        .bind(if is_valid { 1 } else { 0 })
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert status history: {}", e)))?;

        if !is_valid {
            // Log invalid transition to audit_logs as security/compliance event
            let audit_id = uuid::Uuid::now_v7().to_string();
            let details = serde_json::json!({
                "worker_id": worker_id,
                "from_status": current_status,
                "to_status": new_status,
                "reason": reason,
                "error": format!("Invalid transition: {} -> {}", current_status, new_status)
            });

            sqlx::query(
                "INSERT INTO audit_logs
                 (id, tenant_id, user_id, action, resource_type, resource_id, status, details)
                 VALUES (?, ?, ?, 'WorkerLifecycleViolation', 'worker', ?, 'error', ?)",
            )
            .bind(&audit_id)
            .bind(&tenant_id)
            .bind(actor.unwrap_or("system"))
            .bind(worker_id)
            .bind(details.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert audit log: {}", e)))?;

            tx.commit()
                .await
                .map_err(|e| AosError::Database(format!("Failed to commit: {}", e)))?;

            return Err(AosError::Lifecycle(format!(
                "Invalid worker transition: {} -> {}. Valid transitions from {}: {:?}",
                current_status,
                new_status,
                current_status,
                from_status.valid_transitions()
            )));
        }

        // Update worker status
        sqlx::query(
            "UPDATE workers SET
                status = ?,
                last_transition_at = datetime('now'),
                last_transition_reason = ?,
                last_seen_at = datetime('now')
             WHERE id = ?",
        )
        .bind(new_status)
        .bind(reason)
        .bind(worker_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update worker status: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit: {}", e)))?;

        Ok(())
    }

    /// List workers compatible with a specific manifest hash (PRD-01)
    ///
    /// Returns workers that:
    /// - Match the given manifest_hash_b3
    /// - Have status = 'serving'
    /// - Have health_status IN ('healthy', 'unknown') or NULL
    /// - Ordered by avg_latency_ms (lowest first)
    pub async fn list_compatible_workers(&self, manifest_hash: &str) -> Result<Vec<WorkerWithBinding>> {
        let workers = sqlx::query_as::<_, WorkerWithBinding>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at, manifest_hash_b3, schema_version,
                    api_version, registered_at, health_status
             FROM workers
             WHERE manifest_hash_b3 = ?
               AND status = 'serving'
               AND (health_status IS NULL OR health_status IN ('healthy', 'unknown'))
             ORDER BY avg_latency_ms ASC NULLS LAST",
        )
        .bind(manifest_hash)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list compatible workers: {}", e)))?;

        Ok(workers)
    }

    /// List serving workers (PRD-01)
    ///
    /// Returns all workers with status = 'serving' and healthy status.
    /// Used for routing when manifest matching is not required.
    pub async fn list_serving_workers(&self) -> Result<Vec<WorkerWithBinding>> {
        let workers = sqlx::query_as::<_, WorkerWithBinding>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at, manifest_hash_b3, schema_version,
                    api_version, registered_at, health_status
             FROM workers
             WHERE status = 'serving'
               AND (health_status IS NULL OR health_status IN ('healthy', 'unknown'))
             ORDER BY avg_latency_ms ASC NULLS LAST",
        )
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list serving workers: {}", e)))?;

        Ok(workers)
    }

    /// Get worker status history (PRD-01)
    pub async fn get_worker_status_history(
        &self,
        worker_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<WorkerStatusHistoryRecord>> {
        let limit = limit.unwrap_or(50);

        let history = sqlx::query_as::<_, WorkerStatusHistoryRecord>(
            "SELECT id, worker_id, tenant_id, from_status, to_status,
                    reason, actor, valid_transition, created_at
             FROM worker_status_history
             WHERE worker_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(worker_id)
        .bind(limit)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker status history: {}", e)))?;

        Ok(history)
    }

    /// Get worker with binding information (PRD-01)
    pub async fn get_worker_with_binding(&self, worker_id: &str) -> Result<Option<WorkerWithBinding>> {
        let worker = sqlx::query_as::<_, WorkerWithBinding>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at, manifest_hash_b3, schema_version,
                    api_version, registered_at, health_status
             FROM workers WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker with binding: {}", e)))?;

        Ok(worker)
    }

    /// Check if a worker exists with the given ID (PRD-01)
    pub async fn worker_exists(&self, worker_id: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workers WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to check worker existence: {}", e)))?;

        Ok(count > 0)
    }

    /// Count invalid transitions for a worker (PRD-01)
    pub async fn count_invalid_transitions(&self, worker_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM worker_status_history
             WHERE worker_id = ? AND valid_transition = 0",
        )
        .bind(worker_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count invalid transitions: {}", e)))?;

        Ok(count)
    }
}
