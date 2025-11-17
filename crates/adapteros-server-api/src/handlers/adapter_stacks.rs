use crate::auth::Claims;
use crate::state::AppState;
use adapteros_api_types::adapters::*;
use adapteros_core::B3Hash;
use adapteros_core::StackName;
use adapteros_db::sqlx;
use adapteros_db::traits::AdapterRecord;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::{info, warn};

/// Request to create a new adapter stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStackRequest {
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,
    pub metadata: Option<HashMap<String, String>>,
}

/// Response for adapter stack operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
}

/// Workflow type for adapter stacks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Parallel,
    UpstreamDownstream,
    Sequential,
}

/// Create a new adapter stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks",
    request_body = CreateStackRequest,
    responses(
        (status = 201, description = "Stack created", body = StackResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateStackRequest>,
) -> Result<(StatusCode, Json<StackResponse>), (StatusCode, String)> {
    let tenant_id = claims.tenant_id.clone();

    // Validate stack name format
    let stack_name = StackName::parse(&req.name).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid stack name: {}", e),
        )
    })?;

    info!(
        tenant_id = %tenant_id,
        stack_name = %stack_name,
        adapter_count = req.adapter_ids.len(),
        "Creating adapter stack"
    );

    let id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let adapter_ids_json = serde_json::to_string(&req.adapter_ids)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let workflow_type_str = req.workflow_type.as_ref().map(|w| format!("{:?}", w));

    let db_req = adapteros_db::traits::CreateStackRequest {
        tenant_id,
        name: req.name.clone(),
        description: req.description.clone(),
        adapter_ids: req.adapter_ids.clone(),
        workflow_type: req.workflow_type.clone(),
    };

    let id = state.db.insert_stack(&db_req).await.map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            (
                StatusCode::CONFLICT,
                format!("Stack name '{}' already exists for tenant", req.name),
            )
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    })?;

    info!(stack_id = %id, stack_name = %stack_name, tenant_id = %tenant_id, "Adapter stack created");

    Ok((
        StatusCode::CREATED,
        Json(StackResponse {
            id: id.clone(),
            tenant_id,
            name: req.name,
            description: req.description,
            adapter_ids: req.adapter_ids,
            workflow_type: req.workflow_type,
            created_at: now.clone(),
            updated_at: now,
            is_active: false,
        }),
    ))
}

/// List all adapter stacks
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks",
    responses(
        (status = 200, description = "List of stacks", body = Vec<StackResponse>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_stacks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<StackResponse>>, (StatusCode, String)> {
    let tenant_id = claims.tenant_id;

    let rows = state
        .db
        .list_stacks_for_tenant(&tenant_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut stacks = Vec::new();
    for row in rows {
        let adapter_ids: Vec<String> =
            serde_json::from_str(&row.adapter_ids_json).unwrap_or_else(|_| vec![]);

        let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
            "Parallel" => Some(WorkflowType::Parallel),
            "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
            "Sequential" => Some(WorkflowType::Sequential),
            _ => None,
        });

        stacks.push(StackResponse {
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name,
            description: row.description,
            adapter_ids,
            workflow_type,
            created_at: row.created_at,
            updated_at: row.updated_at,
            is_active: false,
        });
    }

    Ok(Json(stacks))
}

/// Get a specific adapter stack
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks/{id}",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 200, description = "Stack details", body = StackResponse),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<StackResponse>, (StatusCode, String)> {
    let tenant_id = claims.tenant_id;

    let row = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!(
                    "Stack with id '{}' not found for tenant '{}'",
                    id, tenant_id
                ),
            )
        })?;

    if row.tenant_id != tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Stack does not belong to your tenant".to_string(),
        ));
    }

    let adapter_ids: Vec<String> =
        serde_json::from_str(&row.adapter_ids_json).unwrap_or_else(|_| vec![]);

    let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
        "Parallel" => Some(WorkflowType::Parallel),
        "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
        "Sequential" => Some(WorkflowType::Sequential),
        _ => None,
    });

    Ok(Json(StackResponse {
        id: row.id,
        tenant_id: row.tenant_id,
        name: row.name,
        description: row.description,
        adapter_ids,
        workflow_type,
        created_at: row.created_at,
        updated_at: row.updated_at,
        is_active: false,
    }))
}

/// Delete an adapter stack
#[utoipa::path(
    delete,
    path = "/v1/adapter-stacks/{id}",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 204, description = "Stack deleted"),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let tenant_id = claims.tenant_id;

    let deleted = state
        .db
        .delete_stack(&tenant_id, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !deleted {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Stack '{}' not found for tenant '{}'", id, tenant_id),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Activate an adapter stack (sets it as the active stack for routing)
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks/{id}/activate",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 200, description = "Stack activated"),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn activate_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let tenant_id = claims.tenant_id;

    // First verify the stack exists and parse adapter IDs
    let stack = sqlx::query!(
        r#"
        SELECT id, name, adapter_ids_json, tenant_id
        FROM adapter_stacks
        WHERE id = ? AND tenant_id = ?
        "#,
        id,
        tenant_id
    )
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        warn!(
            "Database error while fetching stack {} for tenant {}: {}",
            id, tenant_id, e
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
    })?
    .ok_or_else(|| {
        warn!(
            "Attempted to activate non-existent stack: {} for tenant {}",
            id, tenant_id
        );
        (
            StatusCode::NOT_FOUND,
            format!(
                "Stack with id '{}' not found for tenant '{}'",
                id, tenant_id
            ),
        )
    })?;

    if stack.tenant_id != tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Stack does not belong to your tenant".to_string(),
        ));
    }

    let name = stack.name;

    // Parse adapter IDs to ensure they're valid
    let adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json).map_err(|e| {
        warn!("Failed to parse adapter_ids_json for stack {}: {}", name, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid adapter list in stack '{}': {}", name, e),
        )
    })?;

    // Store the active stack ID in application state
    // This would be used by the routing logic
    let previous_stack = {
        let mut active_stack = state.active_stack.write().map_err(|e| {
            warn!("Failed to acquire write lock for active_stack: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal synchronization error".to_string(),
            )
        })?;
        let prev = active_stack.clone();
        *active_stack = Some(id.clone());
        prev
    };

    let new_hash = compute_stack_hash(&state, &tenant_id, &id).await?;

    let old_hash = if let Some(old_id) = &previous_stack {
        Some(compute_stack_hash(&state, &tenant_id, old_id).await?)
    } else {
        None
    };

    let hash_changed = old_hash.as_ref() != Some(&new_hash);

    if hash_changed {
        if let Some(worker_arc) = &state.worker {
            let mut worker = worker_arc.lock().await;
            let old_ids = if let Some(old_id) = &previous_stack {
                let old_stack = state
                    .db
                    .get_stack(&tenant_id, old_id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                    .unwrap();
                serde_json::from_str::<Vec<String>>(&old_stack.adapter_ids_json).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Parse old: {}", e),
                    )
                })?
            } else {
                vec![]
            };

            let add_ids: Vec<_> = adapter_ids
                .iter()
                .filter(|id| !old_ids.contains(id))
                .cloned()
                .collect();
            let remove_ids: Vec<_> = old_ids
                .iter()
                .filter(|id| !adapter_ids.contains(id))
                .cloned()
                .collect();

            tokio::task::spawn_blocking(move || worker.hotswap.swap(&add_ids, &remove_ids))
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Spawn blocking error: {}", e),
                    )
                })?
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            worker.kv_cache.zeroize_all();

            *worker.last_stack_hash.write() = Some(new_hash);

            worker.telemetry.emit_custom("stack.swap", json!({
                "old_stack_hash": old_hash.as_ref().map(|h| h.to_short_hex()),
                "new_stack_hash": new_hash.to_short_hex(),
                "cache_reset": true,
                "tenant_id": tenant_id,
                "stack_id": id,
                "trace_id": tracing::Span::current().id().map(|id| format!("{:x}", id.into_u64())).unwrap_or("unknown".to_string()),
            })).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
    }

    info!(
        tenant_id = %tenant_id,
        "Activated adapter stack '{}' (id: {}) with {} adapters. Previous stack: {:?}",
        name,
        id,
        adapter_ids.len(),
        previous_stack
    );

    // TODO: Notify the router about the stack change
    // This is where we would integrate with the actual router
    // For now, this is just stored in AppState

    Ok(Json(serde_json::json!({
        "message": format!("Stack '{}' activated for tenant '{}'", name, tenant_id),
        "stack_id": id,
        "tenant_id": tenant_id,
        "adapter_count": adapter_ids.len(),
        "previous_stack": previous_stack,
    })))
}

/// Deactivate the current adapter stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks/deactivate",
    responses(
        (status = 200, description = "Stack deactivated"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn deactivate_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let tenant_id = claims.tenant_id;

    let previous_stack = {
        let mut active = state.active_stack.write().unwrap();
        let prev = active.get(&tenant_id).cloned();
        active.insert(tenant_id, None);
        prev
    };

    match previous_stack {
        Some(stack_id) => {
            info!(tenant_id = %tenant_id, "Deactivated adapter stack '{}' for tenant {}", stack_id, tenant_id);
            Ok(Json(serde_json::json!({
                "message": "Active stack deactivated",
                "tenant_id": tenant_id,
                "previous_stack": stack_id,
            })))
        }
        None => {
            info!(tenant_id = %tenant_id, "Deactivate called but no stack was active");
            Ok(Json(serde_json::json!({
                "message": "No stack was active",
                "tenant_id": tenant_id,
            })))
        }
    }
}

async fn compute_stack_hash(
    state: &AppState,
    tenant_id: &str,
    stack_id: &str,
) -> Result<B3Hash, (StatusCode, String)> {
    let stack = state
        .db
        .get_stack(tenant_id, stack_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Stack not found".to_string()))?;

    let adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Parse error: {}", e),
        )
    })?;

    let mut pairs = vec![];

    for id in &adapter_ids {
        let adapter = state
            .db
            .get_adapter_by_id(tenant_id, id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, format!("Adapter {} not found", id)))?;

        pairs.push((id.clone(), adapter.hash_b3));
    }

    // Use canonical compute_stack_hash from adapteros-core
    Ok(adapteros_core::compute_stack_hash(pairs))
}
