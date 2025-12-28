//! Login and bootstrap handlers
//!
//! Contains the main login handler and the bootstrap admin handler.

use crate::auth::{
    hash_password, issue_access_token_ed25519, issue_access_token_hmac,
    issue_refresh_token_ed25519, issue_refresh_token_ed25519_with_kv, issue_refresh_token_hmac,
    issue_refresh_token_hmac_with_kv, validate_refresh_token_ed25519, validate_refresh_token_hmac,
    verify_password,
};
use crate::auth_common::{
    attach_auth_cookie, attach_csrf_cookie, attach_refresh_cookie, AuthConfig,
};
use crate::ip_extraction::ClientIp;
use crate::mfa::{
    decrypt_mfa_secret, derive_mfa_key, verify_and_mark_backup_code, verify_totp, BackupCode,
};
use crate::security::{check_login_lockout, track_auth_attempt, upsert_user_session};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{LoginRequest, LoginResponse};
use adapteros_db::auth_sessions_kv::AuthSessionKvRepository;
use adapteros_db::users::Role;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use blake3;
use chrono::{Duration, TimeZone, Utc};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::helpers::{audit_claims_for_user, emit_auth_event, log_auth_event};
use super::tokens::collect_tenant_summaries;
use super::types::{BootstrapRequest, BootstrapResponse};

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
    use crate::auth::{AuthMode, Claims, PrincipalType, JWT_ISSUER};

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
