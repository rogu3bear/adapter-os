//! Authentication handlers
//!
//! Provides login, logout, and user information endpoints.
//!
//! 【2025-01-20†rectification†auth_handlers_expanded】

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
use axum::{extract::State, http::StatusCode, response::Json, Extension};
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
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    use crate::auth::{generate_token_ed25519, verify_password};
    use crate::audit_helper;
    use tracing::{error, info};

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

    // 3. Generate JWT access token using Ed25519
    let token = match generate_token_ed25519(
        &user.id,
        &user.email,
        &user.role,
        &user.tenant_id,
        &state.ed25519_keypair,
    ) {
        Ok(token) => token,
        Err(e) => {
            error!(error = %e, "Failed to generate JWT token");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal server error".to_string())),
            ));
        }
    };

    // 4. Audit log successful login
    // Create full claims for audit logging
    let claims = Claims {
        sub: user.id.clone(),
        email: user.email.clone(),
        role: user.role.clone(),
        roles: vec![user.role.clone()],
        tenant_id: user.tenant_id.clone(),
        exp: chrono::Utc::now().timestamp() + 8 * 3600, // 8 hours
        iat: chrono::Utc::now().timestamp(),
        jti: format!("{}", uuid::Uuid::now_v7()),
        nbf: chrono::Utc::now().timestamp(),
    };

    if let Err(e) = audit_helper::log_success(
        &state.db,
        &claims,
        "auth.login",
        "user",
        Some(&user.id),
    )
    .await
    {
        error!(error = %e, "Failed to log audit event");
        // Don't fail the login if audit logging fails
    }

    info!(
        user_id = %user.id,
        email = %user.email,
        role = %user.role,
        tenant_id = %user.tenant_id,
        "User logged in successfully"
    );

    // 5. Return tokens in the response
    Ok(Json(LoginResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        token,
        user_id: user.id,
        tenant_id: user.tenant_id,
        role: user.role,
        expires_in: 8 * 3600, // 8 hours in seconds
    }))
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
