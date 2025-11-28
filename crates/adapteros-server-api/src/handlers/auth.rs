//! Authentication handlers
//!
//! Provides login, logout, and user information endpoints.
//!
//! 【2025-01-20†rectification†auth_handlers_expanded】

use crate::audit_helper;
use crate::auth::{verify_password, Claims};
use crate::auth_common::{
    attach_auth_cookie, build_auth_token, build_user_info, AuthConfig, AuthContext,
};
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    Extension,
};
use tracing::{error, info};
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
    Json(request): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), (StatusCode, Json<ErrorResponse>)> {
    // 1. Look up user by email
    let user = match state.db.get_user_by_email(&request.email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            error!(email = %request.email, "User not found");
            // Log failed login attempt (create minimal claims for audit)
            // We don't have a user, so we can't audit properly - skip audit log
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("Invalid credentials".to_string())),
            ));
        }
        Err(e) => {
            error!(error = %e, "Database error during login");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal server error".to_string())),
            ));
        }
    };

    // Check if user is disabled
    if user.disabled {
        error!(email = %request.email, user_id = %user.id, "Login attempt for disabled user");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Account disabled".to_string())),
        ));
    }

    // 2. Verify password using Argon2id
    let password_valid = match verify_password(&request.password, &user.pw_hash) {
        Ok(valid) => valid,
        Err(e) => {
            error!(error = %e, "Password verification error");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal server error".to_string())),
            ));
        }
    };

    if !password_valid {
        error!(email = %request.email, user_id = %user.id, "Invalid password");
        // Audit log failed login (we know the user exists but password is wrong)
        // Create minimal claims for audit logging
        let audit_claims = Claims {
            sub: user.id.clone(),
            email: user.email.clone(),
            role: user.role.clone(),
            roles: vec![user.role.clone()],
            tenant_id: user.tenant_id.clone(),
            admin_tenants: vec![],
            exp: 0,
            iat: 0,
            jti: String::new(),
            nbf: 0,
        };
        let _ = audit_helper::log_action(
            &state.db,
            &audit_claims,
            "auth.login",
            "user",
            Some(&user.id),
            "failure",
            Some("Invalid password"),
        )
        .await;

        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Invalid credentials".to_string())),
        ));
    }

    let auth_cfg = AuthConfig::from_state(&state);
    let ctx = AuthContext::from_user(user).map_err(|err| {
        error!(error = %err, "Failed to build auth context");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal server error".to_string())),
        )
    })?;

    let token = build_auth_token(&ctx, &auth_cfg).map_err(|err| {
        error!(error = %err, "Failed to generate auth token");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal server error".to_string())),
        )
    })?;

    let mut headers = HeaderMap::new();
    attach_auth_cookie(&mut headers, &token, &auth_cfg).map_err(|err| {
        error!(error = %err, "Failed to attach auth cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal server error".to_string())),
        )
    })?;

    // 4. Audit log successful login
    // Create full claims for audit logging
    let claims = Claims {
        sub: ctx.user.id.clone(),
        email: ctx.user.email.clone(),
        role: ctx.role.to_string(),
        roles: vec![ctx.role.to_string()],
        tenant_id: ctx.tenant_id.clone(),
        admin_tenants: vec![],
        exp: chrono::Utc::now().timestamp() + auth_cfg.effective_ttl() as i64,
        iat: chrono::Utc::now().timestamp(),
        jti: format!("{}", uuid::Uuid::now_v7()),
        nbf: chrono::Utc::now().timestamp(),
    };

    if let Err(e) =
        audit_helper::log_success(&state.db, &claims, "auth.login", "user", Some(&ctx.user.id))
            .await
    {
        error!(error = %e, "Failed to log audit event");
        // Don't fail the login if audit logging fails
    }

    info!(
        user_id = %ctx.user.id,
        email = %ctx.user.email,
        role = %ctx.role,
        tenant_id = %ctx.user.tenant_id,
        "User logged in successfully"
    );

    Ok((
        headers,
        Json(LoginResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            token,
            user_id: ctx.user.id.clone(),
            tenant_id: ctx.user.tenant_id.clone(),
            role: ctx.role.to_string(),
            expires_in: auth_cfg.effective_ttl(),
        }),
    ))
}

/// Logout endpoint (client-side token discard)
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    responses(
        (status = 204, description = "Logged out successfully"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn auth_logout(
    Extension(_claims): Extension<Claims>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // With stateless JWT, logout is client-side (discard token)
    // Server doesn't need to track anything
    Ok(StatusCode::NO_CONTENT)
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

    Ok(Json(build_user_info(&ctx)))
}
