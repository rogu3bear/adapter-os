///! Enhanced authentication handlers with comprehensive security
///!
///! Endpoints:
///! - POST /v1/auth/login - Login with email/password
///! - POST /v1/auth/logout - Logout and revoke token
///! - POST /v1/auth/refresh - Refresh JWT token
///! - GET /v1/auth/sessions - List active sessions
///! - DELETE /v1/auth/sessions/:jti - Revoke specific session
///! - POST /v1/auth/bootstrap - Create initial admin user (one-time)
use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::{generate_token_ed25519, hash_password, refresh_token, verify_password, Claims};
use crate::ip_extraction::ClientIp;
use crate::security::{
    create_session, get_user_sessions, is_account_locked, revoke_all_user_tokens, revoke_token,
    track_auth_attempt,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{LoginRequest, LoginResponse, UserInfoResponse};
use adapteros_db::{users::Role, Db};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct BootstrapRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BootstrapResponse {
    pub user_id: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RefreshResponse {
    pub token: String,
    pub expires_at: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionInfo {
    pub jti: String,
    pub created_at: String,
    pub ip_address: Option<String>,
    pub last_activity: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionInfo>,
}

/// Bootstrap initial admin user (one-time operation)
///
/// Can only be called when no users exist in the database.
/// Creates a single admin user for initial system access.
#[utoipa::path(
    post,
    path = "/v1/auth/bootstrap",
    request_body = BootstrapRequest,
    responses(
        (status = 200, description = "Admin user created", body = BootstrapResponse),
        (status = 403, description = "Bootstrap not allowed"),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn bootstrap_admin_handler(
    State(state): State<AppState>,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<BootstrapRequest>,
) -> Result<Json<BootstrapResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if any users exist
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to query user count");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
            )
        })?;

    if user_count > 0 {
        warn!("Bootstrap attempt when users already exist");
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("bootstrap not allowed")
                    .with_code("BOOTSTRAP_FORBIDDEN")
                    .with_string_details("users already exist, bootstrap is disabled"),
            ),
        ));
    }

    // Validate password strength
    if req.password.len() < 12 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("weak password")
                    .with_code("WEAK_PASSWORD")
                    .with_string_details("password must be at least 12 characters"),
            ),
        ));
    }

    // Hash password
    let pw_hash = hash_password(&req.password).map_err(|e| {
        warn!(error = %e, "Failed to hash password");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("password hashing failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Create admin user with "system" tenant
    let user_id = state
        .db
        .create_user(&req.email, &req.display_name, &pw_hash, Role::Admin, "system")
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to create admin user");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("user creation failed").with_code("DATABASE_ERROR")),
            )
        })?;

    // Log to audit (no user context yet, so use system)
    state
        .db
        .log_audit(
            &user_id,
            "admin",
            "system",
            "user.bootstrap",
            "user",
            Some(&user_id),
            "success",
            None,
            Some(&client_ip.0),
            None,
        )
        .await
        .ok();

    info!(
        user_id = %user_id,
        email = %req.email,
        ip = %client_ip.0,
        "Bootstrap admin created"
    );

    Ok(Json(BootstrapResponse {
        user_id,
        message: "Bootstrap admin created successfully".to_string(),
    }))
}

/// Login handler with comprehensive security checks
pub async fn login_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Extract user agent from headers for session tracking
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Check account lockout
    let is_locked = is_account_locked(&state.db, &req.email, 15)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to check account lockout");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

    if is_locked {
        track_auth_attempt(
            &state.db,
            &req.email,
            &client_ip.0,
            false,
            Some("account locked"),
        )
        .await
        .ok();

        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("account locked")
                    .with_code("ACCOUNT_LOCKED")
                    .with_string_details("too many failed attempts, try again later"),
            ),
        ));
    }

    // Get user by email
    let user = state.db.get_user_by_email(&req.email).await.map_err(|e| {
        warn!(error = %e, "Database error during login");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
        )
    })?;

    let user = match user {
        Some(u) if !u.disabled => u,
        Some(_) => {
            track_auth_attempt(
                &state.db,
                &req.email,
                &client_ip.0,
                false,
                Some("account disabled"),
            )
            .await
            .ok();

            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("account disabled")
                        .with_code("ACCOUNT_DISABLED")
                        .with_string_details("this account has been disabled"),
                ),
            ));
        }
        None => {
            track_auth_attempt(
                &state.db,
                &req.email,
                &client_ip.0,
                false,
                Some("user not found"),
            )
            .await
            .ok();

            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("invalid credentials")
                        .with_code("INVALID_CREDENTIALS")
                        .with_string_details("email or password is incorrect"),
                ),
            ));
        }
    };

    // Verify password
    let valid = verify_password(&req.password, &user.pw_hash).map_err(|e| {
        warn!(error = %e, "Password verification error");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    if !valid {
        track_auth_attempt(
            &state.db,
            &req.email,
            &client_ip.0,
            false,
            Some("invalid password"),
        )
        .await
        .ok();

        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("invalid credentials")
                    .with_code("INVALID_CREDENTIALS")
                    .with_string_details("email or password is incorrect"),
            ),
        ));
    }

    // Determine tenant_id (use user's tenant or "system" for admin)
    let tenant_id = if user.role == "admin" {
        "system".to_string()
    } else {
        // For now, default to "default" tenant. In production, get from user record.
        "default".to_string()
    };

    // Generate JWT token
    let token = if state.use_ed25519 {
        generate_token_ed25519(
            &user.id,
            &user.email,
            &user.role,
            &tenant_id,
            &state.ed25519_keypair,
        )
        .map_err(|e| {
            warn!(error = %e, "Failed to generate token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        })?
    } else {
        crate::auth::generate_token(
            &user.id,
            &user.email,
            &user.role,
            &tenant_id,
            &state.jwt_secret,
        )
        .map_err(|e| {
            warn!(error = %e, "Failed to generate token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        })?
    };

    // Decode to get jti and exp
    let claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(&token, &state.ed25519_public_key)
    } else {
        crate::auth::validate_token(&token, &state.jwt_secret)
    }
    .map_err(|e| {
        warn!(error = %e, "Token validation failed after generation");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Create session with user agent for audit tracking (critical - must succeed)
    let expires_at = Utc::now() + Duration::hours(8);
    create_session(
        &state.db,
        &claims.jti,
        &user.id,
        &tenant_id,
        &expires_at.to_rfc3339(),
        Some(&client_ip.0),
        user_agent.as_deref(),
    )
    .await
    .map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to create session - login aborted");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    // Track successful auth (best effort, doesn't fail login)
    track_auth_attempt(&state.db, &req.email, &client_ip.0, true, None)
        .await
        .ok();

    // Log audit (best effort, doesn't fail login)
    state
        .db
        .log_audit(
            &user.id,
            &user.role,
            &tenant_id,
            "auth.login",
            "session",
            Some(&claims.jti),
            "success",
            None,
            Some(&client_ip.0),
            None,
        )
        .await
        .ok();

    info!(
        user_id = %user.id,
        email = %user.email,
        role = %user.role,
        tenant_id = %tenant_id,
        ip = %client_ip.0,
        "User logged in"
    );

    Ok(Json(LoginResponse {
        schema_version: "v1".to_string(),
        token,
        user_id: user.id,
        tenant_id: tenant_id.clone(),
        role: user.role,
        expires_in: 28800, // 8 hours
    }))
}

/// Logout handler - revokes current token
pub async fn logout_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<LogoutResponse>, (StatusCode, Json<ErrorResponse>)> {
    let expires_at = Utc::now() + Duration::hours(8); // Original expiry

    revoke_token(
        &state.db,
        &claims.jti,
        &claims.sub,
        &claims.tenant_id,
        &expires_at.to_rfc3339(),
        Some(&claims.sub),
        Some("logout"),
    )
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to revoke token");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("logout failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    state
        .db
        .log_audit(
            &claims.sub,
            &claims.role,
            &claims.tenant_id,
            "auth.logout",
            "session",
            Some(&claims.jti),
            "success",
            None,
            None,
            None,
        )
        .await
        .ok();

    info!(user_id = %claims.sub, jti = %claims.jti, "User logged out");

    Ok(Json(LogoutResponse {
        message: "Logged out successfully".to_string(),
    }))
}

/// Token refresh handler
#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    responses(
        (status = 200, description = "Token refreshed", body = RefreshResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn refresh_token_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<RefreshResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Generate new token
    let new_token = refresh_token(&claims, &state.ed25519_keypair).map_err(|e| {
        warn!(error = %e, "Failed to refresh token");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Decode new token to get jti and exp
    let new_claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(&new_token, &state.ed25519_public_key)
    } else {
        crate::auth::validate_token(&new_token, &state.jwt_secret)
    }
    .map_err(|e| {
        warn!(error = %e, "Token validation failed after refresh");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Revoke old token
    let expires_at = Utc::now() + Duration::hours(8);
    revoke_token(
        &state.db,
        &claims.jti,
        &claims.sub,
        &claims.tenant_id,
        &expires_at.to_rfc3339(),
        Some(&claims.sub),
        Some("token refresh"),
    )
    .await
    .ok();

    // Create new session (critical - must succeed)
    create_session(
        &state.db,
        &new_claims.jti,
        &claims.sub,
        &claims.tenant_id,
        &expires_at.to_rfc3339(),
        None,
        None,
    )
    .await
    .map_err(|e| {
        warn!(error = %e, user_id = %claims.sub, "Failed to create refreshed session - refresh aborted");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    info!(
        user_id = %claims.sub,
        old_jti = %claims.jti,
        new_jti = %new_claims.jti,
        "Token refreshed"
    );

    Ok(Json(RefreshResponse {
        token: new_token,
        expires_at: new_claims.exp,
    }))
}

/// List active sessions for current user
#[utoipa::path(
    get,
    path = "/v1/auth/sessions",
    responses(
        (status = 200, description = "Active sessions", body = SessionsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn list_sessions_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SessionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let sessions = get_user_sessions(&state.db, &claims.sub)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to get user sessions");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

    let sessions_info: Vec<SessionInfo> = sessions
        .into_iter()
        .map(|(jti, created_at, ip_address, last_activity)| SessionInfo {
            jti,
            created_at,
            ip_address,
            last_activity,
        })
        .collect();

    Ok(Json(SessionsResponse {
        sessions: sessions_info,
    }))
}

/// Revoke a specific session
#[utoipa::path(
    delete,
    path = "/v1/auth/sessions/{jti}",
    params(
        ("jti" = String, Path, description = "Session ID (JTI) to revoke")
    ),
    responses(
        (status = 200, description = "Session revoked", body = LogoutResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn revoke_session_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(jti): Path<String>,
) -> Result<Json<LogoutResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify the session belongs to the user
    let sessions = get_user_sessions(&state.db, &claims.sub)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to get user sessions");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

    let session_exists = sessions.iter().any(|(s_jti, _, _, _)| s_jti == &jti);

    if !session_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("session not found")
                    .with_code("NOT_FOUND")
                    .with_string_details("session does not exist or does not belong to you"),
            ),
        ));
    }

    let expires_at = Utc::now() + Duration::hours(8);
    revoke_token(
        &state.db,
        &jti,
        &claims.sub,
        &claims.tenant_id,
        &expires_at.to_rfc3339(),
        Some(&claims.sub),
        Some("manual revocation"),
    )
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to revoke session");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("revocation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    info!(user_id = %claims.sub, jti = %jti, "Session revoked");

    Ok(Json(LogoutResponse {
        message: "Session revoked successfully".to_string(),
    }))
}

/// Development bypass handler - creates admin user session
/// Only available in debug builds - generates proper JWT even in dev mode
#[utoipa::path(
    post,
    path = "/v1/auth/dev-bypass",
    responses(
        (status = 200, description = "Dev bypass successful", body = LoginResponse),
        (status = 403, description = "Not in development mode")
    ),
    tag = "auth"
)]
pub async fn dev_bypass_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Extension(client_ip): Extension<ClientIp>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    // SECURITY: Only allow in debug builds
    #[cfg(not(debug_assertions))]
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("dev bypass not available")
                    .with_code("DEV_BYPASS_DISABLED")
                    .with_string_details("this endpoint is only available in development mode"),
            ),
        ));
    }

    // Extract user agent from headers for session tracking
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Create dev admin user details
    let user_id = "dev-admin-user".to_string();
    let email = "dev-admin@adapteros.local".to_string();
    let role = "admin".to_string();
    let tenant_id = "system".to_string();

    // Generate proper JWT token (same as login handler, not hardcoded)
    let token = if state.use_ed25519 {
        generate_token_ed25519(&user_id, &email, &role, &tenant_id, &state.ed25519_keypair)
            .map_err(|e| {
                warn!(error = %e, "Failed to generate dev bypass token");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
                )
            })?
    } else {
        crate::auth::generate_token(&user_id, &email, &role, &tenant_id, &state.jwt_secret)
            .map_err(|e| {
                warn!(error = %e, "Failed to generate dev bypass token");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
                )
            })?
    };

    // Decode to get jti and exp for session creation
    let claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(&token, &state.ed25519_public_key)
    } else {
        crate::auth::validate_token(&token, &state.jwt_secret)
    }
    .map_err(|e| {
        warn!(error = %e, "Token validation failed after generation");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Create session with user agent for audit tracking (critical - must succeed)
    let expires_at = Utc::now() + Duration::hours(8);
    create_session(
        &state.db,
        &claims.jti,
        &user_id,
        &tenant_id,
        &expires_at.to_rfc3339(),
        Some(&client_ip.0),
        user_agent.as_deref(),
    )
    .await
    .map_err(|e| {
        warn!(error = %e, user_id = %user_id, "Failed to create dev bypass session - aborted");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    // Log audit (best effort, doesn't fail dev bypass)
    state
        .db
        .log_audit(
            &user_id,
            &role,
            &tenant_id,
            "auth.dev_bypass",
            "session",
            Some(&claims.jti),
            "success",
            None,
            Some(&client_ip.0),
            None,
        )
        .await
        .ok();

    info!(
        user_id = %user_id,
        email = %email,
        ip = %client_ip.0,
        "Dev bypass login successful"
    );

    Ok(Json(LoginResponse {
        schema_version: "v1".to_string(),
        token,
        user_id,
        tenant_id,
        role,
        expires_in: (claims.exp - Utc::now().timestamp()) as u64,
    }))
}
