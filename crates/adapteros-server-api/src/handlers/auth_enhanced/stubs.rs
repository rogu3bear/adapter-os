//! Stub handlers for auth endpoints that are not implemented yet.
//!
//! These endpoints return NOT_IMPLEMENTED with a consistent error payload.

use axum::{http::StatusCode, Json};

use crate::types::ErrorResponse;

fn stub_not_implemented_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(
            ErrorResponse::new(
                "Endpoint not implemented in this build.",
            )
            .with_code("NOT_IMPLEMENTED"),
        ),
    )
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
    stub_not_implemented_error()
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
    stub_not_implemented_error()
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
    stub_not_implemented_error()
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
    stub_not_implemented_error()
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
    stub_not_implemented_error()
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
    stub_not_implemented_error()
}
