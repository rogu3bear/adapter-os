//! Workspace management handlers
//!
//! Provides API endpoints for workspace CRUD, membership management, and resource sharing.
//! Workspaces enable cross-tenant collaboration while maintaining tenant isolation.

use crate::api_error::ApiError;
use crate::audit_helper::{actions, log_success_or_warn, resources};
use crate::control_plane::model_worker_lifecycle_reducer::{
    ModelWorkerLifecycleEvent, ModelWorkerLifecycleReducer,
};
use crate::handlers::{AppState, Claims, ErrorResponse};
use crate::ip_extraction::ClientIp;
use crate::permissions::{require_permission, Permission};
use crate::uds_client::UdsClient;
use crate::PaginatedResponse;
use adapteros_api_types::ModelLoadStatus;
use adapteros_config::resolve_worker_socket_for_cp;
use adapteros_core::AosError;
use adapteros_db::workspaces::{ResourceType, WorkspaceRole};
use adapteros_db::WorkspaceActiveState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct WorkspaceActiveStateRequest {
    /// Base model to mark as active for this workspace (optional).
    pub active_base_model_id: Option<String>,
    /// Plan to mark as active for this workspace (optional).
    pub active_plan_id: Option<String>,
    /// Adapters to keep active for this workspace (optional).
    #[serde(default)]
    pub active_adapter_ids: Vec<String>,
    /// Manifest hash associated with the active plan/model.
    pub manifest_hash_b3: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceActiveStateResponse {
    pub workspace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_base_model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_plan_id: Option<String>,
    #[serde(default)]
    pub active_adapter_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_loaded: Option<bool>,
    #[serde(default)]
    pub model_mismatch: bool,
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
) -> Result<
    Json<adapteros_api_types::PaginatedResponse<WorkspaceResponse>>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_permission(&claims, Permission::WorkspaceView)?;

    let offset = (pagination.page.saturating_sub(1)) * pagination.limit;

    // TENANT ISOLATION: Only list workspaces where the user's tenant is a member
    // This is implemented by list_user_workspaces which filters by tenant_id
    let tenant_id = claims.tenant_id.clone();
    let user_id = claims.sub.clone();

    let workspaces = state
        .db
        .list_user_workspaces(&user_id, &tenant_id)
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

    let total = workspaces.len() as i64;
    let workspaces: Vec<_> = workspaces
        .into_iter()
        .skip(offset as usize)
        .take(pagination.limit as usize)
        .collect();

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
    Extension(client_ip): Extension<ClientIp>,
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
    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_CREATE,
        resources::WORKSPACE,
        Some(&workspace_id),
        Some(client_ip.0.as_str()),
    )
    .await;

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
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access (validates user's tenant is a workspace member)
    // Workspaces don't have a single tenant_id - they're cross-tenant by design.
    // Isolation is enforced through workspace_members table membership validation.
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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
    Extension(client_ip): Extension<ClientIp>,
    Path(workspace_id): Path<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access - must be owner or member
    // Validates user's tenant is a workspace member with appropriate role
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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
        Some(WorkspaceRole::Admin) | Some(WorkspaceRole::Owner) | Some(WorkspaceRole::Member) => {
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
    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_UPDATE,
        resources::WORKSPACE,
        Some(&workspace_id),
        Some(client_ip.0.as_str()),
    )
    .await;

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
    Extension(client_ip): Extension<ClientIp>,
    Path(workspace_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Only owners can delete workspaces
    // Validates user's tenant is a workspace member with owner role
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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
    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_DELETE,
        resources::WORKSPACE,
        Some(&workspace_id),
        Some(client_ip.0.as_str()),
    )
    .await;

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
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access
    // Validates user's tenant is a workspace member before listing all members
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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
    Extension(client_ip): Extension<ClientIp>,
    Path(workspace_id): Path<String>,
    Json(req): Json<AddWorkspaceMemberRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceMemberManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Only owners and members can add members
    // Validates user's tenant is a workspace member with appropriate role
    // Note: req.tenant_id can be different from claims.tenant_id (cross-tenant by design)
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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
    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_MEMBER_ADD,
        resources::WORKSPACE_MEMBER,
        Some(&format!("{}:{}", workspace_id, user_id)),
        Some(client_ip.0.as_str()),
    )
    .await;

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
    Extension(client_ip): Extension<ClientIp>,
    Path((workspace_id, member_id)): Path<(String, String)>,
    Json(req): Json<UpdateWorkspaceMemberRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceMemberManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    let member_id = crate::id_resolver::resolve_any_id(&state.db, &member_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Only owners can update member roles
    // Validates user's tenant is a workspace member with owner role
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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

    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_MEMBER_UPDATE,
        resources::WORKSPACE_MEMBER,
        Some(&format!("{}:{}", workspace_id, member_id)),
        Some(client_ip.0.as_str()),
    )
    .await;

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
    Extension(client_ip): Extension<ClientIp>,
    Path((workspace_id, member_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceMemberManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    let member_id = crate::id_resolver::resolve_any_id(&state.db, &member_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Only owners can remove members
    // Validates user's tenant is a workspace member with owner role
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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

    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_MEMBER_REMOVE,
        resources::WORKSPACE_MEMBER,
        Some(&format!("{}:{}", workspace_id, member_id)),
        Some(client_ip.0.as_str()),
    )
    .await;

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
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access
    // Validates user's tenant is a workspace member before listing shared resources
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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
    Extension(client_ip): Extension<ClientIp>,
    Path(workspace_id): Path<String>,
    Json(req): Json<ShareResourceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access - must be member or owner
    // Validates user's tenant is a workspace member with appropriate role
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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

    // TENANT ISOLATION: Validate resource exists and belongs to tenant
    // Users can only share resources that belong to their own tenant
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
            let adapter = state
                .db
                .get_adapter_for_tenant(&claims.tenant_id, &req.resource_id)
                .await
                .map_err(|e| {
                    error!("Failed to check adapter existence: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to validate resource")
                                .with_code("INTERNAL_ERROR"),
                        ),
                    )
                })?;

            if adapter.is_none() {
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

    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_RESOURCE_SHARE,
        resources::WORKSPACE_RESOURCE,
        Some(&req.resource_id),
        Some(client_ip.0.as_str()),
    )
    .await;

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
    Extension(client_ip): Extension<ClientIp>,
    Path((workspace_id, resource_id)): Path<(String, String)>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    let resource_id = crate::id_resolver::resolve_any_id(&state.db, &resource_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access - must be member or owner
    // Validates user's tenant is a workspace member with appropriate role
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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

    log_success_or_warn(
        &state.db,
        &claims,
        actions::WORKSPACE_RESOURCE_UNSHARE,
        resources::WORKSPACE_RESOURCE,
        Some(&resource_id),
        Some(client_ip.0.as_str()),
    )
    .await;

    Ok(Json(serde_json::json!({"status": "unshared"})))
}

/// Get the active state for a workspace (model/plan/adapters).
#[utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/active",
    params(
        ("workspace_id" = String, Path, description = "Workspace/tenant ID")
    ),
    responses(
        (status = 200, description = "Active workspace state", body = WorkspaceActiveStateResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn get_workspace_active_state(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Result<Json<WorkspaceActiveStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceView)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access (validates user's tenant is a workspace member)
    // Workspaces don't have a single tenant_id - they're cross-tenant by design.
    // Isolation is enforced through workspace_members table membership validation.
    // Admin bypass (admin_tenants=["*"]) is handled by check_workspace_access_with_admin.
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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

    let record = state
        .db
        .get_workspace_active_state(&workspace_id)
        .await
        .map_err(|e| {
            ApiError::internal("Failed to fetch workspace active state").with_details(e.to_string())
        })?;

    let response = build_active_state_response(&state, workspace_id, record).await?;
    Ok(Json(response))
}

/// Set the active state for a workspace.
#[utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/active",
    params(
        ("workspace_id" = String, Path, description = "Workspace/tenant ID")
    ),
    request_body = WorkspaceActiveStateRequest,
    responses(
        (status = 200, description = "Active workspace state updated", body = WorkspaceActiveStateResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Resource not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "workspaces"
)]
pub async fn set_workspace_active_state(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<WorkspaceActiveStateRequest>,
) -> Result<Json<WorkspaceActiveStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage)?;
    let workspace_id = crate::id_resolver::resolve_any_id(&state.db, &workspace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // TENANT ISOLATION: Check workspace access (validates user's tenant is a workspace member)
    // Workspaces don't have a single tenant_id - they're cross-tenant by design.
    // Isolation is enforced through workspace_members table membership validation.
    let role = state
        .db
        .check_workspace_access_with_admin(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &claims.admin_tenants,
        )
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

    if let Some(ref model_id) = req.active_base_model_id {
        let model = state
            .db
            .get_model_for_tenant(&workspace_id, model_id)
            .await
            .map_err(|e| {
                ApiError::internal("Failed to validate base model").with_details(e.to_string())
            })?;

        if model.is_none() {
            return Err(not_found_response("Model", model_id));
        }
    }

    let mut plan_manifest_hash: Option<String> = None;
    if let Some(ref plan_id) = req.active_plan_id {
        let plan = state.db.get_plan(plan_id).await.map_err(|e| {
            ApiError::internal("Failed to validate plan").with_details(e.to_string())
        })?;

        let Some(plan) = plan else {
            return Err(not_found_response("Plan", plan_id));
        };

        if plan.tenant_id != workspace_id {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Plan does not belong to workspace").with_code("FORBIDDEN"),
                ),
            ));
        }

        plan_manifest_hash = Some(plan.manifest_hash_b3);
    }

    for adapter_id in &req.active_adapter_ids {
        let adapter = state
            .db
            .get_adapter_for_tenant(&workspace_id, adapter_id)
            .await
            .map_err(|e| {
                ApiError::internal("Failed to validate adapter").with_details(e.to_string())
            })?;

        if adapter.is_none() {
            return Err(not_found_response("Adapter", adapter_id));
        }
    }

    let manifest_hash = req
        .manifest_hash_b3
        .clone()
        .or(plan_manifest_hash)
        .or_else(|| state.manifest_hash.clone());

    let stored = state
        .db
        .upsert_workspace_active_state(
            &workspace_id,
            req.active_base_model_id.as_deref(),
            req.active_plan_id.as_deref(),
            Some(req.active_adapter_ids.as_slice()),
            manifest_hash.as_deref(),
        )
        .await
        .map_err(|e| {
            ApiError::internal("Failed to store workspace active state").with_details(e.to_string())
        })?;

    let response = build_active_state_response(&state, workspace_id, Some(stored)).await?;
    Ok(Json(response))
}

pub(crate) async fn build_active_state_response(
    state: &AppState,
    workspace_id: String,
    record: Option<WorkspaceActiveState>,
) -> Result<WorkspaceActiveStateResponse, (StatusCode, Json<ErrorResponse>)> {
    let mut active_adapter_ids: Vec<String> = Vec::new();
    let mut active_base_model_id = None;
    let mut active_plan_id = None;
    let mut manifest_hash_b3 = None;
    let mut updated_at = None;

    if let Some(state_record) = record {
        active_base_model_id = state_record.active_base_model_id.clone();
        active_plan_id = state_record.active_plan_id.clone();
        manifest_hash_b3 = state_record.manifest_hash_b3.clone();
        updated_at = Some(state_record.updated_at.clone());

        if let Some(raw) = state_record.active_adapter_ids.as_deref() {
            if !raw.is_empty() {
                active_adapter_ids = serde_json::from_str(raw).map_err(|e| {
                    ApiError::internal("Failed to parse stored adapter ids")
                        .with_details(e.to_string())
                })?;
            }
        }
    }

    let (model_loaded, model_mismatch) = if let Some(model_id) = active_base_model_id.as_deref() {
        match is_model_ready(state, &workspace_id, model_id).await? {
            Some(true) => (Some(true), false),
            Some(false) | None => (Some(false), true),
        }
    } else {
        (None, false)
    };

    Ok(WorkspaceActiveStateResponse {
        workspace_id,
        active_base_model_id,
        active_plan_id,
        active_adapter_ids,
        manifest_hash_b3,
        updated_at,
        model_loaded,
        model_mismatch,
    })
}

async fn is_model_ready(
    state: &AppState,
    tenant_id: &str,
    model_id: &str,
) -> Result<Option<bool>, (StatusCode, Json<ErrorResponse>)> {
    is_model_ready_internal(state, tenant_id, model_id)
        .await
        .map_err(|e| {
            ApiError::internal("Failed to read model status")
                .with_details(e.to_string())
                .into()
        })
}

async fn is_model_ready_internal(
    state: &AppState,
    tenant_id: &str,
    model_id: &str,
) -> Result<Option<bool>, AosError> {
    let status = state
        .db
        .get_base_model_status_for_model(tenant_id, model_id)
        .await?;

    Ok(status.map(|s| ModelLoadStatus::parse_status(&s.status).is_ready()))
}

/// Reconcile active workspace state against worker/model status.
///
/// If an active model is recorded but the worker reports nothing loaded or the
/// model status is not ready, mark the base model status as an error so the
/// mismatch surfaces in readiness probes.
///
/// # Warning
/// This function iterates ALL workspaces globally. Use only for internal
/// background reconciliation tasks. For tenant-specific reconciliation,
/// use [`reconcile_active_models_for_tenant`] instead.
pub async fn reconcile_active_models(state: &AppState) {
    let active_states = match state.db.list_workspace_active_states().await {
        Ok(states) => states,
        Err(e) => {
            error!(
                error = %e,
                "Failed to load active workspace state for reconciliation"
            );
            return;
        }
    };

    if active_states.is_empty() {
        return;
    }

    for record in active_states {
        let worker_loaded = worker_reports_loaded(
            state,
            Some(&record.tenant_id),
            record.active_base_model_id.as_deref(),
        )
        .await;
        reconcile_single_workspace(state, &record, worker_loaded).await;
    }
}

/// Reconcile active workspace state for a specific tenant.
///
/// This is the tenant-scoped version that should be used for on-demand
/// reconciliation to maintain workspace isolation.
pub async fn reconcile_active_models_for_tenant(state: &AppState, tenant_id: &str) {
    // Fetch all active states and filter by tenant in memory
    // (tenant-scoped query not available at DB layer)
    let all_states = match state.db.list_workspace_active_states().await {
        Ok(states) => states,
        Err(e) => {
            error!(
                error = %e,
                tenant_id = %tenant_id,
                "Failed to load active workspace state for tenant reconciliation"
            );
            return;
        }
    };

    // Filter to only the tenant we care about
    let active_states: Vec<_> = all_states
        .into_iter()
        .filter(|s| s.tenant_id == tenant_id)
        .collect();

    if active_states.is_empty() {
        return;
    }

    for record in active_states {
        let worker_loaded = worker_reports_loaded(
            state,
            Some(&record.tenant_id),
            record.active_base_model_id.as_deref(),
        )
        .await;
        reconcile_single_workspace(state, &record, worker_loaded).await;
    }
}

/// Internal helper to reconcile a single workspace record.
async fn reconcile_single_workspace(
    state: &AppState,
    record: &adapteros_db::workspace_active_state::WorkspaceActiveState,
    worker_loaded: bool,
) {
    let Some(model_id) = record.active_base_model_id.as_deref() else {
        return;
    };

    match is_model_ready_internal(state, &record.tenant_id, model_id).await {
        Ok(Some(true)) if worker_loaded => {
            // Active and ready
        }
        Ok(_) => {
            let message = if !worker_loaded {
                "Active model not loaded on worker"
            } else {
                "Active model marked active but not ready"
            };

            let reducer = ModelWorkerLifecycleReducer::from_env(state.db.clone());
            if let Err(e) = reducer
                .reduce(ModelWorkerLifecycleEvent::ModelSwitchResult {
                    tenant_id: record.tenant_id.clone(),
                    worker_id: None,
                    from_model_id: None,
                    to_model_id: Some(model_id.to_string()),
                    to_model_hash_b3: None,
                    success: false,
                    error: Some(message.to_string()),
                    memory_usage_mb: None,
                    reason: "workspace active model reconciliation mismatch".to_string(),
                })
                .await
            {
                error!(
                    error = %e,
                    tenant_id = %record.tenant_id,
                    model_id = %model_id,
                    "Failed to mark active model mismatch"
                );
            }
        }
        Err(e) => {
            error!(
                error = %e,
                tenant_id = %record.tenant_id,
                model_id = %model_id,
                "Failed to reconcile active model status"
            );
        }
    }
}

async fn worker_reports_loaded(
    state: &AppState,
    tenant_id: Option<&str>,
    expected_model_id: Option<&str>,
) -> bool {
    let uds_paths = resolve_worker_socket_paths(state, tenant_id).await;
    if uds_paths.is_empty() {
        return false;
    }

    let client = UdsClient::new(Duration::from_secs(5));
    for uds_path in uds_paths {
        match client.get_model_status(&uds_path).await {
            Ok(status) => {
                let loaded = status
                    .get("status")
                    .and_then(|v| v.as_str())
                    .is_some_and(|v| v.eq_ignore_ascii_case("loaded"));
                if !loaded {
                    continue;
                }

                let active_model_id = status.get("active_model_id").and_then(|v| v.as_str());
                if expected_model_id.is_none() || active_model_id == expected_model_id {
                    return true;
                }
            }
            Err(e) => {
                info!(
                    error = %e,
                    path = %uds_path.display(),
                    "Worker status probe failed during reconciliation"
                );
            }
        }
    }

    false
}

async fn resolve_worker_socket_paths(state: &AppState, tenant_id: Option<&str>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    if let Ok(workers) = state.db.list_all_workers().await {
        paths.extend(
            workers
                .into_iter()
                .filter(|worker| {
                    tenant_id
                        .map(|tid| worker.tenant_id.as_str() == tid)
                        .unwrap_or(true)
                })
                .map(|worker| PathBuf::from(worker.uds_path)),
        );
    }

    if paths.is_empty() {
        match resolve_worker_socket_for_cp() {
            Ok(resolved) => {
                if resolved.path.exists() {
                    paths.push(resolved.path);
                } else {
                    info!(
                        path = %resolved.path.display(),
                        source = %resolved.source,
                        "Resolved worker socket path does not exist during reconciliation"
                    );
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to resolve worker socket for reconciliation");
            }
        }
    }

    paths
}

// NOTE: The previous `ensure_workspace_access` function was removed because it used a weak check
// (workspace_id == claims.tenant_id) that conflated tenant ownership with workspace membership.
// Workspaces are cross-tenant by design, so all access checks must use the database-backed
// `check_workspace_access` function which validates membership via the workspace_members table.

fn not_found_response(entity: &str, id: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(
            ErrorResponse::new(format!("{entity} not found"))
                .with_code("NOT_FOUND")
                .with_string_details(format!("{entity} '{id}' does not exist")),
        ),
    )
}
