//! Authentication handlers
//!
//! Provides login, logout, and user information endpoints.
//!
//! 【2025-01-20†rectification†auth_handlers_expanded】

use axum::{http::StatusCode, response::Json, Json as AxumJson, Extension};
use adapteros_db::Db;
use crate::auth::Claims;
use crate::types::*;
use tracing::warn;

pub async fn login_handler(
    AxumJson(payload): AxumJson<LoginRequest>,
    db: Db,
) -> Result<Json<LoginResponse>, StatusCode> {
    // Check if users table is empty
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if user_count == 0 {
        warn!("No users seeded in DB; bootstrap required");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Existing login logic...
    let user = sqlx::query_as!(User, "SELECT * FROM users WHERE username = $1", payload.username)
        .fetch_optional(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // ... rest of validation and JWT generation ...
    Ok(Json(response))
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
        user_id: claims.sub,
        email: claims.email.unwrap_or_default(),
        role: claims.role,
        tenant_id: claims.tenant_id,
        created_at: None,
        last_login_at: None,
    }))
}

