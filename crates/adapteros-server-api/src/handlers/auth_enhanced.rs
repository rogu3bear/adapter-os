use crate::audit_helper;
use crate::auth::{
    generate_token_ed25519_with_admin_tenants, generate_token_with_admin_tenants, hash_password,
    issue_access_token_ed25519, issue_access_token_hmac, issue_refresh_token_ed25519,
    issue_refresh_token_ed25519_with_kv, issue_refresh_token_hmac,
    issue_refresh_token_hmac_with_kv, validate_refresh_token_ed25519, validate_refresh_token_hmac,
    verify_password, AuthMode, Claims, PrincipalType, JWT_ISSUER,
};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use crate::auth_common::AuthContext;
use crate::auth_common::{
    attach_auth_cookie, attach_csrf_cookie, attach_refresh_cookie, clear_auth_cookies, AuthConfig,
};
use crate::ip_extraction::ClientIp;
use crate::mfa::{
    decrypt_mfa_secret, derive_mfa_key, encrypt_mfa_secret, generate_backup_codes,
    generate_totp_secret, hash_backup_codes, otpauth_uri, verify_and_mark_backup_code, verify_totp,
    BackupCode,
};
use crate::security::{
    check_login_lockout, create_session, get_session_by_id, get_tenant_token_baseline,
    get_user_sessions, is_token_revoked, lock_session, revoke_token, track_auth_attempt,
    upsert_user_session,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{
    LoginRequest, LoginResponse, MfaDisableRequest, MfaEnrollStartResponse, MfaEnrollVerifyRequest,
    MfaEnrollVerifyResponse, MfaStatusResponse, SwitchTenantRequest, SwitchTenantResponse,
    TenantListResponse, TenantSummary,
};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_db::auth_sessions_kv::AuthSessionKvRepository;
use adapteros_db::{
    users::{Role, User},
    Db,
};
use adapteros_telemetry::{build_auth_event, make_auth_payload};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use blake3;
use chrono::{Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

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
pub struct AuthHealthResponse {
    pub status: String,
    pub db: String,
    pub signing_keys: String,
    pub idp_configured: bool,
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

const ADMIN_TENANT_WILDCARD: &str = "*";

fn audit_claims_for_user(user: &User, tenant_id: &str) -> Claims {
    Claims {
        sub: user.id.clone(),
        email: user.email.clone(),
        role: user.role.clone(),
        roles: vec![user.role.clone()],
        tenant_id: tenant_id.to_string(),
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
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    }
}

fn extract_cookie_token(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            let prefix = format!("{name}=");
            for cookie in cookies.split(';') {
                let trimmed = cookie.trim();
                if let Some(token) = trimmed.strip_prefix(&prefix) {
                    return Some(token.to_string());
                }
            }
            None
        })
}

#[allow(clippy::too_many_arguments)]
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

async fn emit_auth_event(
    state: &AppState,
    principal_id: &str,
    tenant_id: &str,
    flow_type: &str,
    success: bool,
    error_code: Option<&str>,
) {
    let identity = IdentityEnvelope::new(
        tenant_id.to_string(),
        "api".to_string(),
        "auth".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );

    let payload = make_auth_payload(
        principal_id.to_string(),
        tenant_id.to_string(),
        flow_type.to_string(),
        success,
        error_code.map(|c| c.to_string()),
    );

    if let Ok(event) = build_auth_event(identity, payload) {
        let _ = state.telemetry_buffer.push(event).await;
    }
}

/// Log a failed token refresh attempt for audit purposes.
/// This captures failures where we may not have valid claims.
async fn log_refresh_failure(
    db: &Db,
    session_id: Option<&str>,
    user_id: Option<&str>,
    tenant_id: Option<&str>,
    error_code: &str,
    error_detail: &str,
    ip_address: Option<&str>,
) {
    let metadata = serde_json::json!({ "error_code": error_code });
    let _ = db
        .log_audit(
            user_id.unwrap_or("unknown"),
            "unknown",
            tenant_id.unwrap_or("unknown"),
            "auth.refresh_failed",
            "session",
            session_id,
            "failure",
            Some(error_detail),
            ip_address,
            Some(&metadata.to_string()),
        )
        .await;
}

/// Emit telemetry event for failed refresh
async fn emit_refresh_failure(
    state: &AppState,
    user_id: Option<&str>,
    tenant_id: Option<&str>,
    error_code: &str,
) {
    emit_auth_event(
        state,
        user_id.unwrap_or("unknown"),
        tenant_id.unwrap_or("unknown"),
        "refresh",
        false,
        Some(error_code),
    )
    .await;
}

async fn collect_tenant_summaries(
    state: &AppState,
    user_id: &str,
    role: &str,
    active_tenant: &str,
    admin_tenants: &[String],
) -> Result<Vec<TenantSummary>, (StatusCode, Json<ErrorResponse>)> {
    let has_wildcard = admin_tenants.iter().any(|t| t == ADMIN_TENANT_WILDCARD);
    // Wildcard admin: return all tenants
    if role == "admin" && has_wildcard {
        let (all_tenants, _) = state.db.list_tenants_paginated(200, 0).await.map_err(|e| {
            warn!(
                error = %e,
                user_id = %user_id,
                role = %role,
                active_tenant = %active_tenant,
                "Failed to list tenants for admin"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

        let tenants = all_tenants
            .into_iter()
            .map(|t| TenantSummary {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                id: t.id,
                name: t.name,
                status: t.status,
                created_at: Some(t.created_at),
            })
            .collect();

        return Ok(tenants);
    }

    let mut tenant_ids: HashSet<String> = HashSet::new();
    tenant_ids.insert(active_tenant.to_string());

    if role == "admin" && !has_wildcard {
        for t in admin_tenants {
            if t != ADMIN_TENANT_WILDCARD {
                tenant_ids.insert(t.clone());
            }
        }

        if let Ok(db_grants) = adapteros_db::get_user_tenant_access(&state.db, user_id).await {
            for t in db_grants {
                tenant_ids.insert(t);
            }
        }
    }

    let mut tenants: Vec<TenantSummary> = Vec::new();
    for tenant_id in tenant_ids {
        if let Some(t) = state.db.get_tenant(&tenant_id).await.map_err(|e| {
            warn!(
                error = %e,
                tenant_id = %tenant_id,
                user_id = %user_id,
                active_tenant = %active_tenant,
                "Failed to fetch tenant"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })? {
            tenants.push(TenantSummary {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                id: t.id,
                name: t.name,
                status: t.status,
                created_at: Some(t.created_at),
            });
        }
    }

    Ok(tenants)
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
        warn!(
            error = %e,
            email = %req.email,
            ip = %client_ip.0,
            "Failed to query user count"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
        )
    })?;

    if user_count > 0 {
        warn!(
            email = %req.email,
            ip = %client_ip.0,
            existing_users = user_count,
            "Bootstrap attempt when users already exist"
        );
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

    // Ensure system tenant exists before creating admin user (KV-capable)
    state.db.ensure_system_tenant().await.map_err(|e| {
        warn!(
            error = %e,
            tenant_id = %"system",
            email = %req.email,
            "Failed to ensure system tenant during bootstrap"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("system tenant creation failed")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Hash password
    let pw_hash = hash_password(&req.password).map_err(|e| {
        warn!(error = %e, email = %req.email, ip = %client_ip.0, "Failed to hash password");
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
            warn!(
                error = %e,
                email = %req.email,
                tenant_id = %"system",
                "Failed to create admin user"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("user creation failed").with_code("DATABASE_ERROR")),
            )
        })?;

    // Grant admin user access to system tenant for cross-tenant operations (best-effort; SQL only)
    if state.db.storage_mode().write_to_sql() && state.db.pool_opt().is_some() {
        if let Err(e) = adapteros_db::grant_user_tenant_access(
            &state.db,
            &user_id,
            "system",
            &user_id, // Self-granted during bootstrap
            Some("Bootstrap admin auto-grant"),
            None, // No expiration
        )
        .await
        {
            warn!(
                error = %e,
                user_id = %user_id,
                tenant_id = %"system",
                "Failed to grant system tenant access during bootstrap (non-fatal)"
            );
            // Non-fatal - user can still access their own tenant
        }
    } else {
        warn!(
            user_id = %user_id,
            "Skipped system tenant grant (SQL disabled or pool unavailable)"
        );
    }

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

    // Check account lockout (user + IP)
    let lockout_state = check_login_lockout(&state.db, &req.email, &client_ip.0)
        .await
        .map_err(|e| {
            warn!(
                error = %e,
                email = %req.email,
                ip = %client_ip.0,
                "Failed to check account lockout"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

    if let Some(lockout) = lockout_state {
        let lockout_until_str = lockout.until.to_rfc3339();
        track_auth_attempt(
            &state.db,
            &req.email,
            &client_ip.0,
            false,
            Some("account locked"),
        )
        .await
        .ok();

        let lockout_claims = state
            .db
            .get_user_by_email(&req.email)
            .await
            .ok()
            .flatten()
            .map(|u| {
                let tenant = if u.tenant_id.is_empty() {
                    "default".to_string()
                } else {
                    u.tenant_id.clone()
                };
                audit_claims_for_user(&u, &tenant)
            })
            .unwrap_or_else(|| Claims {
                sub: req.email.clone(),
                email: req.email.clone(),
                role: "unknown".to_string(),
                roles: vec![],
                tenant_id: "default".to_string(),
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
                auth_mode: AuthMode::BearerToken,
                principal_type: Some(PrincipalType::User),
            });

        log_auth_event(
            &state.db,
            &lockout_claims,
            "auth.lockout",
            "session",
            None,
            "failure",
            Some("too many failed attempts"),
            Some(&client_ip.0),
        )
        .await;

        log_auth_event(
            &state.db,
            &lockout_claims,
            "auth.login_failure_excess",
            "session",
            None,
            "failure",
            Some("too many failed attempts"),
            Some(&client_ip.0),
        )
        .await;

        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("account locked")
                    .with_code("ACCOUNT_LOCKED")
                    .with_string_details(format!(
                        "too many failed attempts, retry after {}",
                        lockout_until_str
                    )),
            ),
        ));
    }

    // Get user by email
    let user = state.db.get_user_by_email(&req.email).await.map_err(|e| {
        warn!(
            error = %e,
            email = %req.email,
            ip = %client_ip.0,
            "Database error during login"
        );
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

            if let Ok(Some(lockout)) =
                check_login_lockout(&state.db, &req.email, &client_ip.0).await
            {
                let anon_claims = Claims {
                    sub: req.email.clone(),
                    email: req.email.clone(),
                    role: "unknown".to_string(),
                    roles: vec![],
                    tenant_id: "default".to_string(),
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
                    auth_mode: AuthMode::BearerToken,
                    principal_type: Some(PrincipalType::User),
                };

                log_auth_event(
                    &state.db,
                    &anon_claims,
                    "auth.login_failure_excess",
                    "session",
                    None,
                    "failure",
                    Some("too many failed attempts"),
                    Some(&client_ip.0),
                )
                .await;

                emit_auth_event(
                    &state,
                    &req.email,
                    &anon_claims.tenant_id,
                    "login",
                    false,
                    Some("ACCOUNT_LOCKED"),
                )
                .await;

                return Err((
                    StatusCode::FORBIDDEN,
                    Json(
                        ErrorResponse::new("account locked")
                            .with_code("ACCOUNT_LOCKED")
                            .with_string_details(format!(
                                "too many failed attempts, retry after {}",
                                lockout.until.to_rfc3339()
                            )),
                    ),
                ));
            }

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
    let verification = verify_password(&req.password, &user.pw_hash).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            ip = %client_ip.0,
            "Password verification error"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let tenant_id = if user.tenant_id.is_empty() {
        "default".to_string()
    } else {
        user.tenant_id.clone()
    };

    if !verification.valid {
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

        emit_auth_event(
            &state,
            &user.id,
            &tenant_id,
            "login",
            false,
            Some("INVALID_CREDENTIALS"),
        )
        .await;

        if let Ok(Some(lockout)) = check_login_lockout(&state.db, &req.email, &client_ip.0).await {
            log_auth_event(
                &state.db,
                &audit_claims,
                "auth.login_failure_excess",
                "session",
                None,
                "failure",
                Some("too many failed attempts"),
                Some(&client_ip.0),
            )
            .await;

            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("account locked")
                        .with_code("ACCOUNT_LOCKED")
                        .with_string_details(format!(
                            "too many failed attempts, retry after {}",
                            lockout.until.to_rfc3339()
                        )),
                ),
            ));
        }

        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("invalid credentials")
                    .with_code("INVALID_CREDENTIALS")
                    .with_string_details("email or password is incorrect"),
            ),
        ));
    }

    if verification.needs_rehash {
        if let Ok(new_hash) = hash_password(&req.password) {
            if let Err(e) = state.db.update_user_password(&user.id, &new_hash).await {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %tenant_id,
                    "Failed to upgrade password hash (continuing)"
                );
            }
        }
    }

    // Enforce MFA if enabled for the user
    let mut mfa_level: Option<String> = None;
    if user.mfa_enabled {
        let provided_code = if let Some(code) = req.totp_code.as_deref() {
            code
        } else {
            track_auth_attempt(
                &state.db,
                &req.email,
                &client_ip.0,
                false,
                Some("mfa required"),
            )
            .await
            .ok();
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("MFA required")
                        .with_code("MFA_REQUIRED")
                        .with_string_details("enter your TOTP or backup code"),
                ),
            ));
        };

        let secret_enc = user.mfa_secret_enc.clone().ok_or_else(|| {
            error!(
                user_id = %user.id,
                tenant_id = %tenant_id,
                "MFA enabled but secret missing"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

        let key = derive_mfa_key(state.jwt_secret.as_slice());
        let secret = decrypt_mfa_secret(&secret_enc, &key).map_err(|e| {
            error!(
                error = %e,
                user_id = %user.id,
                tenant_id = %tenant_id,
                "Failed to decrypt MFA secret"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

        let mut used_backup = false;
        let totp_ok = verify_totp(&secret, provided_code);
        if totp_ok {
            let now = chrono::Utc::now().to_rfc3339();
            if let Err(e) = state.db.update_user_mfa_last_verified(&user.id, &now).await {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %tenant_id,
                    "Failed to update MFA last verified"
                );
            }
            mfa_level = Some("totp".to_string());
        } else if let Some(json_codes) = user.mfa_backup_codes_json.as_ref() {
            match serde_json::from_str::<Vec<BackupCode>>(json_codes) {
                Ok(mut codes) => {
                    if verify_and_mark_backup_code(&mut codes, provided_code).is_some() {
                        used_backup = true;
                        let now = chrono::Utc::now().to_rfc3339();
                        let updated =
                            serde_json::to_string(&codes).unwrap_or_else(|_| json_codes.clone());
                        if let Err(e) = state
                            .db
                            .update_user_backup_codes(&user.id, &updated, Some(&now))
                            .await
                        {
                            warn!(
                                error = %e,
                                user_id = %user.id,
                                tenant_id = %tenant_id,
                                "Failed to persist backup code usage"
                            );
                        }
                        mfa_level = Some("backup_code".to_string());
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        user_id = %user.id,
                        tenant_id = %tenant_id,
                        "Failed to parse backup codes JSON"
                    )
                }
            }
        }

        if mfa_level.is_none() {
            track_auth_attempt(
                &state.db,
                &req.email,
                &client_ip.0,
                false,
                Some("invalid mfa"),
            )
            .await
            .ok();

            let audit_claims = audit_claims_for_user(&user, &tenant_id);
            log_auth_event(
                &state.db,
                &audit_claims,
                "auth.login.mfa",
                "session",
                None,
                "failure",
                Some(if used_backup {
                    "invalid backup code"
                } else {
                    "invalid mfa code"
                }),
                Some(&client_ip.0),
            )
            .await;

            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new(if used_backup {
                        "invalid backup code"
                    } else {
                        "invalid mfa code"
                    })
                    .with_code("INVALID_MFA_CODE"),
                ),
            ));
        }
    }

    // Get admin tenant access list if user is admin
    let admin_tenants = if user.role == "admin" {
        adapteros_db::get_user_tenant_access(&state.db, &user.id)
            .await
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %tenant_id,
                    "Failed to get admin tenant access, defaulting to empty"
                );
                vec![]
            })
    } else {
        vec![]
    };

    let auth_cfg = AuthConfig::from_state(&state);
    let tenants =
        collect_tenant_summaries(&state, &user.id, &user.role, &tenant_id, &admin_tenants).await?;
    if tenants.is_empty() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("no tenant access")
                    .with_code("NO_TENANT_ACCESS")
                    .with_string_details("You are signed in but have no tenant access. Ask an admin to grant access."),
            ),
        ));
    }

    // Device + session identifiers
    let device_id = req.device_id.clone();
    let session_id = format!("sess-{}", Uuid::now_v7());
    let rot_id = format!("rot-{}", Uuid::now_v7());
    let roles_vec = vec![user.role.clone()];

    // Generate access + refresh tokens (embed session_id; refresh carries rot_id)
    let access_token = if state.use_ed25519 {
        issue_access_token_ed25519(
            &user.id,
            &user.email,
            &user.role,
            &roles_vec,
            &tenant_id,
            &admin_tenants,
            device_id.as_deref(),
            &session_id,
            mfa_level.as_deref(),
            &state.ed25519_keypair,
            Some(auth_cfg.access_ttl()),
        )
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %user.id,
                tenant_id = %tenant_id,
                session_id = %session_id,
                "Failed to generate access token"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        })?
    } else {
        issue_access_token_hmac(
            &user.id,
            &user.email,
            &user.role,
            &roles_vec,
            &tenant_id,
            &admin_tenants,
            device_id.as_deref(),
            &session_id,
            mfa_level.as_deref(),
            &state.jwt_secret,
            Some(auth_cfg.access_ttl()),
        )
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %user.id,
                tenant_id = %tenant_id,
                session_id = %session_id,
                "Failed to generate access token"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        })?
    };

    let kv_repo = state.db.kv_backend().map(|kv| {
        let backend: std::sync::Arc<dyn adapteros_db::KvBackend> = kv.clone();
        AuthSessionKvRepository::new(backend)
    });

    let (refresh_token, refresh_exp_ts, refresh_hash) = if state.use_ed25519 {
        if let Some(repo) = kv_repo.as_ref() {
            issue_refresh_token_ed25519_with_kv(
                repo,
                &user.id,
                &tenant_id,
                &roles_vec,
                device_id.as_deref(),
                &session_id,
                &rot_id,
                &state.ed25519_keypair,
                Some(auth_cfg.effective_ttl()),
                Some(&client_ip.0),
                user_agent.as_deref(),
            )
            .await
            .map_err(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %tenant_id,
                    session_id = %session_id,
                    "Failed to generate refresh token"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
                )
            })?
        } else {
            let token = issue_refresh_token_ed25519(
                &user.id,
                &tenant_id,
                &roles_vec,
                device_id.as_deref(),
                &session_id,
                &rot_id,
                &state.ed25519_keypair,
                Some(auth_cfg.effective_ttl()),
            )
            .map_err(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %tenant_id,
                    session_id = %session_id,
                    "Failed to generate refresh token"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
                )
            })?;
            let claims = validate_refresh_token_ed25519(
                &token,
                &state.ed25519_public_keys,
                &state.ed25519_public_key,
            )
            .map_err(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %tenant_id,
                    session_id = %session_id,
                    "Failed to validate generated refresh token"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
                )
            })?;
            let hash = blake3::hash(token.as_bytes()).to_hex().to_string();
            (token, claims.exp, hash)
        }
    } else if let Some(repo) = kv_repo.as_ref() {
        issue_refresh_token_hmac_with_kv(
            repo,
            &user.id,
            &tenant_id,
            &roles_vec,
            device_id.as_deref(),
            &session_id,
            &rot_id,
            &state.jwt_secret,
            Some(auth_cfg.effective_ttl()),
            Some(&client_ip.0),
            user_agent.as_deref(),
        )
        .await
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %user.id,
                tenant_id = %tenant_id,
                session_id = %session_id,
                "Failed to generate refresh token"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        })?
    } else {
        let token = issue_refresh_token_hmac(
            &user.id,
            &tenant_id,
            &roles_vec,
            device_id.as_deref(),
            &session_id,
            &rot_id,
            &state.jwt_secret,
            Some(auth_cfg.effective_ttl()),
        )
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %user.id,
                tenant_id = %tenant_id,
                session_id = %session_id,
                "Failed to generate refresh token"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        })?;
        let claims = validate_refresh_token_hmac(&token, &state.hmac_keys, &state.jwt_secret)
            .map_err(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %tenant_id,
                    session_id = %session_id,
                    "Failed to validate generated refresh token"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
                )
            })?;
        let hash = blake3::hash(token.as_bytes()).to_hex().to_string();
        (token, claims.exp, hash)
    };

    let refresh_expires_at = Utc
        .timestamp_opt(refresh_exp_ts, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339();

    // Session expiry uses the longer session TTL
    let session_expires_at =
        (Utc::now() + Duration::seconds(auth_cfg.effective_ttl() as i64)).to_rfc3339();

    // Decode to get jti and exp
    let claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &access_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(&access_token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %tenant_id,
            session_id = %session_id,
            "Token validation failed after generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Persist session with device binding metadata
    upsert_user_session(
        &state.db,
        &session_id,
        &user.id,
        &tenant_id,
        device_id.as_deref(),
        Some(&rot_id),
        Some(&refresh_hash),
        &session_expires_at,
        &refresh_expires_at,
        Some(&client_ip.0),
        user_agent.as_deref(),
        false,
    )
    .await
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %tenant_id,
            session_id = %session_id,
            "Failed to create session - login aborted"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    // Track successful auth (best effort, doesn't fail login)
    track_auth_attempt(&state.db, &req.email, &client_ip.0, true, None)
        .await
        .ok();

    // Update last login timestamp (best effort, doesn't fail login)
    let now = chrono::Utc::now().to_rfc3339();
    state.db.update_user_last_login(&user.id, &now).await.ok();

    // Log audit (best effort, doesn't fail login)
    log_auth_event(
        &state.db,
        &claims,
        "auth.session_created",
        "session",
        Some(&claims.jti),
        "success",
        None,
        Some(&client_ip.0),
    )
    .await;
    log_auth_event(
        &state.db,
        &claims,
        "auth.login_success",
        "session",
        Some(&claims.jti),
        "success",
        None,
        Some(&client_ip.0),
    )
    .await;

    emit_auth_event(&state, &user.id, &tenant_id, "login", true, None).await;

    info!(
        user_id = %user.id,
        email = %user.email,
        role = %user.role,
        tenant_id = %tenant_id,
        ip = %client_ip.0,
        "User logged in"
    );

    // Attach auth cookie for browser-based authentication
    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &access_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %tenant_id,
            session_id = %session_id,
            "Failed to attach auth cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut response_headers, &refresh_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %tenant_id,
            session_id = %session_id,
            "Failed to attach refresh cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;
    let csrf_token = uuid::Uuid::new_v4().to_string();
    attach_csrf_cookie(
        &mut response_headers,
        &csrf_token,
        &auth_cfg,
        auth_cfg.effective_ttl(),
    )
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %tenant_id,
            session_id = %session_id,
            "Failed to attach csrf cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;

    Ok((
        response_headers,
        Json(LoginResponse {
            schema_version: "v1".to_string(),
            token: access_token,
            user_id: user.id,
            tenant_id: tenant_id.clone(),
            role: user.role,
            expires_in: auth_cfg.access_ttl(),
            tenants: Some(tenants),
            mfa_level,
        }),
    ))
}

/// Logout handler - revokes current token
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    responses(
        (status = 200, description = "Logout successful", body = LogoutResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal error")
    ),
    tag = "auth",
    security(("bearerAuth" = []))
)]
pub async fn logout_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<(HeaderMap, Json<LogoutResponse>), (StatusCode, Json<ErrorResponse>)> {
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
        warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            jti = %claims.jti,
            "Failed to revoke token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("logout failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    if let Some(session_id) = claims.session_id.as_ref() {
        if let Err(e) = lock_session(&state.db, session_id).await {
            warn!(
                error = %e,
                user_id = %claims.sub,
                tenant_id = %claims.tenant_id,
                session_id = %session_id,
                "Failed to lock session during logout"
            );
        }

        if let Some(repo) = state.db.kv_backend().map(|kv| {
            let backend: std::sync::Arc<dyn adapteros_db::KvBackend> = kv.clone();
            AuthSessionKvRepository::new(backend)
        }) {
            if let Err(e) = repo.lock_session(session_id).await {
                warn!(
                    error = %e,
                    user_id = %claims.sub,
                    tenant_id = %claims.tenant_id,
                    session_id = %session_id,
                    "Failed to lock KV session during logout"
                );
            }
        }
    }

    state
        .db
        .log_audit(
            &claims.sub,
            &claims.role,
            &claims.tenant_id,
            "auth.session_revoked",
            "session",
            Some(&claims.jti),
            "success",
            None,
            None,
            None,
        )
        .await
        .ok();

    emit_auth_event(&state, &claims.sub, &claims.tenant_id, "logout", true, None).await;

    info!(user_id = %claims.sub, jti = %claims.jti, "User logged out");

    // Clear cookies
    let auth_cfg = AuthConfig::from_state(&state);
    let mut headers = HeaderMap::new();
    clear_auth_cookies(&mut headers, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            jti = %claims.jti,
            "Failed to clear cookies on logout"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("logout failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    Ok((
        headers,
        Json(LogoutResponse {
            message: "Logged out successfully".to_string(),
        }),
    ))
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
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<RefreshResponse>), (StatusCode, Json<ErrorResponse>)> {
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Extract client IP for audit logging
    let client_ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());

    let refresh_token = match extract_cookie_token(&headers, "refresh_token") {
        Some(token) => token,
        None => {
            log_refresh_failure(
                &state.db,
                None,
                None,
                None,
                "MISSING_TOKEN",
                "refresh token missing",
                client_ip.as_deref(),
            )
            .await;
            emit_refresh_failure(&state, None, None, "MISSING_TOKEN").await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("session expired")
                        .with_code("SESSION_EXPIRED")
                        .with_string_details("refresh token missing"),
                ),
            ));
        }
    };

    let refresh_claims = match if state.use_ed25519 {
        validate_refresh_token_ed25519(
            &refresh_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        validate_refresh_token_hmac(
            &refresh_token,
            &state.hmac_keys,
            state.jwt_secret.as_slice(),
        )
    } {
        Ok(claims) => claims,
        Err(e) => {
            warn!(
                error = %e,
                client_ip = %client_ip.as_deref().unwrap_or("unknown"),
                "Failed to validate refresh token"
            );
            log_refresh_failure(
                &state.db,
                None,
                None,
                None,
                "INVALID_TOKEN",
                "refresh token invalid or expired",
                client_ip.as_deref(),
            )
            .await;
            emit_refresh_failure(&state, None, None, "INVALID_TOKEN").await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("session expired")
                        .with_code("SESSION_EXPIRED")
                        .with_string_details("refresh token invalid or expired"),
                ),
            ));
        }
    };

    let session_id = refresh_claims.session_id.clone();
    if session_id.is_empty() {
        log_refresh_failure(
            &state.db,
            None,
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "MISSING_SESSION_ID",
            "missing session_id in token",
            client_ip.as_deref(),
        )
        .await;
        emit_refresh_failure(
            &state,
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "MISSING_SESSION_ID",
        )
        .await;
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("session expired")
                    .with_code("SESSION_EXPIRED")
                    .with_string_details("missing session_id"),
            ),
        ));
    }

    let token_device = refresh_claims.device_id.clone();
    let incoming_rot = refresh_claims.rot_id.clone();
    let refresh_hash = blake3::hash(refresh_token.as_bytes()).to_hex().to_string();
    let now_ts = Utc::now().timestamp();

    // Enforce tenant token baseline (refresh issued_at must meet baseline)
    if let Some(baseline) = get_tenant_token_baseline(&state.db, &refresh_claims.tenant_id)
        .await
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %refresh_claims.sub,
                tenant_id = %refresh_claims.tenant_id,
                session_id = %session_id,
                "Failed to load tenant token baseline during refresh"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?
    {
        if let Ok(baseline_dt) = chrono::DateTime::parse_from_rfc3339(&baseline) {
            if refresh_claims.iat < baseline_dt.timestamp() {
                log_refresh_failure(
                    &state.db,
                    Some(&session_id),
                    Some(&refresh_claims.sub),
                    Some(&refresh_claims.tenant_id),
                    "BASELINE_VIOLATION",
                    "refresh issued before tenant baseline",
                    client_ip.as_deref(),
                )
                .await;
                emit_refresh_failure(
                    &state,
                    Some(&refresh_claims.sub),
                    Some(&refresh_claims.tenant_id),
                    "BASELINE_VIOLATION",
                )
                .await;
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(
                        ErrorResponse::new("session expired")
                            .with_code("SESSION_EXPIRED")
                            .with_string_details("refresh issued before tenant baseline"),
                    ),
                ));
            }
        }
    }

    // Ensure session hasn't been explicitly revoked
    let revoked = is_token_revoked(&state.db, &session_id)
        .await
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %refresh_claims.sub,
                tenant_id = %refresh_claims.tenant_id,
                session_id = %session_id,
                "Failed to check revocation during refresh"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;
    if revoked {
        log_refresh_failure(
            &state.db,
            Some(&session_id),
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "TOKEN_REVOKED",
            "session revoked",
            client_ip.as_deref(),
        )
        .await;
        emit_refresh_failure(
            &state,
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "TOKEN_REVOKED",
        )
        .await;
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("token revoked")
                    .with_code("TOKEN_REVOKED")
                    .with_string_details("session revoked"),
            ),
        ));
    }

    let session = match get_session_by_id(&state.db, &session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            log_refresh_failure(
                &state.db,
                Some(&session_id),
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "SESSION_NOT_FOUND",
                "session not found",
                client_ip.as_deref(),
            )
            .await;
            emit_refresh_failure(
                &state,
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "SESSION_NOT_FOUND",
            )
            .await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("session expired")
                        .with_code("SESSION_EXPIRED")
                        .with_string_details("session not found"),
                ),
            ));
        }
        Err(e) => {
            warn!(
                error = %e,
                user_id = %refresh_claims.sub,
                tenant_id = %refresh_claims.tenant_id,
                session_id = %session_id,
                "Failed to load session for refresh"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            ));
        }
    };

    if session.locked != 0 {
        log_refresh_failure(
            &state.db,
            Some(&session_id),
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "SESSION_LOCKED",
            "session locked",
            client_ip.as_deref(),
        )
        .await;
        emit_refresh_failure(
            &state,
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "SESSION_LOCKED",
        )
        .await;
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("session expired")
                    .with_code("SESSION_EXPIRED")
                    .with_string_details("session locked"),
            ),
        ));
    }

    if let (Some(token_device), Some(session_device)) =
        (token_device.as_ref(), session.device_id.as_ref())
    {
        if token_device != session_device {
            warn!(
                session_id = %session_id,
                user_id = %refresh_claims.sub,
                tenant_id = %refresh_claims.tenant_id,
                token_device = %token_device,
                session_device = %session_device,
                "Device mismatch on refresh"
            );
            log_refresh_failure(
                &state.db,
                Some(&session_id),
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "DEVICE_MISMATCH",
                "device mismatch",
                client_ip.as_deref(),
            )
            .await;
            emit_refresh_failure(
                &state,
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "DEVICE_MISMATCH",
            )
            .await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("unauthorized")
                        .with_code("UNAUTHORIZED")
                        .with_string_details("device mismatch"),
                ),
            ));
        }
    }

    if let Some(stored_rot) = session.rot_id.as_ref() {
        if stored_rot != &incoming_rot {
            warn!(
                session_id = %session_id,
                user_id = %refresh_claims.sub,
                tenant_id = %refresh_claims.tenant_id,
                "Rotation id mismatch on refresh"
            );
            log_refresh_failure(
                &state.db,
                Some(&session_id),
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "ROTATION_MISMATCH",
                "rotation mismatch",
                client_ip.as_deref(),
            )
            .await;
            emit_refresh_failure(
                &state,
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "ROTATION_MISMATCH",
            )
            .await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("unauthorized")
                        .with_code("UNAUTHORIZED")
                        .with_string_details("rotation mismatch"),
                ),
            ));
        }
    }

    if let Some(stored_hash) = session.refresh_hash.as_ref() {
        if stored_hash != &refresh_hash {
            warn!(
                session_id = %session_id,
                user_id = %refresh_claims.sub,
                tenant_id = %refresh_claims.tenant_id,
                "Refresh hash mismatch"
            );
            log_refresh_failure(
                &state.db,
                Some(&session_id),
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "HASH_MISMATCH",
                "refresh mismatch",
                client_ip.as_deref(),
            )
            .await;
            emit_refresh_failure(
                &state,
                Some(&refresh_claims.sub),
                Some(&refresh_claims.tenant_id),
                "HASH_MISMATCH",
            )
            .await;
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("unauthorized")
                        .with_code("UNAUTHORIZED")
                        .with_string_details("refresh mismatch"),
                ),
            ));
        }
    }

    let session_exp_ts = session
        .refresh_expires_at
        .as_ref()
        .or(Some(&session.expires_at))
        .and_then(|dt| chrono::DateTime::parse_from_rfc3339(dt).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(refresh_claims.exp);

    if refresh_claims.exp <= now_ts || session_exp_ts <= now_ts {
        log_refresh_failure(
            &state.db,
            Some(&session_id),
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "SESSION_EXPIRED",
            "session expired",
            client_ip.as_deref(),
        )
        .await;
        emit_refresh_failure(
            &state,
            Some(&refresh_claims.sub),
            Some(&refresh_claims.tenant_id),
            "SESSION_EXPIRED",
        )
        .await;
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("session expired")
                    .with_code("SESSION_EXPIRED")
                    .with_string_details("session expired"),
            ),
        ));
    }

    // Load user to recover role/email/admin tenants for fresh tokens
    let user = match state.db.get_user(&refresh_claims.sub).await {
        Ok(Some(u)) => u,
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("unauthorized").with_code("UNAUTHORIZED")),
            ))
        }
    };

    let admin_tenants = if user.role == "admin" {
        adapteros_db::get_user_tenant_access(&state.db, &user.id)
            .await
            .unwrap_or_default()
    } else {
        vec![]
    };

    let roles_vec = if refresh_claims.roles.is_empty() {
        vec![user.role.clone()]
    } else {
        refresh_claims.roles.clone()
    };
    let device_id = token_device.or(session.device_id.clone());
    let new_rot_id = format!("rot-{}", Uuid::now_v7());

    // Generate fresh tokens
    let auth_cfg = AuthConfig::from_state(&state);

    let new_access_token = if state.use_ed25519 {
        issue_access_token_ed25519(
            &user.id,
            &user.email,
            &user.role,
            &roles_vec,
            &refresh_claims.tenant_id,
            &admin_tenants,
            device_id.as_deref(),
            &session_id,
            None,
            &state.ed25519_keypair,
            Some(auth_cfg.access_ttl()),
        )
    } else {
        issue_access_token_hmac(
            &user.id,
            &user.email,
            &user.role,
            &roles_vec,
            &refresh_claims.tenant_id,
            &admin_tenants,
            device_id.as_deref(),
            &session_id,
            None,
            &state.jwt_secret,
            Some(auth_cfg.access_ttl()),
        )
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %refresh_claims.tenant_id,
            session_id = %session_id,
            "Failed to generate refreshed access token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let kv_repo = state.db.kv_backend().map(|kv| {
        let backend: std::sync::Arc<dyn adapteros_db::KvBackend> = kv.clone();
        AuthSessionKvRepository::new(backend)
    });

    let (new_refresh_token, new_refresh_exp_ts, new_refresh_hash) = if state.use_ed25519 {
        if let Some(repo) = kv_repo.as_ref() {
            match issue_refresh_token_ed25519_with_kv(
                repo,
                &user.id,
                &refresh_claims.tenant_id,
                &roles_vec,
                device_id.as_deref(),
                &session_id,
                &new_rot_id,
                &state.ed25519_keypair,
                Some(auth_cfg.effective_ttl()),
                None,
                user_agent.as_deref(),
            )
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        error = %e,
                        user_id = %user.id,
                        tenant_id = %refresh_claims.tenant_id,
                        session_id = %session_id,
                        "Failed to generate refreshed session token"
                    );
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR"),
                        ),
                    ));
                }
            }
        } else {
            let token = issue_refresh_token_ed25519(
                &user.id,
                &refresh_claims.tenant_id,
                &roles_vec,
                device_id.as_deref(),
                &session_id,
                &new_rot_id,
                &state.ed25519_keypair,
                Some(auth_cfg.effective_ttl()),
            )
            .map_err(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %refresh_claims.tenant_id,
                    session_id = %session_id,
                    "Failed to generate refreshed session token"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
                )
            })?;
            let claims = validate_refresh_token_ed25519(
                &token,
                &state.ed25519_public_keys,
                &state.ed25519_public_key,
            )
            .map_err(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %refresh_claims.tenant_id,
                    session_id = %session_id,
                    "Failed to validate refreshed token"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
                )
            })?;
            let hash = blake3::hash(token.as_bytes()).to_hex().to_string();
            (token, claims.exp, hash)
        }
    } else if let Some(repo) = kv_repo.as_ref() {
        match issue_refresh_token_hmac_with_kv(
            repo,
            &user.id,
            &refresh_claims.tenant_id,
            &roles_vec,
            device_id.as_deref(),
            &session_id,
            &new_rot_id,
            &state.jwt_secret,
            Some(auth_cfg.effective_ttl()),
            None,
            user_agent.as_deref(),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %refresh_claims.tenant_id,
                    session_id = %session_id,
                    "Failed to generate refreshed session token"
                );
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
                ));
            }
        }
    } else {
        let token = issue_refresh_token_hmac(
            &user.id,
            &refresh_claims.tenant_id,
            &roles_vec,
            device_id.as_deref(),
            &session_id,
            &new_rot_id,
            &state.jwt_secret,
            Some(auth_cfg.effective_ttl()),
        )
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %user.id,
                tenant_id = %refresh_claims.tenant_id,
                session_id = %session_id,
                "Failed to generate refreshed session token"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
            )
        })?;
        let claims = validate_refresh_token_hmac(&token, &state.hmac_keys, &state.jwt_secret)
            .map_err(|e| {
                warn!(
                    error = %e,
                    user_id = %user.id,
                    tenant_id = %refresh_claims.tenant_id,
                    session_id = %session_id,
                    "Failed to validate refreshed token"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token refresh failed").with_code("INTERNAL_ERROR")),
                )
            })?;
        let hash = blake3::hash(token.as_bytes()).to_hex().to_string();
        (token, claims.exp, hash)
    };

    let refresh_expires_at = Utc
        .timestamp_opt(new_refresh_exp_ts, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339();

    // Session expiry uses the longer session TTL
    let session_expires_at =
        (Utc::now() + Duration::seconds(auth_cfg.effective_ttl() as i64)).to_rfc3339();

    // Persist rotation
    upsert_user_session(
        &state.db,
        &session_id,
        &user.id,
        &refresh_claims.tenant_id,
        device_id.as_deref(),
        Some(&new_rot_id),
        Some(&new_refresh_hash),
        &session_expires_at,
        &refresh_expires_at,
        None,
        user_agent.as_deref(),
        false,
    )
    .await
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %refresh_claims.tenant_id,
            session_id = %session_id,
            "Failed to persist rotated session"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    if let Some(repo) = kv_repo.as_ref() {
        if let Err(e) = repo
            .rotate_session(
                &session_id,
                &new_rot_id,
                Some(&new_refresh_hash),
                new_refresh_exp_ts,
            )
            .await
        {
            warn!(
                error = %e,
                user_id = %user.id,
                tenant_id = %refresh_claims.tenant_id,
                session_id = %session_id,
                "Failed to rotate KV session"
            );
        }
    }

    let new_access_claims = match if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &new_access_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(
            &new_access_token,
            &state.hmac_keys,
            state.jwt_secret.as_slice(),
        )
    } {
        Ok(claims) => claims,
        Err(e) => {
            warn!(
                error = %e,
                user_id = %user.id,
                tenant_id = %refresh_claims.tenant_id,
                session_id = %session_id,
                "Token validation failed after refresh"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            ));
        }
    };

    log_auth_event(
        &state.db,
        &new_access_claims,
        "auth.session_refreshed",
        "session",
        Some(&session_id),
        "success",
        None,
        None,
    )
    .await;

    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &new_access_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %refresh_claims.tenant_id,
            session_id = %session_id,
            "Failed to attach refreshed auth cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut response_headers, &new_refresh_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %refresh_claims.tenant_id,
            session_id = %session_id,
            "Failed to attach refreshed session cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;
    let csrf_token = uuid::Uuid::new_v4().to_string();
    attach_csrf_cookie(
        &mut response_headers,
        &csrf_token,
        &auth_cfg,
        auth_cfg.effective_ttl(),
    )
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %refresh_claims.tenant_id,
            session_id = %session_id,
            "Failed to attach csrf cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;

    emit_auth_event(
        &state,
        &refresh_claims.sub,
        &refresh_claims.tenant_id,
        "refresh",
        true,
        None,
    )
    .await;

    info!(
        user_id = %refresh_claims.sub,
        session_id = %session_id,
        new_rot_id = %new_rot_id,
        "Token refreshed"
    );

    Ok((
        response_headers,
        Json(RefreshResponse {
            token: new_access_token,
            expires_at: new_access_claims.exp,
        }),
    ))
}

/// Auth subsystem health check
#[utoipa::path(
    get,
    path = "/v1/auth/health",
    responses(
        (status = 200, description = "Auth health", body = AuthHealthResponse),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn auth_health_handler(
    State(state): State<AppState>,
) -> Result<Json<AuthHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    let db_status = if let Some(pool) = state.db.pool_opt() {
        match sqlx::query("SELECT 1").fetch_one(pool).await {
            Ok(_) => "ok".to_string(),
            Err(e) => {
                warn!(error = %e, "DB health check failed for auth health");
                "unhealthy".to_string()
            }
        }
    } else {
        "unknown".to_string()
    };

    let signing_keys = if state.use_ed25519 {
        "eddsa".to_string()
    } else {
        "hmac".to_string()
    };

    Ok(Json(AuthHealthResponse {
        status: "ok".to_string(),
        db: db_status,
        signing_keys,
        idp_configured: false,
    }))
}

/// List tenants the current user can access (for tenant picker)
#[utoipa::path(
    get,
    path = "/v1/auth/tenants",
    responses(
        (status = 200, description = "User tenants", body = TenantListResponse),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn list_user_tenants_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<TenantListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenants = collect_tenant_summaries(
        &state,
        &claims.sub,
        &claims.role,
        &claims.tenant_id,
        &claims.admin_tenants,
    )
    .await?;

    Ok(Json(TenantListResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenants,
    }))
}

/// Switch active tenant (re-issue access + refresh tokens)
#[utoipa::path(
    post,
    path = "/v1/auth/tenants/switch",
    request_body = SwitchTenantRequest,
    responses(
        (status = 200, description = "Tenant switched", body = SwitchTenantResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Tenant access denied")
    ),
    tag = "auth"
)]
pub async fn switch_tenant_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<SwitchTenantRequest>,
) -> Result<(HeaderMap, Json<SwitchTenantResponse>), (StatusCode, Json<ErrorResponse>)> {
    let target_tenant = req.tenant_id;
    let dev_no_auth = cfg!(debug_assertions) && std::env::var("AOS_DEV_NO_AUTH").is_ok();

    // Fast path: same tenant
    if target_tenant == claims.tenant_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("already on tenant")
                    .with_code("TENANT_ALREADY_ACTIVE")
                    .with_string_details("requested tenant is already active"),
            ),
        ));
    }

    // Verify access
    let mut allowed = false;
    if claims.role == "admin" {
        if claims
            .admin_tenants
            .iter()
            .any(|t| t == ADMIN_TENANT_WILDCARD)
            || claims.admin_tenants.contains(&target_tenant)
        {
            allowed = true;
        } else if let Ok(grants) =
            adapteros_db::get_user_tenant_access(&state.db, &claims.sub).await
        {
            allowed = grants.contains(&target_tenant);
        }
    } else if claims.tenant_id == target_tenant {
        allowed = true;
    }

    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("tenant access denied")
                    .with_code("TENANT_ACCESS_DENIED")
                    .with_failure_code(adapteros_api_types::FailureCode::TenantAccessDenied)
                    .with_string_details(
                        "You have no role in this tenant. Request access from an admin.",
                    ),
            ),
        ));
    }

    // Dev no-auth bypass: synthesize a response without hitting user DB/session tables.
    if dev_no_auth {
        let tenants = collect_tenant_summaries(
            &state,
            &claims.sub,
            &claims.role,
            &target_tenant,
            &claims.admin_tenants,
        )
        .await?;

        let headers = HeaderMap::new();
        return Ok((
            headers,
            Json(SwitchTenantResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                token: "dev-no-auth".to_string(),
                user_id: claims.sub.clone(),
                tenant_id: target_tenant,
                role: claims.role.clone(),
                expires_in: (claims.exp - claims.iat).max(0) as u64,
                tenants: Some(tenants),
                mfa_level: None,
            }),
        ));
    }

    // Load user to get role/email (authoritative)
    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %claims.sub,
                target_tenant = %target_tenant,
                "Failed to load user for tenant switch"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("user not found").with_code("UNAUTHORIZED")),
            )
        })?;

    let auth_cfg = AuthConfig::from_state(&state);

    let admin_tenants = if user.role == "admin" {
        adapteros_db::get_user_tenant_access(&state.db, &user.id)
            .await
            .unwrap_or_else(|_| claims.admin_tenants.clone())
    } else {
        vec![]
    };

    let access_token = if state.use_ed25519 {
        generate_token_ed25519_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.ed25519_keypair,
            auth_cfg.access_ttl(),
        )
    } else {
        generate_token_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.jwt_secret,
            auth_cfg.access_ttl(),
        )
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to generate access token for tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let refresh_token = if state.use_ed25519 {
        generate_token_ed25519_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.ed25519_keypair,
            auth_cfg.effective_ttl(),
        )
    } else {
        generate_token_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.jwt_secret,
            auth_cfg.effective_ttl(),
        )
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to generate refresh token for tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let access_claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &access_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(&access_token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Token validation failed after tenant switch generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let expires_at = Utc::now() + Duration::seconds(auth_cfg.effective_ttl() as i64);
    if let Err(e) = create_session(
        &state.db,
        &access_claims.jti,
        &user.id,
        &target_tenant,
        &expires_at.to_rfc3339(),
        None,
        None,
    )
    .await
    {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to create session during tenant switch"
        );
    }

    let tenants =
        collect_tenant_summaries(&state, &user.id, &user.role, &target_tenant, &admin_tenants)
            .await?;

    let mut headers = HeaderMap::new();
    attach_auth_cookie(&mut headers, &access_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to attach auth cookie during tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut headers, &refresh_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to attach refresh cookie during tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;

    Ok((
        headers,
        Json(SwitchTenantResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            token: access_token,
            user_id: user.id,
            tenant_id: target_tenant,
            role: user.role,
            expires_in: auth_cfg.access_ttl(),
            tenants: Some(tenants),
            mfa_level: None,
        }),
    ))
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
            warn!(
                error = %e,
                user_id = %claims.sub,
                tenant_id = %claims.tenant_id,
                "Failed to get user sessions"
            );
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
            warn!(
                error = %e,
                user_id = %claims.sub,
                tenant_id = %claims.tenant_id,
                "Failed to get user sessions"
            );
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
        warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            session_id = %jti,
            "Failed to revoke session"
        );
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

    if let Err(e) = lock_session(&state.db, &jti).await {
        warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            session_id = %jti,
            "Failed to lock revoked session"
        );
    }
    if let Some(repo) = state.db.kv_backend().map(|kv| {
        let backend: std::sync::Arc<dyn adapteros_db::KvBackend> = kv.clone();
        AuthSessionKvRepository::new(backend)
    }) {
        if let Err(e) = repo.lock_session(&jti).await {
            warn!(
                error = %e,
                user_id = %claims.sub,
                tenant_id = %claims.tenant_id,
                session_id = %jti,
                "Failed to lock revoked KV session"
            );
        }
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
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: String::new(),
            nbf: 0,
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::DevBypass,
            principal_type: Some(PrincipalType::DevBypass),
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
            warn!(
                error = %e,
                user_id = %user_id,
                tenant_id = %tenant_id,
                "Failed to ensure dev user exists in database, continuing anyway"
            );
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
        failed_attempts: 0,
        last_failed_at: None,
        lockout_until: None,
        mfa_enabled: false,
        mfa_secret_enc: None,
        mfa_backup_codes_json: None,
        mfa_enrolled_at: None,
        mfa_last_verified_at: None,
        mfa_recovery_last_used_at: None,
        password_rotated_at: None,
        token_rotated_at: None,
        last_login_at: Some(Utc::now().to_rfc3339()),
    };

    let mut ctx = AuthContext::from_user(dev_user).map_err(|err| {
        warn!(
            error = %err,
            user_id = %user_id,
            tenant_id = %tenant_id,
            "Failed to build dev auth context"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    if role == "admin" {
        ctx.admin_tenants = vec![ADMIN_TENANT_WILDCARD.to_string()];
    }

    let session_id = format!("sess-dev-{}", Uuid::now_v7());
    let roles_vec = vec![ctx.role.to_string()];

    let token = if state.use_ed25519 {
        issue_access_token_ed25519(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &roles_vec,
            &ctx.tenant_id,
            &ctx.admin_tenants,
            None,
            &session_id,
            None,
            &state.ed25519_keypair,
            Some(auth_cfg.access_ttl()),
        )
    } else {
        issue_access_token_hmac(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &roles_vec,
            &ctx.tenant_id,
            &ctx.admin_tenants,
            None,
            &session_id,
            None,
            &state.jwt_secret,
            Some(auth_cfg.access_ttl()),
        )
    }
    .map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to generate dev bypass token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let rot_id = format!("rot-{}", Uuid::now_v7());
    let refresh_token = if state.use_ed25519 {
        issue_refresh_token_ed25519(
            &ctx.user.id,
            &ctx.tenant_id,
            &roles_vec,
            None,
            &session_id,
            &rot_id,
            &state.ed25519_keypair,
            Some(auth_cfg.effective_ttl()),
        )
    } else {
        issue_refresh_token_hmac(
            &ctx.user.id,
            &ctx.tenant_id,
            &roles_vec,
            None,
            &session_id,
            &rot_id,
            &state.jwt_secret,
            Some(auth_cfg.effective_ttl()),
        )
    }
    .map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to generate dev bypass refresh token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let refresh_claims = if state.use_ed25519 {
        validate_refresh_token_ed25519(
            &refresh_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        validate_refresh_token_hmac(&refresh_token, &state.hmac_keys, &state.jwt_secret)
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Refresh token validation failed after generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(&token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Token validation failed after generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let response_expires_in = std::cmp::max(auth_cfg.access_ttl(), 60);
    let session_ttl = std::cmp::max(auth_cfg.effective_ttl(), response_expires_in);
    let session_expires_at = Utc::now() + Duration::seconds(session_ttl as i64);
    let refresh_expires_at = Utc
        .timestamp_opt(refresh_claims.exp, 0)
        .single()
        .unwrap_or_else(Utc::now);
    let refresh_hash = blake3::hash(refresh_token.as_bytes()).to_hex().to_string();
    upsert_user_session(
        &state.db,
        &session_id,
        &user_id,
        &tenant_id,
        None,
        Some(&rot_id),
        Some(&refresh_hash),
        &session_expires_at.to_rfc3339(),
        &refresh_expires_at.to_rfc3339(),
        Some(&client_ip.0),
        user_agent.as_deref(),
        false,
    )
    .await
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user_id,
            tenant_id = %tenant_id,
            session_id = %session_id,
            "Failed to upsert dev bypass session - aborted"
        );
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

    let admin_tenants = adapteros_db::get_user_tenant_access(&state.db, &ctx.user.id)
        .await
        .unwrap_or_default();
    let role_string = ctx.role.to_string();
    let tenants = collect_tenant_summaries(
        &state,
        &ctx.user.id,
        &role_string,
        &ctx.user.tenant_id,
        &admin_tenants,
    )
    .await?;

    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to attach auth cookie for dev bypass"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut response_headers, &refresh_token, &auth_cfg).map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to attach refresh cookie for dev bypass"
        );
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
            expires_in: response_expires_in,
            tenants: Some(tenants),
            mfa_level: None,
        }),
    ))
}

/// Request for dev bootstrap endpoint
#[cfg(all(feature = "dev-bypass", debug_assertions))]
#[derive(Debug, Deserialize, ToSchema)]
pub struct DevBootstrapRequest {
    /// Admin email (defaults to "dev-admin@adapteros.local")
    #[serde(default = "default_dev_email")]
    pub email: String,
    /// Admin password (defaults to "dev-password-123")
    #[serde(default = "default_dev_password")]
    pub password: String,
}

#[cfg(all(feature = "dev-bypass", debug_assertions))]
fn default_dev_email() -> String {
    "dev-admin@adapteros.local".to_string()
}

#[cfg(all(feature = "dev-bypass", debug_assertions))]
fn default_dev_password() -> String {
    "dev-password-123".to_string()
}

/// Response from dev bootstrap endpoint
#[cfg(all(feature = "dev-bypass", debug_assertions))]
#[derive(Debug, Serialize, ToSchema)]
pub struct DevBootstrapResponse {
    /// The system tenant ID
    pub system_tenant_id: String,
    /// The created admin user ID
    pub admin_user_id: String,
    /// Admin email
    pub email: String,
    /// Admin password (only shown once)
    pub password: String,
    /// JWT token for immediate use
    pub token: String,
    /// Instructions message
    pub message: String,
}

/// Development bootstrap endpoint - creates system tenant and admin user
///
/// This endpoint is only available in debug builds with the dev-bypass feature.
/// It creates the "system" tenant if it doesn't exist, creates an admin user,
/// grants the admin access to the system tenant, and returns a valid JWT token.
///
/// **WARNING**: This endpoint is for development only and should never be enabled in production.
#[cfg(all(feature = "dev-bypass", debug_assertions))]
#[utoipa::path(
    post,
    path = "/v1/dev/bootstrap",
    request_body = DevBootstrapRequest,
    responses(
        (status = 200, description = "Dev environment bootstrapped", body = DevBootstrapResponse),
        (status = 403, description = "Not in development mode"),
        (status = 500, description = "Internal error")
    ),
    tag = "dev"
)]
pub async fn dev_bootstrap_handler(
    State(state): State<AppState>,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<DevBootstrapRequest>,
) -> Result<(HeaderMap, Json<DevBootstrapResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);

    if !auth_cfg.dev_login_allowed() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("dev bootstrap not available")
                    .with_code("DEV_BOOTSTRAP_DISABLED")
                    .with_string_details("this endpoint is only available in development mode"),
            ),
        ));
    }

    info!(email = %req.email, ip = %client_ip.0, "Dev bootstrap requested");

    // 1. Create or get "system" tenant
    let system_tenant_id = match sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = 'system'",
    )
    .fetch_optional(state.db.pool())
    .await
    {
        Ok(Some(id)) => {
            info!(tenant_id = %id, "System tenant already exists");
            id
        }
        Ok(None) => {
            // Create system tenant with known ID "system"
            info!(tenant_id = %"system", "Creating system tenant");
            sqlx::query(
                "INSERT INTO tenants (id, name, itar_flag, created_at) VALUES ('system', 'System', 0, datetime('now'))"
            )
            .execute(state.db.pool())
            .await
            .map_err(|e| {
                warn!(
                    error = %e,
                    tenant_id = %"system",
                    "Failed to create system tenant"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Failed to create system tenant").with_code("DATABASE_ERROR")),
                )
            })?;

            // Initialize policy bindings for system tenant
            if let Err(e) = state
                .db
                .initialize_tenant_policy_bindings("system", "system")
                .await
            {
                warn!(
                    error = %e,
                    tenant_id = %"system",
                    "Failed to initialize system tenant policy bindings (non-fatal)"
                );
            }

            "system".to_string()
        }
        Err(e) => {
            warn!(error = %e, tenant_id = %"system", "Failed to check for system tenant");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
            ));
        }
    };

    // 2. Create admin user (or get existing)
    let admin_user_id = match state.db.get_user_by_email(&req.email).await {
        Ok(Some(user)) => {
            info!(email = %req.email, user_id = %user.id, "Admin user already exists");
            user.id
        }
        Ok(None) => {
            // Hash password and create user
            let pw_hash = hash_password(&req.password).map_err(|e| {
                warn!(
                    error = %e,
                    email = %req.email,
                    tenant_id = %system_tenant_id,
                    "Failed to hash password"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("password hashing failed").with_code("INTERNAL_ERROR")),
                )
            })?;

            let user_id = state
                .db
                .create_user(
                    &req.email,
                    "Dev Admin",
                    &pw_hash,
                    Role::Admin,
                    &system_tenant_id,
                )
                .await
                .map_err(|e| {
                    warn!(
                        error = %e,
                        email = %req.email,
                        tenant_id = %system_tenant_id,
                        "Failed to create admin user"
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("user creation failed").with_code("DATABASE_ERROR"),
                        ),
                    )
                })?;

            info!(user_id = %user_id, email = %req.email, "Created dev admin user");
            user_id
        }
        Err(e) => {
            warn!(
                error = %e,
                email = %req.email,
                tenant_id = %system_tenant_id,
                "Failed to check for admin user"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
            ));
        }
    };

    // 3. Grant admin access to system tenant
    if let Err(e) = adapteros_db::grant_user_tenant_access(
        &state.db,
        &admin_user_id,
        &system_tenant_id,
        "dev-bootstrap",
        Some("Dev bootstrap auto-grant"),
        None, // No expiration
    )
    .await
    {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to grant tenant access (may already exist)"
        );
        // Non-fatal - may already have access
    }

    // 4. Generate token with admin_tenants populated
    let admin_tenants = vec![system_tenant_id.clone()];

    let token = if state.use_ed25519 {
        generate_token_ed25519_with_admin_tenants(
            &admin_user_id,
            &req.email,
            "admin",
            &system_tenant_id,
            &admin_tenants,
            &state.ed25519_keypair,
            auth_cfg.effective_ttl(),
        )
    } else {
        generate_token_with_admin_tenants(
            &admin_user_id,
            &req.email,
            "admin",
            &system_tenant_id,
            &admin_tenants,
            &state.jwt_secret,
            auth_cfg.effective_ttl(),
        )
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to generate token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Validate the generated token
    let claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(&token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Token validation failed after generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Create session
    let expires_at = Utc::now() + Duration::seconds(auth_cfg.effective_ttl() as i64);
    create_session(
        &state.db,
        &claims.jti,
        &admin_user_id,
        &system_tenant_id,
        &expires_at.to_rfc3339(),
        Some(&client_ip.0),
        None,
    )
    .await
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to create session"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    // 5. Attach cookie
    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to attach auth cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;

    info!(
        admin_user_id = %admin_user_id,
        email = %req.email,
        system_tenant_id = %system_tenant_id,
        ip = %client_ip.0,
        "Dev bootstrap completed successfully"
    );

    Ok((
        response_headers,
        Json(DevBootstrapResponse {
            system_tenant_id,
            admin_user_id,
            email: req.email,
            password: req.password,
            token,
            message: "Dev environment bootstrapped. Use the token or credentials to authenticate. This endpoint is only available in debug builds.".to_string(),
        }),
    ))
}

/// Get MFA status for current user
#[utoipa::path(
    get,
    path = "/v1/auth/mfa/status",
    responses(
        (status = 200, description = "MFA status", body = MfaStatusResponse),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn mfa_status_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MfaStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("internal error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("unauthorized").with_code("UNAUTHORIZED")),
        ))?;

    Ok(Json(MfaStatusResponse {
        mfa_enabled: user.mfa_enabled,
        enrolled_at: user.mfa_enrolled_at,
    }))
}

/// Start MFA enrollment: generate secret and store encrypted pending secret.
#[utoipa::path(
    post,
    path = "/v1/auth/mfa/start",
    responses(
        (status = 200, description = "MFA enrollment started", body = MfaEnrollStartResponse),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn mfa_start_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MfaEnrollStartResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (secret_bytes, secret_b32) = generate_totp_secret();
    let key = derive_mfa_key(state.jwt_secret.as_slice());
    let encrypted_secret = encrypt_mfa_secret(&secret_bytes, &key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to prepare MFA secret")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let now = chrono::Utc::now().to_rfc3339();
    state
        .db
        .set_user_mfa_secret(&claims.sub, &encrypted_secret, &now)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to save MFA secret")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let otpauth = otpauth_uri(&claims.email, "AdapterOS", &secret_b32);

    if let Err(e) = audit_helper::log_success(
        &state.db,
        &claims,
        "auth.mfa.start",
        "user",
        Some(&claims.sub),
    )
    .await
    {
        tracing::warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Audit log failed"
        );
    }

    Ok(Json(MfaEnrollStartResponse {
        secret: secret_b32,
        otpauth_url: otpauth,
    }))
}

/// Verify MFA enrollment and return backup codes.
#[utoipa::path(
    post,
    path = "/v1/auth/mfa/verify",
    request_body = MfaEnrollVerifyRequest,
    responses(
        (status = 200, description = "MFA enabled", body = MfaEnrollVerifyResponse),
        (status = 400, description = "Invalid MFA code", body = ErrorResponse),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn mfa_verify_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<MfaEnrollVerifyRequest>,
) -> Result<Json<MfaEnrollVerifyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("internal error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("unauthorized").with_code("UNAUTHORIZED")),
        ))?;

    let secret_enc = user.mfa_secret_enc.clone().ok_or((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new("MFA not initialized").with_code("MFA_NOT_INITIALIZED")),
    ))?;

    let key = derive_mfa_key(state.jwt_secret.as_slice());
    let secret = decrypt_mfa_secret(&secret_enc, &key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to decrypt secret")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if !verify_totp(&secret, &req.totp_code) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid MFA code").with_code("INVALID_MFA_CODE")),
        ));
    }

    let backup_codes = generate_backup_codes();
    let hashed = hash_backup_codes(&backup_codes);
    let hashed_json = serde_json::to_string(&hashed).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to encode backup codes")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let now = chrono::Utc::now().to_rfc3339();
    state
        .db
        .enable_user_mfa(&claims.sub, &secret_enc, &hashed_json, &now)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to persist MFA state")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if let Err(e) = audit_helper::log_success(
        &state.db,
        &claims,
        "auth.mfa.enable",
        "user",
        Some(&claims.sub),
    )
    .await
    {
        tracing::warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Audit log failed"
        );
    }

    Ok(Json(MfaEnrollVerifyResponse { backup_codes }))
}

/// Disable MFA using TOTP or backup code.
#[utoipa::path(
    post,
    path = "/v1/auth/mfa/disable",
    request_body = MfaDisableRequest,
    responses(
        (status = 204, description = "MFA disabled"),
        (status = 400, description = "Invalid request or code", body = ErrorResponse),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn mfa_disable_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<MfaDisableRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("internal error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("unauthorized").with_code("UNAUTHORIZED")),
        ))?;

    if !user.mfa_enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("MFA not enabled").with_code("MFA_NOT_ENABLED")),
        ));
    }

    let provided = req
        .totp_code
        .as_deref()
        .or(req.backup_code.as_deref())
        .ok_or((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("MFA code required").with_code("MFA_CODE_REQUIRED")),
        ))?;

    let secret_enc = user.mfa_secret_enc.clone().ok_or((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new("MFA secret missing").with_code("MFA_SECRET_MISSING")),
    ))?;

    let key = derive_mfa_key(state.jwt_secret.as_slice());
    let secret = decrypt_mfa_secret(&secret_enc, &key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to decrypt secret")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut valid = verify_totp(&secret, provided);
    if !valid {
        if let Some(json_codes) = user.mfa_backup_codes_json.as_ref() {
            if let Ok(mut codes) = serde_json::from_str::<Vec<BackupCode>>(json_codes) {
                if verify_and_mark_backup_code(&mut codes, provided).is_some() {
                    valid = true;
                    // Persist used flag
                    let now = chrono::Utc::now().to_rfc3339();
                    let updated =
                        serde_json::to_string(&codes).unwrap_or_else(|_| json_codes.clone());
                    let _ = state
                        .db
                        .update_user_backup_codes(&claims.sub, &updated, Some(&now))
                        .await;
                }
            }
        }
    }

    if !valid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid MFA code").with_code("INVALID_MFA_CODE")),
        ));
    }

    state.db.disable_user_mfa(&claims.sub).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to disable MFA")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if let Err(e) = audit_helper::log_success(
        &state.db,
        &claims,
        "auth.mfa.disable",
        "user",
        Some(&claims.sub),
    )
    .await
    {
        tracing::warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Audit log failed"
        );
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Authentication configuration response for frontend
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuthConfigResponse {
    /// Whether user registration is allowed
    pub allow_registration: bool,
    /// Whether email verification is required
    pub require_email_verification: bool,
    /// Access token lifetime in minutes
    pub access_token_ttl_minutes: u32,
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
        access_token_ttl_minutes: (auth_cfg.access_ttl() / 60) as u32,
        session_timeout_minutes: (auth_cfg.effective_ttl() / 60) as u32,
        max_login_attempts: config.auth.lockout_threshold,
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
