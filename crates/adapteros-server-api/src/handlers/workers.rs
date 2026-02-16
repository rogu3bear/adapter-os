use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::sse::{SseStreamType, SystemHealthEvent};
use crate::state::AppState;
use crate::types::*;
use crate::worker_capabilities::{normalize_worker_capabilities, parse_worker_capabilities};
use adapteros_api_types::workers::{
    WorkerRegistrationRequest, WorkerRegistrationResponse, WorkerStatusNotification,
};
use adapteros_config::reject_tmp_socket;
use adapteros_core::{
    identity::IdentityEnvelope, version::API_SCHEMA_VERSION, AosError, WorkerStatus,
};
use adapteros_db::users::Role;
use adapteros_db::workers::{is_schema_compatible, WorkerRegistrationParams};
use adapteros_telemetry::{build_health_event, make_health_payload, HealthEventKind};
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde::Deserialize;
use std::str::FromStr;
use tracing::{error, info, warn};

#[derive(Deserialize)]
pub struct ListWorkersQuery {
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub include_inactive: Option<bool>,
}

pub(crate) fn is_terminal_worker_status(status: &str) -> bool {
    status.eq_ignore_ascii_case("stopped")
        || status.eq_ignore_ascii_case("error")
        || status.eq_ignore_ascii_case("crashed")
        || status.eq_ignore_ascii_case("failed")
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
#[utoipa::path(
    post,
    path = "/v1/workers/spawn",
    request_body = SpawnWorkerRequest,
    responses(
        (status = 200, description = "Worker spawned", body = WorkerResponse),
        (status = 403, description = "Forbidden - tenant mismatch", body = ErrorResponse),
        (status = 404, description = "Node not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn worker_spawn(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<SpawnWorkerRequest>,
) -> ApiResult<WorkerResponse> {
    require_any_role(&claims, &[Role::Operator])?;

    // PRD-RECT-002: Validate caller can spawn workers for the requested tenant
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    if !is_admin && req.tenant_id != claims.tenant_id {
        return Err(
            ApiError::forbidden("cannot spawn worker for another tenant")
                .with_details("Non-admin users can only spawn workers for their own tenant"),
        );
    }
    if is_admin && req.tenant_id != claims.tenant_id {
        crate::security::validate_tenant_isolation(&claims, &req.tenant_id)?;
    }

    // Look up node by ID
    let node = state
        .db
        .get_node(&req.node_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            ApiError::not_found("node").with_details(format!("Node ID: {}", req.node_id))
        })?;

    // Prepare spawn request for node agent
    let spawn_req = serde_json::json!({
        "tenant_id": req.tenant_id,
        "plan_id": req.plan_id,
        "node_id": req.node_id,
        "uds_path": req.uds_path,
    });

    // Send HTTP POST to node agent
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| {
            ApiError::internal("failed to create HTTP client").with_details(e.to_string())
        })?;
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
        ApiError::bad_gateway("failed to contact node agent")
            .with_details(format!("{} (after {} attempts)", e, max_attempts))
    })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(ApiError::internal("node agent spawn failed").with_details(error_text));
    }

    let spawn_response: serde_json::Value = response.json().await.map_err(|e| {
        ApiError::internal("failed to parse node agent response").with_details(e.to_string())
    })?;

    let pid = spawn_response["pid"].as_i64().ok_or_else(|| {
        ApiError::internal("invalid response from node agent")
            .with_details("missing or invalid PID field")
    })? as i32;

    // Create UDS path for worker
    // SECURITY: Validate tenant_id doesn't contain path traversal sequences
    if req.tenant_id.contains("..") || req.tenant_id.contains('/') || req.tenant_id.contains('\\') {
        return Err(ApiError::bad_request("invalid tenant_id")
            .with_details(format!("tenant_id '{}' contains invalid characters (path traversal sequences or slashes are not allowed)", req.tenant_id)));
    }

    let uds_path = format!("/var/run/aos/{}/worker.sock", req.tenant_id);

    // SECURITY: Validate constructed path is safe
    let uds_path_buf = std::path::PathBuf::from(&uds_path);
    reject_tmp_socket(&uds_path_buf, "worker-socket").map_err(|e| {
        ApiError::internal("worker socket path validation failed").with_details(e.to_string())
    })?;

    // Register worker using Db trait method
    use adapteros_db::workers::WorkerInsertBuilder;
    let worker_id = crate::id_generator::readable_id(adapteros_id::IdPrefix::Wrk, "worker");
    let mut builder = WorkerInsertBuilder::new()
        .id(&worker_id)
        .tenant_id(&req.tenant_id)
        .node_id(&req.node_id)
        .plan_id(&req.plan_id)
        .uds_path(&uds_path)
        .status(WorkerStatus::Created.as_str());
    builder = builder.pid(pid);
    let params = builder.build().map_err(|e| {
        ApiError::internal("failed to build worker parameters").with_details(e.to_string())
    })?;
    state
        .db
        .insert_worker(params)
        .await
        .map_err(ApiError::db_error)?;

    // Return worker info
    let display_name = adapteros_id::display_name_for(&worker_id);
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
        capabilities_detail: None,
        backend: None,
        model_id: None,
        model_hash: None,
        tokenizer_hash_b3: None,
        tokenizer_vocab_size: None,
        model_loaded: false,
        cache_used_mb: None,
        cache_max_mb: None,
        cache_pinned_entries: None,
        cache_active_entries: None,
        coreml_failure_stage: None,
        coreml_failure_reason: None,
        display_name,
    }))
}

/// List workers with optional tenant filter
///
/// PRD-RECT-002: Non-admin users can only list workers from their own tenant.
/// Admins can list workers from any tenant or list all workers.
#[utoipa::path(
    get,
    path = "/v1/workers",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        (
            "include_inactive" = Option<bool>,
            Query,
            description = "Include terminal workers in results (stopped/error/crashed/failed)"
        )
    ),
    responses(
        (status = 200, description = "Workers list", body = Vec<WorkerResponse>),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn list_workers(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListWorkersQuery>,
) -> ApiResult<Vec<WorkerResponse>> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");

    // PRD-RECT-002: Determine effective tenant filter based on role
    let effective_tenant_id = if is_admin {
        // Admin: can query any tenant or all workers (use provided query.tenant_id or None)
        query.tenant_id.clone()
    } else {
        // Non-admin: forced to their own tenant (ignore query.tenant_id)
        Some(claims.tenant_id.clone())
    };

    let workers = if let Some(tenant_id) = effective_tenant_id {
        state
            .db
            .list_workers_by_tenant(&tenant_id)
            .await
            .map_err(ApiError::db_error)?
    } else {
        // Only admin reaches here (listing all workers)
        state
            .db
            .list_all_workers()
            .await
            .map_err(ApiError::db_error)?
    };

    let include_inactive = query.include_inactive.unwrap_or(false);
    let workers = if include_inactive {
        workers
    } else {
        workers
            .into_iter()
            .filter(|w| !is_terminal_worker_status(&w.status))
            .collect()
    };

    // Build response with model lookups
    let mut response = Vec::with_capacity(workers.len());
    for w in workers {
        // Look up model_id from hash if available
        let model_id = if let Some(ref hash) = w.model_hash_b3 {
            state
                .db
                .get_model_by_hash(hash)
                .await
                .ok()
                .flatten()
                .map(|m| m.id)
        } else {
            None
        };

        // Get runtime info if available
        let runtime = state.worker_runtime.get(&w.id);
        let (cache_used_mb, cache_max_mb, cache_pinned_entries, cache_active_entries) =
            if let Some(ref rt) = runtime {
                (
                    rt.cache_used_mb,
                    rt.cache_max_mb,
                    rt.cache_pinned_entries,
                    rt.cache_active_entries,
                )
            } else {
                (None, None, None, None)
            };

        let tokenizer_hash_b3 = runtime
            .as_ref()
            .and_then(|rt| rt.tokenizer_hash_b3.clone())
            .or_else(|| w.tokenizer_hash_b3.clone());
        let tokenizer_vocab_size = runtime
            .as_ref()
            .and_then(|rt| rt.tokenizer_vocab_size)
            .or_else(|| w.tokenizer_vocab_size.map(|v| v as u32));

        // Parse capabilities from JSON
        let capabilities = w
            .capabilities_json
            .as_ref()
            .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
            .unwrap_or_default();

        let capabilities_detail = runtime
            .as_ref()
            .and_then(|rt| rt.capabilities_detail.clone())
            .or_else(|| {
                w.capabilities_json
                    .as_ref()
                    .and_then(|json| serde_json::from_str(json).ok())
            });

        let display_name = adapteros_id::display_name_for(&w.id);
        response.push(WorkerResponse {
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
            capabilities,
            capabilities_detail,
            backend: w.backend,
            model_id,
            model_hash: w.model_hash_b3.clone(),
            model_loaded: w.model_hash_b3.is_some(),
            tokenizer_hash_b3,
            tokenizer_vocab_size,
            cache_used_mb,
            cache_max_mb,
            cache_pinned_entries,
            cache_active_entries,
            coreml_failure_stage: runtime
                .as_ref()
                .and_then(|rt| rt.coreml_failure_stage.clone()),
            coreml_failure_reason: runtime
                .as_ref()
                .and_then(|rt| rt.coreml_failure_reason.clone()),
            display_name,
        });
    }

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
) -> ApiResult<crate::types::WorkerStopResponse> {
    // Require worker manage permission
    crate::permissions::require_permission(&claims, crate::permissions::Permission::WorkerManage)?;

    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id).await?;

    // PRD-RECT-002: Admins with admin_tenants grants can access workers across tenants.
    // Returns 404 for both missing and cross-tenant workers (for non-admins).
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    let worker = if is_admin {
        let w = state
            .db
            .get_worker(&worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?;
        crate::security::validate_tenant_isolation(&claims, &w.tenant_id)?;
        w
    } else {
        state
            .db
            .get_worker_for_tenant(&claims.tenant_id, &worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?
    };

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
                return Err(ApiError::conflict("invalid state transition")
                    .with_details(format!(
                        "Cannot stop worker in '{}' state. Valid transitions from serving: draining, crashed",
                        previous_status
                    )));
            }
            return Err(ApiError::internal("failed to transition worker status")
                .with_details(e.to_string()));
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

    // Emit SSE lifecycle event for drain start
    state
        .sse_manager
        .emit_lifecycle(
            SseStreamType::Alerts,
            &SystemHealthEvent::DrainStarted {
                worker_id: worker_id.clone(),
                previous_status: previous_status.clone(),
            },
        )
        .await;

    // Emit telemetry event
    tracing::info!(
        event = "worker.stop.initiated",
        worker_id = %worker_id,
        previous_status = %previous_status,
        actor = %claims.sub,
        "Worker stop initiated (draining)"
    );

    // Audit log
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "worker.stop",
        crate::audit_helper::resources::WORKER,
        Some(&worker_id),
        None,
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(crate::types::WorkerStopResponse {
        worker_id,
        success: true,
        message: "Worker draining initiated. Worker will transition to stopped when complete."
            .to_string(),
        previous_status,
        stopped_at,
    }))
}

/// Drain a worker (transition to draining state without SIGTERM)
///
/// Gracefully drains a worker by transitioning it to the 'draining' state.
/// Unlike `stop_worker`, this does NOT send SIGTERM to the process.
/// The worker continues processing in-flight requests and should eventually
/// transition to 'stopped' via the status notification endpoint.
///
/// **Use case:** UI-initiated drain where the operator wants to stop new requests
/// but allow current work to complete before deciding whether to stop the process.
///
/// **Permissions:** Requires `WorkerManage` permission (Operator or Admin role).
///
/// **Telemetry:** Emits `worker.drain` event.
///
/// # Example
/// ```
/// POST /v1/workers/{worker_id}/drain
/// ```
#[utoipa::path(
    post,
    path = "/v1/workers/{worker_id}/drain",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Worker drain initiated", body = crate::types::WorkerStopResponse),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 409, description = "Invalid state transition", body = ErrorResponse),
        (status = 500, description = "Failed to drain worker", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn drain_worker(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> ApiResult<crate::types::WorkerStopResponse> {
    // Require worker manage permission
    crate::permissions::require_permission(&claims, crate::permissions::Permission::WorkerManage)?;

    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id).await?;

    // PRD-RECT-002: Admins with admin_tenants grants can access workers across tenants.
    // Returns 404 for both missing and cross-tenant workers (for non-admins).
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    let worker = if is_admin {
        let w = state
            .db
            .get_worker(&worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?;
        crate::security::validate_tenant_isolation(&claims, &w.tenant_id)?;
        w
    } else {
        state
            .db
            .get_worker_for_tenant(&claims.tenant_id, &worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?
    };

    let previous_status = worker.status.clone();

    // Use validated state transition to 'draining'
    match state
        .db
        .transition_worker_status(
            &worker_id,
            "draining",
            "operator drain request",
            Some(&claims.sub),
        )
        .await
    {
        Ok(_) => {
            // Emit SSE lifecycle event for drain
            state
                .sse_manager
                .emit_lifecycle(
                    SseStreamType::Alerts,
                    &SystemHealthEvent::DrainStarted {
                        worker_id: worker_id.clone(),
                        previous_status: previous_status.clone(),
                    },
                )
                .await;

            info!(
                event = "worker.drain",
                worker_id = %worker_id,
                previous_status = %previous_status,
                actor = %claims.sub,
                "Worker transitioned to draining"
            );
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("Lifecycle") || err_str.contains("Invalid") {
                warn!(
                    event = "worker.drain.invalid_transition",
                    worker_id = %worker_id,
                    previous_status = %previous_status,
                    error = %e,
                    "Invalid state transition attempted"
                );
                return Err(
                    ApiError::conflict("invalid state transition").with_details(format!(
                        "Cannot drain worker in '{}' state. Valid source states: healthy, serving",
                        previous_status
                    )),
                );
            }
            return Err(ApiError::internal("failed to transition worker status")
                .with_details(e.to_string()));
        }
    }

    let drained_at = chrono::Utc::now().to_rfc3339();

    // Audit log
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "worker.drain",
        crate::audit_helper::resources::WORKER,
        Some(&worker_id),
        None,
    )
    .await
    {
        warn!(error = %e, "Audit log failed");
    }

    Ok(Json(crate::types::WorkerStopResponse {
        worker_id,
        success: true,
        message: "Worker draining initiated. Worker will stop accepting new requests.".to_string(),
        previous_status,
        stopped_at: drained_at,
    }))
}

/// Restart a worker process
///
/// Initiates a restart by transitioning the worker to `draining` and sending
/// SIGTERM to allow graceful shutdown. The worker supervisor is expected to
/// start a replacement process.
#[utoipa::path(
    post,
    path = "/v1/workers/{worker_id}/restart",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Worker restart initiated", body = crate::types::WorkerStopResponse),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 409, description = "Invalid state transition", body = ErrorResponse),
        (status = 500, description = "Failed to restart worker", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn restart_worker(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> ApiResult<crate::types::WorkerStopResponse> {
    crate::permissions::require_permission(&claims, crate::permissions::Permission::WorkerManage)?;

    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id).await?;

    // PRD-RECT-002: Admins with admin_tenants grants can access workers across tenants.
    // Returns 404 for both missing and cross-tenant workers (for non-admins).
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    let worker = if is_admin {
        let w = state
            .db
            .get_worker(&worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?;
        crate::security::validate_tenant_isolation(&claims, &w.tenant_id)?;
        w
    } else {
        state
            .db
            .get_worker_for_tenant(&claims.tenant_id, &worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?
    };

    let previous_status = worker.status.clone();
    let mut transitioned_to_draining = false;

    // Restart follows stop semantics: worker must be healthy/serving or already draining.
    if previous_status.eq_ignore_ascii_case("draining") {
        info!(
            event = "worker.restart.already_draining",
            worker_id = %worker_id,
            actor = %claims.sub,
            "Worker already draining; restart signal will be re-sent"
        );
    } else if previous_status.eq_ignore_ascii_case("healthy")
        || previous_status.eq_ignore_ascii_case("serving")
    {
        match state
            .db
            .transition_worker_status(
                &worker_id,
                "draining",
                "operator restart request",
                Some(&claims.sub),
            )
            .await
        {
            Ok(_) => {
                transitioned_to_draining = true;
                info!(
                    event = "worker.restart.draining",
                    worker_id = %worker_id,
                    actor = %claims.sub,
                    "Worker transitioned to draining for restart"
                );
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("Lifecycle") || err_str.contains("Invalid") {
                    warn!(
                        event = "worker.restart.invalid_transition",
                        worker_id = %worker_id,
                        previous_status = %previous_status,
                        error = %e,
                        "Invalid state transition attempted"
                    );
                    return Err(
                        ApiError::conflict("invalid state transition").with_details(format!(
                            "Cannot restart worker in '{}' state. Valid source states: healthy, serving, draining",
                            previous_status
                        )),
                    );
                }
                return Err(ApiError::internal("failed to transition worker status")
                    .with_details(e.to_string()));
            }
        }
    } else {
        return Err(
            ApiError::conflict("invalid state transition").with_details(format!(
            "Cannot restart worker in '{}' state. Valid source states: healthy, serving, draining",
            previous_status
        )),
        );
    }

    if let Some(pid) = worker.pid {
        info!(
            event = "worker.restart.signal",
            worker_id = %worker_id,
            pid = %pid,
            "Signaling worker process for restart (SIGTERM)"
        );

        #[cfg(unix)]
        {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGTERM,
            );
        }
    }

    let restarted_at = chrono::Utc::now().to_rfc3339();

    if transitioned_to_draining {
        state
            .sse_manager
            .emit_lifecycle(
                SseStreamType::Alerts,
                &SystemHealthEvent::DrainStarted {
                    worker_id: worker_id.clone(),
                    previous_status: previous_status.clone(),
                },
            )
            .await;
    }

    info!(
        event = "worker.restart.initiated",
        worker_id = %worker_id,
        previous_status = %previous_status,
        actor = %claims.sub,
        "Worker restart initiated"
    );

    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "worker.restart",
        crate::audit_helper::resources::WORKER,
        Some(&worker_id),
        None,
    )
    .await
    {
        warn!(error = %e, "Audit log failed");
    }

    Ok(Json(crate::types::WorkerStopResponse {
        worker_id,
        success: true,
        message: if transitioned_to_draining {
            "Worker restart initiated. Worker is draining and will be restarted by supervisor."
                .to_string()
        } else {
            "Worker is already draining. Restart signal has been re-sent.".to_string()
        },
        previous_status,
        stopped_at: restarted_at,
    }))
}

/// Decommission (remove) a worker record.
///
/// Guardrail: decommission is only allowed when the worker is already in a
/// terminal lifecycle state (`stopped`, `error`, `crashed`, or `failed`).
#[utoipa::path(
    delete,
    path = "/v1/workers/{worker_id}",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Worker decommissioned", body = crate::types::WorkerStopResponse),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 409, description = "Worker must be terminal before decommission", body = ErrorResponse),
        (status = 500, description = "Failed to decommission worker", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn decommission_worker(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> ApiResult<crate::types::WorkerStopResponse> {
    crate::permissions::require_permission(&claims, crate::permissions::Permission::WorkerManage)?;

    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id).await?;

    // PRD-RECT-002: Admins with admin_tenants grants can access workers across tenants.
    // Returns 404 for both missing and cross-tenant workers (for non-admins).
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    let worker = if is_admin {
        let w = state
            .db
            .get_worker(&worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?;
        crate::security::validate_tenant_isolation(&claims, &w.tenant_id)?;
        w
    } else {
        state
            .db
            .get_worker_for_tenant(&claims.tenant_id, &worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?
    };

    let previous_status = worker.status.clone();
    if let Err(err) = state.db.remove_worker_if_terminal(&worker_id).await {
        let error_message = err.to_string();
        match err {
            AosError::Lifecycle(_) => {
                warn!(
                    event = "worker.decommission.rejected_non_terminal",
                    worker_id = %worker_id,
                    status = %previous_status,
                    actor = %claims.sub,
                    "Worker decommission rejected because worker is non-terminal"
                );
                if let Err(e) = crate::audit_helper::log_failure(
                    &state.db,
                    &claims,
                    "worker.decommission",
                    crate::audit_helper::resources::WORKER,
                    Some(&worker_id),
                    &error_message,
                    None,
                )
                .await
                {
                    warn!(error = %e, "Audit log failed");
                }
                return Err(ApiError::conflict("worker is not in terminal state")
                    .with_details(error_message));
            }
            AosError::NotFound(_) => {
                return Err(
                    ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
                );
            }
            _ => {
                return Err(
                    ApiError::internal("failed to decommission worker").with_details(error_message)
                );
            }
        }
    }

    let decommissioned_at = chrono::Utc::now().to_rfc3339();

    info!(
        event = "worker.decommissioned",
        worker_id = %worker_id,
        previous_status = %previous_status,
        actor = %claims.sub,
        "Worker decommissioned"
    );

    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "worker.decommission",
        crate::audit_helper::resources::WORKER,
        Some(&worker_id),
        None,
    )
    .await
    {
        warn!(error = %e, "Audit log failed");
    }

    Ok(Json(crate::types::WorkerStopResponse {
        worker_id,
        success: true,
        message: "Worker decommissioned and removed from control plane.".to_string(),
        previous_status,
        stopped_at: decommissioned_at,
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
) -> ApiResult<WorkerRegistrationResponse> {
    let heartbeat_interval_secs: u32 = state
        .config
        .read()
        .map(|cfg| cfg.server.worker_heartbeat_interval_secs)
        .unwrap_or(30)
        .try_into()
        .unwrap_or(30);

    info!(
        worker_id = %req.worker_id,
        tenant_id = %req.tenant_id,
        plan_id = %req.plan_id,
        manifest_hash = %req.manifest_hash,
        "Worker registration request received"
    );

    // 0. Check strict mode mismatch and warn if different
    if req.strict_mode != state.strict_mode {
        warn!(
            worker_id = %req.worker_id,
            worker_strict = req.strict_mode,
            cp_strict = state.strict_mode,
            "Strict mode mismatch between worker and control plane. \
             This may cause inconsistent error handling behavior."
        );
    }

    // 1. Check if worker already exists
    let exists = state
        .db
        .worker_exists(&req.worker_id)
        .await
        .map_err(ApiError::db_error)?;

    if exists {
        info!(
            worker_id = %req.worker_id,
            "Worker already present; updating registration metadata"
        );
    }

    // 2. Get plan and extract expected manifest_hash
    // Use get_plan_by_plan_id since req.plan_id is the logical plan identifier (e.g., "dev"),
    // not the primary key (which is a UUID)
    let plan = state
        .db
        .get_plan_by_plan_id(&req.plan_id)
        .await
        .map_err(ApiError::db_error)?;

    let plan = match plan {
        Some(p) => p,
        None => {
            // In development mode, auto-create the manifest and plan
            let is_dev_mode = state.runtime_mode.map(|m| m.is_dev()).unwrap_or(false);

            if is_dev_mode {
                info!(
                    worker_id = %req.worker_id,
                    plan_id = %req.plan_id,
                    manifest_hash = %req.manifest_hash,
                    "Plan not found in dev mode, auto-creating manifest and plan"
                );

                // Ensure manifest exists
                let manifest_exists = state
                    .db
                    .get_manifest_by_hash(&req.manifest_hash)
                    .await
                    .map_err(ApiError::db_error)?;

                if manifest_exists.is_none() {
                    // Create placeholder manifest
                    state
                        .db
                        .create_manifest(&req.tenant_id, &req.manifest_hash, "{}")
                        .await
                        .map_err(ApiError::db_error)?;
                    info!(
                        manifest_hash = %req.manifest_hash,
                        tenant_id = %req.tenant_id,
                        "Auto-created manifest for dev mode"
                    );
                }

                // Create the plan with kernel hashes for dev mode
                // Uses MLX as the primary backend for Apple Silicon
                let kernel_hashes_json = r#"["mlx_primary"]"#;
                state
                    .db
                    .create_plan(
                        &req.plan_id,
                        &req.tenant_id,
                        &req.plan_id,
                        &req.manifest_hash,
                        kernel_hashes_json,
                        &req.manifest_hash,
                    )
                    .await
                    .map_err(ApiError::db_error)?;
                info!(
                    plan_id = %req.plan_id,
                    manifest_hash = %req.manifest_hash,
                    tenant_id = %req.tenant_id,
                    "Auto-created plan for dev mode"
                );

                // Fetch the newly created plan
                state
                    .db
                    .get_plan_by_plan_id(&req.plan_id)
                    .await
                    .map_err(ApiError::db_error)?
                    .ok_or_else(|| {
                        ApiError::internal("Plan creation succeeded but plan not found")
                    })?
            } else {
                warn!(
                    worker_id = %req.worker_id,
                    plan_id = %req.plan_id,
                    "Plan not found, rejecting registration"
                );
                return Ok(Json(WorkerRegistrationResponse {
                    accepted: false,
                    worker_id: req.worker_id,
                    rejection_reason: Some(format!("Plan not found: {}", req.plan_id)),
                    heartbeat_interval_secs,
                    kv_quota_bytes: None,
                    kv_residency_policy_id: None,
                    cp_strict_mode: state.strict_mode,
                }));
            }
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
            heartbeat_interval_secs,
            kv_quota_bytes: None,
            kv_residency_policy_id: None,
            cp_strict_mode: state.strict_mode,
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
            heartbeat_interval_secs,
            kv_quota_bytes: None,
            kv_residency_policy_id: None,
            cp_strict_mode: state.strict_mode,
        }));
    }

    // 5. Get tenant for KV quota information
    let tenant = state
        .db
        .get_tenant(&req.tenant_id)
        .await
        .map_err(ApiError::db_error)?;

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

    let capabilities_detail = req
        .capabilities_detail
        .clone()
        .map(normalize_worker_capabilities)
        .or_else(|| {
            let derived =
                parse_worker_capabilities(None, req.backend.as_deref(), &req.capabilities);
            if derived.is_none() {
                warn!(
                    worker_id = %req.worker_id,
                    backend = ?req.backend,
                    "Worker registration missing structured capabilities; routing will degrade"
                );
            }
            derived
        });

    // 7. Register worker in database
    // Use plan.id (UUID) not req.plan_id (logical name like "dev") because
    // workers.plan_id has a FK constraint to plans.id (the UUID)
    let params = WorkerRegistrationParams {
        worker_id: req.worker_id.clone(),
        tenant_id: req.tenant_id.clone(),
        node_id,
        plan_id: plan.id.clone(),
        uds_path: req.uds_path.clone(),
        pid: req.pid,
        manifest_hash: req.manifest_hash.clone(),
        backend: req.backend.clone(),
        model_hash_b3: req.model_hash.clone(),
        tokenizer_hash_b3: req.tokenizer_hash_b3.clone(),
        tokenizer_vocab_size: req.tokenizer_vocab_size.map(|v| v as i64),
        capabilities_json: capabilities_detail
            .as_ref()
            .and_then(|caps| serde_json::to_string(caps).ok())
            .or_else(|| {
                if req.capabilities.is_empty() {
                    None
                } else {
                    serde_json::to_string(&req.capabilities).ok()
                }
            }),
        schema_version: req.schema_version.clone(),
        api_version: req.api_version.clone(),
    };

    state
        .db
        .register_worker(params)
        .await
        .map_err(ApiError::db_error)?;

    state.worker_runtime.insert(
        req.worker_id.clone(),
        crate::state::WorkerRuntimeInfo {
            backend: req.backend.clone(),
            model_hash: req.model_hash.clone(),
            capabilities: req.capabilities.clone(),
            capabilities_detail: capabilities_detail.clone(),
            cache_used_mb: None,
            cache_max_mb: None,
            cache_pinned_entries: None,
            cache_active_entries: None,
            tokenizer_hash_b3: req.tokenizer_hash_b3.clone(),
            tokenizer_vocab_size: req.tokenizer_vocab_size,
            coreml_failure_stage: None,
            coreml_failure_reason: None,
            loaded_model_hash: None,
            model_load_state: None,
            cache_stats: None,
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
        capabilities_detail = ?capabilities_detail,
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
        heartbeat_interval_secs,
        kv_quota_bytes,
        kv_residency_policy_id,
        cp_strict_mode: state.strict_mode,
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
) -> Result<Json<serde_json::Value>, ApiError> {
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
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            ApiError::not_found("Worker").with_details(format!("Worker ID: {}", req.worker_id))
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
                ApiError::bad_request("Invalid status transition").with_details(err_str)
            } else if err_str.contains("not found") {
                ApiError::not_found("Worker").with_details(format!("Worker ID: {}", req.worker_id))
            } else {
                ApiError::internal("Failed to update worker status").with_details(err_str)
            }
        })?;

    info!(
        worker_id = %req.worker_id,
        status = %req.status,
        "Worker status updated successfully"
    );

    // Emit SSE lifecycle event for worker state change
    state
        .sse_manager
        .emit_lifecycle(
            SseStreamType::Alerts,
            &SystemHealthEvent::WorkerStateChanged {
                worker_id: req.worker_id.clone(),
                previous: previous_status.clone().unwrap_or_default(),
                current: req.status.clone(),
                reason: req.reason.clone(),
            },
        )
        .await;

    let event_kind = if req.status.eq_ignore_ascii_case("crashed") {
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

    // Update cache metrics and model state in worker runtime if provided
    let has_cache_updates = req.cache_used_mb.is_some()
        || req.cache_max_mb.is_some()
        || req.cache_pinned_entries.is_some()
        || req.cache_active_entries.is_some()
        || req.cache_memory_bytes.is_some()
        || req.cache_hit_ratio.is_some();
    let has_model_updates = req.loaded_model_hash.is_some() || req.model_load_state.is_some();

    if has_cache_updates || has_model_updates {
        if let Some(mut entry) = state.worker_runtime.get_mut(&req.worker_id) {
            // Update legacy cache fields for backward compatibility
            if let Some(v) = req.cache_used_mb {
                entry.cache_used_mb = Some(v);
            }
            if let Some(v) = req.cache_max_mb {
                entry.cache_max_mb = Some(v);
            }
            if let Some(v) = req.cache_pinned_entries {
                entry.cache_pinned_entries = Some(v);
            }
            if let Some(v) = req.cache_active_entries {
                entry.cache_active_entries = Some(v);
            }

            // Update model state for smarter routing
            if let Some(ref hash) = req.loaded_model_hash {
                entry.loaded_model_hash = Some(hash.clone());
            }
            if let Some(ref load_state) = req.model_load_state {
                entry.model_load_state = Some(load_state.clone());
            }

            // Update aggregated cache stats
            if has_cache_updates {
                let cache_stats = entry.cache_stats.get_or_insert_with(Default::default);
                if let Some(v) = req.cache_used_mb {
                    cache_stats.used_mb = Some(v);
                }
                if let Some(v) = req.cache_max_mb {
                    cache_stats.max_mb = Some(v);
                }
                if let Some(v) = req.cache_pinned_entries {
                    cache_stats.pinned_entries = Some(v);
                }
                if let Some(v) = req.cache_active_entries {
                    cache_stats.active_entries = Some(v);
                }
                if let Some(v) = req.cache_memory_bytes {
                    cache_stats.memory_bytes = Some(v);
                }
                if let Some(v) = req.cache_hit_ratio {
                    cache_stats.hit_ratio = Some(v);
                }
            }
        }
    }

    // Persist tokenizer metadata when provided
    if req.tokenizer_hash_b3.is_some() || req.tokenizer_vocab_size.is_some() {
        if let Some(mut entry) = state.worker_runtime.get_mut(&req.worker_id) {
            if let Some(hash) = req.tokenizer_hash_b3.clone() {
                entry.tokenizer_hash_b3 = Some(hash);
            }
            if let Some(vocab) = req.tokenizer_vocab_size {
                entry.tokenizer_vocab_size = Some(vocab);
            }
        }

        let _ = state
            .db
            .update_worker_heartbeat(
                &req.worker_id,
                None,
                req.tokenizer_hash_b3.as_deref(),
                req.tokenizer_vocab_size.map(|v| v as i64),
            )
            .await;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "worker_id": req.worker_id,
        "status": req.status,
    })))
}

/// Worker heartbeat endpoint
///
/// Lightweight liveness update that also captures tokenizer metadata and cache stats.
#[utoipa::path(
    post,
    path = "/v1/workers/heartbeat",
    request_body = WorkerHeartbeatRequest,
    responses(
        (status = 200, description = "Heartbeat accepted", body = WorkerHeartbeatResponse),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn worker_heartbeat(
    State(state): State<AppState>,
    Json(req): Json<WorkerHeartbeatRequest>,
) -> Result<Json<WorkerHeartbeatResponse>, ApiError> {
    // Validate status enum
    WorkerStatus::from_str(&req.status)
        .map_err(|e| ApiError::bad_request("invalid status").with_details(e.to_string()))?;

    // Persist heartbeat timestamp and tokenizer metadata
    if let Err(e) = state
        .db
        .update_worker_heartbeat(
            &req.worker_id,
            Some(&req.status),
            req.tokenizer_hash_b3.as_deref(),
            req.tokenizer_vocab_size.map(|v| v as i64),
        )
        .await
    {
        return match e {
            AosError::NotFound(_) => {
                Err(ApiError::not_found("worker")
                    .with_details(format!("Worker ID: {}", req.worker_id)))
            }
            _ => {
                Err(ApiError::internal("failed to record worker heartbeat")
                    .with_details(e.to_string()))
            }
        };
    }

    // Update runtime cache
    let mut entry = state
        .worker_runtime
        .entry(req.worker_id.clone())
        .or_default();

    if let Some(v) = req.cache_used_mb {
        entry.cache_used_mb = Some(v);
    }
    if let Some(v) = req.cache_max_mb {
        entry.cache_max_mb = Some(v);
    }
    if let Some(v) = req.cache_pinned_entries {
        entry.cache_pinned_entries = Some(v);
    }
    if let Some(v) = req.cache_active_entries {
        entry.cache_active_entries = Some(v);
    }
    if let Some(hash) = req.tokenizer_hash_b3.clone() {
        entry.tokenizer_hash_b3 = Some(hash);
    }
    if let Some(vocab) = req.tokenizer_vocab_size {
        entry.tokenizer_vocab_size = Some(vocab);
    }
    if let Some(stage) = req.coreml_failure_stage.clone() {
        entry.coreml_failure_stage = Some(stage);
    }
    if let Some(reason) = req.coreml_failure_reason.clone() {
        entry.coreml_failure_reason = Some(reason);
    }

    let next_heartbeat_secs: u32 = state
        .config
        .read()
        .map(|cfg| cfg.server.worker_heartbeat_interval_secs)
        .unwrap_or(30)
        .try_into()
        .unwrap_or(30);

    Ok(Json(WorkerHeartbeatResponse {
        acknowledged: true,
        next_heartbeat_secs,
    }))
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
) -> ApiResult<Vec<adapteros_db::workers::WorkerStatusHistoryRecord>> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id).await?;

    // PRD-RECT-002: Use tenant-scoped query for non-admins to prevent enumeration.
    // Admins can access workers across tenants (intentional for admin operations).
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    let _worker = if is_admin {
        // Admin: can access any worker
        state
            .db
            .get_worker(&worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?
    } else {
        // Non-admin: tenant-scoped query, returns 404 for cross-tenant (not 403)
        state
            .db
            .get_worker_for_tenant(&claims.tenant_id, &worker_id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| {
                // Same error for both "not found" and "cross-tenant" cases
                ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
            })?
    };

    let history = state
        .db
        .get_worker_status_history(&worker_id, query.limit)
        .await
        .map_err(ApiError::db_error)?;

    Ok(Json(history))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<i32>,
}

/// Receive fatal error report from worker (PRD-09 Phase 4)
///
/// Called by workers via the fatal error channel to report critical errors.
/// This endpoint records the incident in the database and logs it.
///
/// **Permissions:** Internal endpoint - typically called via UDS, but can be exposed for testing.
///
/// # Example
/// ```
/// POST /v1/workers/fatal
/// {
///   "worker_id": "worker-123",
///   "reason": "Out of memory",
///   "backtrace_snippet": "...",
///   "timestamp": "2025-01-15T10:30:00Z"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/workers/fatal",
    request_body = crate::types::WorkerFatal,
    responses(
        (status = 200, description = "Worker fatal incident recorded"),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn receive_worker_fatal(
    State(state): State<AppState>,
    Json(fatal_msg): Json<crate::types::WorkerFatal>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Log the fatal error at the control plane
    tracing::error!(
        event = "worker.fatal.received",
        worker_id = %fatal_msg.worker_id,
        reason = %fatal_msg.reason,
        timestamp = %fatal_msg.timestamp,
        has_backtrace = fatal_msg.backtrace_snippet.is_some(),
        "Control plane received worker fatal error"
    );

    // Get worker record to retrieve tenant_id
    let worker = state
        .db
        .get_worker(&fatal_msg.worker_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            ApiError::not_found("worker")
                .with_details(format!("Worker ID: {}", fatal_msg.worker_id))
        })?;

    // Insert worker incident with incident_type = "fatal"
    let incident_id = state
        .db
        .insert_worker_incident(
            &fatal_msg.worker_id,
            &worker.tenant_id,
            adapteros_db::WorkerIncidentType::Fatal,
            &fatal_msg.reason,
            fatal_msg.backtrace_snippet.as_deref(),
            None, // latency_at_incident_ms
        )
        .await
        .map_err(ApiError::db_error)?;

    // Log successful incident recording
    tracing::info!(
        event = "worker.incident.recorded",
        incident_id = %incident_id,
        worker_id = %fatal_msg.worker_id,
        tenant_id = %worker.tenant_id,
        incident_type = "fatal",
        "Worker fatal error recorded in database"
    );

    // Return success response
    Ok(Json(serde_json::json!({
        "status": "recorded",
        "incident_id": incident_id,
        "worker_id": fatal_msg.worker_id,
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Query parameters for list_worker_incidents
#[derive(Debug, Deserialize)]
pub struct ListIncidentsParams {
    pub limit: Option<i32>,
}

/// List worker incidents (PRD-09)
///
/// Returns a list of incidents for a specific worker, ordered by creation time (newest first).
///
/// **Permissions:** Operator, Admin, SRE
///
/// # Path Parameters
/// - `worker_id` - The ID of the worker
///
/// # Query Parameters
/// - `limit` - Maximum number of incidents to return (default: 50)
#[utoipa::path(
    get,
    path = "/v1/workers/{worker_id}/incidents",
    tag = "workers",
    params(
        ("worker_id" = String, Path, description = "Worker ID"),
        ("limit" = Option<i32>, Query, description = "Maximum incidents to return")
    ),
    responses(
        (status = 200, description = "List of worker incidents"),
        (status = 404, description = "Worker not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_worker_incidents(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Query(params): Query<ListIncidentsParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // PRD-RECT-002: Use tenant-scoped query to prevent cross-tenant worker access.
    // Returns 404 for both missing and cross-tenant workers.
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id).await?;

    let _worker = state
        .db
        .get_worker_for_tenant(&claims.tenant_id, &worker_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            // Returns same error for both "not found" and "cross-tenant" cases
            ApiError::not_found("worker").with_details(format!("Worker ID: {}", worker_id))
        })?;

    // Get incidents for the worker
    let incidents = state
        .db
        .list_worker_incidents(&worker_id, params.limit)
        .await
        .map_err(ApiError::db_error)?;

    Ok(Json(serde_json::json!({
        "worker_id": worker_id,
        "incidents": incidents,
        "count": incidents.len()
    })))
}

/// Get worker health summary (PRD-09)
///
/// Returns a summary of health status for all workers, including counts by status
/// and a list of workers with their current health metrics.
///
/// **Permissions:** Operator, Admin, SRE
#[utoipa::path(
    get,
    path = "/v1/workers/health/summary",
    tag = "workers",
    responses(
        (status = 200, description = "Worker health summary"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_worker_health_summary(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Get all workers with health metrics
    let workers = state
        .db
        .list_all_workers()
        .await
        .map_err(ApiError::db_error)?;

    // Get health records for workers that have them
    let mut health_records = Vec::new();
    let mut healthy_count = 0;
    let mut degraded_count = 0;
    let mut crashed_count = 0;
    let mut unknown_count = 0;

    for worker in &workers {
        let health = state.db.get_worker_health(&worker.id).await.ok().flatten();

        let status = health
            .as_ref()
            .map(|h| h.health_status.as_str())
            .unwrap_or("unknown");

        match status {
            "healthy" => healthy_count += 1,
            "degraded" => degraded_count += 1,
            "crashed" => crashed_count += 1,
            _ => unknown_count += 1,
        }

        // Get recent incident count (last 24 hours)
        let recent_incidents = state
            .db
            .get_recent_incident_count(&worker.id, 24)
            .await
            .unwrap_or(0);

        health_records.push(serde_json::json!({
            "worker_id": worker.id,
            "tenant_id": worker.tenant_id,
            "status": worker.status,
            "health_status": status,
            "avg_latency_ms": health.as_ref().and_then(|h| h.avg_latency_ms),
            "last_response_at": health.as_ref().and_then(|h| h.last_response_at.clone()),
            "consecutive_slow": health.as_ref().and_then(|h| h.consecutive_slow_responses),
            "consecutive_failures": health.as_ref().and_then(|h| h.consecutive_failures),
            "recent_incidents_24h": recent_incidents
        }));
    }

    Ok(Json(serde_json::json!({
        "summary": {
            "total": workers.len(),
            "healthy": healthy_count,
            "degraded": degraded_count,
            "crashed": crashed_count,
            "unknown": unknown_count
        },
        "workers": health_records,
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}
