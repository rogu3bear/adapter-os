//! Debugging handlers
//!
//! Handlers for process logs, crash dumps, debug sessions, and troubleshooting.

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use sqlx::Row;
use std::collections::HashMap;

fn is_missing_table_error(error: &sqlx::Error) -> bool {
    error.to_string().contains("no such table")
}

fn is_valid_session_type(session_type: &str) -> bool {
    matches!(session_type, "live" | "replay" | "analysis")
}

fn is_valid_step_type(step_type: &str) -> bool {
    matches!(step_type, "diagnostic" | "recovery" | "prevention")
}

async fn ensure_worker_access(
    state: &AppState,
    claims: &Claims,
    worker_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let worker = state
        .db
        .get_worker_for_tenant(&claims.tenant_id, worker_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if worker.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("worker not found").with_code("NOT_FOUND")),
        ));
    }

    Ok(())
}

// ========== Handlers ==========

/// Get process logs for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/logs",
    params(
        ("worker_id" = String, Path, description = "Worker ID"),
        ("level" = Option<String>, Query, description = "Log level filter"),
        ("limit" = Option<i32>, Query, description = "Maximum logs to return")
    ),
    responses(
        (status = 200, description = "Process logs", body = Vec<ProcessLogResponse>)
    )
)]
pub async fn list_process_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessLogResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    ensure_worker_access(&state, &claims, &worker_id).await?;

    let level_filter = match params.get("level") {
        Some(level) => {
            let normalized = level.to_lowercase();
            if !matches!(
                normalized.as_str(),
                "debug" | "info" | "warn" | "error" | "fatal"
            ) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid level filter")
                            .with_code("BAD_REQUEST")
                            .with_string_details(level.to_string()),
                    ),
                ));
            }
            Some(normalized)
        }
        None => None,
    };
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(100);

    let mut sql = String::from(
        "SELECT id, worker_id, level, message, timestamp, metadata_json \
         FROM process_logs WHERE worker_id = ?",
    );
    if level_filter.is_some() {
        sql.push_str(" AND level = ?");
    }
    sql.push_str(" ORDER BY timestamp DESC LIMIT ?");

    let mut query = sqlx::query(&sql).bind(&worker_id);
    if let Some(level) = level_filter {
        query = query.bind(level);
    }
    let rows = match query.bind(limit).fetch_all(state.db.pool()).await {
        Ok(rows) => rows,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok(Json(Vec::new()));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let logs = rows
        .into_iter()
        .map(|row| ProcessLogResponse {
            id: row.get("id"),
            worker_id: row.get("worker_id"),
            level: row.get("level"),
            message: row.get("message"),
            timestamp: row.get("timestamp"),
            metadata_json: row.get("metadata_json"),
        })
        .collect();

    Ok(Json(logs))
}

/// Get process crash dumps for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/crashes",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Process crash dumps", body = Vec<ProcessCrashDumpResponse>)
    )
)]
pub async fn list_process_crashes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<Vec<ProcessCrashDumpResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    ensure_worker_access(&state, &claims, &worker_id).await?;

    let rows = match sqlx::query(
        "SELECT id, worker_id, crash_type, stack_trace, memory_snapshot_json, \
         crash_timestamp, recovery_action, recovered_at \
         FROM process_crash_dumps WHERE worker_id = ? \
         ORDER BY crash_timestamp DESC LIMIT 100",
    )
    .bind(&worker_id)
    .fetch_all(state.db.pool())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok(Json(Vec::new()));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let crashes = rows
        .into_iter()
        .map(|row| ProcessCrashDumpResponse {
            id: row.get("id"),
            worker_id: row.get("worker_id"),
            crash_type: row.get("crash_type"),
            stack_trace: row.get("stack_trace"),
            memory_snapshot_json: row.get("memory_snapshot_json"),
            crash_timestamp: row.get("crash_timestamp"),
            recovery_action: row.get("recovery_action"),
            recovered_at: row.get("recovered_at"),
        })
        .collect();

    Ok(Json(crashes))
}

/// Start a debug session for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/debug",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = StartDebugSessionRequest,
    responses(
        (status = 200, description = "Debug session started", body = ProcessDebugSessionResponse)
    )
)]
pub async fn start_debug_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<StartDebugSessionRequest>,
) -> Result<Json<ProcessDebugSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    let request_worker_id = crate::id_resolver::resolve_any_id(&state.db, &req.worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    if request_worker_id != worker_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("worker_id mismatch")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "path worker_id {} does not match request worker_id {}",
                        worker_id, req.worker_id
                    )),
            ),
        ));
    }

    if !is_valid_session_type(req.session_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid session_type")
                    .with_code("BAD_REQUEST")
                    .with_string_details(req.session_type.clone()),
            ),
        ));
    }

    ensure_worker_access(&state, &claims, &worker_id).await?;

    let session_id =
        crate::id_generator::readable_id(adapteros_core::ids::IdKind::Session, "debug");
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO process_debug_sessions \
         (id, worker_id, session_type, status, config_json, started_at, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&worker_id)
    .bind(&req.session_type)
    .bind("active")
    .bind(&req.config_json)
    .bind(&now)
    .bind(&now)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        if is_missing_table_error(&e) {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("process_debug_sessions table missing")
                        .with_code("MISSING_TABLE"),
                ),
            );
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(ProcessDebugSessionResponse {
        id: session_id,
        worker_id,
        session_type: req.session_type,
        status: "active".to_string(),
        config_json: req.config_json,
        started_at: now,
        ended_at: None,
        results_json: None,
    }))
}

/// Run a troubleshooting step for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/troubleshoot",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = RunTroubleshootingStepRequest,
    responses(
        (status = 200, description = "Troubleshooting step started", body = ProcessTroubleshootingStepResponse)
    )
)]
pub async fn run_troubleshooting_step(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<RunTroubleshootingStepRequest>,
) -> Result<Json<ProcessTroubleshootingStepResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    let request_worker_id = crate::id_resolver::resolve_any_id(&state.db, &req.worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    if request_worker_id != worker_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("worker_id mismatch")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "path worker_id {} does not match request worker_id {}",
                        worker_id, req.worker_id
                    )),
            ),
        ));
    }

    if !is_valid_step_type(req.step_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid step_type")
                    .with_code("BAD_REQUEST")
                    .with_string_details(req.step_type.clone()),
            ),
        ));
    }

    ensure_worker_access(&state, &claims, &worker_id).await?;

    let step_id = crate::id_generator::readable_id(adapteros_core::ids::IdKind::Run, "step");
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO process_troubleshooting_steps \
         (id, worker_id, step_name, step_type, status, command, started_at, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&step_id)
    .bind(&worker_id)
    .bind(&req.step_name)
    .bind(&req.step_type)
    .bind("running")
    .bind(&req.command)
    .bind(&now)
    .bind(&now)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        if is_missing_table_error(&e) {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("process_troubleshooting_steps table missing")
                        .with_code("MISSING_TABLE"),
                ),
            );
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(ProcessTroubleshootingStepResponse {
        id: step_id,
        worker_id,
        step_name: req.step_name,
        step_type: req.step_type,
        status: "running".to_string(),
        command: req.command,
        output: None,
        error_message: None,
        started_at: now,
        completed_at: None,
    }))
}
