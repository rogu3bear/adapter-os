//! Worker lifecycle management with status transitions and health tracking.
//!
//! ## Worker Status Temporal Ordering (ANCHOR, AUDIT, RECTIFY)
//!
//! - **ANCHOR**: DB trigger (0278) + Rust validation ensures monotonic timestamps
//! - **AUDIT**: Tracks `temporal_ordering_violations` counter for monitoring
//! - **RECTIFY**: Invalid temporal ordering aborts insert with clear error message

use crate::{models::Worker, Db};
use adapteros_core::{AosError, Result, WorkerStatus};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, error, warn};

/// AUDIT: Global counter for temporal ordering violations
static TEMPORAL_ORDERING_VIOLATIONS: AtomicU64 = AtomicU64::new(0);

/// Get count of temporal ordering violations
pub fn temporal_ordering_violations() -> u64 {
    TEMPORAL_ORDERING_VIOLATIONS.load(Ordering::Relaxed)
}

/// Valid worker incident types matching the database CHECK constraint.
///
/// The database constraint in `migrations/0125_worker_health_metrics.sql` enforces:
/// `CHECK(incident_type IN ('fatal', 'crash', 'hung', 'degraded', 'recovered'))`
///
/// Using this enum ensures compile-time validation of incident types,
/// preventing runtime CHECK constraint violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerIncidentType {
    /// Worker encountered a fatal error (panic, unrecoverable error)
    Fatal,
    /// Worker process crashed or became unresponsive
    Crash,
    /// Worker is hung (not responding to health checks)
    Hung,
    /// Worker is degraded (responding slowly, above latency threshold)
    Degraded,
    /// Worker recovered from a degraded or crashed state
    Recovered,
}

impl WorkerIncidentType {
    /// Returns the string representation matching the database CHECK constraint.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fatal => "fatal",
            Self::Crash => "crash",
            Self::Hung => "hung",
            Self::Degraded => "degraded",
            Self::Recovered => "recovered",
        }
    }

    /// All valid incident types (for validation and documentation).
    pub const ALL: &'static [WorkerIncidentType] = &[
        Self::Fatal,
        Self::Crash,
        Self::Hung,
        Self::Degraded,
        Self::Recovered,
    ];
}

impl std::fmt::Display for WorkerIncidentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for WorkerIncidentType {
    type Err = AosError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fatal" => Ok(Self::Fatal),
            "crash" => Ok(Self::Crash),
            "hung" => Ok(Self::Hung),
            "degraded" => Ok(Self::Degraded),
            "recovered" => Ok(Self::Recovered),
            _ => Err(AosError::Validation(format!(
                "Invalid incident_type '{}'. Must be one of: fatal, crash, hung, degraded, recovered",
                s
            ))),
        }
    }
}

// SQLx integration for WorkerIncidentType
// Allows the enum to be used directly in sqlx queries and FromRow structs

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for WorkerIncidentType {
    fn decode(
        value: sqlx::sqlite::SqliteValueRef<'r>,
    ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        s.parse::<WorkerIncidentType>()
            .map_err(|e| Box::new(e) as sqlx::error::BoxDynError)
    }
}

impl sqlx::Type<sqlx::Sqlite> for WorkerIncidentType {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <&str as sqlx::Type<sqlx::Sqlite>>::type_info()
    }

    fn compatible(ty: &sqlx::sqlite::SqliteTypeInfo) -> bool {
        <&str as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for WorkerIncidentType {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&str as sqlx::Encode<sqlx::Sqlite>>::encode_by_ref(&self.as_str(), buf)
    }
}

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
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers
             WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(workers)
    }

    pub async fn list_all_workers(&self) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers
             ORDER BY started_at DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(workers)
    }

    pub async fn list_workers_by_node(&self, node_id: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers
             WHERE node_id = ?
             ORDER BY started_at DESC",
        )
        .bind(node_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(workers)
    }

    // NOTE: update_worker_status() was removed in PRD-RECT topology fix.
    // Use transition_worker_status() for all status changes - it enforces the state machine.

    /// List all active workers (status = 'active')
    pub async fn list_active_workers(&self) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers
             WHERE status = 'active'
             ORDER BY started_at DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(workers)
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
    ///     .uds_path("var/run/worker.sock")
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Pre-register a worker with 'pending' status before socket bind.
    /// Prevents `/readyz` race where worker process started but socket not yet bound.
    pub async fn pre_register_worker(
        &self,
        worker_id: &str,
        tenant_id: &str,
        node_id: &str,
        plan_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO workers (id, tenant_id, node_id, plan_id, status, started_at) \
             VALUES (?, ?, ?, ?, 'pending', datetime('now'))",
        )
        .bind(worker_id)
        .bind(tenant_id)
        .bind(node_id)
        .bind(plan_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to pre-register worker {}: {}",
                worker_id, e
            ))
        })?;

        debug!(
            worker_id = %worker_id,
            tenant_id = %tenant_id,
            node_id = %node_id,
            "Pre-registered worker with pending status"
        );

        Ok(())
    }

    /// Update worker heartbeat and optionally status/tokenizer metadata
    pub async fn update_worker_heartbeat(
        &self,
        id: &str,
        status: Option<&str>,
        tokenizer_hash_b3: Option<&str>,
        tokenizer_vocab_size: Option<i64>,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE workers SET
                status = COALESCE(?, status),
                tokenizer_hash_b3 = COALESCE(?, tokenizer_hash_b3),
                tokenizer_vocab_size = COALESCE(?, tokenizer_vocab_size),
                last_seen_at = datetime('now')
             WHERE id = ?",
        )
        .bind(status)
        .bind(tokenizer_hash_b3)
        .bind(tokenizer_vocab_size)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(AosError::NotFound(format!("Worker not found: {}", id)));
        }
        Ok(())
    }

    /// Get a worker by ID
    pub async fn get_worker(&self, worker_id: &str) -> Result<Option<Worker>> {
        let worker = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers
             WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(worker)
    }

    /// Get a worker by ID with tenant scoping (PRD-RECT-002)
    ///
    /// Returns None if the worker doesn't exist OR belongs to a different tenant,
    /// preventing cross-tenant enumeration attacks.
    pub async fn get_worker_for_tenant(
        &self,
        tenant_id: &str,
        worker_id: &str,
    ) -> Result<Option<Worker>> {
        let worker = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(worker_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
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
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to check worker training status: {}", e))
        })?;

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
        .fetch_one(self.pool())
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
        .fetch_one(self.pool())
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
        .fetch_one(self.pool())
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
        .fetch_all(self.pool())
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
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker detail: {}", e)))?;

        Ok(worker)
    }

    /// Get count of telemetry events for a worker by event type
    pub async fn get_worker_telemetry_count(
        &self,
        worker_id: &str,
        event_type: &str,
    ) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM telemetry_events
             WHERE worker_id = ? AND event_type = ?",
        )
        .bind(worker_id)
        .bind(event_type)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0);

        Ok(count)
    }

    /// Get average latency for a worker from recent telemetry events
    pub async fn get_worker_avg_latency_recent(
        &self,
        worker_id: &str,
        minutes: i32,
    ) -> Result<Option<f64>> {
        let avg = sqlx::query_scalar::<_, Option<f64>>(
            "SELECT AVG(CAST(json_extract(payload, '$.latency_ms') AS REAL))
             FROM telemetry_events
             WHERE worker_id = ? AND event_type = 'inference_complete'
             AND timestamp > datetime('now', ? || ' minutes')",
        )
        .bind(worker_id)
        .bind(format!("-{}", minutes))
        .fetch_one(self.pool())
        .await
        .unwrap_or(None);

        Ok(avg)
    }

    /// Get active training tasks for a worker (running or pending)
    pub async fn get_worker_active_training_tasks(
        &self,
        worker_id: &str,
    ) -> Result<Vec<ActiveTrainingTask>> {
        let tasks = sqlx::query_as::<_, ActiveTrainingTask>(
            "SELECT id, 'training' as task_type, status, started_at, progress_pct
             FROM training_jobs
             WHERE worker_id = ? AND status IN ('running', 'pending')",
        )
        .bind(worker_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get active training tasks: {}", e)))?;

        Ok(tasks)
    }

    // =========================================================================
    // Worker Health Monitoring & Hung Detection
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
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update worker health metrics: {}", e))
        })?;

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
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker health: {}", e)))?;

        Ok(record)
    }

    /// List workers with health filtering
    pub async fn list_workers_by_health(&self, health_status: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers WHERE health_status = ? ORDER BY started_at DESC",
        )
        .bind(health_status)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workers by health: {}", e)))?;

        Ok(workers)
    }

    /// List healthy workers for a tenant (for routing)
    pub async fn list_healthy_workers_by_tenant(&self, tenant_id: &str) -> Result<Vec<Worker>> {
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3, capabilities_json, tokenizer_hash_b3, tokenizer_vocab_size
             FROM workers
             WHERE tenant_id = ? AND health_status IN ('healthy', 'unknown')
             ORDER BY avg_latency_ms ASC NULLS LAST",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list healthy workers: {}", e)))?;

        Ok(workers)
    }

    // =========================================================================
    // Worker Incident Tracking
    // =========================================================================

    /// Insert a worker incident
    ///
    /// Uses [`WorkerIncidentType`] enum to ensure compile-time validation of incident types,
    /// preventing database CHECK constraint violations.
    ///
    /// # Arguments
    /// * `worker_id` - The worker ID
    /// * `tenant_id` - The tenant ID
    /// * `incident_type` - The type of incident (uses enum for type safety)
    /// * `reason` - Human-readable description of what happened
    /// * `backtrace_snippet` - Optional backtrace for debugging
    /// * `latency_at_incident_ms` - Optional latency measurement at time of incident
    ///
    /// # Example
    /// ```ignore
    /// use adapteros_db::workers::WorkerIncidentType;
    ///
    /// db.insert_worker_incident(
    ///     "worker-123",
    ///     "tenant-456",
    ///     WorkerIncidentType::Fatal,
    ///     "PANIC: Out of memory",
    ///     Some("at src/inference.rs:42"),
    ///     None,
    /// ).await?;
    /// ```
    pub async fn insert_worker_incident(
        &self,
        worker_id: &str,
        tenant_id: &str,
        incident_type: WorkerIncidentType,
        reason: &str,
        backtrace_snippet: Option<&str>,
        latency_at_incident_ms: Option<f64>,
    ) -> Result<String> {
        let id = crate::new_id(adapteros_id::IdPrefix::Wrk);

        sqlx::query(
            "INSERT INTO worker_incidents
             (id, worker_id, tenant_id, incident_type, reason, backtrace_snippet, latency_at_incident_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(worker_id)
        .bind(tenant_id)
        .bind(incident_type.as_str())
        .bind(reason)
        .bind(backtrace_snippet)
        .bind(latency_at_incident_ms)
        .execute(self.pool())
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
        .fetch_all(self.pool())
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
        .fetch_all(self.pool())
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
        .fetch_one(self.pool())
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
        .fetch_one(self.pool())
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

/// Worker health metrics for health monitoring and hung detection
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

/// Worker incident record for tracking worker failures and anomalies
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct WorkerIncident {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    /// Strongly-typed incident type (fatal, crash, hung, degraded, recovered).
    /// Automatically decoded from database TEXT column via sqlx.
    pub incident_type: WorkerIncidentType,
    pub reason: String,
    pub backtrace_snippet: Option<String>,
    pub latency_at_incident_ms: Option<f64>,
    pub created_at: String,
}

// =========================================================================
// Worker Lifecycle & Manifest Binding
// =========================================================================

/// Check if worker schema version is compatible with control plane schema
///
/// Compatibility rule: major.minor must match, patch version is ignored.
/// This allows workers and control planes with different patch versions to
/// interoperate while ensuring breaking changes (major/minor) are caught.
///
/// # Examples
///
/// ```
/// use adapteros_db::workers::is_schema_compatible;
///
/// assert!(is_schema_compatible("1.0.0", "1.0.5"));  // patch ignored
/// assert!(is_schema_compatible("1.0", "1.0.0"));   // missing patch OK
/// assert!(!is_schema_compatible("1.0.0", "1.1.0")); // minor mismatch
/// assert!(!is_schema_compatible("1.0.0", "2.0.0")); // major mismatch
/// ```
pub fn is_schema_compatible(worker_version: &str, cp_version: &str) -> bool {
    let parse_major_minor = |v: &str| -> Option<(u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        let major = parts.first()?.parse::<u32>().ok()?;
        let minor = parts
            .get(1)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        Some((major, minor))
    };

    match (
        parse_major_minor(worker_version),
        parse_major_minor(cp_version),
    ) {
        (Some((w_maj, w_min)), Some((cp_maj, cp_min))) => w_maj == cp_maj && w_min == cp_min,
        _ => false,
    }
}

/// Worker with manifest binding fields for lifecycle tracking
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
    pub backend: Option<String>,
    pub model_hash_b3: Option<String>,
    pub capabilities_json: Option<String>,
    pub schema_version: Option<String>,
    pub api_version: Option<String>,
    pub registered_at: Option<String>,
    pub health_status: Option<String>,
}

/// Worker status history record for lifecycle audit trail
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

/// Parameters for worker registration with manifest binding
#[derive(Debug, Clone)]
pub struct WorkerRegistrationParams {
    pub worker_id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub pid: i32,
    pub manifest_hash: String,
    pub backend: Option<String>,
    pub model_hash_b3: Option<String>,
    pub capabilities_json: Option<String>,
    pub tokenizer_hash_b3: Option<String>,
    pub tokenizer_vocab_size: Option<i64>,
    pub schema_version: String,
    pub api_version: String,
}

impl Db {
    // =========================================================================
    // Worker Registration & Lifecycle Management
    // =========================================================================

    /// Register a worker with manifest binding
    ///
    /// Inserts or updates a worker record with manifest hash and version information,
    /// then transitions lifecycle to `registered`.
    pub async fn register_worker(&self, params: WorkerRegistrationParams) -> Result<()> {
        // Normalize common path artifacts (e.g. "/./") so we don't treat the same
        // socket path as different workers across restarts.
        let uds_path_norm = params.uds_path.replace("/./", "/");

        let mut tx = self.begin_write_tx().await?;

        // If a worker re-registers on the same UDS path with a new ID, the older
        // records become aliases for a single live socket. Retire them so routing
        // doesn't select a stale worker_id.
        //
        // Note: We do this by *exact string match after simple normalization*;
        // we intentionally avoid filesystem canonicalization because the socket
        // may not exist yet at registration time.
        let superseded_worker_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM workers\n             WHERE tenant_id = ?\n               AND replace(uds_path, '/./', '/') = ?\n               AND id != ?\n               AND status IN ('pending', 'created', 'registered', 'healthy', 'draining')",
        )
        .bind(&params.tenant_id)
        .bind(&uds_path_norm)
        .bind(&params.worker_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list superseded workers for uds_path {}: {}",
                uds_path_norm, e
            ))
        })?;

        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workers WHERE id = ?")
            .bind(&params.worker_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to check worker existence: {}", e)))?;

        if exists == 0 {
            sqlx::query(
                "INSERT INTO workers (
                    id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    manifest_hash_b3, backend, model_hash_b3, capabilities_json,
                    tokenizer_hash_b3, tokenizer_vocab_size,
                    schema_version, api_version,
                    started_at, registered_at
                 ) VALUES (?, ?, ?, ?, ?, ?, 'created', ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), NULL)",
            )
            .bind(&params.worker_id)
            .bind(&params.tenant_id)
            .bind(&params.node_id)
            .bind(&params.plan_id)
            .bind(&uds_path_norm)
            .bind(params.pid)
            .bind(&params.manifest_hash)
            .bind(&params.backend)
            .bind(&params.model_hash_b3)
            .bind(&params.capabilities_json)
            .bind(&params.tokenizer_hash_b3)
            .bind(params.tokenizer_vocab_size)
            .bind(&params.schema_version)
            .bind(&params.api_version)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to register worker: {}", e)))?;
        } else {
            sqlx::query(
                "UPDATE workers
                 SET tenant_id = ?, node_id = ?, plan_id = ?, uds_path = ?, pid = ?,
                     manifest_hash_b3 = ?, backend = ?, model_hash_b3 = ?, capabilities_json = ?,
                     tokenizer_hash_b3 = ?, tokenizer_vocab_size = ?,
                     schema_version = ?, api_version = ?
                 WHERE id = ?",
            )
            .bind(&params.tenant_id)
            .bind(&params.node_id)
            .bind(&params.plan_id)
            .bind(&uds_path_norm)
            .bind(params.pid)
            .bind(&params.manifest_hash)
            .bind(&params.backend)
            .bind(&params.model_hash_b3)
            .bind(&params.capabilities_json)
            .bind(&params.tokenizer_hash_b3)
            .bind(params.tokenizer_vocab_size)
            .bind(&params.schema_version)
            .bind(&params.api_version)
            .bind(&params.worker_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update worker metadata: {}", e)))?;
        }

        tx.commit().await.map_err(|e| {
            AosError::Database(format!("Failed to commit registration metadata: {}", e))
        })?;

        // Ensure lifecycle moves to registered and stamp registration time
        self.transition_worker_status(
            &params.worker_id,
            adapteros_core::WorkerStatus::Registered.as_str(),
            "registration accepted",
            None,
        )
        .await?;

        sqlx::query("UPDATE workers SET registered_at = COALESCE(registered_at, datetime('now')) WHERE id = ?")
            .bind(&params.worker_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to stamp registered_at: {}", e)))?;

        // Best-effort: retire superseded workers that share the same UDS path.
        // This prevents routing from selecting a stale worker_id that happens
        // to have low latency metrics.
        for wid in superseded_worker_ids {
            if let Err(e) = self
                .transition_worker_status(
                    &wid,
                    adapteros_core::WorkerStatus::Error.as_str(),
                    "superseded_by_new_worker_on_same_uds_path",
                    None,
                )
                .await
            {
                warn!(
                    worker_id = %wid,
                    error = %e,
                    "Failed to retire superseded worker; stale routing may persist"
                );
            }
        }

        Ok(())
    }

    /// Transition worker status with validation and history
    ///
    /// Validates the transition, records history, and optionally logs to audit_logs
    /// if the transition is invalid.
    ///
    /// # Arguments
    /// * `worker_id` - The worker ID
    /// * `new_status` - Target status (created, registered, healthy, draining, stopped, error)
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
        let mut tx = self.begin_write_tx().await?;

        // Get current status and tenant_id
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT status, tenant_id FROM workers WHERE id = ?")
                .bind(worker_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::Database(format!("Failed to fetch worker: {}", e)))?;

        let (current_status, tenant_id) =
            row.ok_or_else(|| AosError::NotFound(format!("Worker not found: {}", worker_id)))?;

        // Parse and validate transition
        let from_status = WorkerStatus::from_str(&current_status)
            .map_err(|e| AosError::Validation(format!("Invalid from status: {}", e)))?;
        let to_status = WorkerStatus::from_str(new_status)
            .map_err(|e| AosError::Validation(format!("Invalid to status: {}", e)))?;
        let is_valid = from_status.can_transition_to(to_status);

        // ANCHOR: Temporal ordering validation (belt-and-suspenders with DB trigger)
        // Check that we're not inserting a record with a timestamp before existing entries
        // This catches clock drift issues before they hit the DB trigger
        let last_timestamp: Option<(String,)> = sqlx::query_as(
            "SELECT created_at FROM worker_status_history
             WHERE worker_id = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(worker_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to check temporal ordering: {}", e)))?;

        if let Some((last_ts,)) = last_timestamp {
            // Get current time from DB to use same clock
            let now_result: (String,) = sqlx::query_as("SELECT datetime('now')")
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| AosError::Database(format!("Failed to get current time: {}", e)))?;

            if now_result.0 < last_ts {
                // AUDIT: Track violation
                TEMPORAL_ORDERING_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
                error!(
                    worker_id = %worker_id,
                    last_timestamp = %last_ts,
                    current_time = %now_result.0,
                    total_violations = TEMPORAL_ORDERING_VIOLATIONS.load(Ordering::Relaxed),
                    "Temporal ordering violation detected - clock may be drifting"
                );
                return Err(AosError::Validation(format!(
                    "Cannot insert status history: current time ({}) is before last entry ({}). \
                     This may indicate clock drift.",
                    now_result.0, last_ts
                )));
            }
        }

        // Generate history record ID
        let history_id = crate::new_id(adapteros_id::IdPrefix::Wrk);

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
            let audit_id = crate::new_id(adapteros_id::IdPrefix::Aud);
            let metadata = serde_json::json!({
                "worker_id": worker_id,
                "from_status": current_status,
                "to_status": new_status,
                "reason": reason,
                "error": format!("Invalid transition: {} -> {}", current_status, new_status)
            });
            let actor_str = actor.unwrap_or("system");

            sqlx::query(
                "INSERT INTO audit_logs
                 (id, timestamp, tenant_id, user_id, user_role, action, resource_type, resource_id, status, metadata_json)
                 VALUES (?, datetime('now'), ?, ?, 'system', 'WorkerLifecycleViolation', 'worker', ?, 'error', ?)",
            )
            .bind(&audit_id)
            .bind(&tenant_id)
            .bind(actor_str)
            .bind(worker_id)
            .bind(metadata.to_string())
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
                last_seen_at = datetime('now'),
                registered_at = CASE WHEN ? = 'registered' THEN COALESCE(registered_at, datetime('now')) ELSE registered_at END
             WHERE id = ?",
        )
        .bind(new_status)
        .bind(reason)
        .bind(new_status)
        .bind(worker_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update worker status: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit: {}", e)))?;

        Ok(())
    }

    /// List workers compatible with a specific manifest hash
    ///
    /// Returns workers that:
    /// - Match the given manifest_hash_b3
    /// - Have schema_version compatible with control plane (major.minor match)
    /// - Have status = 'healthy'
    /// - Have health_status IN ('healthy', 'unknown') or NULL
    /// - Ordered by avg_latency_ms (lowest first)
    pub async fn list_compatible_workers(
        &self,
        manifest_hash: &str,
    ) -> Result<Vec<WorkerWithBinding>> {
        use adapteros_core::version::API_SCHEMA_VERSION;

        let workers = sqlx::query_as::<_, WorkerWithBinding>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3,
                    capabilities_json, schema_version, api_version, registered_at, health_status
             FROM workers
             WHERE manifest_hash_b3 = ?
               AND status = 'healthy'
               AND (health_status IS NULL OR health_status IN ('healthy', 'unknown'))
             ORDER BY avg_latency_ms ASC NULLS LAST",
        )
        .bind(manifest_hash)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list compatible workers: {}", e)))?;

        let initial_count = workers.len();

        // Filter by schema version compatibility (major.minor must match)
        let compatible_workers: Vec<WorkerWithBinding> = workers
            .into_iter()
            .filter(|w| match w.schema_version.as_deref() {
                Some(sv) if is_schema_compatible(sv, API_SCHEMA_VERSION) => true,
                Some(sv) => {
                    debug!(
                        worker_id = %w.id,
                        worker_schema = %sv,
                        cp_schema = %API_SCHEMA_VERSION,
                        "Worker excluded: schema version incompatible"
                    );
                    false
                }
                None => {
                    warn!(
                        worker_id = %w.id,
                        "Worker excluded: schema_version is NULL (worker may need re-registration)"
                    );
                    false
                }
            })
            .collect();

        if compatible_workers.len() < initial_count {
            debug!(
                manifest_hash = %manifest_hash,
                initial_count = initial_count,
                compatible_count = compatible_workers.len(),
                filtered_out = initial_count - compatible_workers.len(),
                "Some workers filtered out due to schema incompatibility"
            );
        }

        Ok(compatible_workers)
    }

    /// List workers compatible with a specific manifest hash and tenant
    ///
    /// Returns workers that:
    /// - Match the given manifest_hash_b3
    /// - Belong to the specified tenant_id
    /// - Have schema_version compatible with control plane (major.minor match)
    /// - Have status = 'healthy'
    /// - Have health_status IN ('healthy', 'unknown') or NULL
    /// - Ordered by avg_latency_ms (lowest first)
    ///
    /// This is the preferred method for inference routing as it combines
    /// manifest compatibility, schema compatibility, and tenant isolation.
    pub async fn list_compatible_workers_for_tenant(
        &self,
        manifest_hash: &str,
        tenant_id: &str,
    ) -> Result<Vec<WorkerWithBinding>> {
        use adapteros_core::version::API_SCHEMA_VERSION;

        let workers = sqlx::query_as::<_, WorkerWithBinding>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3,
                    capabilities_json, schema_version, api_version, registered_at, health_status
             FROM workers
             WHERE manifest_hash_b3 = ?
               AND tenant_id = ?
               AND status = 'healthy'
               AND (health_status IS NULL OR health_status IN ('healthy', 'unknown'))
             ORDER BY avg_latency_ms ASC NULLS LAST",
        )
        .bind(manifest_hash)
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list compatible workers for tenant: {}",
                e
            ))
        })?;

        let initial_count = workers.len();

        // Filter by schema version compatibility (major.minor must match)
        // This is defense-in-depth; incompatible workers shouldn't be registered
        let compatible_workers: Vec<WorkerWithBinding> = workers
            .into_iter()
            .filter(|w| match w.schema_version.as_deref() {
                Some(sv) if is_schema_compatible(sv, API_SCHEMA_VERSION) => true,
                Some(sv) => {
                    debug!(
                        worker_id = %w.id,
                        worker_schema = %sv,
                        cp_schema = %API_SCHEMA_VERSION,
                        tenant_id = %tenant_id,
                        "Worker excluded: schema version incompatible"
                    );
                    false
                }
                None => {
                    warn!(
                        worker_id = %w.id,
                        tenant_id = %tenant_id,
                        "Worker excluded: schema_version is NULL (worker may need re-registration)"
                    );
                    false
                }
            })
            .collect();

        if compatible_workers.len() < initial_count {
            debug!(
                manifest_hash = %manifest_hash,
                tenant_id = %tenant_id,
                initial_count = initial_count,
                compatible_count = compatible_workers.len(),
                filtered_out = initial_count - compatible_workers.len(),
                "Some workers filtered out due to schema incompatibility"
            );
        }

        Ok(compatible_workers)
    }

    /// List healthy workers
    ///
    /// Returns all workers with:
    /// - status = 'healthy'
    /// - schema_version compatible with control plane (major.minor match)
    /// - healthy status
    ///
    /// Used for routing when manifest matching is not required.
    pub async fn list_healthy_workers(&self) -> Result<Vec<WorkerWithBinding>> {
        use adapteros_core::version::API_SCHEMA_VERSION;

        let workers = sqlx::query_as::<_, WorkerWithBinding>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3,
                    capabilities_json, schema_version, api_version, registered_at, health_status
             FROM workers
             WHERE status = 'healthy'
               AND (health_status IS NULL OR health_status IN ('healthy', 'unknown'))
             ORDER BY avg_latency_ms ASC NULLS LAST",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list serving workers: {}", e)))?;

        let initial_count = workers.len();

        // Filter by schema version compatibility (major.minor must match)
        let compatible_workers: Vec<WorkerWithBinding> = workers
            .into_iter()
            .filter(|w| match w.schema_version.as_deref() {
                Some(sv) if is_schema_compatible(sv, API_SCHEMA_VERSION) => true,
                Some(sv) => {
                    debug!(
                        worker_id = %w.id,
                        worker_schema = %sv,
                        cp_schema = %API_SCHEMA_VERSION,
                        "Worker excluded: schema version incompatible"
                    );
                    false
                }
                None => {
                    warn!(
                        worker_id = %w.id,
                        "Worker excluded: schema_version is NULL (worker may need re-registration)"
                    );
                    false
                }
            })
            .collect();

        if compatible_workers.len() < initial_count {
            debug!(
                initial_count = initial_count,
                compatible_count = compatible_workers.len(),
                filtered_out = initial_count - compatible_workers.len(),
                "Some healthy workers filtered out due to schema incompatibility"
            );
        }

        Ok(compatible_workers)
    }

    /// Get worker status history
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
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker status history: {}", e)))?;

        Ok(history)
    }

    /// Get worker with binding information
    pub async fn get_worker_with_binding(
        &self,
        worker_id: &str,
    ) -> Result<Option<WorkerWithBinding>> {
        let worker = sqlx::query_as::<_, WorkerWithBinding>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at, manifest_hash_b3, backend, model_hash_b3,
                    capabilities_json, schema_version, api_version, registered_at, health_status
             FROM workers WHERE id = ?",
        )
        .bind(worker_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get worker with binding: {}", e)))?;

        Ok(worker)
    }

    /// Check if a worker exists with the given ID
    pub async fn worker_exists(&self, worker_id: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE id = ?")
            .bind(worker_id)
            .fetch_one(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to check worker existence: {}", e)))?;

        Ok(count > 0)
    }

    /// Purge terminal workers older than `retention_days`.
    ///
    /// Deletes rows from `workers` where status is terminal (`stopped`, `error`,
    /// `crashed`, `failed`) and the
    /// last status transition (or last heartbeat, whichever is available) is older
    /// than `retention_days` days ago.
    ///
    /// Cascading foreign keys on `worker_status_history` and `worker_incidents`
    /// handle related row cleanup automatically.
    ///
    /// Returns the number of deleted rows.
    pub async fn purge_terminal_workers(&self, retention_days: u32) -> Result<u64> {
        let cutoff = format!("-{} days", retention_days);

        let result = sqlx::query(
            "DELETE FROM workers
             WHERE status IN ('stopped', 'error', 'crashed', 'failed')
               AND COALESCE(last_transition_at, last_seen_at, started_at) < datetime('now', ?)",
        )
        .bind(&cutoff)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to purge terminal workers: {}", e)))?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            debug!(
                deleted = deleted,
                retention_days = retention_days,
                "Purged terminal workers"
            );
        }

        Ok(deleted)
    }

    /// Count invalid transitions for a worker
    pub async fn count_invalid_transitions(&self, worker_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM worker_status_history
             WHERE worker_id = ? AND valid_transition = 0",
        )
        .bind(worker_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to count invalid transitions: {}", e)))?;

        Ok(count)
    }
}

// =========================================================================
// Unit Tests for Schema Version Compatibility
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_compatible_same_version() {
        assert!(is_schema_compatible("1.0.0", "1.0.0"));
        assert!(is_schema_compatible("2.5.3", "2.5.3"));
    }

    #[test]
    fn test_schema_compatible_patch_ignored() {
        assert!(is_schema_compatible("1.0.0", "1.0.5"));
        assert!(is_schema_compatible("1.0.5", "1.0.0"));
        assert!(is_schema_compatible("1.0.1", "1.0.99"));
        assert!(is_schema_compatible("2.3.0", "2.3.100"));
    }

    #[test]
    fn test_schema_compatible_missing_patch() {
        assert!(is_schema_compatible("1.0", "1.0.0"));
        assert!(is_schema_compatible("1.0.0", "1.0"));
        assert!(is_schema_compatible("2.5", "2.5.10"));
    }

    #[test]
    fn test_schema_incompatible_minor_mismatch() {
        assert!(!is_schema_compatible("1.0.0", "1.1.0"));
        assert!(!is_schema_compatible("1.1.0", "1.0.0"));
        assert!(!is_schema_compatible("2.5.0", "2.6.0"));
        assert!(!is_schema_compatible("1.0.5", "1.1.5"));
    }

    #[test]
    fn test_schema_incompatible_major_mismatch() {
        assert!(!is_schema_compatible("1.0.0", "2.0.0"));
        assert!(!is_schema_compatible("2.0.0", "1.0.0"));
        assert!(!is_schema_compatible("1.5.3", "2.5.3"));
        assert!(!is_schema_compatible("3.0.0", "1.0.0"));
    }

    #[test]
    fn test_schema_invalid_versions() {
        assert!(!is_schema_compatible("", "1.0.0"));
        assert!(!is_schema_compatible("1.0.0", ""));
        assert!(!is_schema_compatible("invalid", "1.0.0"));
        assert!(!is_schema_compatible("1.0.0", "invalid"));
        assert!(!is_schema_compatible("abc.def.ghi", "1.0.0"));
    }

    #[test]
    fn test_schema_compatible_single_digit() {
        // Single digit major version only (minor defaults to 0)
        assert!(is_schema_compatible("1", "1.0.0"));
        assert!(is_schema_compatible("1.0.0", "1"));
        assert!(!is_schema_compatible("1", "2.0.0"));
    }

    #[test]
    fn test_schema_version_edge_cases() {
        // Large version numbers
        assert!(is_schema_compatible("999.999.999", "999.999.0"));
        assert!(!is_schema_compatible("999.999.999", "999.998.999"));

        // Zero versions
        assert!(is_schema_compatible("0.0.0", "0.0.0"));
        assert!(is_schema_compatible("0.0.0", "0.0.1"));
        assert!(!is_schema_compatible("0.0.0", "0.1.0"));
    }

    #[test]
    fn test_worker_registration_scenarios() {
        // Worker with same major.minor as control plane - ACCEPT
        assert!(
            is_schema_compatible("1.0.0", "1.0.0"),
            "Worker 1.0.0 should be accepted by CP 1.0.0"
        );

        // Worker with older patch - ACCEPT
        assert!(
            is_schema_compatible("1.0.0", "1.0.5"),
            "Worker 1.0.0 should be accepted by CP 1.0.5 (patch ignored)"
        );

        // Worker with newer patch - ACCEPT
        assert!(
            is_schema_compatible("1.0.10", "1.0.5"),
            "Worker 1.0.10 should be accepted by CP 1.0.5 (patch ignored)"
        );

        // Worker with older minor - REJECT
        assert!(
            !is_schema_compatible("1.0.0", "1.1.0"),
            "Worker 1.0.0 should be rejected by CP 1.1.0 (minor mismatch)"
        );

        // Worker with newer minor - REJECT
        assert!(
            !is_schema_compatible("1.2.0", "1.1.0"),
            "Worker 1.2.0 should be rejected by CP 1.1.0 (minor mismatch)"
        );

        // Worker with different major - REJECT
        assert!(
            !is_schema_compatible("2.0.0", "1.0.0"),
            "Worker 2.0.0 should be rejected by CP 1.0.0 (major mismatch)"
        );
    }

    // =========================================================================
    // Unit Tests for WorkerIncidentType
    // =========================================================================

    #[test]
    fn test_incident_type_as_str() {
        assert_eq!(WorkerIncidentType::Fatal.as_str(), "fatal");
        assert_eq!(WorkerIncidentType::Crash.as_str(), "crash");
        assert_eq!(WorkerIncidentType::Hung.as_str(), "hung");
        assert_eq!(WorkerIncidentType::Degraded.as_str(), "degraded");
        assert_eq!(WorkerIncidentType::Recovered.as_str(), "recovered");
    }

    #[test]
    fn test_incident_type_display() {
        assert_eq!(format!("{}", WorkerIncidentType::Fatal), "fatal");
        assert_eq!(format!("{}", WorkerIncidentType::Crash), "crash");
        assert_eq!(format!("{}", WorkerIncidentType::Hung), "hung");
        assert_eq!(format!("{}", WorkerIncidentType::Degraded), "degraded");
        assert_eq!(format!("{}", WorkerIncidentType::Recovered), "recovered");
    }

    #[test]
    fn test_incident_type_from_str_valid() {
        assert_eq!(
            "fatal".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Fatal
        );
        assert_eq!(
            "crash".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Crash
        );
        assert_eq!(
            "hung".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Hung
        );
        assert_eq!(
            "degraded".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Degraded
        );
        assert_eq!(
            "recovered".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Recovered
        );
    }

    #[test]
    fn test_incident_type_from_str_case_insensitive() {
        assert_eq!(
            "FATAL".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Fatal
        );
        assert_eq!(
            "Crash".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Crash
        );
        assert_eq!(
            "HUNG".parse::<WorkerIncidentType>().unwrap(),
            WorkerIncidentType::Hung
        );
    }

    #[test]
    fn test_incident_type_from_str_invalid() {
        assert!("invalid".parse::<WorkerIncidentType>().is_err());
        assert!("timeout".parse::<WorkerIncidentType>().is_err());
        assert!("error".parse::<WorkerIncidentType>().is_err());
        assert!("".parse::<WorkerIncidentType>().is_err());
    }

    #[test]
    fn test_incident_type_all_constant() {
        // Verify ALL contains exactly 5 types
        assert_eq!(WorkerIncidentType::ALL.len(), 5);

        // Verify each type is in ALL
        assert!(WorkerIncidentType::ALL.contains(&WorkerIncidentType::Fatal));
        assert!(WorkerIncidentType::ALL.contains(&WorkerIncidentType::Crash));
        assert!(WorkerIncidentType::ALL.contains(&WorkerIncidentType::Hung));
        assert!(WorkerIncidentType::ALL.contains(&WorkerIncidentType::Degraded));
        assert!(WorkerIncidentType::ALL.contains(&WorkerIncidentType::Recovered));
    }

    #[test]
    fn test_incident_type_roundtrip() {
        // Verify as_str -> from_str roundtrip for all types
        for incident_type in WorkerIncidentType::ALL {
            let s = incident_type.as_str();
            let parsed: WorkerIncidentType = s.parse().unwrap();
            assert_eq!(*incident_type, parsed);
        }
    }

    #[tokio::test]
    async fn purge_terminal_workers_deletes_crashed_and_failed() {
        let db = Db::new_in_memory().await.expect("db init should succeed");

        sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name, created_at)
             VALUES ('default', 'default', datetime('now'))",
        )
        .execute(db.pool())
        .await
        .expect("tenant insert should succeed");

        sqlx::query(
            "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at)
             VALUES ('node-01', 'test-host', 'http://localhost:9000', 'active', datetime('now'), '{}', datetime('now'))",
        )
        .execute(db.pool())
        .await
        .expect("node insert should succeed");

        sqlx::query(
            "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json)
             VALUES ('test-manifest-id', 'default', 'test-manifest-hash', '{}')",
        )
        .execute(db.pool())
        .await
        .expect("manifest insert should succeed");

        sqlx::query(
            "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3)
             VALUES ('test-plan', 'default', 'test-plan-hash', 'test-manifest-hash', '{}', 'layout-hash')",
        )
        .execute(db.pool())
        .await
        .expect("plan insert should succeed");

        sqlx::query(
            "INSERT INTO workers
             (id, tenant_id, node_id, plan_id, uds_path, pid, status, manifest_hash_b3, schema_version, api_version, started_at, registered_at, last_transition_at)
             VALUES
             ('worker-crashed', 'default', 'node-01', 'test-plan', '/var/run/crashed.sock', 1001, 'crashed', 'test-manifest-hash', '1.0.0', '1.0.0', datetime('now', '-30 days'), datetime('now', '-30 days'), datetime('now', '-30 days')),
             ('worker-failed', 'default', 'node-01', 'test-plan', '/var/run/failed.sock', 1002, 'failed', 'test-manifest-hash', '1.0.0', '1.0.0', datetime('now', '-30 days'), datetime('now', '-30 days'), datetime('now', '-30 days')),
             ('worker-healthy', 'default', 'node-01', 'test-plan', '/var/run/healthy.sock', 1003, 'healthy', 'test-manifest-hash', '1.0.0', '1.0.0', datetime('now', '-30 days'), datetime('now', '-30 days'), datetime('now', '-30 days'))",
        )
        .execute(db.pool())
        .await
        .expect("worker insert should succeed");

        let deleted = db
            .purge_terminal_workers(7)
            .await
            .expect("purge should succeed");
        assert_eq!(deleted, 2, "crashed and failed workers should be purged");

        let crashed_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE id = 'worker-crashed'")
            .fetch_one(db.pool())
            .await
            .expect("query should succeed");
        let failed_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE id = 'worker-failed'")
            .fetch_one(db.pool())
            .await
            .expect("query should succeed");
        let healthy_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE id = 'worker-healthy'")
            .fetch_one(db.pool())
            .await
            .expect("query should succeed");

        assert_eq!(crashed_count, 0, "crashed worker should be deleted");
        assert_eq!(failed_count, 0, "failed worker should be deleted");
        assert_eq!(healthy_count, 1, "non-terminal worker should remain");
    }
}
