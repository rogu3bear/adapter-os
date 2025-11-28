use crate::{models::Worker, Db};
use adapteros_core::{AosError, Result};

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
    ///     .uds_path("/tmp/worker.sock")
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
