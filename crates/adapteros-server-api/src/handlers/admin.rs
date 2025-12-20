//! Admin handlers
//!
//! Provides REST endpoints for admin-only operations like user management.
//!
//! 【2025-11-30†feature†admin_users_endpoint】

use crate::auth::Claims;
use crate::error_helpers::db_error_with_details;
use crate::middleware::require_role;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::{Role, User};
use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListUsersParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    pub role: Option<String>,
    pub tenant_id: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_page_size() -> i64 {
    100
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListUsersResponse {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    pub users: Vec<UserResponse>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserResponse {
    pub user_id: String,
    pub id: String, // Alias for user_id
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub tenant_id: String,
    pub created_at: String,
    pub last_login_at: Option<String>,
    pub mfa_enabled: Option<bool>,
    pub permissions: Option<Vec<String>>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        UserResponse {
            user_id: user.id.clone(),
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            role: user.role,
            tenant_id: user.tenant_id,
            created_at: user.created_at,
            last_login_at: None, // TODO: Add last_login_at column to users table
            mfa_enabled: Some(user.mfa_enabled),
            permissions: None, // Requires role-based computation
        }
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
        .map_err(db_error_with_details)?;

    let user_responses: Vec<UserResponse> = users.into_iter().map(|u| u.into()).collect();

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
