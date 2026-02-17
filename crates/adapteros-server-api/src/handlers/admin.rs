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
    AdminConfigResponse, AdminStatusResponse, CreateUserRequest, ListUsersParams,
    ListUsersResponse, UpdateUserRequest, UserResponse,
};
use adapteros_db::users::{Role, User};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::atomic::Ordering;

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

fn admin_lifecycle(state: &AppState) -> String {
    state
        .boot_state
        .as_ref()
        .map(|boot| boot.current_state().as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Get high-level admin status
#[utoipa::path(
    tag = "admin",
    get,
    path = "/v1/admin/status",
    responses(
        (status = 200, description = "Admin status summary", body = AdminStatusResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required")
    )
)]
pub async fn admin_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<AdminStatusResponse> {
    require_role(&claims, Role::Admin)?;

    let lifecycle = admin_lifecycle(&state);
    let runtime_mode = state
        .runtime_mode
        .map(|mode| mode.as_str().to_string())
        .unwrap_or_else(|| "dev".to_string());
    let maintenance_mode = state
        .boot_state
        .as_ref()
        .map(|boot| boot.is_maintenance())
        .unwrap_or(false);
    let draining_mode = state
        .boot_state
        .as_ref()
        .map(|boot| boot.is_draining())
        .unwrap_or(false);

    let status = match lifecycle.as_str() {
        "failed" => "error",
        "degraded" => "degraded",
        "maintenance" => "maintenance",
        "draining" => "draining",
        _ => "ok",
    }
    .to_string();

    let rag_enabled = matches!(
        state.rag_status.as_ref(),
        Some(crate::state::RagStatus::Enabled { .. })
    );

    Ok(Json(AdminStatusResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        runtime_mode,
        lifecycle,
        strict_mode: state.strict_mode,
        maintenance_mode,
        draining_mode,
        in_flight_requests: state.in_flight_requests.load(Ordering::Relaxed) as u64,
        registered_workers: state.worker_runtime.len(),
        rag_enabled,
    }))
}

/// Get sanitized admin configuration summary
#[utoipa::path(
    tag = "admin",
    get,
    path = "/v1/admin/config",
    responses(
        (status = 200, description = "Sanitized admin configuration", body = AdminConfigResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin role required"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn admin_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<AdminConfigResponse> {
    require_role(&claims, Role::Admin)?;

    let config = state
        .config
        .read()
        .map_err(|_| ApiError::internal("Configuration lock poisoned"))?;

    let review_webhook_configured = config
        .server
        .review_webhook_url
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    Ok(Json(AdminConfigResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        environment: config
            .general
            .as_ref()
            .and_then(|general| general.environment.clone()),
        production_mode: config.server.production_mode,
        dev_bypass_enabled: config.security.dev_bypass,
        require_mfa: config.security.require_mfa.unwrap_or(false),
        allow_registration: config.security.allow_registration.unwrap_or(false),
        jwt_mode: config.security.jwt_mode.clone(),
        token_ttl_seconds: config.security.token_ttl_seconds,
        access_token_ttl_seconds: config.security.access_token_ttl_seconds,
        session_ttl_seconds: config.security.session_ttl_seconds,
        ssrf_protection: config.server.ssrf_protection,
        metrics_enabled: config.metrics.enabled,
        review_webhook_configured,
        max_adapters: config.performance.max_adapters,
        max_workers: config.performance.max_workers,
        concurrent_requests: config.capacity_limits.concurrent_requests,
        max_concurrent_training_jobs: config.capacity_limits.max_concurrent_training_jobs,
        worker_heartbeat_interval_secs: config.server.worker_heartbeat_interval_secs,
        streaming_heartbeat_interval_secs: config.streaming.inference_heartbeat_interval_secs,
        streaming_idle_timeout_secs: config.streaming.inference_idle_timeout_secs,
        self_hosting_mode: config.self_hosting.mode.clone(),
        self_hosting_repo_allowlist_count: config.self_hosting.repo_allowlist.len(),
    }))
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
