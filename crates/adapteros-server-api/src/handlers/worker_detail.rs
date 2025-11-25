//! Worker Detail Handler
//!
//! Provides detailed information about workers including:
//! - Worker type and status
//! - CPU and memory usage
//! - Active tasks
//! - Uptime and performance metrics

use crate::permissions::{require_any_role, Role};
use crate::{AppState, Claims, ErrorResponse};
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

/// Worker detail response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct WorkerDetailResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub worker_type: WorkerType,
    pub status: String,
    pub pid: Option<i32>,
    pub uds_path: String,
    pub resource_usage: WorkerResourceUsage,
    pub active_tasks: Vec<WorkerTask>,
    pub adapters_loaded: Vec<String>,
    pub uptime_seconds: u64,
    pub memory_headroom_pct: Option<f32>,
    pub k_current: Option<i32>,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
}

/// Worker type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    Inference,
    Training,
    Router,
    System,
}

/// Worker resource usage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct WorkerResourceUsage {
    pub cpu_usage_percent: f32,
    pub memory_usage_mb: f32,
    pub memory_limit_mb: Option<f32>,
    pub thread_count: i32,
    pub requests_processed: i64,
    pub errors_count: i64,
    pub avg_latency_ms: f32,
    pub timestamp: u64,
}

/// Worker task
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct WorkerTask {
    pub task_id: String,
    pub task_type: String,
    pub status: String,
    pub started_at: String,
    pub progress_percent: Option<f32>,
}

fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Get worker detail
#[utoipa::path(
    tag = "workers",
    get,
    path = "/v1/workers/{worker_id}/detail",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Worker detail", body = WorkerDetailResponse)
    )
)]
pub async fn get_worker_detail(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<WorkerDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator or above
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Fetch worker from database
    let worker = sqlx::query_as::<_, WorkerRecord>(
        "SELECT id, tenant_id, node_id, plan_id, status, pid, uds_path,
                memory_headroom_pct, k_current, adapters_loaded_json,
                started_at, last_heartbeat_at
         FROM workers WHERE id = ?",
    )
    .bind(&worker_id)
    .fetch_one(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("worker not found")
                    .with_code("WORKER_NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    // Calculate uptime
    let uptime_seconds = calculate_uptime(&worker.started_at, timestamp);

    // Determine worker type
    let worker_type = determine_worker_type(&worker, &state).await;

    // Get resource usage
    let resource_usage = get_worker_resource_usage(&worker, &state, timestamp).await;

    // Get active tasks
    let active_tasks = get_active_tasks(&worker.id, &state).await;

    // Parse adapters loaded
    let adapters_loaded: Vec<String> = serde_json::from_str(
        &worker
            .adapters_loaded_json
            .unwrap_or_else(|| "[]".to_string()),
    )
    .unwrap_or_default();

    Ok(Json(WorkerDetailResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        id: worker.id,
        tenant_id: worker.tenant_id,
        node_id: worker.node_id,
        plan_id: worker.plan_id,
        worker_type,
        status: worker.status,
        pid: worker.pid,
        uds_path: worker.uds_path,
        resource_usage,
        active_tasks,
        adapters_loaded,
        uptime_seconds,
        memory_headroom_pct: worker.memory_headroom_pct,
        k_current: worker.k_current,
        started_at: worker.started_at,
        last_heartbeat_at: worker.last_heartbeat_at,
    }))
}

/// Worker record from database
#[derive(Debug, sqlx::FromRow)]
struct WorkerRecord {
    id: String,
    tenant_id: String,
    node_id: String,
    plan_id: String,
    status: String,
    pid: Option<i32>,
    uds_path: String,
    memory_headroom_pct: Option<f32>,
    k_current: Option<i32>,
    adapters_loaded_json: Option<String>,
    started_at: String,
    last_heartbeat_at: Option<String>,
}

/// Calculate worker uptime
fn calculate_uptime(started_at: &str, current_timestamp: u64) -> u64 {
    use chrono::{DateTime, Utc};

    if let Ok(started) = DateTime::parse_from_rfc3339(started_at) {
        let started_timestamp = started.timestamp() as u64;
        current_timestamp.saturating_sub(started_timestamp)
    } else {
        0
    }
}

/// Determine worker type based on plan and configuration
async fn determine_worker_type(worker: &WorkerRecord, state: &AppState) -> WorkerType {
    // Check if worker is associated with training jobs
    let is_training = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM training_jobs WHERE worker_id = ? AND status = 'running'",
    )
    .bind(&worker.id)
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0);

    if is_training > 0 {
        return WorkerType::Training;
    }

    // Check if worker is router (k_current suggests routing capability)
    if worker.k_current.is_some() {
        return WorkerType::Router;
    }

    // Default to inference worker
    WorkerType::Inference
}

/// Get worker resource usage
async fn get_worker_resource_usage(
    worker: &WorkerRecord,
    state: &AppState,
    timestamp: u64,
) -> WorkerResourceUsage {
    // Get process metrics if PID is available
    let (cpu_usage, memory_usage_mb, thread_count) = if let Some(pid) = worker.pid {
        get_process_metrics(pid)
    } else {
        (0.0, 0.0, 0)
    };

    // Get request metrics from telemetry
    let requests_processed = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM telemetry_events
         WHERE worker_id = ? AND event_type = 'inference_complete'",
    )
    .bind(&worker.id)
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0);

    let errors_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM telemetry_events
         WHERE worker_id = ? AND event_type = 'error'",
    )
    .bind(&worker.id)
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0);

    let avg_latency_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(CAST(json_extract(payload, '$.latency_ms') AS REAL))
         FROM telemetry_events
         WHERE worker_id = ? AND event_type = 'inference_complete'
         AND timestamp > datetime('now', '-5 minutes')",
    )
    .bind(&worker.id)
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or(0.0) as f32;

    WorkerResourceUsage {
        cpu_usage_percent: cpu_usage,
        memory_usage_mb: memory_usage_mb,
        memory_limit_mb: None, // Would be set if cgroups are used
        thread_count,
        requests_processed,
        errors_count,
        avg_latency_ms,
        timestamp,
    }
}

/// Get process metrics for a given PID
#[cfg(target_os = "macos")]
fn get_process_metrics(pid: i32) -> (f32, f32, i32) {
    use sysinfo::{Pid, System};

    let mut sys = System::new_all();
    sys.refresh_all();

    if let Some(process) = sys.process(Pid::from_u32(pid as u32)) {
        let cpu_usage = process.cpu_usage();
        let memory_mb = process.memory() as f32 / 1_048_576.0; // Convert bytes to MB
        let thread_count = process.tasks().map(|t| t.len()).unwrap_or(0) as i32;
        (cpu_usage, memory_mb, thread_count)
    } else {
        (0.0, 0.0, 0)
    }
}

#[cfg(not(target_os = "macos"))]
fn get_process_metrics(pid: i32) -> (f32, f32, i32) {
    // Placeholder for non-macOS systems
    (0.0, 0.0, 0)
}

/// Get active tasks for worker
async fn get_active_tasks(worker_id: &str, state: &AppState) -> Vec<WorkerTask> {
    // Get training jobs
    let training_tasks = sqlx::query_as::<_, TaskRecord>(
        "SELECT id, 'training' as task_type, status, started_at, progress_pct
         FROM training_jobs
         WHERE worker_id = ? AND status IN ('running', 'pending')",
    )
    .bind(worker_id)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    training_tasks
        .into_iter()
        .map(|task| WorkerTask {
            task_id: task.id,
            task_type: task.task_type,
            status: task.status,
            started_at: task
                .started_at
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            progress_percent: task.progress_pct,
        })
        .collect()
}

/// Task record from database
#[derive(Debug, sqlx::FromRow)]
struct TaskRecord {
    id: String,
    task_type: String,
    status: String,
    started_at: Option<String>,
    progress_pct: Option<f32>,
}
