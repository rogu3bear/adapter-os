use crate::auth::{
    generate_token_ed25519_with_admin_tenants, generate_token_with_admin_tenants, hash_password,
    refresh_token, verify_password, Claims,
};
use crate::auth_common::{attach_auth_cookie, AuthConfig};
use crate::ip_extraction::ClientIp;
use crate::security::{
    create_session, get_user_sessions, is_account_locked, revoke_token, track_auth_attempt,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{LoginRequest, LoginResponse};
use adapteros_db::{
    users::{Role, User},
    Db,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
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

fn audit_claims_for_user(user: &User, tenant_id: &str) -> Claims {
    Claims {
        sub: user.id.clone(),
        email: user.email.clone(),
        role: user.role.clone(),
        roles: vec![user.role.clone()],
        tenant_id: tenant_id.to_string(),
        admin_tenants: vec![],
        exp: 0,
        iat: 0,
        jti: String::new(),
        nbf: 0,
    }
}

async fn log_auth_event(
    db: &Db,
    claims: &Claims,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    status: &str,
    error_message: Option<&str>,
    ip_address: Option<&str>,
) {
    let _ = db
        .log_audit(
            &claims.sub,
            &claims.role,
            &claims.tenant_id,
            action,
            resource_type,
            resource_id,
            status,
            error_message,
            ip_address,
            None,
        )
        .await;
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
    // Check if any users exist using Db trait method
    let user_count = state.db.count_users().await.map_err(|e| {
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
        .create_user(
            &req.email,
            &req.display_name,
            &pw_hash,
            Role::Admin,
            "system",
        )
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
) -> Result<(HeaderMap, Json<LoginResponse>), (StatusCode, Json<ErrorResponse>)> {
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
        Some(u) => {
            track_auth_attempt(
                &state.db,
                &req.email,
                &client_ip.0,
                false,
                Some("account disabled"),
            )
            .await
            .ok();

            let tenant_id = if u.role == "admin" {
                "system".to_string()
            } else {
                "default".to_string()
            };
            let audit_claims = audit_claims_for_user(&u, &tenant_id);
            log_auth_event(
                &state.db,
                &audit_claims,
                "auth.login",
                "session",
                None,
                "failure",
                Some("account disabled"),
                Some(&client_ip.0),
            )
            .await;

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

    let tenant_id = if user.role == "admin" {
        "system".to_string()
    } else {
        "default".to_string()
    };

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

        let audit_claims = audit_claims_for_user(&user, &tenant_id);
        log_auth_event(
            &state.db,
            &audit_claims,
            "auth.login",
            "session",
            None,
            "failure",
            Some("invalid password"),
            Some(&client_ip.0),
        )
        .await;

        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("invalid credentials")
                    .with_code("INVALID_CREDENTIALS")
                    .with_string_details("email or password is incorrect"),
            ),
        ));
    }

    // Get token TTL from config (default 8 hours)
    let token_ttl = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;
        config.security.token_ttl_seconds.unwrap_or(8 * 3600)
    };

    // Get admin tenant access list if user is admin
    let admin_tenants = if user.role == "admin" {
        adapteros_db::get_user_tenant_access(&state.db, &user.id)
            .await
            .unwrap_or_else(|e| {
                warn!(error = %e, user_id = %user.id, "Failed to get admin tenant access, defaulting to empty");
                vec![]
            })
    } else {
        vec![]
    };

    // Generate JWT token
    let token = if state.use_ed25519 {
        generate_token_ed25519_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &tenant_id,
            &admin_tenants,
            &state.ed25519_keypair,
            token_ttl,
        )
        .map_err(|e| {
            warn!(error = %e, "Failed to generate token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        })?
    } else {
        generate_token_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &tenant_id,
            &admin_tenants,
            &state.jwt_secret,
            token_ttl,
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
    log_auth_event(
        &state.db,
        &claims,
        "auth.login",
        "session",
        Some(&claims.jti),
        "success",
        None,
        Some(&client_ip.0),
    )
    .await;

    info!(
        user_id = %user.id,
        email = %user.email,
        role = %user.role,
        tenant_id = %tenant_id,
        ip = %client_ip.0,
        "User logged in"
    );

    // Attach auth cookie for browser-based authentication
    let auth_cfg = AuthConfig::from_state(&state);
    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|e| {
        warn!(error = %e, "Failed to attach auth cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;

    Ok((
        response_headers,
        Json(LoginResponse {
            schema_version: "v1".to_string(),
            token,
            user_id: user.id,
            tenant_id: tenant_id.clone(),
            role: user.role,
            expires_in: 28800, // 8 hours
        }),
    ))
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
#[axum::debug_handler]
pub async fn refresh_token_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<RefreshResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Extract config value before any await to avoid holding RwLockReadGuard across await
    let token_ttl = state.config.read()
        .map(|cfg| cfg.security.token_ttl_seconds.unwrap_or(8 * 3600))
        .unwrap_or_else(|_| {
            warn!("Config lock poisoned during token refresh, using default TTL");
            8 * 3600 // Default 8 hours
        });

    let new_token = match refresh_token(&claims, &state.ed25519_keypair, token_ttl) {
        Ok(token) => token,
        Err(e) => {
            warn!(error = %e, "Failed to refresh token");
            log_auth_event(
                &state.db,
                &claims,
                "auth.refresh",
                "session",
                None,
                "failure",
                Some("token refresh failed"),
                None,
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
            ));
        }
    };

    let new_claims = match if state.use_ed25519 {
        crate::auth::validate_token_ed25519(&new_token, &state.ed25519_public_key)
    } else {
        crate::auth::validate_token(&new_token, &state.jwt_secret)
    } {
        Ok(claims) => claims,
        Err(e) => {
            warn!(error = %e, "Token validation failed after refresh");
            log_auth_event(
                &state.db,
                &claims,
                "auth.refresh",
                "session",
                None,
                "failure",
                Some("token validation failed"),
                None,
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            ));
        }
    };

    let expires_at = Utc::now() + Duration::hours(8);
    let expires_at_str = expires_at.to_rfc3339();

    let mut tx = match state.db.pool().begin().await {
        Ok(tx) => tx,
        Err(e) => {
            warn!(error = %e, "Failed to begin transaction for token refresh");
            log_auth_event(
                &state.db,
                &claims,
                "auth.refresh",
                "session",
                None,
                "failure",
                Some("transaction begin failed"),
                None,
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            ));
        }
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO user_sessions (jti, user_id, tenant_id, created_at, expires_at, last_activity)
         VALUES (?, ?, ?, datetime('now'), ?, datetime('now'))",
    )
    .bind(&new_claims.jti)
    .bind(&claims.sub)
    .bind(&claims.tenant_id)
    .bind(&expires_at_str)
    .execute(&mut *tx)
    .await
    {
        warn!(error = %e, user_id = %claims.sub, "Failed to create refreshed session");
        log_auth_event(
            &state.db,
            &claims,
            "auth.refresh",
            "session",
            Some(&new_claims.jti),
            "failure",
            Some("session creation failed"),
            None,
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        ));
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO revoked_tokens (jti, user_id, tenant_id, revoked_at, revoked_by, reason, expires_at)
         VALUES (?, ?, ?, datetime('now'), ?, 'token refresh', ?)
         ON CONFLICT(jti) DO NOTHING",
    )
    .bind(&claims.jti)
    .bind(&claims.sub)
    .bind(&claims.tenant_id)
    .bind(&claims.sub)
    .bind(&expires_at_str)
    .execute(&mut *tx)
    .await
    {
        warn!(error = %e, "Failed to revoke old token during refresh");
        log_auth_event(
            &state.db,
            &claims,
            "auth.refresh",
            "session",
            Some(&claims.jti),
            "failure",
            Some("token revocation failed"),
            None,
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        ));
    }

    if let Err(e) = tx.commit().await {
        warn!(error = %e, user_id = %claims.sub, "Failed to commit token refresh transaction");
        log_auth_event(
            &state.db,
            &claims,
            "auth.refresh",
            "session",
            Some(&new_claims.jti),
            "failure",
            Some("transaction commit failed"),
            None,
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        ));
    }

    log_auth_event(
        &state.db,
        &claims,
        "auth.refresh",
        "session",
        Some(&new_claims.jti),
        "success",
        None,
        None,
    )
    .await;

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
    let sessions = match get_user_sessions(&state.db, &claims.sub).await {
        Ok(sessions) => sessions,
        Err(e) => {
            warn!(error = %e, "Failed to get user sessions");
            log_auth_event(
                &state.db,
                &claims,
                "auth.sessions.list",
                "session",
                None,
                "failure",
                Some("failed to read sessions"),
                None,
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            ));
        }
    };

    let sessions_info: Vec<SessionInfo> = sessions
        .into_iter()
        .map(|(jti, created_at, ip_address, last_activity)| SessionInfo {
            jti,
            created_at,
            ip_address,
            last_activity,
        })
        .collect();

    log_auth_event(
        &state.db,
        &claims,
        "auth.sessions.list",
        "session",
        None,
        "success",
        None,
        None,
    )
    .await;

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
    let sessions = match get_user_sessions(&state.db, &claims.sub).await {
        Ok(sessions) => sessions,
        Err(e) => {
            warn!(error = %e, "Failed to get user sessions");
            log_auth_event(
                &state.db,
                &claims,
                "auth.session.revoke",
                "session",
                Some(&jti),
                "failure",
                Some("failed to read sessions"),
                None,
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            ));
        }
    };

    let session_exists = sessions.iter().any(|(s_jti, _, _, _)| s_jti == &jti);

    if !session_exists {
        log_auth_event(
            &state.db,
            &claims,
            "auth.session.revoke",
            "session",
            Some(&jti),
            "failure",
            Some("session not found"),
            None,
        )
        .await;
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
    if let Err(e) = revoke_token(
        &state.db,
        &jti,
        &claims.sub,
        &claims.tenant_id,
        &expires_at.to_rfc3339(),
        Some(&claims.sub),
        Some("manual revocation"),
    )
    .await
    {
        warn!(error = %e, "Failed to revoke session");
        log_auth_event(
            &state.db,
            &claims,
            "auth.session.revoke",
            "session",
            Some(&jti),
            "failure",
            Some("revocation failed"),
            None,
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("revocation failed").with_code("INTERNAL_ERROR")),
        ));
    }

    log_auth_event(
        &state.db,
        &claims,
        "auth.session.revoke",
        "session",
        Some(&jti),
        "success",
        None,
        None,
    )
    .await;
    info!(user_id = %claims.sub, jti = %jti, "Session revoked");

    Ok(Json(LogoutResponse {
        message: "Session revoked successfully".to_string(),
    }))
}

/// Development bypass handler - creates admin user session
/// Only available in debug builds - generates proper JWT even in dev mode
#[cfg(all(feature = "dev-bypass", debug_assertions))]
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
    headers: HeaderMap,
    Extension(client_ip): Extension<ClientIp>,
) -> Result<(HeaderMap, Json<LoginResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);

    if !auth_cfg.dev_login_allowed() {
        let guard_claims = Claims {
            sub: "dev-bypass".to_string(),
            email: "dev-bypass@adapteros.local".to_string(),
            role: "system".to_string(),
            roles: vec!["system".to_string()],
            tenant_id: "system".to_string(),
            admin_tenants: vec![],
            exp: 0,
            iat: 0,
            jti: String::new(),
            nbf: 0,
        };
        log_auth_event(
            &state.db,
            &guard_claims,
            "auth.dev_login",
            "session",
            None,
            "failure",
            Some("dev bypass disabled"),
            Some(&client_ip.0),
        )
        .await;
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
    let tenant_id = "default".to_string(); // Use "default" tenant which exists in DB

    // Ensure the dev user exists in the database so /auth/me works
    info!(user_id = %user_id, "Creating/updating dev user in database");
    match state
        .db
        .ensure_user(
            &user_id,
            &email,
            "Developer Admin",
            "", // empty password hash for dev user
            adapteros_db::users::Role::Admin,
            &tenant_id,
        )
        .await
    {
        Ok(()) => {
            info!(user_id = %user_id, "Dev user ensured in database successfully");
        }
        Err(e) => {
            warn!(error = %e, user_id = %user_id, "Failed to ensure dev user exists in database, continuing anyway");
            // Don't fail - the user can still authenticate, they just won't see their profile in /me
        }
    }

    let dev_user = User {
        id: user_id.clone(),
        email: email.clone(),
        display_name: "Developer Admin".to_string(),
        pw_hash: String::new(),
        role: role.clone(),
        disabled: false,
        created_at: Utc::now().to_rfc3339(),
        tenant_id: tenant_id.clone(),
    };

    let ctx = AuthContext::from_user(dev_user).map_err(|err| {
        warn!(error = %err, "Failed to build dev auth context");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let token = build_auth_token(&ctx, &auth_cfg).map_err(|err| {
        warn!(error = %err, "Failed to generate dev bypass token");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

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

    let expires_at = Utc::now() + Duration::seconds(auth_cfg.effective_ttl() as i64);
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

    log_auth_event(
        &state.db,
        &claims,
        "auth.dev_login",
        "session",
        Some(&claims.jti),
        "success",
        None,
        Some(&client_ip.0),
    )
    .await;

    info!(
        user_id = %user_id,
        email = %email,
        ip = %client_ip.0,
        "Dev bypass login successful"
    );

    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|err| {
        warn!(error = %err, "Failed to attach auth cookie for dev bypass");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    Ok((
        response_headers,
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

/// Authentication configuration response for frontend
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuthConfigResponse {
    /// Whether user registration is allowed
    pub allow_registration: bool,
    /// Whether email verification is required
    pub require_email_verification: bool,
    /// Session timeout in minutes
    pub session_timeout_minutes: u32,
    /// Maximum failed login attempts before lockout
    pub max_login_attempts: u32,
    /// Minimum password length
    pub password_min_length: u32,
    /// Whether MFA is required
    pub mfa_required: bool,
    /// Allowed email domains for registration (empty = all)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    /// Whether running in production mode
    pub production_mode: bool,
    /// Whether dev login bypass is enabled in config
    pub dev_token_enabled: bool,
    /// Whether dev bypass is actually allowed (computed from config)
    pub dev_bypass_allowed: bool,
    /// JWT signing mode (eddsa or hmac)
    pub jwt_mode: String,
    /// Token expiry in hours
    pub token_expiry_hours: u32,
}

/// Get authentication configuration
///
/// Returns authentication settings for the frontend to use.
/// This endpoint is public (no auth required) so the login page
/// can display dev bypass button when available.
#[utoipa::path(
    get,
    path = "/v1/auth/config",
    responses(
        (status = 200, description = "Auth configuration", body = AuthConfigResponse),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn get_auth_config_handler(
    State(state): State<AppState>,
) -> Result<Json<AuthConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.read().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;
    let auth_cfg = AuthConfig::from_state(&state);

    let response = AuthConfigResponse {
        allow_registration: false, // Registration not implemented yet
        require_email_verification: false,
        session_timeout_minutes: (auth_cfg.effective_ttl() / 60) as u32,
        max_login_attempts: 5, // Hardcoded for now, could be in config
        password_min_length: 12,
        mfa_required: config.security.require_mfa.unwrap_or(false),
        allowed_domains: None,
        production_mode: config.server.production_mode,
        dev_token_enabled: config.security.dev_login_enabled,
        dev_bypass_allowed: auth_cfg.dev_login_allowed(),
        jwt_mode: config
            .security
            .jwt_mode
            .clone()
            .unwrap_or_else(|| "eddsa".to_string()),
        token_expiry_hours: (auth_cfg.effective_ttl() / 3600) as u32,
    };

    Ok(Json(response))
}
