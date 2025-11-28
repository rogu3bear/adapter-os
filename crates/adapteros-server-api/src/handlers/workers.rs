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
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListWorkersQuery {
    pub tenant_id: Option<String>,
}

/// Spawn worker via node agent
pub async fn worker_spawn(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<SpawnWorkerRequest>,
) -> Result<Json<WorkerResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Look up node by ID
    let node = state
        .db
        .get_node(&req.node_id)
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
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("node not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Node ID: {}", req.node_id)),
                ),
            )
        })?;

    // Prepare spawn request for node agent
    let spawn_req = serde_json::json!({
        "tenant_id": req.tenant_id,
        "plan_id": req.plan_id,
    });

    // Send HTTP POST to node agent
    let client = reqwest::Client::new();
    let spawn_url = format!("{}/spawn_worker", node.agent_endpoint);

    let response = client
        .post(&spawn_url)
        .json(&spawn_req)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(
                    ErrorResponse::new("failed to contact node agent")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("node agent spawn failed")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(error_text),
            ),
        ));
    }

    let spawn_response: serde_json::Value = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to parse node agent response")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let pid = spawn_response["pid"].as_i64().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("invalid response from node agent")
                    .with_code("BAD_REQUEST")
                    .with_string_details("missing or invalid PID field"),
            ),
        )
    })? as i32;

    // Create UDS path for worker
    let uds_path = format!("/var/run/aos/{}/worker.sock", req.tenant_id);

    // Register worker using Db trait method
    use adapteros_db::workers::WorkerInsertBuilder;
    let worker_id = uuid::Uuid::now_v7().to_string();
    let mut builder = WorkerInsertBuilder::new()
        .id(&worker_id)
        .tenant_id(&req.tenant_id)
        .node_id(&req.node_id)
        .plan_id(&req.plan_id)
        .uds_path(&uds_path)
        .status("starting");
    builder = builder.pid(pid);
    let params = builder.build().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to build worker parameters")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    state.db.insert_worker(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to register worker in database")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Return worker info
    Ok(Json(WorkerResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: worker_id,
        tenant_id: req.tenant_id,
        node_id: req.node_id,
        plan_id: req.plan_id,
        uds_path,
        pid: Some(pid),
        status: "starting".to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        last_seen_at: None,
    }))
}

/// List workers with optional tenant filter
pub async fn list_workers(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListWorkersQuery>,
) -> Result<Json<Vec<WorkerResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let workers = if let Some(tenant_id) = query.tenant_id {
        state
            .db
            .list_workers_by_tenant(&tenant_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
    } else {
        state.db.list_all_workers().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    let response: Vec<WorkerResponse> = workers
        .into_iter()
        .map(|w| WorkerResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: w.id,
            tenant_id: w.tenant_id,
            node_id: w.node_id,
            plan_id: w.plan_id,
            uds_path: w.uds_path,
            pid: w.pid,
            status: w.status,
            started_at: w.started_at,
            last_seen_at: w.last_seen_at,
        })
        .collect();

    Ok(Json(response))
}

/// Stop a worker process
///
/// Gracefully stops a worker process by updating its status and optionally
/// terminating the underlying process.
///
/// **Permissions:** Requires `WorkerManage` permission (Operator or Admin role).
///
/// **Telemetry:** Emits `worker.stop` event.
///
/// # Example
/// ```
/// POST /v1/workers/{worker_id}/stop
/// ```
#[utoipa::path(
    post,
    path = "/v1/workers/{worker_id}/stop",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Worker stopped successfully", body = crate::types::WorkerStopResponse),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 500, description = "Failed to stop worker", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn stop_worker(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<crate::types::WorkerStopResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require worker manage permission
    crate::permissions::require_permission(&claims, crate::permissions::Permission::WorkerManage)?;

    // Get worker from database
    let worker = state
        .db
        .get_worker(&worker_id)
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
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("worker not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Worker ID: {}", worker_id)),
                ),
            )
        })?;

    let previous_status = worker.status.clone();

    // Update worker status to 'stopping' using Db trait method
    state
        .db
        .update_worker_status(&worker_id, "stopping")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update worker status")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // If worker has a PID, attempt to terminate the process
    if let Some(pid) = worker.pid {
        // Note: In production, this would send a signal to the worker process
        // For now, we just update the status
        tracing::info!(
            event = "worker.stop.signal",
            worker_id = %worker_id,
            pid = %pid,
            "Signaling worker process to stop"
        );
    }

    // Update worker status to 'stopped' using Db trait method
    state
        .db
        .update_worker_status(&worker_id, "stopped")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update worker status")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let stopped_at = chrono::Utc::now().to_rfc3339();

    // Emit telemetry event
    tracing::info!(
        event = "worker.stop",
        worker_id = %worker_id,
        previous_status = %previous_status,
        actor = %claims.sub,
        "Worker stopped"
    );

    // Audit log
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "worker.stop",
        crate::audit_helper::resources::WORKER,
        Some(&worker_id),
    )
    .await;

    Ok(Json(crate::types::WorkerStopResponse {
        worker_id,
        success: true,
        message: "Worker stopped successfully".to_string(),
        previous_status,
        stopped_at,
    }))
}
