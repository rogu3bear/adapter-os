//! Stub handlers for disabled auth endpoints
//!
//! These endpoints are disabled when running in dev-bypass-only mode.
//! They return a helpful error message directing users to use dev bypass.
//! Exception: list_user_tenants_handler provides real data for dev bypass mode.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Deserialize;

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{TenantListResponse, TenantSummary};
use adapteros_api_types::API_SCHEMA_VERSION;

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
// Tenant handlers (working in dev bypass mode)
// ============================================================================

/// List tenants accessible to the current user
///
/// In dev bypass mode, this returns the user's tenant (from claims) and any
/// tenants they have been granted access to. For admins with wildcard access,
/// it returns all tenants.
#[utoipa::path(
    get,
    path = "/v1/auth/tenants",
    responses(
        (status = 200, description = "List of accessible tenants", body = TenantListResponse),
        (status = 500, description = "Database error")
    ),
    tag = "auth"
)]
pub async fn list_user_tenants_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<TenantListResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if user has admin wildcard access to all tenants
    let has_wildcard = claims.admin_tenants.iter().any(|t| t == "*");

    let tenants = if has_wildcard {
        // Admin with wildcard: return all tenants
        let (db_tenants, _total) = state.db.list_tenants_paginated(100, 0).await.map_err(|e| {
            tracing::error!(error = %e, "Failed to list tenants for admin");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list tenants")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        db_tenants
            .into_iter()
            .map(|t| TenantSummary {
                schema_version: API_SCHEMA_VERSION.to_string(),
                id: t.id,
                name: t.name,
                status: t.status,
                created_at: Some(t.created_at),
            })
            .collect()
    } else {
        // Regular user: return their primary tenant + any granted access
        let mut tenant_summaries = Vec::new();

        // Add the user's primary tenant
        if let Ok(Some(primary)) = state.db.get_tenant(&claims.tenant_id).await {
            tenant_summaries.push(TenantSummary {
                schema_version: API_SCHEMA_VERSION.to_string(),
                id: primary.id,
                name: primary.name,
                status: primary.status,
                created_at: Some(primary.created_at),
            });
        }

        // Add any explicitly granted tenants from admin_tenants
        for tenant_id in &claims.admin_tenants {
            if tenant_id != &claims.tenant_id {
                if let Ok(Some(tenant)) = state.db.get_tenant(tenant_id).await {
                    tenant_summaries.push(TenantSummary {
                        schema_version: API_SCHEMA_VERSION.to_string(),
                        id: tenant.id,
                        name: tenant.name,
                        status: tenant.status,
                        created_at: Some(tenant.created_at),
                    });
                }
            }
        }

        tenant_summaries
    };

    Ok(Json(TenantListResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        tenants,
    }))
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
