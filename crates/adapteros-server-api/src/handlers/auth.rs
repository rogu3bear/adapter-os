//! Authentication handlers
//!
//! Provides login, logout, and user information endpoints.
//!
//! 【2025-01-20†rectification†auth_handlers_expanded】

use axum::{extract::State, http::StatusCode, response::Json, Extension};
use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
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
    State(_state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub implementation: always fail with unauthorized
    // Real implementation would validate credentials and generate JWT
    Err((
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse::new("Invalid credentials".to_string())),
    ))
}

/// Logout endpoint (client-side token discard)
pub async fn auth_logout(
    Extension(_claims): Extension<Claims>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // With stateless JWT, logout is client-side (discard token)
    // Server doesn't need to track anything
    Ok(StatusCode::NO_CONTENT)
}

/// Get current user info
pub async fn auth_me(
    Extension(claims): Extension<Claims>,
) -> Result<Json<UserInfoResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(UserInfoResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        user_id: claims.sub,
        email: claims.email,
        role: claims.role,
        created_at: String::new(),
    }))
}

