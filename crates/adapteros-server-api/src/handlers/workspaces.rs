//! Workspace management handlers
//!
//! Provides API endpoints for workspace CRUD, membership management, and resource sharing.
//! Workspaces enable cross-tenant collaboration while maintaining tenant isolation.

use crate::audit_helper::{actions, log_success, resources};
use crate::handlers::{AppState, Claims, ErrorResponse};
use crate::permissions::{require_permission, Permission};
use crate::PaginatedResponse;
use adapteros_db::workspaces::{ResourceType, WorkspaceRole};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{error, info};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddWorkspaceMemberRequest {
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub role: String,
    pub permissions_json: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateWorkspaceMemberRequest {
    pub role: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ShareResourceRequest {
    pub resource_type: String,
    pub resource_id: String,
}

/// List all workspaces with pagination
#[utoipa::path(
    get,
    path = "/v1/workspaces",
    params(
        ("page" = Option<u32>, Query, description = "Page number (1-indexed)"),
        ("limit" = Option<u32>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "Paginated list of workspaces", body = PaginatedResponse<WorkspaceResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(pagination): Query<adapteros_api_types::PaginationParams>,
) -> Result<Json<adapteros_api_types::PaginatedResponse<WorkspaceResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceView)?;

    let offset = (pagination.page.saturating_sub(1)) * pagination.limit;
    let (workspaces, total) = state
        .db
        .list_workspaces_paginated(pagination.limit as i64, offset as i64)
        .await
        .map_err(|e| {
            error!("Failed to list workspaces: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list workspaces")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let data: Vec<WorkspaceResponse> = workspaces
        .into_iter()
        .map(|w| WorkspaceResponse {
            id: w.id,
            name: w.name,
            description: w.description,
            created_by: w.created_by,
            created_at: w.created_at,
            updated_at: w.updated_at,
        })
        .collect();

    let pages = ((total as f64) / (pagination.limit as f64)).ceil() as u32;
    let response = adapteros_api_types::PaginatedResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        data,
        total: total as u64,
        page: pagination.page,
        limit: pagination.limit,
        pages,
    };

    Ok(Json(response))
}

/// List workspaces for current user
#[utoipa::path(
    get,
    path = "/v1/workspaces/me",
    responses(
        (status = 200, description = "User's workspaces", body = Vec<WorkspaceResponse>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn list_user_workspaces(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<WorkspaceResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceView)?;

    let tenant_id = claims.tenant_id.clone();
    let user_id = claims.sub.clone();

    let workspaces = state
        .db
        .list_user_workspaces(&user_id, &tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to list user workspaces: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list user workspaces")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses: Vec<WorkspaceResponse> = workspaces
        .into_iter()
        .map(|w| WorkspaceResponse {
            id: w.id,
            name: w.name,
            description: w.description,
            created_by: w.created_by,
            created_at: w.created_at,
            updated_at: w.updated_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Create a new workspace
#[utoipa::path(
    post,
    path = "/v1/workspaces",
    request_body = CreateWorkspaceRequest,
    responses(
        (status = 200, description = "Workspace created", body = WorkspaceResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn create_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage)?;

    info!("Creating workspace: {} by user: {}", req.name, claims.sub);

    let workspace_id = state
        .db
        .create_workspace(&req.name, req.description.as_deref(), &claims.sub)
        .await
        .map_err(|e| {
            error!("Failed to create workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create workspace")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Add creator as owner
    state
        .db
        .add_workspace_member(
            &workspace_id,
            &claims.tenant_id,
            Some(&claims.sub),
            WorkspaceRole::Owner,
            Some("[\"read\", \"write\", \"execute\"]"),
            &claims.sub,
        )
        .await
        .map_err(|e| {
            tracing::warn!("Failed to add creator as workspace owner: {}", e);
            // Non-fatal, workspace was created
        })
        .ok();

    let workspace = state
        .db
        .get_workspace(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to get created workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve created workspace")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Workspace not found after creation").with_code("NOT_FOUND"),
                ),
            )
        })?;

    // Audit log successful creation
    if let Err(e) = log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_CREATE,
        resources::WORKSPACE,
        Some(&workspace_id),
    )
    .await
    {
        tracing::warn!("Failed to log audit event: {}", e);
    }

    Ok(Json(WorkspaceResponse {
        id: workspace.id,
        name: workspace.name,
        description: workspace.description,
        created_by: workspace.created_by,
        created_at: workspace.created_at,
        updated_at: workspace.updated_at,
    }))
}

/// Get a workspace by ID
#[utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Workspace details", body = WorkspaceResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Workspace not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn get_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Result<Json<WorkspaceResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceView)?;

    // Check workspace access
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    if role.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to workspace").with_code("FORBIDDEN")),
        ));
    }

    let workspace = state
        .db
        .get_workspace(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to get workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to get workspace").with_code("INTERNAL_ERROR")),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Workspace not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(WorkspaceResponse {
        id: workspace.id,
        name: workspace.name,
        description: workspace.description,
        created_by: workspace.created_by,
        created_at: workspace.created_at,
        updated_at: workspace.updated_at,
    }))
}

/// Update a workspace
#[utoipa::path(
    put,
    path = "/v1/workspaces/{workspace_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID")
    ),
    request_body = UpdateWorkspaceRequest,
    responses(
        (status = 200, description = "Workspace updated", body = WorkspaceResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Workspace not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn update_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage)?;

    // Check workspace access - must be owner or member
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) | Some(WorkspaceRole::Member) => {
            // Allowed
        }
        Some(WorkspaceRole::Viewer) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Viewer role cannot update workspace")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
        None => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse::new("Access denied to workspace").with_code("FORBIDDEN")),
            ));
        }
    }

    state
        .db
        .update_workspace(
            &workspace_id,
            req.name.as_deref(),
            req.description.as_deref(),
        )
        .await
        .map_err(|e| {
            error!("Failed to update workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to update workspace").with_code("INTERNAL_ERROR")),
            )
        })?;

    let workspace = state
        .db
        .get_workspace(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to get updated workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve updated workspace")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Workspace not found").with_code("NOT_FOUND")),
            )
        })?;

    // Audit log successful update
    if let Err(e) = log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_UPDATE,
        resources::WORKSPACE,
        Some(&workspace_id),
    )
    .await
    {
        tracing::warn!("Failed to log audit event: {}", e);
    }

    Ok(Json(WorkspaceResponse {
        id: workspace.id,
        name: workspace.name,
        description: workspace.description,
        created_by: workspace.created_by,
        created_at: workspace.created_at,
        updated_at: workspace.updated_at,
    }))
}

/// Delete a workspace
#[utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "Workspace deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Workspace not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn delete_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage)?;

    // Only owners can delete workspaces
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) => {
            // Allowed
        }
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Only workspace owners can delete workspaces")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
    }

    state
        .db
        .delete_workspace(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to delete workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to delete workspace").with_code("INTERNAL_ERROR")),
            )
        })?;

    // Audit log successful deletion
    if let Err(e) = log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_DELETE,
        resources::WORKSPACE,
        Some(&workspace_id),
    )
    .await
    {
        tracing::warn!("Failed to log audit event: {}", e);
    }

    Ok(Json(
        serde_json::json!({"status": "deleted", "id": workspace_id}),
    ))
}

/// List workspace members
#[utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/members",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "List of workspace members"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Workspace not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn list_workspace_members(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceView)?;

    // Check workspace access
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    if role.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to workspace").with_code("FORBIDDEN")),
        ));
    }

    let members = state
        .db
        .list_workspace_members(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to list workspace members: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list workspace members")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    let responses: Vec<serde_json::Value> = members
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "workspace_id": m.workspace_id,
                "tenant_id": m.tenant_id,
                "user_id": m.user_id,
                "role": m.role,
                "permissions_json": m.permissions_json,
                "added_by": m.added_by,
                "added_at": m.added_at,
            })
        })
        .collect();

    Ok(Json(responses))
}

/// Add a workspace member
#[utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/members",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID")
    ),
    request_body = AddWorkspaceMemberRequest,
    responses(
        (status = 200, description = "Member added"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn add_workspace_member(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<AddWorkspaceMemberRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceMemberManage)?;

    // Only owners and members can add members
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) | Some(WorkspaceRole::Member) => {
            // Allowed
        }
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Only owners and members can add workspace members")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
    }

    let workspace_role = WorkspaceRole::from_str(&req.role).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid role")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let member_id = state
        .db
        .add_workspace_member(
            &workspace_id,
            &req.tenant_id,
            req.user_id.as_deref(),
            workspace_role,
            req.permissions_json.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| {
            error!("Failed to add workspace member: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to add workspace member")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let user_id = req.user_id.clone().unwrap_or_else(|| req.tenant_id.clone());
    log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_MEMBER_ADD,
        resources::WORKSPACE_MEMBER,
        Some(&format!("{}:{}", workspace_id, user_id)),
    )
    .await
    .ok();

    Ok(Json(
        serde_json::json!({"id": member_id, "status": "added"}),
    ))
}

/// Update workspace member role
#[utoipa::path(
    put,
    path = "/v1/workspaces/{workspace_id}/members/{member_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID"),
        ("member_id" = String, Path, description = "Member ID")
    ),
    request_body = UpdateWorkspaceMemberRequest,
    responses(
        (status = 200, description = "Member role updated"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Member not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn update_workspace_member(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, member_id)): Path<(String, String)>,
    Json(req): Json<UpdateWorkspaceMemberRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceMemberManage)?;

    // Only owners can update member roles
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) => {
            // Allowed
        }
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Only workspace owners can update member roles")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
    }

    // Get member to find tenant_id and user_id
    let members = state
        .db
        .list_workspace_members(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to list workspace members: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list workspace members")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    let member = members
        .into_iter()
        .find(|m| m.id == member_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Member not found").with_code("NOT_FOUND")),
            )
        })?;

    let workspace_role = WorkspaceRole::from_str(&req.role).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid role")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    state
        .db
        .update_workspace_member_role(
            &workspace_id,
            &member.tenant_id,
            member.user_id.as_deref(),
            workspace_role,
        )
        .await
        .map_err(|e| {
            error!("Failed to update workspace member role: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update workspace member role")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_MEMBER_UPDATE,
        resources::WORKSPACE_MEMBER,
        Some(&format!("{}:{}", workspace_id, member_id)),
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({"status": "updated"})))
}

/// Remove workspace member
#[utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/members/{member_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID"),
        ("member_id" = String, Path, description = "Member ID")
    ),
    responses(
        (status = 200, description = "Member removed"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Member not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn remove_workspace_member(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, member_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceMemberManage)?;

    // Only owners can remove members
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) => {
            // Allowed
        }
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Only workspace owners can remove members")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
    }

    // Get member to find tenant_id and user_id
    let members = state
        .db
        .list_workspace_members(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to list workspace members: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list workspace members")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    let member = members
        .into_iter()
        .find(|m| m.id == member_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Member not found").with_code("NOT_FOUND")),
            )
        })?;

    state
        .db
        .remove_workspace_member(&workspace_id, &member.tenant_id, member.user_id.as_deref())
        .await
        .map_err(|e| {
            error!("Failed to remove workspace member: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to remove workspace member")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_MEMBER_REMOVE,
        resources::WORKSPACE_MEMBER,
        Some(&format!("{}:{}", workspace_id, member_id)),
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({"status": "removed"})))
}

/// List workspace resources
#[utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/resources",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID")
    ),
    responses(
        (status = 200, description = "List of workspace resources"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Workspace not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn list_workspace_resources(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceView)?;

    // Check workspace access
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    if role.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to workspace").with_code("FORBIDDEN")),
        ));
    }

    let resources = state
        .db
        .list_workspace_resources(&workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to list workspace resources: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list workspace resources")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    let responses: Vec<serde_json::Value> = resources
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "workspace_id": r.workspace_id,
                "resource_type": r.resource_type,
                "resource_id": r.resource_id,
                "shared_by": r.shared_by,
                "shared_by_tenant_id": r.shared_by_tenant_id,
                "shared_at": r.shared_at,
            })
        })
        .collect();

    Ok(Json(responses))
}

/// Share a resource to workspace
#[utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/resources",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID")
    ),
    request_body = ShareResourceRequest,
    responses(
        (status = 200, description = "Resource shared"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Resource not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn share_workspace_resource(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<ShareResourceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage)?;

    // Check workspace access - must be member or owner
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) | Some(WorkspaceRole::Member) => {
            // Allowed
        }
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Only owners and members can share resources")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
    }

    // Validate resource exists and belongs to tenant
    let resource_type = ResourceType::from_str(&req.resource_type).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid resource type")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Validate resource exists and belongs to the tenant
    match resource_type {
        ResourceType::Adapter => {
            let adapter = state.db.get_adapter(&req.resource_id).await.map_err(|e| {
                error!("Failed to check adapter existence: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate resource")
                            .with_code("INTERNAL_ERROR"),
                    ),
                )
            })?;

            match adapter {
                Some(a) if a.tenant_id == claims.tenant_id => {
                    // Adapter exists and belongs to tenant
                }
                Some(_) => {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(
                            ErrorResponse::new("Resource belongs to a different tenant")
                                .with_code("FORBIDDEN"),
                        ),
                    ));
                }
                None => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(
                            ErrorResponse::new("Adapter not found")
                                .with_code("NOT_FOUND")
                                .with_string_details(format!(
                                    "Adapter '{}' does not exist",
                                    req.resource_id
                                )),
                        ),
                    ));
                }
            }
        }
        ResourceType::Node => {
            let node = state.db.get_node(&req.resource_id).await.map_err(|e| {
                error!("Failed to check node existence: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate resource")
                            .with_code("INTERNAL_ERROR"),
                    ),
                )
            })?;

            if node.is_none() {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Node not found")
                            .with_code("NOT_FOUND")
                            .with_string_details(format!(
                                "Node '{}' does not exist",
                                req.resource_id
                            )),
                    ),
                ));
            }
            // Note: Nodes don't have tenant_id, so we only check existence
        }
        ResourceType::Model => {
            let model = state.db.get_model(&req.resource_id).await.map_err(|e| {
                error!("Failed to check model existence: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate resource")
                            .with_code("INTERNAL_ERROR"),
                    ),
                )
            })?;

            if model.is_none() {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Model not found")
                            .with_code("NOT_FOUND")
                            .with_string_details(format!(
                                "Model '{}' does not exist",
                                req.resource_id
                            )),
                    ),
                ));
            }
            // Note: Models are shared across tenants, so we only check existence
        }
    }

    // Resource validation passed - proceed with sharing
    let resource_id = state
        .db
        .add_workspace_resource(
            &workspace_id,
            resource_type,
            &req.resource_id,
            &claims.sub,
            &claims.tenant_id,
        )
        .await
        .map_err(|e| {
            error!("Failed to share resource to workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to share resource")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_RESOURCE_SHARE,
        resources::WORKSPACE_RESOURCE,
        Some(&req.resource_id),
    )
    .await
    .ok();

    Ok(Json(
        serde_json::json!({"id": resource_id, "status": "shared"}),
    ))
}

/// Remove resource from workspace
#[utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/resources/{resource_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID"),
        ("resource_id" = String, Path, description = "Resource ID"),
        ("resource_type" = String, Query, description = "Resource type (adapter, node, or model)")
    ),
    responses(
        (status = 200, description = "Resource unshared"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn unshare_workspace_resource(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, resource_id)): Path<(String, String)>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage)?;

    // Check workspace access - must be member or owner
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) | Some(WorkspaceRole::Member) => {
            // Allowed
        }
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Only owners and members can unshare resources")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
    }

    let resource_type_str = params.get("resource_type").ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("resource_type parameter required").with_code("BAD_REQUEST")),
        )
    })?;

    let resource_type = ResourceType::from_str(resource_type_str).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid resource type")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    state
        .db
        .remove_workspace_resource(&workspace_id, resource_type, &resource_id)
        .await
        .map_err(|e| {
            error!("Failed to unshare resource from workspace: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to unshare resource")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    log_success(
        &state.db,
        &claims,
        actions::WORKSPACE_RESOURCE_UNSHARE,
        resources::WORKSPACE_RESOURCE,
        Some(&resource_id),
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({"status": "unshared"})))
}
