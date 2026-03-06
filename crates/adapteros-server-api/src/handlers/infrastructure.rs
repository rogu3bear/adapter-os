use crate::auth::Claims;
use crate::ip_extraction::ClientIp;
use crate::middleware::require_any_role;
use crate::permissions::{has_permission, require_permission, Permission};
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
use std::str::FromStr;

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
    Extension(client_ip): Extension<ClientIp>,
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
        Some(client_ip.0.as_str()),
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
    let node_id = crate::id_resolver::resolve_any_id(&state.db, &node_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

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
    Extension(client_ip): Extension<ClientIp>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;
    let node_id = crate::id_resolver::resolve_any_id(&state.db, &node_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

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
        Some(client_ip.0.as_str()),
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
    Extension(client_ip): Extension<ClientIp>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;
    let node_id = crate::id_resolver::resolve_any_id(&state.db, &node_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

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
        Some(client_ip.0.as_str()),
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
    let node_id = crate::id_resolver::resolve_any_id(&state.db, &node_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

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
        .get_effective_base_model_status_for_tenant(&tenant_id)
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
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<Vec<JobResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Generic job surfaces are read-only here; require an existing view permission.
    require_permission(&claims, Permission::DatasetView).map_err(|e| {
        <crate::api_error::ApiError as Into<(StatusCode, Json<ErrorResponse>)>>::into(e)
    })?;

    // Tenant scoping: default to the caller's tenant. Only tenant managers can
    // query other tenants or unscoped lists.
    let role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid role in authentication token")
                    .with_code("INVALID_ROLE")
                    .with_string_details(claims.role.clone()),
            ),
        )
    })?;
    let can_manage_tenants = has_permission(&role, Permission::TenantManage);
    let tenant_filter = if can_manage_tenants {
        query.tenant_id.as_deref()
    } else {
        Some(claims.tenant_id.as_str())
    };

    let jobs = state.db.list_jobs(tenant_filter).await.map_err(|e| {
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

/// Get a job by id
#[utoipa::path(
    get,
    path = "/v1/jobs/{job_id}",
    params(
        ("job_id" = String, Path, description = "Job identifier")
    ),
    responses(
        (status = 200, description = "Job", body = JobDetailResponse),
        (status = 404, description = "Not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "jobs"
)]
pub async fn get_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<JobDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView).map_err(|e| {
        <crate::api_error::ApiError as Into<(StatusCode, Json<ErrorResponse>)>>::into(e)
    })?;

    let role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid role in authentication token")
                    .with_code("INVALID_ROLE")
                    .with_string_details(claims.role.clone()),
            ),
        )
    })?;
    let can_manage_tenants = has_permission(&role, Permission::TenantManage);

    let job = state.db.get_job(&job_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let Some(job) = job else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("job not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Job ID: {}", job_id)),
            ),
        ));
    };

    // Tenant isolation: do not leak existence across tenants.
    if !can_manage_tenants && job.tenant_id.as_deref() != Some(claims.tenant_id.as_str()) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("job not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Job ID: {}", job_id)),
            ),
        ));
    }

    Ok(Json(JobDetailResponse {
        id: job.id,
        kind: job.kind,
        status: job.status,
        tenant_id: job.tenant_id,
        user_id: job.user_id,
        payload_json: job.payload_json,
        result_json: job.result_json,
        logs_path: job.logs_path,
        created_at: job.created_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
    }))
}

#[cfg(test)]
mod jobs_tests {
    use super::*;
    use crate::auth::{AuthMode, PrincipalType, JWT_ISSUER};
    use crate::state::MetricsConfig as StateMetricsConfig;
    use crate::telemetry::MetricsRegistry;
    use crate::test_utils;
    use crate::{ApiConfig, PathsConfig};
    use adapteros_core::{BackendKind, SeedMode};
    use adapteros_metrics_exporter::MetricsExporter;
    use adapteros_telemetry::metrics::MetricsConfig as TelemetryMetricsConfig;
    use adapteros_telemetry::MetricsCollector;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use std::sync::{Arc, RwLock};
    use tower::ServiceExt;

    fn make_claims(tenant_id: &str, role: &str) -> Claims {
        let now = chrono::Utc::now().timestamp();
        Claims {
            sub: "user-jobs-test".to_string(),
            email: "user@example.com".to_string(),
            role: role.to_string(),
            roles: vec![role.to_string()],
            tenant_id: tenant_id.to_string(),
            admin_tenants: vec![],
            device_id: None,
            session_id: Some("session".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: now + 3600,
            iat: now,
            jti: "jti-jobs-test".to_string(),
            nbf: now,
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        }
    }

    async fn build_test_state() -> AppState {
        let db = adapteros_db::Db::new_in_memory().await.unwrap();
        let jwt_secret = b"jobs-test-secret-32-bytes-xxxx!".to_vec();
        let base_tempdir = test_utils::tempdir_with_prefix("aos-test-jobs-");
        let base_dir = base_tempdir.into_path();
        for dir in [
            "artifacts",
            "bundles",
            "adapters",
            "plan",
            "datasets",
            "documents",
        ] {
            let path = base_dir.join(dir);
            std::fs::create_dir_all(&path).unwrap();
        }

        let config = Arc::new(RwLock::new(ApiConfig {
            metrics: StateMetricsConfig {
                enabled: false,
                bearer_token: "test".to_string(),
            },
            directory_analysis_timeout_secs: 1,
            use_session_stack_for_routing: false,
            capacity_limits: Default::default(),
            general: None,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            self_hosting: Default::default(),
            performance: Default::default(),
            streaming: Default::default(),
            paths: PathsConfig {
                artifacts_root: base_dir.join("artifacts").to_string_lossy().to_string(),
                bundles_root: base_dir.join("bundles").to_string_lossy().to_string(),
                adapters_root: base_dir.join("adapters").to_string_lossy().to_string(),
                plan_dir: base_dir.join("plan").to_string_lossy().to_string(),
                datasets_root: base_dir.join("datasets").to_string_lossy().to_string(),
                documents_root: base_dir.join("documents").to_string_lossy().to_string(),
                synthesis_model_path: None,
                training_worker_bin: None,
            },
            chat_context: Default::default(),
            seed_mode: SeedMode::BestEffort,
            backend_profile: BackendKind::Auto,
            worker_id: 0,
            timeouts: Default::default(),
            rate_limit: None,
            inference_cache: Default::default(),
        }));
        let metrics_exporter =
            Arc::new(MetricsExporter::new(vec![0.1, 1.0]).expect("metrics exporter"));
        let metrics_collector = Arc::new(MetricsCollector::new(TelemetryMetricsConfig::default()));
        let metrics_registry = Arc::new(MetricsRegistry::new());
        let uma_monitor = Arc::new(adapteros_lora_worker::memory::UmaPressureMonitor::new(
            10, None,
        ));

        let state = AppState::new(
            db,
            jwt_secret,
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            uma_monitor,
        );

        // Seed FK fixtures required by jobs table constraints.
        adapteros_db::sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
            .bind("tenant-a")
            .bind("Tenant A")
            .execute(state.db.pool_result().expect("db pool"))
            .await
            .expect("seed tenant-a");
        adapteros_db::sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
            .bind("tenant-b")
            .bind("Tenant B")
            .execute(state.db.pool_result().expect("db pool"))
            .await
            .expect("seed tenant-b");
        adapteros_db::sqlx::query(
            "INSERT OR IGNORE INTO users (id, email, display_name, pw_hash, role, tenant_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("user-jobs-test")
        .bind("user@example.com")
        .bind("Jobs Test User")
        .bind("test-hash")
        .bind("viewer")
        .bind("tenant-a")
        .execute(state.db.pool_result().expect("db pool"))
        .await
        .expect("seed jobs test user");

        state
    }

    fn app(state: AppState) -> Router {
        Router::new()
            .route("/v1/jobs", get(list_jobs))
            .route("/v1/jobs/{job_id}", get(get_job))
            .with_state(state)
    }

    #[tokio::test]
    async fn get_job_is_tenant_scoped_returns_404_cross_tenant() {
        let state = build_test_state().await;
        let job_id = state
            .db
            .create_job(
                "training_dataset_from_upload",
                Some("tenant-a"),
                Some("user-jobs-test"),
                "{\"k\":1}",
            )
            .await
            .unwrap();

        let mut req = Request::builder()
            .uri(format!("/v1/jobs/{}", job_id))
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(make_claims("tenant-b", "viewer"));

        let resp = app(state).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_job_includes_logs_path_when_present() {
        let state = build_test_state().await;
        let job_id = state
            .db
            .create_job(
                "training_dataset_from_upload",
                Some("tenant-a"),
                Some("user-jobs-test"),
                "{\"k\":1}",
            )
            .await
            .unwrap();

        state
            .db
            .update_job_logs_path(&job_id, Some("/abs/var/logs/actions/jobs/job_example.log"))
            .await
            .unwrap();

        let mut req = Request::builder()
            .uri(format!("/v1/jobs/{}", job_id))
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(make_claims("tenant-a", "viewer"));

        let resp = app(state).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            parsed
                .get("logs_path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
            "/abs/var/logs/actions/jobs/job_example.log"
        );
    }

    #[tokio::test]
    async fn list_jobs_defaults_to_claims_tenant_when_not_tenant_manager() {
        let state = build_test_state().await;
        let _ = state
            .db
            .create_job(
                "training_dataset_from_upload",
                Some("tenant-a"),
                Some("user-jobs-test"),
                "{\"k\":1}",
            )
            .await
            .unwrap();
        let _ = state
            .db
            .create_job(
                "training_dataset_from_upload",
                Some("tenant-b"),
                Some("user-jobs-test"),
                "{\"k\":1}",
            )
            .await
            .unwrap();

        let mut req = Request::builder()
            .uri("/v1/jobs?tenant_id=tenant-b")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(make_claims("tenant-a", "viewer"));

        let resp = app(state).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1, "should only list tenant-a jobs");
    }
}

// Note: worker_spawn, list_workers, stop_worker moved to handlers/workers.rs (PRD-RECT topology fix)

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
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

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
