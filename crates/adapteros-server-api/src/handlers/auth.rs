//! Authentication handlers
//!
//! Provides login, logout, and user information endpoints.
//!
//! 【2025-01-20†rectification†auth_handlers_expanded】

use crate::audit_helper;
use crate::auth::{dev_no_auth_enabled, hash_password, verify_password, Claims, JWT_ISSUER};
use crate::auth_common::{
    attach_auth_cookie, attach_csrf_cookie, attach_refresh_cookie, build_auth_token,
    build_user_info, clear_auth_cookies, AuthConfig, AuthContext,
};
use crate::mfa::{
    decrypt_mfa_secret, derive_mfa_key, verify_and_mark_backup_code, verify_totp, BackupCode,
};
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    Extension,
};
use chrono::Utc;
use tracing::{error, info, warn};
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
    let verification = match verify_password(&request.password, &user.pw_hash) {
        Ok(result) => result,
        Err(e) => {
            error!(error = %e, "Password verification error");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal server error".to_string())),
            ));
        }
    };

    if verification.needs_rehash {
        if let Ok(new_hash) = hash_password(&request.password) {
            state
                .db
                .update_user_password(&user.id, &new_hash)
                .await
                .ok();
        }
    }

    if !verification.valid {
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
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: String::new(),
            nbf: 0,
            iss: JWT_ISSUER.to_string(),
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
    let mut mfa_level: Option<String> = None;

    // 2b. Enforce MFA if enabled for this user
    if user.mfa_enabled {
        let provided_code = request.totp_code.as_deref().ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("MFA required".to_string()).with_code("MFA_REQUIRED")),
            )
        })?;

        let secret_enc = user.mfa_secret_enc.as_ref().ok_or_else(|| {
            error!(user_id = %user.id, "MFA enabled but secret missing");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal server error".to_string())),
            )
        })?;

        let key = derive_mfa_key(auth_cfg.jwt_secret);
        let secret = decrypt_mfa_secret(secret_enc, &key).map_err(|e| {
            error!(error = %e, user_id = %user.id, "Failed to decrypt MFA secret");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal server error".to_string())),
            )
        })?;

        let mut used_backup = false;
        let totp_ok = verify_totp(&secret, provided_code);
        if totp_ok {
            let now = chrono::Utc::now().to_rfc3339();
            if let Err(e) = state.db.update_user_mfa_last_verified(&user.id, &now).await {
                warn!(error = %e, user_id = %user.id, "Failed to update MFA last verified timestamp");
            }
            mfa_level = Some("totp".to_string());
        } else {
            // Try backup codes if available
            if let Some(json_codes) = user.mfa_backup_codes_json.as_ref() {
                match serde_json::from_str::<Vec<BackupCode>>(json_codes) {
                    Ok(mut codes) => {
                        if verify_and_mark_backup_code(&mut codes, provided_code).is_some() {
                            let now = chrono::Utc::now().to_rfc3339();
                            let updated_json = serde_json::to_string(&codes)
                                .unwrap_or_else(|_| json_codes.clone());
                            if let Err(e) = state
                                .db
                                .update_user_backup_codes(&user.id, &updated_json, Some(&now))
                                .await
                            {
                                warn!(error = %e, user_id = %user.id, "Failed to persist backup code usage");
                            }
                            mfa_level = Some("backup_code".to_string());
                            used_backup = true;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, user_id = %user.id, "Failed to parse backup codes JSON");
                    }
                }
            }

            if mfa_level.is_none() {
                // Audit failed MFA attempt
                let audit_claims = Claims {
                    sub: user.id.clone(),
                    email: user.email.clone(),
                    role: user.role.clone(),
                    roles: vec![user.role.clone()],
                    tenant_id: user.tenant_id.clone(),
                    admin_tenants: vec![],
                    device_id: None,
                    session_id: None,
                    mfa_level: None,
                    rot_id: None,
                    exp: 0,
                    iat: 0,
                    jti: String::new(),
                    nbf: 0,
                    iss: JWT_ISSUER.to_string(),
                };
                let _ = audit_helper::log_action(
                    &state.db,
                    &audit_claims,
                    "auth.login.mfa",
                    "user",
                    Some(&user.id),
                    "failure",
                    Some("Invalid MFA code"),
                )
                .await;

                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse::new(if used_backup {
                        "Invalid backup code".to_string()
                    } else {
                        "Invalid MFA code".to_string()
                    })),
                ));
            }
        }
    }
    let mut ctx = AuthContext::from_user(user).map_err(|err| {
        error!(error = %err, "Failed to build auth context");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal server error".to_string())),
        )
    })?;

    if ctx.role.to_string() == "admin" {
        ctx.admin_tenants = adapteros_db::get_user_tenant_access(&state.db, &ctx.user.id)
            .await
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    user_id = %ctx.user.id,
                    "Failed to get admin tenant access, defaulting to empty"
                );
                Vec::new()
            });
    }

    let token = build_auth_token(&ctx, &auth_cfg, mfa_level.as_deref()).map_err(|err| {
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
    attach_refresh_cookie(&mut headers, &token, &auth_cfg).map_err(|err| {
        error!(error = %err, "Failed to attach refresh cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal server error".to_string())),
        )
    })?;
    let csrf_token = uuid::Uuid::new_v4().to_string();
    attach_csrf_cookie(
        &mut headers,
        &csrf_token,
        &auth_cfg,
        auth_cfg.effective_ttl(),
    )
    .map_err(|err| {
        error!(error = %err, "Failed to attach csrf cookie");
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
        admin_tenants: ctx.admin_tenants.clone(),
        device_id: None,
        session_id: None,
        mfa_level: mfa_level.clone(),
        rot_id: None,
        exp: chrono::Utc::now().timestamp() + auth_cfg.access_ttl() as i64,
        iat: chrono::Utc::now().timestamp(),
        jti: format!("{}", uuid::Uuid::now_v7()),
        nbf: chrono::Utc::now().timestamp(),
        iss: JWT_ISSUER.to_string(),
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
            expires_in: auth_cfg.access_ttl(),
            tenants: None,
            mfa_level,
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
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<(HeaderMap, StatusCode), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);
    let mut headers = HeaderMap::new();
    clear_auth_cookies(&mut headers, &auth_cfg).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal server error".to_string())),
        )
    })?;

    Ok((headers, StatusCode::NO_CONTENT))
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
