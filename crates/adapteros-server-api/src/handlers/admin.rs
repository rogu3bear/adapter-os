//! Admin handlers
//!
//! Provides REST endpoints for admin-only operations like user management.
//!
//! 【2025-11-30†feature†admin_users_endpoint】

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::middleware::require_role;
use crate::state::AppState;
use crate::types::*;
pub use adapteros_api_types::admin::{
    CreateUserRequest, ListUsersParams, ListUsersResponse, UpdateUserRequest, UserResponse,
};
use adapteros_db::users::{Role, User};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};

/// Convert a database User to an API UserResponse
fn user_to_response(user: User) -> UserResponse {
    UserResponse {
        user_id: user.id.clone(),
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        role: user.role,
        tenant_id: user.tenant_id,
        created_at: user.created_at,
        last_login_at: None,
        mfa_enabled: Some(user.mfa_enabled),
        permissions: None, // Requires role-based computation
    }
}

/// List users with pagination and filtering
///
/// Requires Admin role. Supports filtering by role and tenant_id.
#[utoipa::path(
    tag = "admin",
    get,
    path = "/v1/admin/users",
    params(
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("page_size" = Option<i64>, Query, description = "Page size (default: 100)"),
        ("role" = Option<String>, Query, description = "Filter by role"),
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID")
    ),
    responses(
        (status = 200, description = "List of users", body = ListUsersResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListUsersParams>,
) -> ApiResult<ListUsersResponse> {
    // Require Admin role
    require_role(&claims, Role::Admin)?;

    let (users, total) = state
        .db
        .list_users(
            params.page,
            params.page_size,
            params.role.as_deref(),
            params.tenant_id.as_deref(),
        )
        .await
        .map_err(ApiError::db_error)?;

    let user_responses: Vec<UserResponse> = users.into_iter().map(user_to_response).collect();

    Ok(Json(ListUsersResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        users: user_responses,
        total,
        page: params.page,
        page_size: params.page_size,
    }))
}

/// Get a specific user by ID
#[utoipa::path(
    tag = "admin",
    get,
    path = "/v1/admin/users/{user_id}",
    params(
        ("user_id" = String, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User details", body = UserResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_user(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(user_id): Path<String>,
) -> ApiResult<UserResponse> {
    require_role(&claims, Role::Admin)?;

    let user_id = crate::id_resolver::resolve_any_id(&state.db, &user_id).await?;
    let user = state
        .db
        .get_user(&user_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("User"))?;

    Ok(Json(user_to_response(user)))
}

/// Create a new user
#[utoipa::path(
    tag = "admin",
    post,
    path = "/v1/admin/users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = UserResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_user(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), ApiError> {
    require_role(&claims, Role::Admin)?;

    let role: Role = req
        .role
        .parse()
        .map_err(|_| ApiError::bad_request(format!("invalid role: {}", req.role)))?;

    let pw_hash = crate::auth::hash_password(&req.password)
        .map_err(|e| ApiError::internal(format!("password hashing failed: {}", e)))?;

    let user_id = state
        .db
        .create_user(
            &req.email,
            &req.display_name,
            &pw_hash,
            role,
            &req.tenant_id,
        )
        .await
        .map_err(ApiError::db_error)?;

    let user = state
        .db
        .get_user(&user_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::internal("user created but not found"))?;

    Ok((StatusCode::CREATED, Json(user_to_response(user))))
}

/// Update a user
#[utoipa::path(
    tag = "admin",
    put,
    path = "/v1/admin/users/{user_id}",
    params(
        ("user_id" = String, Path, description = "User ID")
    ),
    request_body = UpdateUserRequest,
    responses(
        (status = 200, description = "User updated", body = UserResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_user(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> ApiResult<UserResponse> {
    require_role(&claims, Role::Admin)?;

    let user_id = crate::id_resolver::resolve_any_id(&state.db, &user_id).await?;

    // Verify user exists
    state
        .db
        .get_user(&user_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("User"))?;

    if let Some(role_str) = &req.role {
        let role: Role = role_str
            .parse()
            .map_err(|_| ApiError::bad_request(format!("invalid role: {}", role_str)))?;
        state
            .db
            .update_user_role(&user_id, role)
            .await
            .map_err(ApiError::db_error)?;
    }

    if let Some(disabled) = req.disabled {
        state
            .db
            .update_user_disabled(&user_id, disabled)
            .await
            .map_err(ApiError::db_error)?;
    }

    // Note: display_name update is not yet supported by the DB layer.
    // If needed, a DB method should be added.

    let user = state
        .db
        .get_user(&user_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("User"))?;

    Ok(Json(user_to_response(user)))
}

/// Delete a user
#[utoipa::path(
    tag = "admin",
    delete,
    path = "/v1/admin/users/{user_id}",
    params(
        ("user_id" = String, Path, description = "User ID")
    ),
    responses(
        (status = 204, description = "User deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_user(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_role(&claims, Role::Admin)?;

    let user_id = crate::id_resolver::resolve_any_id(&state.db, &user_id).await?;

    // Verify user exists
    state
        .db
        .get_user(&user_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("User"))?;

    state
        .db
        .delete_user(&user_id)
        .await
        .map_err(ApiError::db_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// Re-export admin handlers from parent module for routes.rs
pub use super::{__path_query_audit_logs, query_audit_logs};
