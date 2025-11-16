//! Adapter stack management endpoints

use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

/// Request to create a new adapter stack
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateStackRequest {
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
}

/// Request to update an adapter stack
#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateStackRequest {
    pub description: Option<String>,
    pub adapter_ids: Option<Vec<String>>,
}

/// Response for adapter stack
#[derive(Debug, Serialize)]
pub struct StackResponse {
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Response for active stack status
#[derive(Debug, Serialize)]
pub struct ActiveStackResponse {
    pub active_stack: Option<String>,
    pub adapter_count: Option<usize>,
}

/// List all adapter stacks
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks",
    responses(
        (status = 200, description = "List of adapter stacks", body = Vec<StackResponse>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn list_stacks(
    State(state): State<AppState>,
) -> Result<Json<Vec<StackResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let stacks = state.registry.list_stacks().map_err(|e| {
        error!("Failed to list adapter stacks: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to list adapter stacks")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<StackResponse> = stacks
        .into_iter()
        .map(|s| StackResponse {
            name: s.name,
            description: s.description,
            adapter_ids: s.adapter_ids,
            created_at: s.created_at,
            updated_at: s.updated_at,
        })
        .collect();

    Ok(Json(response))
}

/// Get a specific adapter stack by name
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks/{name}",
    params(
        ("name" = String, Path, description = "Adapter stack name")
    ),
    responses(
        (status = 200, description = "Adapter stack details", body = StackResponse),
        (status = 404, description = "Adapter stack not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn get_stack(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<StackResponse>, (StatusCode, Json<ErrorResponse>)> {
    let stack = state.registry.get_stack(&name).map_err(|e| {
        error!("Failed to get adapter stack: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get adapter stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let stack = stack.ok_or_else(|| {
        warn!("Adapter stack not found: {}", name);
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Adapter stack not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Stack name: {}", name)),
            ),
        )
    })?;

    Ok(Json(StackResponse {
        name: stack.name,
        description: stack.description,
        adapter_ids: stack.adapter_ids,
        created_at: stack.created_at,
        updated_at: stack.updated_at,
    }))
}

/// Create a new adapter stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks",
    request_body = CreateStackRequest,
    responses(
        (status = 201, description = "Adapter stack created", body = StackResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 409, description = "Stack already exists", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn create_stack(
    State(state): State<AppState>,
    Json(req): Json<CreateStackRequest>,
) -> Result<(StatusCode, Json<StackResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Validate request
    if req.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Stack name cannot be empty")
                    .with_code("INVALID_REQUEST"),
            ),
        ));
    }

    if req.adapter_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Stack must contain at least one adapter")
                    .with_code("INVALID_REQUEST"),
            ),
        ));
    }

    // Check if stack already exists
    if state.registry.get_stack(&req.name).map_err(|e| {
        error!("Failed to check if stack exists: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to check stack existence")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("Stack already exists")
                    .with_code("CONFLICT")
                    .with_string_details(format!("Stack name: {}", req.name)),
            ),
        ));
    }

    // Create the stack
    state
        .registry
        .create_stack(&req.name, req.description.as_deref(), &req.adapter_ids)
        .map_err(|e| {
            error!("Failed to create adapter stack: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create adapter stack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!("Created adapter stack: {}", req.name);

    // Fetch the created stack
    let stack = state.registry.get_stack(&req.name).map_err(|e| {
        error!("Failed to fetch created stack: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to fetch created stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let stack = stack.ok_or_else(|| {
        error!("Created stack not found: {}", req.name);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Created stack not found")
                    .with_code("INTERNAL_ERROR"),
            ),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(StackResponse {
            name: stack.name,
            description: stack.description,
            adapter_ids: stack.adapter_ids,
            created_at: stack.created_at,
            updated_at: stack.updated_at,
        }),
    ))
}

/// Update an existing adapter stack
#[utoipa::path(
    put,
    path = "/v1/adapter-stacks/{name}",
    params(
        ("name" = String, Path, description = "Adapter stack name")
    ),
    request_body = UpdateStackRequest,
    responses(
        (status = 200, description = "Adapter stack updated", body = StackResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Stack not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn update_stack(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<UpdateStackRequest>,
) -> Result<Json<StackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate that at least one field is being updated
    if req.description.is_none() && req.adapter_ids.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("At least one field must be updated")
                    .with_code("INVALID_REQUEST"),
            ),
        ));
    }

    // Validate adapter_ids if provided
    if let Some(ref adapter_ids) = req.adapter_ids {
        if adapter_ids.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Stack must contain at least one adapter")
                        .with_code("INVALID_REQUEST"),
                ),
            ));
        }
    }

    // Update the stack
    state
        .registry
        .update_stack(
            &name,
            req.description.as_deref(),
            req.adapter_ids.as_deref(),
        )
        .map_err(|e| {
            error!("Failed to update adapter stack: {}", e);
            let status = if e.to_string().contains("does not exist") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new("Failed to update adapter stack")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!("Updated adapter stack: {}", name);

    // Fetch the updated stack
    let stack = state.registry.get_stack(&name).map_err(|e| {
        error!("Failed to fetch updated stack: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to fetch updated stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let stack = stack.ok_or_else(|| {
        error!("Updated stack not found: {}", name);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Updated stack not found")
                    .with_code("INTERNAL_ERROR"),
            ),
        )
    })?;

    Ok(Json(StackResponse {
        name: stack.name,
        description: stack.description,
        adapter_ids: stack.adapter_ids,
        created_at: stack.created_at,
        updated_at: stack.updated_at,
    }))
}

/// Delete an adapter stack
#[utoipa::path(
    delete,
    path = "/v1/adapter-stacks/{name}",
    params(
        ("name" = String, Path, description = "Adapter stack name")
    ),
    responses(
        (status = 204, description = "Adapter stack deleted"),
        (status = 400, description = "Cannot delete active stack", body = ErrorResponse),
        (status = 404, description = "Stack not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn delete_stack(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    state.registry.delete_stack(&name).map_err(|e| {
        error!("Failed to delete adapter stack: {}", e);
        let status = if e.to_string().contains("does not exist") {
            StatusCode::NOT_FOUND
        } else if e.to_string().contains("active stack") {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (
            status,
            Json(
                ErrorResponse::new("Failed to delete adapter stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    info!("Deleted adapter stack: {}", name);

    Ok(StatusCode::NO_CONTENT)
}

/// Activate an adapter stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks/{name}/activate",
    params(
        ("name" = String, Path, description = "Adapter stack name")
    ),
    responses(
        (status = 200, description = "Adapter stack activated", body = ActiveStackResponse),
        (status = 404, description = "Stack not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn activate_stack(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ActiveStackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Activate in database
    state.registry.activate_stack(&name).map_err(|e| {
        error!("Failed to activate adapter stack: {}", e);
        let status = if e.to_string().contains("does not exist") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (
            status,
            Json(
                ErrorResponse::new("Failed to activate adapter stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the stack details
    let stack = state.registry.get_stack(&name).map_err(|e| {
        error!("Failed to fetch activated stack: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to fetch activated stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?.ok_or_else(|| {
        error!("Stack not found after activation: {}", name);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Stack not found after activation")
                    .with_code("INTERNAL_ERROR"),
            ),
        )
    })?;

    // Translate adapter IDs to indices
    let adapter_indices = translate_adapter_ids_to_indices(&state, &stack.adapter_ids)
        .await
        .map_err(|e| {
            error!("Failed to translate adapter IDs to indices: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to translate adapter IDs to indices")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Update router filter
    {
        let mut router = state.router.write().unwrap();
        router.set_active_stack(adapter_indices);
    }

    info!(
        "Activated adapter stack: {} with {} adapters",
        name,
        stack.adapter_ids.len()
    );

    Ok(Json(ActiveStackResponse {
        active_stack: Some(name),
        adapter_count: Some(stack.adapter_ids.len()),
    }))
}

/// Helper function to translate adapter IDs (strings) to adapter indices (usize)
/// Uses the lifecycle manager's current adapter list to build the mapping
async fn translate_adapter_ids_to_indices(
    state: &AppState,
    adapter_ids: &[String],
) -> Result<Vec<usize>, String> {
    // For now, use a simple approach: query all adapters from registry
    // and build an ID→index mapping based on sorted adapter IDs
    let all_adapters = state.registry.list_adapters()
        .map_err(|e| format!("Failed to list adapters: {}", e))?;

    // Sort by ID for deterministic index assignment
    let mut sorted_adapters = all_adapters;
    sorted_adapters.sort_by(|a, b| a.id.cmp(&b.id));

    // Build ID→index map
    let id_to_index: std::collections::HashMap<String, usize> = sorted_adapters
        .iter()
        .enumerate()
        .map(|(idx, adapter)| (adapter.id.clone(), idx))
        .collect();

    // Translate requested adapter IDs to indices
    let mut indices = Vec::new();
    for adapter_id in adapter_ids {
        if let Some(&idx) = id_to_index.get(adapter_id) {
            indices.push(idx);
        } else {
            return Err(format!("Adapter ID {} not found in registry", adapter_id));
        }
    }

    Ok(indices)
}

/// Deactivate the currently active adapter stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks/deactivate",
    responses(
        (status = 200, description = "Adapter stack deactivated", body = ActiveStackResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn deactivate_stack(
    State(state): State<AppState>,
) -> Result<Json<ActiveStackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Deactivate in database
    state.registry.deactivate_stack().map_err(|e| {
        error!("Failed to deactivate adapter stack: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to deactivate adapter stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Clear router filter
    {
        let mut router = state.router.write().unwrap();
        router.clear_active_stack();
    }

    info!("Deactivated adapter stack");

    Ok(Json(ActiveStackResponse {
        active_stack: None,
        adapter_count: None,
    }))
}

/// Get the currently active adapter stack
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks/active",
    responses(
        (status = 200, description = "Active adapter stack status", body = ActiveStackResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "Adapter Stacks"
)]
pub async fn get_active_stack(
    State(state): State<AppState>,
) -> Result<Json<ActiveStackResponse>, (StatusCode, Json<ErrorResponse>)> {
    let active_stack = state.registry.get_active_stack().map_err(|e| {
        error!("Failed to get active adapter stack: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get active adapter stack")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let adapter_count = if let Some(ref stack_name) = active_stack {
        let stack = state.registry.get_stack(stack_name).map_err(|e| {
            error!("Failed to fetch active stack details: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to fetch active stack details")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
        stack.map(|s| s.adapter_ids.len())
    } else {
        None
    };

    Ok(Json(ActiveStackResponse {
        active_stack,
        adapter_count,
    }))
}
