use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

/// List all nodes
pub async fn list_nodes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<NodeResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let nodes = state.db.list_nodes().await.map_err(ApiError::db_error)?;

    let response: Vec<NodeResponse> = nodes
        .into_iter()
        .map(|n| NodeResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: n.id,
            hostname: n.hostname,
            agent_endpoint: n.agent_endpoint,
            status: n.status,
            last_seen_at: n.last_seen_at,
        })
        .collect();

    Ok(Json(response))
}

/// Register node
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
        .map_err(|e| ApiError::internal("failed to register node").with_details(e.to_string()))?;

    let node = state.db.get_node(&id).await.map_err(ApiError::db_error)?;

    let node = node.ok_or_else(|| ApiError::internal("node not found after registration").with_details("Node was registered but could not be retrieved"))?;

    // Audit log: node registered
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_REGISTER,
        crate::audit_helper::resources::NODE,
        Some(&node.id),
    )
    .await {

        tracing::warn!(error = %e, "Audit log failed");

    }

    Ok(Json(NodeResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: node.id,
        hostname: node.hostname,
        agent_endpoint: node.agent_endpoint,
        status: node.status,
        last_seen_at: node.last_seen_at,
    }))
}

/// Test node connection (ping)
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
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Node"))?;

    // Try to ping the node agent
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| ApiError::internal("Failed to create HTTP client").with_details(e.to_string()))?;

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
                tokio::time::sleep(backoff).await;
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
pub async fn mark_node_offline(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
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
        .map_err(|e| ApiError::internal("Failed to update node status").with_details(e.to_string()))?;

    // Audit log: node marked offline
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_OFFLINE,
        crate::audit_helper::resources::NODE,
        Some(&node_id),
    )
    .await {

        tracing::warn!(error = %e, "Audit log failed");

    }

    Ok(StatusCode::NO_CONTENT)
}

/// Evict node (delete from registry)
pub async fn evict_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;
    let node_id = crate::id_resolver::resolve_any_id(&state.db, &node_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Check for running workers on this node
    let workers = state.db.list_all_workers().await.map_err(ApiError::db_error)?;

    let node_has_workers = workers.iter().any(|w| w.node_id == node_id);

    if node_has_workers {
        return Err(ApiError::conflict("Node has running workers; stop all workers before evicting node"));
    }

    // Delete node using Db trait method
    state.db.delete_node(&node_id).await.map_err(|e| ApiError::internal("Failed to delete node").with_details(e.to_string()))?;

    // Audit log: node evicted
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_EVICT,
        crate::audit_helper::resources::NODE,
        Some(&node_id),
    )
    .await {

        tracing::warn!(error = %e, "Audit log failed");

    }

    Ok(StatusCode::NO_CONTENT)
}

/// Get node details
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
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Node"))?;

    // Get workers running on this node
    let all_workers = state.db.list_all_workers().await.map_err(ApiError::db_error)?;

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
        id: node.id,
        hostname: node.hostname,
        agent_endpoint: node.agent_endpoint,
        status: node.status,
        last_seen_at: node.last_seen_at,
        workers,
        recent_logs: {
            // Attempt to fetch recent logs, but don't fail if unavailable
            match adapteros_db::sqlx::query_as::<_, (String,)>(
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
