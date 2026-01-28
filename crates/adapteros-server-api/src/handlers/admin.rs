//! Admin handlers
//!
//! Provides REST endpoints for admin-only operations like user management.
//!
//! 【2025-11-30†feature†admin_users_endpoint】

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::middleware::require_role;
use crate::state::AppState;
use crate::types::*;
pub use adapteros_api_types::admin::{ListUsersParams, ListUsersResponse, UserResponse};
use adapteros_db::users::{Role, User};
use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::Json,
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
) -> Result<Json<ListUsersResponse>, (StatusCode, Json<ErrorResponse>)> {
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

// Re-export admin handlers from parent module for routes.rs
pub use super::{__path_query_audit_logs, query_audit_logs};
