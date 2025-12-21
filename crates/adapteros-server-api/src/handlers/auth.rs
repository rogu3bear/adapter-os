//! Authentication handlers
//!
//! Provides login, logout, and user information endpoints.
//!
//! 【2025-01-20†rectification†auth_handlers_expanded】

use crate::auth::{dev_no_auth_enabled, Claims};
use crate::auth_common::{build_user_info, AuthContext};
use crate::ip_extraction::ClientIp;
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    Extension,
};
use chrono::Utc;
use tracing::error;
use utoipa;

/// Login endpoint
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn auth_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(client_ip): Extension<ClientIp>,
    Json(request): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), (StatusCode, Json<ErrorResponse>)> {
    crate::handlers::auth_enhanced::login_handler(
        State(state),
        headers,
        Extension(client_ip),
        Json(request),
    )
    .await
}

/// Get current user info
#[utoipa::path(
    get,
    path = "/v1/auth/me",
    responses(
        (status = 200, description = "Current user info", body = UserInfoResponse),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn auth_me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<UserInfoResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Dev bypass: return synthetic admin claims without DB lookup
    if dev_no_auth_enabled() && claims.admin_tenants.contains(&"*".to_string()) {
        let now = Utc::now().to_rfc3339();
        let email = if claims.email.is_empty() {
            format!("{}@adapteros.local", claims.sub)
        } else {
            claims.email.clone()
        };

        return Ok(Json(UserInfoResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            user_id: claims.sub.clone(),
            email: email.clone(),
            role: claims.role.clone(),
            created_at: now.clone(),
            tenant_id: claims.tenant_id.clone(),
            display_name: email,
            permissions: vec![],
            admin_tenants: claims.admin_tenants.clone(),
            last_login_at: None,
            mfa_enabled: Some(false),
            token_last_rotated_at: None,
        }));
    }

    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            error!(error = %e, user_id = %claims.sub, "Failed to load user for auth_me");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal server error".to_string())),
            )
        })?
        .ok_or_else(|| {
            // User was authenticated via JWT but no longer exists in database
            // This is an auth issue (401), not a server error (500)
            tracing::warn!(user_id = %claims.sub, "Authenticated user no longer exists in database");
            (
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("User not found".to_string())
                        .with_code("USER_NOT_FOUND")
                        .with_string_details("Authenticated user no longer exists. Please log in again.".to_string()),
                ),
            )
        })?;

    let ctx = AuthContext::from_user(user).map_err(|err| {
        error!(error = %err, "Failed to build auth context for auth_me");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal server error".to_string())),
        )
    })?;

    let mut user_info = build_user_info(&ctx);
    user_info.admin_tenants = claims.admin_tenants.clone();

    Ok(Json(user_info))
}
