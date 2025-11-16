use crate::state::AppState;
use adapteros_api_types::adapters::*;
use adapteros_db::sqlx;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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

    let stacks = rows
        .into_iter()
        .map(|row| {
            let adapter_ids: Vec<String> = serde_json::from_str(&row.adapter_ids_json)
                .unwrap_or_else(|_| vec![]);
            let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
                "Parallel" => Some(WorkflowType::Parallel),
                "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
                "Sequential" => Some(WorkflowType::Sequential),
                _ => None,
            });

            StackResponse {
                id: row.id.unwrap_or_default(),
                name: row.name,
                description: row.description,
                adapter_ids,
                workflow_type,
                created_at: row.created_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                updated_at: row.updated_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                is_active: false,
            }
        })
        .collect();

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
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Stack not found".to_string()))?;

    let adapter_ids: Vec<String> = serde_json::from_str(&row.adapter_ids_json)
        .unwrap_or_else(|_| vec![]);
    let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
        "Parallel" => Some(WorkflowType::Parallel),
        "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
        "Sequential" => Some(WorkflowType::Sequential),
        _ => None,
    });

    Ok(Json(StackResponse {
        id: row.id.unwrap_or_default(),
        name: row.name,
        description: row.description,
        adapter_ids,
        workflow_type,
        created_at: row.created_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        updated_at: row.updated_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
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
    // First verify the stack exists
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
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Stack not found".to_string()))?;

    // Store the active stack ID in application state
    // This would be used by the routing logic
    {
        let mut active_stack = state.active_stack.write().unwrap();
        *active_stack = Some(id.clone());
    }

    Ok(Json(serde_json::json!({
        "message": format!("Stack '{}' activated", stack.name),
        "stack_id": id,
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
    {
        let mut active_stack = state.active_stack.write().unwrap();
        *active_stack = None;
    }

    Ok(Json(serde_json::json!({
        "message": "Active stack deactivated",
    })))
}