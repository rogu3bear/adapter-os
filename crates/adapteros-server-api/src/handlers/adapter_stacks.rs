use crate::state::AppState;
use adapteros_api_types::adapters::*;
use adapteros_core::StackName;
use adapteros_db::sqlx;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    Json(req): Json<CreateStackRequest>,
) -> Result<(StatusCode, Json<StackResponse>), (StatusCode, String)> {
    // Validate stack name format
    let stack_name = StackName::parse(&req.name).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid stack name: {}", e),
        )
    })?;

    info!(
        stack_name = %stack_name,
        adapter_count = req.adapter_ids.len(),
        "Creating adapter stack"
    );

    let id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let adapter_ids_json = serde_json::to_string(&req.adapter_ids)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let workflow_type_str = req.workflow_type.as_ref().map(|w| format!("{:?}", w));

    sqlx::query!(
        r#"
        INSERT INTO adapter_stacks (id, name, description, adapter_ids_json, workflow_type, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        req.name,
        req.description,
        adapter_ids_json,
        workflow_type_str,
        now,
        now
    )
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            (
                StatusCode::CONFLICT,
                format!("Stack name '{}' already exists", req.name),
            )
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    })?;

    info!(stack_id = %id, stack_name = %stack_name, "Adapter stack created");

    Ok((
        StatusCode::CREATED,
        Json(StackResponse {
            id: id.clone(),
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
) -> Result<Json<Vec<StackResponse>>, (StatusCode, String)> {
    let rows = sqlx::query!(
        r#"
        SELECT id, name, description, adapter_ids_json, workflow_type, created_at, updated_at
        FROM adapter_stacks
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut stacks = Vec::new();
    for row in rows {
        // Properly handle potential missing or invalid data
        let id = row.id.ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Stack missing ID in database"),
            )
        })?;

        let name = row.name;

        let adapter_ids: Vec<String> = serde_json::from_str(&row.adapter_ids_json)
            .map_err(|e| {
                // Log the error but don't fail - return empty list for corrupted data
                tracing::warn!(
                    "Corrupted adapter_ids_json in stack {}: {}. Using empty list.",
                    name,
                    e
                );
                // Continue with empty list for this specific case
            })
            .unwrap_or_else(|_| vec![]);

        let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
            "Parallel" => Some(WorkflowType::Parallel),
            "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
            "Sequential" => Some(WorkflowType::Sequential),
            invalid => {
                tracing::warn!("Invalid workflow_type '{}' for stack {}", invalid, name);
                None
            }
        });

        let created_at = row.created_at.ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Stack {} missing created_at timestamp", name),
            )
        })?;

        let updated_at = row.updated_at.ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Stack {} missing updated_at timestamp", name),
            )
        })?;

        stacks.push(StackResponse {
            id,
            name,
            description: row.description,
            adapter_ids,
            workflow_type,
            created_at,
            updated_at,
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
    Path(id): Path<String>,
) -> Result<Json<StackResponse>, (StatusCode, String)> {
    let row = sqlx::query!(
        r#"
        SELECT id, name, description, adapter_ids_json, workflow_type, created_at, updated_at
        FROM adapter_stacks
        WHERE id = ?
        "#,
        id
    )
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Stack with id '{}' not found", id),
        )
    })?;

    // Validate that critical fields are present
    let stack_id = row.id.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Stack record missing ID field"),
        )
    })?;

    let name = row.name;

    let adapter_ids: Vec<String> = serde_json::from_str(&row.adapter_ids_json)
        .map_err(|e| {
            tracing::warn!("Failed to parse adapter_ids_json for stack {}: {}", name, e);
        })
        .unwrap_or_else(|_| vec![]);

    let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
        "Parallel" => Some(WorkflowType::Parallel),
        "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
        "Sequential" => Some(WorkflowType::Sequential),
        invalid => {
            tracing::warn!("Invalid workflow_type '{}' for stack {}", invalid, name);
            None
        }
    });

    let created_at = row.created_at.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Stack {} missing created_at timestamp", name),
        )
    })?;

    let updated_at = row.updated_at.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Stack {} missing updated_at timestamp", name),
        )
    })?;

    Ok(Json(StackResponse {
        id: stack_id,
        name,
        description: row.description,
        adapter_ids,
        workflow_type,
        created_at,
        updated_at,
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
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let result = sqlx::query!(
        r#"
        DELETE FROM adapter_stacks
        WHERE id = ?
        "#,
        id
    )
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Stack not found".to_string()));
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
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // First verify the stack exists and parse adapter IDs
    let stack = sqlx::query!(
        r#"
        SELECT id, name, adapter_ids_json
        FROM adapter_stacks
        WHERE id = ?
        "#,
        id
    )
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        warn!("Database error while fetching stack {}: {}", id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
    })?
    .ok_or_else(|| {
        warn!("Attempted to activate non-existent stack: {}", id);
        (
            StatusCode::NOT_FOUND,
            format!("Stack with id '{}' not found", id),
        )
    })?;

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

    info!(
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
        "message": format!("Stack '{}' activated", name),
        "stack_id": id,
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
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let previous_stack = {
        let mut active_stack = state.active_stack.write().map_err(|e| {
            warn!("Failed to acquire write lock for active_stack: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal synchronization error".to_string(),
            )
        })?;
        let prev = active_stack.clone();
        *active_stack = None;
        prev
    };

    match previous_stack {
        Some(stack_id) => {
            info!("Deactivated adapter stack '{}'", stack_id);
            Ok(Json(serde_json::json!({
                "message": "Active stack deactivated",
                "previous_stack": stack_id,
            })))
        }
        None => {
            info!("Deactivate called but no stack was active");
            Ok(Json(serde_json::json!({
                "message": "No stack was active",
            })))
        }
    }
}
