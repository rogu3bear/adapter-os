//! Bootstrap and MFA handlers for the enhanced auth surface.
//!
//! This module retains the historical filename to avoid wider module churn,
//! but the handlers are fully implemented for production flows.

use crate::auth::{hash_password, Claims};
use crate::mfa::{
    decrypt_mfa_secret, derive_mfa_key, encrypt_mfa_secret, generate_backup_codes,
    generate_totp_secret, hash_backup_codes, otpauth_uri, verify_and_mark_backup_code, verify_totp,
    BackupCode,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{
    MfaDisableRequest, MfaEnrollStartResponse, MfaEnrollVerifyRequest, MfaEnrollVerifyResponse,
    MfaStatusResponse,
};
use adapteros_db::users::{Role, User};
use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::Utc;
use tracing::{info, warn};

use super::audit::{log_auth_event, AuthEvent};

const MIN_PASSWORD_LENGTH: usize = 12;
const SYSTEM_TENANT_ID: &str = "system";

fn internal_error(message: impl Into<String>, code: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new(message.into()).with_code(code)),
    )
}

async fn get_current_user(
    state: &AppState,
    user_id: &str,
) -> Result<User, (StatusCode, Json<ErrorResponse>)> {
    state
        .db
        .get_user(user_id)
        .await
        .map_err(|e| {
            warn!(error = %e, user_id = %user_id, "Failed to load current user");
            internal_error("Failed to load user", "DATABASE_ERROR")
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("User not found").with_code("USER_NOT_FOUND")),
            )
        })
}

// ============================================================================
// Bootstrap
// ============================================================================

#[utoipa::path(
    post,
    path = "/v1/auth/bootstrap",
    request_body = super::types::BootstrapRequest,
    responses(
        (status = 200, description = "Bootstrap completed", body = super::types::BootstrapResponse),
        (status = 400, description = "Invalid bootstrap payload"),
        (status = 409, description = "Bootstrap already completed"),
        (status = 500, description = "Internal bootstrap failure")
    ),
    tag = "auth"
)]
pub async fn bootstrap_admin_handler(
    State(state): State<AppState>,
    Json(req): Json<super::types::BootstrapRequest>,
) -> Result<Json<super::types::BootstrapResponse>, (StatusCode, Json<ErrorResponse>)> {
    let email = super::validation::normalize_email(&req.email);
    if !super::validation::is_valid_email(&email) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid email format").with_code("INVALID_EMAIL")),
        ));
    }

    let display_name = req.display_name.trim();
    if display_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Display name is required").with_code("INVALID_DISPLAY_NAME")),
        ));
    }

    if req.password.len() < MIN_PASSWORD_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "Password must be at least {} characters",
                    MIN_PASSWORD_LENGTH
                ))
                .with_code("WEAK_PASSWORD"),
            ),
        ));
    }

    let existing_users = state.db.count_users().await.map_err(|e| {
        warn!(error = %e, "Failed to check bootstrap status");
        internal_error("Failed to check bootstrap status", "DATABASE_ERROR")
    })?;

    if existing_users > 0 {
        log_auth_event(
            AuthEvent::BootstrapFailedAlreadyInitialized,
            None,
            Some(&email),
            Some(SYSTEM_TENANT_ID),
            None,
            None,
            Some("users_exist"),
        );
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("System has already been bootstrapped")
                    .with_code("BOOTSTRAP_ALREADY_COMPLETED"),
            ),
        ));
    }

    state.db.ensure_system_tenant().await.map_err(|e| {
        warn!(error = %e, "Failed to ensure system tenant during bootstrap");
        internal_error(
            "Failed to initialize system tenant",
            "BOOTSTRAP_TENANT_ERROR",
        )
    })?;

    let pw_hash = hash_password(&req.password).map_err(|e| {
        warn!(error = %e, "Failed to hash bootstrap admin password");
        internal_error("Failed to secure password", "PASSWORD_HASH_ERROR")
    })?;

    let user_id = state
        .db
        .create_user(
            &email,
            display_name,
            &pw_hash,
            Role::Admin,
            SYSTEM_TENANT_ID,
        )
        .await
        .map_err(|e| {
            warn!(error = %e, email = %email, "Failed to create bootstrap admin");
            internal_error("Failed to create admin user", "BOOTSTRAP_USER_ERROR")
        })?;

    if let Err(e) = adapteros_db::grant_user_tenant_access(
        &state.db,
        &user_id,
        SYSTEM_TENANT_ID,
        "bootstrap",
        Some("Initial admin bootstrap"),
        None,
    )
    .await
    {
        warn!(
            error = %e,
            user_id = %user_id,
            "Failed to grant bootstrap tenant access (continuing)"
        );
    }

    log_auth_event(
        AuthEvent::BootstrapSuccess,
        Some(&user_id),
        None,
        Some(SYSTEM_TENANT_ID),
        None,
        None,
        None,
    );

    info!(user_id = %user_id, email = %email, "Auth bootstrap completed");

    Ok(Json(super::types::BootstrapResponse {
        user_id,
        message: "Bootstrap completed".to_string(),
    }))
}

// ============================================================================
// MFA
// ============================================================================

#[utoipa::path(
    get,
    path = "/v1/auth/mfa/status",
    responses(
        (status = 200, description = "Current MFA status", body = MfaStatusResponse),
        (status = 404, description = "User not found")
    ),
    tag = "auth"
)]
pub async fn mfa_status_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MfaStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user = get_current_user(&state, &claims.sub).await?;
    Ok(Json(MfaStatusResponse {
        mfa_enabled: user.mfa_enabled,
        enrolled_at: user.mfa_enrolled_at,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/auth/mfa/start",
    responses(
        (status = 200, description = "MFA enrollment started", body = MfaEnrollStartResponse),
        (status = 404, description = "User not found"),
        (status = 500, description = "Failed to generate MFA secret")
    ),
    tag = "auth"
)]
pub async fn mfa_start_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MfaEnrollStartResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user = get_current_user(&state, &claims.sub).await?;
    let (secret_bytes, secret_b32) = generate_totp_secret();
    let mfa_key = derive_mfa_key(state.jwt_secret.as_slice());
    let encrypted = encrypt_mfa_secret(&secret_bytes, &mfa_key).map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to encrypt MFA secret");
        internal_error("Failed to initialize MFA", "MFA_SECRET_ERROR")
    })?;

    let enrolled_at = Utc::now().to_rfc3339();
    state
        .db
        .set_user_mfa_secret(&user.id, &encrypted, &enrolled_at)
        .await
        .map_err(|e| {
            warn!(error = %e, user_id = %user.id, "Failed to persist MFA secret");
            internal_error("Failed to persist MFA setup", "DATABASE_ERROR")
        })?;

    Ok(Json(MfaEnrollStartResponse {
        secret: secret_b32.clone(),
        otpauth_url: otpauth_uri(&user.email, "AdapterOS", &secret_b32),
    }))
}

#[utoipa::path(
    post,
    path = "/v1/auth/mfa/verify",
    request_body = MfaEnrollVerifyRequest,
    responses(
        (status = 200, description = "MFA enrollment verified", body = MfaEnrollVerifyResponse),
        (status = 400, description = "MFA setup not started"),
        (status = 401, description = "Invalid TOTP code"),
        (status = 404, description = "User not found")
    ),
    tag = "auth"
)]
pub async fn mfa_verify_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<MfaEnrollVerifyRequest>,
) -> Result<Json<MfaEnrollVerifyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user = get_current_user(&state, &claims.sub).await?;
    let secret_enc = user.mfa_secret_enc.clone().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("MFA enrollment has not been started")
                    .with_code("MFA_NOT_STARTED"),
            ),
        )
    })?;

    let mfa_key = derive_mfa_key(state.jwt_secret.as_slice());
    let secret = decrypt_mfa_secret(&secret_enc, &mfa_key).map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to decrypt MFA secret during verify");
        internal_error("Failed to verify MFA setup", "MFA_SECRET_ERROR")
    })?;

    if !verify_totp(&secret, &req.totp_code) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Invalid TOTP code").with_code("INVALID_MFA_CODE")),
        ));
    }

    let backup_codes = generate_backup_codes();
    let hashed_codes = hash_backup_codes(&backup_codes);
    let backup_codes_json = serde_json::to_string(&hashed_codes).map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to serialize MFA backup codes");
        internal_error("Failed to finalize MFA setup", "MFA_BACKUP_CODE_ERROR")
    })?;
    let verified_at = Utc::now().to_rfc3339();

    state
        .db
        .enable_user_mfa(&user.id, &secret_enc, &backup_codes_json, &verified_at)
        .await
        .map_err(|e| {
            warn!(error = %e, user_id = %user.id, "Failed to enable MFA");
            internal_error("Failed to enable MFA", "DATABASE_ERROR")
        })?;

    Ok(Json(MfaEnrollVerifyResponse { backup_codes }))
}

#[utoipa::path(
    post,
    path = "/v1/auth/mfa/disable",
    request_body = MfaDisableRequest,
    responses(
        (status = 200, description = "MFA disabled", body = MfaStatusResponse),
        (status = 400, description = "No verification code provided"),
        (status = 401, description = "Invalid verification code"),
        (status = 404, description = "User not found")
    ),
    tag = "auth"
)]
pub async fn mfa_disable_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    payload: Option<Json<MfaDisableRequest>>,
) -> Result<Json<MfaStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let req = payload.map(|Json(body)| body).unwrap_or(MfaDisableRequest {
        totp_code: None,
        backup_code: None,
    });
    let user = get_current_user(&state, &claims.sub).await?;

    if !user.mfa_enabled {
        return Ok(Json(MfaStatusResponse {
            mfa_enabled: false,
            enrolled_at: None,
        }));
    }

    let verified = if let Some(totp_code) = req.totp_code.as_deref() {
        let secret_enc = user.mfa_secret_enc.clone().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("MFA secret is missing").with_code("MFA_NOT_STARTED")),
            )
        })?;
        let mfa_key = derive_mfa_key(state.jwt_secret.as_slice());
        let secret = decrypt_mfa_secret(&secret_enc, &mfa_key).map_err(|e| {
            warn!(error = %e, user_id = %user.id, "Failed to decrypt MFA secret during disable");
            internal_error("Failed to verify MFA code", "MFA_SECRET_ERROR")
        })?;
        verify_totp(&secret, totp_code)
    } else if let Some(backup_code) = req.backup_code.as_deref() {
        let mut codes: Vec<BackupCode> = user
            .mfa_backup_codes_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| {
                warn!(error = %e, user_id = %user.id, "Failed to parse backup codes");
                internal_error("Failed to verify backup code", "MFA_BACKUP_CODE_ERROR")
            })?
            .unwrap_or_default();

        verify_and_mark_backup_code(&mut codes, backup_code).is_some()
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Provide either a TOTP code or backup code")
                    .with_code("MISSING_MFA_VERIFICATION"),
            ),
        ));
    };

    if !verified {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Invalid MFA verification code").with_code("INVALID_MFA_CODE")),
        ));
    }

    state.db.disable_user_mfa(&user.id).await.map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to disable MFA");
        internal_error("Failed to disable MFA", "DATABASE_ERROR")
    })?;

    Ok(Json(MfaStatusResponse {
        mfa_enabled: false,
        enrolled_at: None,
    }))
}

// ============================================================================
// Dev bypass stubs (when dev-bypass feature is disabled)
// ============================================================================

#[cfg(not(all(feature = "dev-bypass", debug_assertions)))]
#[utoipa::path(
    post,
    path = "/v1/auth/dev-bypass",
    responses(
        (status = 501, description = "Dev bypass disabled")
    ),
    tag = "auth"
)]
pub async fn dev_bypass_handler() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(
            ErrorResponse::new("Endpoint not implemented in this build.")
                .with_code("NOT_IMPLEMENTED"),
        ),
    )
}

#[cfg(not(all(feature = "dev-bypass", debug_assertions)))]
#[utoipa::path(
    post,
    path = "/v1/dev/bootstrap",
    responses(
        (status = 501, description = "Dev bootstrap disabled")
    ),
    tag = "dev"
)]
pub async fn dev_bootstrap_handler(
    Json(_body): Json<serde_json::Value>,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(
            ErrorResponse::new("Endpoint not implemented in this build.")
                .with_code("NOT_IMPLEMENTED"),
        ),
    )
}
