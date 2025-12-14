use crate::auth::Claims;
use crate::error_helpers::{bad_gateway, db_error_msg, internal_error_msg, not_found_with_details};
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::*;
use adapteros_api_types::workers::{
    WorkerRegistrationRequest, WorkerRegistrationResponse, WorkerStatusNotification,
};
use adapteros_core::{identity::IdentityEnvelope, version::API_SCHEMA_VERSION, WorkerStatus};
use adapteros_db::users::Role;
use adapteros_db::workers::{is_schema_compatible, WorkerRegistrationParams};
use adapteros_telemetry::{build_health_event, make_health_payload, HealthEventKind};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use tracing::{error, info, warn};

#[derive(Deserialize)]
pub struct ListWorkersQuery {
    pub tenant_id: Option<String>,
}

async fn push_worker_health_event(
    state: &AppState,
    tenant_id: &str,
    worker_id: &str,
    kind: HealthEventKind,
    previous_status: Option<String>,
    new_status: Option<String>,
    error: Option<String>,
) {
    let identity = IdentityEnvelope::new(
        tenant_id.to_string(),
        "control_plane".to_string(),
        "worker_lifecycle".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );

    let payload = make_health_payload(
        worker_id.to_string(),
        tenant_id.to_string(),
        kind,
        previous_status,
        new_status,
        None,
        None,
        error,
    );

    if let Ok(event) = build_health_event(identity, payload) {
        let buffer = state.telemetry_buffer.clone();
        let _ = buffer.push(event).await;
    }
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
        .map_err(|e| db_error_msg("database error", e))?
        .ok_or_else(|| {
            not_found_with_details("node not found", format!("Node ID: {}", req.node_id))
        })?;

    // Prepare spawn request for node agent
    let spawn_req = serde_json::json!({
        "tenant_id": req.tenant_id,
        "plan_id": req.plan_id,
    });

    // Send HTTP POST to node agent
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| internal_error_msg("failed to create HTTP client", e))?;
    let spawn_url = format!("{}/spawn_worker", node.agent_endpoint);

    let max_attempts = 3u32;
    let mut attempt = 0u32;
    let mut backoff = std::time::Duration::from_millis(100);
    let response = loop {
        attempt += 1;
        match client.post(&spawn_url).json(&spawn_req).send().await {
            Ok(response) => break Ok(response),
            Err(e) => {
                if attempt >= max_attempts {
                    break Err(e);
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(std::time::Duration::from_millis(800));
            }
        }
    }
    .map_err(|e| {
        bad_gateway(
            "failed to contact node agent",
            format!("{} (after {} attempts)", e, max_attempts),
        )
    })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(internal_error_msg("node agent spawn failed", error_text));
    }

    let spawn_response: serde_json::Value = response
        .json()
        .await
        .map_err(|e| internal_error_msg("failed to parse node agent response", e))?;

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
        .status(WorkerStatus::Created.as_str());
    builder = builder.pid(pid);
    let params = builder
        .build()
        .map_err(|e| internal_error_msg("failed to build worker parameters", e))?;
    state
        .db
        .insert_worker(params)
        .await
        .map_err(|e| db_error_msg("failed to register worker in database", e))?;

    // Return worker info
    Ok(Json(WorkerResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: worker_id,
        tenant_id: req.tenant_id,
        node_id: req.node_id,
        plan_id: req.plan_id,
        uds_path,
        pid: Some(pid),
        status: WorkerStatus::Created.as_str().to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        last_seen_at: None,
        capabilities: Vec::new(),
        backend: None,
        model_id: None,
        model_hash: None,
        model_loaded: false,
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
            capabilities: Vec::new(),
            backend: None,
            model_id: None,
            model_hash: None,
            model_loaded: false,
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

    // Use validated state transitions
    // Transition to 'draining' first (valid from 'healthy' state)
    match state
        .db
        .transition_worker_status(
            &worker_id,
            "draining",
            "operator stop request",
            Some(&claims.sub),
        )
        .await
    {
        Ok(_) => {
            tracing::info!(
                event = "worker.draining",
                worker_id = %worker_id,
                actor = %claims.sub,
                "Worker transitioned to draining"
            );
        }
        Err(e) => {
            // Check if it's a lifecycle error (invalid transition)
            let err_str = e.to_string();
            if err_str.contains("Lifecycle") || err_str.contains("Invalid") {
                tracing::warn!(
                    event = "worker.stop.invalid_transition",
                    worker_id = %worker_id,
                    previous_status = %previous_status,
                    error = %e,
                    "Invalid state transition attempted"
                );
                return Err((
                    StatusCode::CONFLICT,
                    Json(
                        ErrorResponse::new("invalid state transition")
                            .with_code("LIFECYCLE_ERROR")
                            .with_string_details(format!(
                                "Cannot stop worker in '{}' state. Valid transitions from serving: draining, crashed",
                                previous_status
                            )),
                    ),
                ));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to transition worker status")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    }

    // If worker has a PID, signal it to drain gracefully
    if let Some(pid) = worker.pid {
        // Send SIGTERM to allow graceful shutdown
        // Worker should respond by completing in-flight requests and transitioning to 'stopped'
        tracing::info!(
            event = "worker.stop.signal",
            worker_id = %worker_id,
            pid = %pid,
            "Signaling worker process to drain (SIGTERM)"
        );

        // Best-effort signal - worker may already be gone
        #[cfg(unix)]
        {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGTERM,
            );
        }
    }

    // Note: The worker is now in 'draining' state and will transition to 'stopped'
    // when it finishes processing in-flight requests. The worker calls back via
    // POST /v1/workers/status to notify of the transition.
    //
    // For immediate stop (e.g., after timeout), a separate force-stop endpoint
    // could transition directly to 'crashed' state.

    let stopped_at = chrono::Utc::now().to_rfc3339();

    // Emit telemetry event
    tracing::info!(
        event = "worker.stop.initiated",
        worker_id = %worker_id,
        previous_status = %previous_status,
        actor = %claims.sub,
        "Worker stop initiated (draining)"
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
        message: "Worker draining initiated. Worker will transition to stopped when complete."
            .to_string(),
        previous_status,
        stopped_at,
    }))
}

// =========================================================================
// Worker Registration & Lifecycle
// =========================================================================

/// Register a worker with manifest binding
///
/// Workers call this endpoint on startup to register with the control plane.
/// The control plane validates:
/// 1. The plan exists
/// 2. The worker's manifest_hash matches the plan's expected manifest_hash_b3
/// 3. The schema_version is compatible
///
/// On successful registration, the worker is inserted with status 'registered'.
/// The worker must then call the status notification endpoint to transition to 'healthy'.
///
/// **Note:** This endpoint is called by workers, not by users. No auth required.
///
/// # Example
/// ```
/// POST /v1/workers/register
/// {
///   "worker_id": "01234567-89ab-cdef-0123-456789abcdef",
///   "tenant_id": "tenant-123",
///   "plan_id": "plan-456",
///   "manifest_hash": "abc123...",
///   "schema_version": "1.0",
///   "api_version": "1.0",
///   "pid": 12345,
///   "uds_path": "/var/run/aos/tenant-123/worker.sock",
///   "capabilities": ["coreml", "mlx"]
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/workers/register",
    request_body = WorkerRegistrationRequest,
    responses(
        (status = 200, description = "Registration response", body = WorkerRegistrationResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn register_worker(
    State(state): State<AppState>,
    Json(req): Json<WorkerRegistrationRequest>,
) -> Result<Json<WorkerRegistrationResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        worker_id = %req.worker_id,
        tenant_id = %req.tenant_id,
        plan_id = %req.plan_id,
        manifest_hash = %req.manifest_hash,
        "Worker registration request received"
    );

    // 1. Check if worker already exists
    let exists = state
        .db
        .worker_exists(&req.worker_id)
        .await
        .map_err(|e| db_error_msg("failed to check worker existence", e))?;

    if exists {
        info!(
            worker_id = %req.worker_id,
            "Worker already present; updating registration metadata"
        );
    }

    // 2. Get plan and extract expected manifest_hash
    let plan = state
        .db
        .get_plan(&req.plan_id)
        .await
        .map_err(|e| db_error_msg("failed to fetch plan", e))?;

    let plan = match plan {
        Some(p) => p,
        None => {
            warn!(
                worker_id = %req.worker_id,
                plan_id = %req.plan_id,
                "Plan not found, rejecting registration"
            );
            return Ok(Json(WorkerRegistrationResponse {
                accepted: false,
                worker_id: req.worker_id,
                rejection_reason: Some(format!("Plan not found: {}", req.plan_id)),
                heartbeat_interval_secs: 30,
                kv_quota_bytes: None,
                kv_residency_policy_id: None,
            }));
        }
    };

    // 3. Validate manifest compatibility
    if req.manifest_hash != plan.manifest_hash_b3 {
        warn!(
            worker_id = %req.worker_id,
            expected_hash = %plan.manifest_hash_b3,
            actual_hash = %req.manifest_hash,
            "Manifest hash mismatch, rejecting registration"
        );

        // Log rejection (audit via tracing since no Claims available)
        warn!(
            tenant_id = %plan.tenant_id,
            worker_id = %req.worker_id,
            reason = "manifest_hash_mismatch",
            expected = %plan.manifest_hash_b3,
            actual = %req.manifest_hash,
            "Worker registration rejected"
        );

        return Ok(Json(WorkerRegistrationResponse {
            accepted: false,
            worker_id: req.worker_id,
            rejection_reason: Some(format!(
                "Manifest hash mismatch: expected {}, got {}",
                plan.manifest_hash_b3, req.manifest_hash
            )),
            heartbeat_interval_secs: 30,
            kv_quota_bytes: None,
            kv_residency_policy_id: None,
        }));
    }

    // 4. Validate schema_version compatibility using semver (major.minor must match)
    if !is_schema_compatible(&req.schema_version, API_SCHEMA_VERSION) {
        warn!(
            worker_id = %req.worker_id,
            worker_schema = %req.schema_version,
            cp_schema = %API_SCHEMA_VERSION,
            "Schema version incompatible, rejecting registration"
        );
        return Ok(Json(WorkerRegistrationResponse {
            accepted: false,
            worker_id: req.worker_id,
            rejection_reason: Some(format!(
                "Schema version incompatible: worker={}, control_plane={}. Major.minor must match.",
                req.schema_version, API_SCHEMA_VERSION
            )),
            heartbeat_interval_secs: 30,
            kv_quota_bytes: None,
            kv_residency_policy_id: None,
        }));
    }

    // 5. Get tenant for KV quota information
    let tenant = state
        .db
        .get_tenant(&req.tenant_id)
        .await
        .map_err(|e| db_error_msg("failed to fetch tenant", e))?;

    let (kv_quota_bytes, kv_residency_policy_id) = match tenant {
        Some(t) => {
            let quota = t.max_kv_cache_bytes.map(|bytes| bytes as u64);
            (quota, t.kv_residency_policy_id)
        }
        None => {
            warn!(
                worker_id = %req.worker_id,
                tenant_id = %req.tenant_id,
                "Tenant not found during worker registration, using default KV quota (unlimited)"
            );
            (None, None)
        }
    };

    // 6. Get node_id from plan's tenant (for single-node, use "local")
    let node_id = "local".to_string();

    // 7. Register worker in database
    let params = WorkerRegistrationParams {
        worker_id: req.worker_id.clone(),
        tenant_id: req.tenant_id.clone(),
        node_id,
        plan_id: req.plan_id.clone(),
        uds_path: req.uds_path.clone(),
        pid: req.pid,
        manifest_hash: req.manifest_hash.clone(),
        schema_version: req.schema_version.clone(),
        api_version: req.api_version.clone(),
    };

    state
        .db
        .register_worker(params)
        .await
        .map_err(|e| db_error_msg("failed to register worker", e))?;

    state.worker_runtime.insert(
        req.worker_id.clone(),
        crate::state::WorkerRuntimeInfo {
            backend: req.backend.clone(),
            model_hash: req.model_hash.clone(),
            capabilities: req.capabilities.clone(),
        },
    );

    info!(
        worker_id = %req.worker_id,
        tenant_id = %req.tenant_id,
        plan_id = %req.plan_id,
        "Worker registered successfully with status 'registered'"
    );

    // 8. Log successful registration (via tracing since no Claims available)
    info!(
        event = "worker.registered",
        worker_id = %req.worker_id,
        tenant_id = %req.tenant_id,
        plan_id = %req.plan_id,
        manifest_hash = %req.manifest_hash,
        schema_version = %req.schema_version,
        api_version = %req.api_version,
        capabilities = ?req.capabilities,
        kv_quota_bytes = ?kv_quota_bytes,
        kv_residency_policy_id = ?kv_residency_policy_id,
        "Worker registration successful"
    );

    push_worker_health_event(
        &state,
        &req.tenant_id,
        &req.worker_id,
        HealthEventKind::WorkerRegistered,
        None,
        Some("registered".to_string()),
        None,
    )
    .await;

    Ok(Json(WorkerRegistrationResponse {
        accepted: true,
        worker_id: req.worker_id,
        rejection_reason: None,
        heartbeat_interval_secs: 30,
        kv_quota_bytes,
        kv_residency_policy_id,
    }))
}

/// Notify worker status change
///
/// Workers call this endpoint to notify the control plane of status changes.
/// This uses validated state transitions.
///
/// **Note:** This endpoint is called by workers, not by users. No auth required.
///
/// # Example
/// ```
/// POST /v1/workers/status
/// {
///   "worker_id": "01234567-89ab-cdef-0123-456789abcdef",
///   "status": "serving",
///   "reason": "model loaded successfully"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/workers/status",
    request_body = WorkerStatusNotification,
    responses(
        (status = 200, description = "Status updated"),
        (status = 400, description = "Invalid transition", body = ErrorResponse),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn notify_worker_status(
    State(state): State<AppState>,
    Json(req): Json<WorkerStatusNotification>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        worker_id = %req.worker_id,
        status = %req.status,
        reason = %req.reason,
        "Worker status notification received"
    );

    let worker_row = state
        .db
        .get_worker(&req.worker_id)
        .await
        .map_err(|e| db_error_msg("failed to fetch worker", e))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Worker not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Worker ID: {}", req.worker_id)),
                ),
            )
        })?;

    let previous_status = Some(worker_row.status.clone());
    let worker_tenant_id = worker_row.tenant_id.clone();

    // Use transition_worker_status which validates and records history
    state
        .db
        .transition_worker_status(&req.worker_id, &req.status, &req.reason, None)
        .await
        .map_err(|e| {
            // Check if it's a lifecycle error (invalid transition)
            let err_str = e.to_string();
            if err_str.contains("Invalid worker transition") {
                error!(
                    worker_id = %req.worker_id,
                    error = %err_str,
                    "Invalid worker status transition"
                );
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("Invalid status transition")
                            .with_code("INVALID_TRANSITION")
                            .with_string_details(err_str),
                    ),
                )
            } else if err_str.contains("not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Worker not found")
                            .with_code("NOT_FOUND")
                            .with_string_details(format!("Worker ID: {}", req.worker_id)),
                    ),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to update worker status")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(err_str),
                    ),
                )
            }
        })?;

    info!(
        worker_id = %req.worker_id,
        status = %req.status,
        "Worker status updated successfully"
    );

    let event_kind = if req.status.to_ascii_lowercase() == "crashed" {
        HealthEventKind::FatalError
    } else {
        HealthEventKind::HealthStateChange
    };

    push_worker_health_event(
        &state,
        &worker_tenant_id,
        &req.worker_id,
        event_kind,
        previous_status,
        Some(req.status.clone()),
        Some(req.reason.clone()),
    )
    .await;

    Ok(Json(serde_json::json!({
        "success": true,
        "worker_id": req.worker_id,
        "status": req.status,
    })))
}

/// Get worker status history
///
/// Returns the status transition history for a worker.
///
/// **Permissions:** Requires Operator or Admin role.
#[utoipa::path(
    get,
    path = "/v1/workers/{worker_id}/history",
    params(
        ("worker_id" = String, Path, description = "Worker ID"),
        ("limit" = Option<i32>, Query, description = "Max records to return (default 50)")
    ),
    responses(
        (status = 200, description = "Status history"),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn get_worker_history(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<
    Json<Vec<adapteros_db::workers::WorkerStatusHistoryRecord>>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Check worker exists
    let worker = state
        .db
        .get_worker(&worker_id)
        .await
        .map_err(|e| db_error_msg("database error", e))?
        .ok_or_else(|| {
            not_found_with_details("worker not found", format!("Worker ID: {}", worker_id))
        })?;

    // Verify tenant access (for multi-tenant scenarios)
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    if claims.tenant_id != worker.tenant_id && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Access denied")
                    .with_code("FORBIDDEN")
                    .with_string_details("Worker belongs to different tenant"),
            ),
        ));
    }

    let history = state
        .db
        .get_worker_status_history(&worker_id, query.limit)
        .await
        .map_err(|e| db_error_msg("failed to get worker history", e))?;

    Ok(Json(history))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<i32>,
}
