//! Stub handlers for disabled auth endpoints
//!
//! These endpoints are disabled when running in dev-bypass-only mode.
//! They return a helpful error message directing users to use dev bypass.

use axum::{http::StatusCode, Json};
use serde::Deserialize;

use crate::types::ErrorResponse;

fn dev_bypass_only_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(
            ErrorResponse::new(
                "Authentication is disabled. Server is running in dev-bypass mode. \
                 Use AOS_DEV_NO_AUTH=1 to authenticate automatically.",
            )
            .with_code("AUTH_DISABLED"),
        ),
    )
}

// ============================================================================
// Login stubs
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    #[allow(dead_code)]
    pub email: String,
    #[allow(dead_code)]
    pub password: String,
}

#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn login_handler(Json(_req): Json<LoginRequest>) -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

// ============================================================================
// Bootstrap stub
// ============================================================================

#[utoipa::path(
    post,
    path = "/v1/auth/bootstrap",
    request_body = super::types::BootstrapRequest,
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn bootstrap_admin_handler(
    Json(_req): Json<super::types::BootstrapRequest>,
) -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

// ============================================================================
// Logout stub
// ============================================================================

#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn logout_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

// ============================================================================
// Refresh stub
// ============================================================================

#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn refresh_token_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

// ============================================================================
// MFA stubs
// ============================================================================

#[utoipa::path(
    get,
    path = "/v1/auth/mfa/status",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn mfa_status_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

#[utoipa::path(
    post,
    path = "/v1/auth/mfa/start",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn mfa_start_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

#[utoipa::path(
    post,
    path = "/v1/auth/mfa/verify",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn mfa_verify_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

#[utoipa::path(
    post,
    path = "/v1/auth/mfa/disable",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn mfa_disable_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

// ============================================================================
// Session stubs
// ============================================================================

#[utoipa::path(
    get,
    path = "/v1/auth/sessions",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn list_sessions_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

#[utoipa::path(
    delete,
    path = "/v1/auth/sessions/{session_id}",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn revoke_session_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

// ============================================================================
// Tenant stubs
// ============================================================================

#[utoipa::path(
    get,
    path = "/v1/auth/tenants",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn list_user_tenants_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}

#[utoipa::path(
    post,
    path = "/v1/auth/tenants/switch",
    responses(
        (status = 501, description = "Auth disabled - use dev bypass")
    ),
    tag = "auth"
)]
pub async fn switch_tenant_handler() -> (StatusCode, Json<ErrorResponse>) {
    dev_bypass_only_error()
}
