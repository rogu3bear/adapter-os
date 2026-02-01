use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::sqlx;
use adapteros_db::users::Role;
use adapteros_db::workers::WorkerIncidentType;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use std::collections::HashMap;

/// List nodes
#[utoipa::path(
    get,
    path = "/v1/nodes",
    responses(
        (status = 200, description = "Nodes list", body = Vec<NodeResponse>),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "nodes"
)]
pub async fn list_nodes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<NodeResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let nodes = state.db.list_nodes().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<NodeResponse> = nodes
        .into_iter()
        .map(|n| NodeResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            node: n,
        })
        .collect();

    Ok(Json(response))
}

/// Register node
#[utoipa::path(
    post,
    path = "/v1/nodes/register",
    request_body = RegisterNodeRequest,
    responses(
        (status = 200, description = "Node registered", body = NodeResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "nodes"
)]
pub async fn register_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<Json<NodeResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let id = state
        .db
        .register_node(&req.hostname, &req.agent_endpoint)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register node")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let node = state.db.get_node(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let node = node.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("node not found after registration").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: node registered
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_REGISTER,
        crate::audit_helper::resources::NODE,
        Some(&node.id),
    )
    .await;

    Ok(Json(NodeResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        node,
    }))
}

/// Test node connection (ping)
#[utoipa::path(
    post,
    path = "/v1/nodes/{node_id}/ping",
    params(
        ("node_id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 200, description = "Ping result", body = NodePingResponse),
        (status = 404, description = "Node not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "nodes"
)]
pub async fn test_node_connection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodePingResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
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
                Json(ErrorResponse::new("node not found").with_code("NOT_FOUND")),
            )
        })?;

    // Try to ping the node agent
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create HTTP client")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let ping_url = format!("{}/health", node.agent_endpoint);
    let max_attempts = 3u32;
    let mut attempt = 0u32;
    let mut backoff = std::time::Duration::from_millis(100);
    let result = loop {
        attempt += 1;
        match client.get(&ping_url).send().await {
            Ok(response) => break Ok(response),
            Err(e) => {
                if attempt >= max_attempts {
                    break Err(e);
                }
                // Add jitter to prevent thundering herd on retries (deterministic when configured)
                let jitter_ms = adapteros_core::compute_jitter_delay(50, 1.0); // 0-100ms range
                let jitter = std::time::Duration::from_millis(jitter_ms);
                tokio::time::sleep(backoff + jitter).await;
                backoff = (backoff * 2).min(std::time::Duration::from_millis(800));
            }
        }
    };

    let (status, latency_ms) = match result {
        Ok(response) if response.status().is_success() => {
            ("reachable".to_string(), start.elapsed().as_millis() as f64)
        }
        Ok(response) => (
            format!("error: HTTP {}", response.status()),
            start.elapsed().as_millis() as f64,
        ),
        Err(e) => (
            format!("unreachable: {}", e),
            start.elapsed().as_millis() as f64,
        ),
    };

    Ok(Json(NodePingResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        node_id: node.id,
        status,
        latency_ms,
    }))
}

/// Mark node offline
#[utoipa::path(
    post,
    path = "/v1/nodes/{node_id}/offline",
    params(
        ("node_id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 204, description = "Node marked offline"),
        (status = 404, description = "Node not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "nodes"
)]
pub async fn mark_node_offline(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Update node status using Db trait method
    state
        .db
        .update_node_status(&node_id, "offline")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update node status")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Audit log: node marked offline
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_OFFLINE,
        crate::audit_helper::resources::NODE,
        Some(&node_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Evict node (delete from registry)
#[utoipa::path(
    delete,
    path = "/v1/nodes/{node_id}",
    params(
        ("node_id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 204, description = "Node evicted"),
        (status = 404, description = "Node not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "nodes"
)]
pub async fn evict_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Check for running workers on this node
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let node_has_workers = workers.iter().any(|w| w.node_id == node_id);

    if node_has_workers {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("node has running workers")
                    .with_code("CONFLICT")
                    .with_string_details("Stop all workers before evicting node"),
            ),
        ));
    }

    // Delete node using Db trait method
    state.db.delete_node(&node_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to delete node")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Audit log: node evicted
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_EVICT,
        crate::audit_helper::resources::NODE,
        Some(&node_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Get node details
#[utoipa::path(
    get,
    path = "/v1/nodes/{node_id}/details",
    params(
        ("node_id" = String, Path, description = "Node ID")
    ),
    responses(
        (status = 200, description = "Node details", body = NodeDetailsResponse),
        (status = 404, description = "Node not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "nodes"
)]
pub async fn get_node_details(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodeDetailsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
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
                Json(ErrorResponse::new("node not found").with_code("NOT_FOUND")),
            )
        })?;

    // Get workers running on this node
    let all_workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let workers: Vec<WorkerInfo> = all_workers
        .iter()
        .filter(|w| w.node_id == node_id)
        .map(|w| WorkerInfo {
            id: w.id.clone(),
            tenant_id: w.tenant_id.clone(),
            plan_id: w.plan_id.clone(),
            status: w.status.clone(),
        })
        .collect();

    Ok(Json(NodeDetailsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        detail: adapteros_types::nodes::NodeDetail {
            node,                                                 // Wrap the Node struct
            workers: workers.into_iter().map(|w| w.id).collect(), // Map to IDs
        },
        recent_logs: {
            // Attempt to fetch recent logs, but don't fail if unavailable
            match sqlx::query_as::<_, (String,)>(
                "SELECT message FROM node_logs WHERE node_id = ? ORDER BY timestamp DESC LIMIT 10",
            )
            .bind(&node_id)
            .fetch_all(state.db.pool())
            .await
            {
                Ok(rows) => rows.into_iter().map(|(msg,)| msg).collect(),
                Err(e) => {
                    tracing::warn!("Failed to fetch node logs for {}: {}", node_id, e);
                    vec![]
                }
            }
        },
    }))
}

/// Get base model status
///
/// # Endpoint
/// GET /v1/models/status
///
/// # Authentication
/// Optional - unauthenticated requests receive limited response
///
/// # Permissions (when authenticated)
/// Requires one of: Operator, Admin, Compliance
///
/// # Query Parameters
/// - `tenant_id`: Optional tenant ID filter (defaults to "default", only applies when authenticated)
///
/// # Response
/// Returns the current base model load status. Response varies by authentication:
///
/// **Unauthenticated response (limited data):**
/// - `model_id`: "none"
/// - `model_name`: "No Model Loaded"
/// - `model_path`: null
/// - `status`: "unloaded"
/// - `loaded_at`: null
/// - `unloaded_at`: null
/// - `error_message`: null
/// - `memory_usage_mb`: null
/// - `is_loaded`: false
/// - `updated_at`: Current timestamp
///
/// **Authenticated response (full data):**
/// - `model_id`: Identifier of the loaded model (or "none")
/// - `model_name`: Human-readable model name (or "No Model Loaded")
/// - `model_path`: Filesystem path to model files
/// - `status`: Load status (loaded, unloaded, loading, error)
/// - `loaded_at`: Timestamp when model was loaded
/// - `unloaded_at`: Timestamp when model was unloaded
/// - `error_message`: Error message if status is error
/// - `memory_usage_mb`: Memory consumption in MB
/// - `is_loaded`: Boolean flag indicating if model is currently in memory
/// - `updated_at`: Last status update timestamp
///
/// # Errors
/// - `NOT_FOUND` (404): Model referenced in status record not found in database (authenticated only)
/// - `INTERNAL_ERROR` (500): Database query failure (authenticated only)
///
/// # Example
/// ```
/// GET /v1/models/status?tenant_id=default
/// ```
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/models/status",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID")
    ),
    responses(
        (status = 200, description = "Base model status", body = BaseModelStatusResponse),
        (status = 404, description = "No base model status found", body = ErrorResponse)
    )
)]
pub async fn get_base_model_status(
    State(state): State<AppState>,
    claims: Option<Extension<Claims>>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<BaseModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if user is authenticated
    let is_authenticated = if let Some(Extension(ref claims_inner)) = claims {
        // Verify user has one of the required roles
        require_any_role(claims_inner, &[Role::Operator, Role::Admin, Role::Viewer]).is_ok()
    } else {
        false
    };

    // When unauthenticated, return basic limited data only
    if !is_authenticated {
        return Ok(Json(BaseModelStatusResponse {
            model_id: "none".to_string(),
            model_name: "No Model Loaded".to_string(),
            model_path: None,
            status: adapteros_api_types::ModelLoadStatus::NoModel,
            loaded_at: None,
            unloaded_at: None,
            error_message: None,
            memory_usage_mb: None,
            is_loaded: false,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }));
    }

    // Authenticated path - return full data
    // PRD-RECT-002: Validate caller has access to the requested tenant
    let claims_inner = claims.as_ref().map(|c| &c.0).ok_or_else(|| {
        tracing::error!("claims unexpectedly None despite is_authenticated=true");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("authentication state inconsistency")
                    .with_code("AUTH_STATE_ERROR"),
            ),
        )
    })?;
    let tenant_id = query
        .tenant_id
        .clone()
        .unwrap_or_else(|| claims_inner.tenant_id.clone());
    let is_admin = claims_inner
        .roles
        .iter()
        .any(|r| r.to_lowercase() == "admin");
    if !is_admin && tenant_id != claims_inner.tenant_id {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("model status not found").with_code("NOT_FOUND")),
        ));
    }
    if is_admin && tenant_id != claims_inner.tenant_id {
        crate::security::validate_tenant_isolation(claims_inner, &tenant_id)?;
    }

    // Get base model status from database
    let status_record = state
        .db
        .get_base_model_status(&tenant_id)
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

    // If no status record exists, return default unloaded status
    if let Some(status_record) = status_record {
        // Get model details
        let model = state
            .db
            .get_model_for_tenant(&tenant_id, &status_record.model_id)
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
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
                )
            })?;

        let status_enum = adapteros_api_types::ModelLoadStatus::parse_status(&status_record.status);
        let is_loaded = status_enum.is_ready();

        Ok(Json(BaseModelStatusResponse {
            model_id: status_record.model_id,
            model_name: model.name,
            model_path: model.model_path,
            status: status_enum,
            loaded_at: status_record.loaded_at,
            unloaded_at: status_record.unloaded_at,
            error_message: status_record.error_message,
            memory_usage_mb: status_record.memory_usage_mb,
            is_loaded,
            updated_at: status_record.updated_at,
        }))
    } else {
        // Return default unloaded status when no record exists
        Ok(Json(BaseModelStatusResponse {
            model_id: "none".to_string(),
            model_name: "No Model Loaded".to_string(),
            model_path: None,
            status: adapteros_api_types::ModelLoadStatus::NoModel,
            loaded_at: None,
            unloaded_at: None,
            error_message: None,
            memory_usage_mb: None,
            is_loaded: false,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }))
    }
}

#[derive(Deserialize)]
pub struct ListJobsQuery {
    pub tenant_id: Option<String>,
}

/// List jobs
#[utoipa::path(
    get,
    path = "/v1/jobs",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID")
    ),
    responses(
        (status = 200, description = "Jobs list", body = Vec<JobResponse>),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "jobs"
)]
pub async fn list_jobs(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<Vec<JobResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let jobs = state
        .db
        .list_jobs(query.tenant_id.as_deref())
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

    let response: Vec<JobResponse> = jobs
        .into_iter()
        .map(|j| JobResponse {
            id: j.id,
            kind: j.kind,
            status: j.status,
            created_at: j.created_at,
        })
        .collect();

    Ok(Json(response))
}

// ============================================================================
// Worker Management Handlers
// ============================================================================

/// Spawn worker via node agent
#[utoipa::path(
    post,
    path = "/v1/workers/spawn",
    request_body = SpawnWorkerRequest,
    responses(
        (status = 200, description = "Worker spawned", body = WorkerResponse),
        (status = 404, description = "Node not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "workers"
)]
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
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create HTTP client")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
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
        (
            StatusCode::BAD_GATEWAY,
            Json(
                ErrorResponse::new("failed to contact node agent")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(format!("{} (after {} attempts)", e, max_attempts)),
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

    /// Response from node agent spawn endpoint
    #[derive(Deserialize)]
    struct SpawnResponse {
        pid: i64,
    }

    let spawn_response: SpawnResponse = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to parse node agent response")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Validate PID is within valid range (1 to i32::MAX)
    const MIN_VALID_PID: i64 = 1;
    const MAX_VALID_PID: i64 = i32::MAX as i64;

    if spawn_response.pid < MIN_VALID_PID || spawn_response.pid > MAX_VALID_PID {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("invalid response from node agent")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(format!(
                        "PID {} is outside valid range ({}-{})",
                        spawn_response.pid, MIN_VALID_PID, MAX_VALID_PID
                    )),
            ),
        ));
    }

    let pid = spawn_response.pid as i32;

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
        .status(adapteros_core::WorkerStatus::Created.as_str());
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
    }))
}

#[derive(Deserialize)]
pub struct ListWorkersQuery {
    pub tenant_id: Option<String>,
}

/// List workers with optional tenant filter
#[utoipa::path(
    get,
    path = "/v1/workers",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID")
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
) -> Result<Json<Vec<WorkerResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Non-admin users are forced to their own tenant
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    let effective_tenant_id = if is_admin {
        query.tenant_id.clone()
    } else {
        Some(claims.tenant_id.clone())
    };

    let workers = if let Some(tenant_id) = effective_tenant_id {
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

    async fn resolve_plan_model_info(
        db: &adapteros_db::Db,
        plan_id: &str,
    ) -> (Option<String>, Option<String>) {
        let plan = match db.get_plan(plan_id).await {
            Ok(Some(p)) => p,
            _ => return (None, None),
        };

        let manifest_row = match db.get_manifest_by_hash(&plan.manifest_hash_b3).await {
            Ok(Some(m)) => m,
            _ => return (None, None),
        };

        let parsed = serde_json::from_str::<adapteros_model_hub::manifest::ManifestV3>(
            &manifest_row.body_json,
        )
        .or_else(|_| {
            serde_yaml::from_str::<adapteros_model_hub::manifest::ManifestV3>(
                &manifest_row.body_json,
            )
        });

        match parsed {
            Ok(manifest) => (
                Some(manifest.base.model_id),
                Some(manifest.base.model_hash.to_hex()),
            ),
            Err(_) => (None, None),
        }
    }

    // Limit cache size to prevent unbounded memory growth
    const PLAN_MODEL_CACHE_MAX: usize = 1000;
    let mut plan_model_cache: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    let mut response: Vec<WorkerResponse> = Vec::with_capacity(workers.len());

    for w in workers {
        let runtime = state
            .worker_runtime
            .get(&w.id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default();

        let (model_id, resolved_model_hash) = match plan_model_cache.get(&w.plan_id) {
            Some(cached) => cached.clone(),
            None => {
                let resolved = resolve_plan_model_info(&state.db, &w.plan_id).await;
                // Only cache if under capacity limit to prevent unbounded growth
                if plan_model_cache.len() < PLAN_MODEL_CACHE_MAX {
                    plan_model_cache.insert(w.plan_id.clone(), resolved.clone());
                }
                resolved
            }
        };

        let model_hash = resolved_model_hash
            .or(runtime.model_hash.clone())
            .or(w.model_hash_b3.clone());
        let model_loaded = matches!(w.status.as_str(), "healthy" | "draining" | "serving");
        let backend = runtime.backend.clone().or(w.backend.clone());
        let tokenizer_hash_b3 = runtime
            .tokenizer_hash_b3
            .clone()
            .or_else(|| w.tokenizer_hash_b3.clone());
        let tokenizer_vocab_size = runtime
            .tokenizer_vocab_size
            .or_else(|| w.tokenizer_vocab_size.map(|v| v as u32));
        let capabilities = if runtime.capabilities.is_empty() {
            w.capabilities_json
                .as_ref()
                .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                .unwrap_or_default()
        } else {
            runtime.capabilities.clone()
        };
        let capabilities_detail = runtime.capabilities_detail.clone().or_else(|| {
            w.capabilities_json
                .as_ref()
                .and_then(|json| serde_json::from_str(json).ok())
        });

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
            backend,
            model_id,
            model_hash,
            tokenizer_hash_b3,
            tokenizer_vocab_size,
            model_loaded,
            cache_used_mb: runtime.cache_used_mb,
            cache_max_mb: runtime.cache_max_mb,
            cache_pinned_entries: runtime.cache_pinned_entries,
            cache_active_entries: runtime.cache_active_entries,
        });
    }

    Ok(Json(response))
}

/// Stop a worker process
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
    crate::permissions::require_permission(&claims, crate::permissions::Permission::WorkerManage)?;

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

    if let Some(pid) = worker.pid {
        tracing::info!(
            event = "worker.stop.signal",
            worker_id = %worker_id,
            pid = %pid,
            "Signaling worker process to stop"
        );
    }

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

    tracing::info!(
        event = "worker.stop",
        worker_id = %worker_id,
        previous_status = %previous_status,
        actor = %claims.sub,
        "Worker stopped"
    );

    crate::audit_helper::log_success_or_warn(
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

/// Receive fatal error report from worker
pub async fn receive_worker_fatal(
    State(state): State<AppState>,
    Json(fatal_msg): Json<crate::types::WorkerFatal>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    tracing::error!(
        event = "worker.fatal.received",
        worker_id = %fatal_msg.worker_id,
        reason = %fatal_msg.reason,
        timestamp = %fatal_msg.timestamp,
        has_backtrace = fatal_msg.backtrace_snippet.is_some(),
        "Control plane received worker fatal error"
    );

    let worker = state
        .db
        .get_worker(&fatal_msg.worker_id)
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
                        .with_string_details(format!("Worker ID: {}", fatal_msg.worker_id)),
                ),
            )
        })?;

    let incident_id = state
        .db
        .insert_worker_incident(
            &fatal_msg.worker_id,
            &worker.tenant_id,
            WorkerIncidentType::Fatal,
            &fatal_msg.reason,
            fatal_msg.backtrace_snippet.as_deref(),
            None,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to record incident")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    tracing::info!(
        event = "worker.incident.recorded",
        incident_id = %incident_id,
        worker_id = %fatal_msg.worker_id,
        tenant_id = %worker.tenant_id,
        incident_type = "fatal",
        "Worker fatal error recorded in database"
    );

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

/// List worker incidents
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
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let _worker = state
        .db
        .get_worker_for_tenant(&claims.tenant_id, &worker_id)
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

    let incidents = state
        .db
        .list_worker_incidents(&worker_id, params.limit)
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

    Ok(Json(serde_json::json!({
        "worker_id": worker_id,
        "incidents": incidents,
        "count": incidents.len()
    })))
}

/// Get worker health summary
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
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let workers = state
        .db
        .list_workers_by_tenant(&claims.tenant_id)
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

    let mut health_records = Vec::new();
    let mut healthy_count = 0;
    let mut degraded_count = 0;
    let mut crashed_count = 0;
    let mut unknown_count = 0;

    for worker in &workers {
        let health = state.db.get_worker_health(&worker.id).await.ok().flatten();

        let status = match &health {
            Some(h) => h.health_status.as_str(),
            None => {
                tracing::debug!(worker_id = %worker.id, "No health record found for worker, marking as unchecked");
                "unchecked"
            }
        };

        match status {
            "healthy" => healthy_count += 1,
            "degraded" => degraded_count += 1,
            "crashed" => crashed_count += 1,
            _ => unknown_count += 1,
        }

        let recent_incidents = state
            .db
            .get_recent_incident_count(&worker.id, 24)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(worker_id = %worker.id, error = %e, "Failed to get recent incident count");
                0
            });

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

/// Get build version information (PRD-RECT-001)
///
/// Returns build fingerprint including version, git SHA, platform, enabled features, and backends.
/// This endpoint is public (no auth required) for service discovery and monitoring.
#[utoipa::path(
    get,
    path = "/version",
    responses(
        (status = 200, description = "Build info", body = adapteros_core::BuildInfo)
    ),
    tag = "system"
)]
pub async fn get_version() -> Json<adapteros_core::BuildInfo> {
    Json(adapteros_core::BuildInfo::current())
}
