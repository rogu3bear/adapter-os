//! MFA (Multi-Factor Authentication) handlers
//!
//! Contains handlers for MFA enrollment, verification, and management.

use crate::audit_helper;
use crate::auth::Claims;
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
use axum::{extract::State, http::StatusCode, Extension, Json};
use tracing::warn;

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
        warn!(
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
        warn!(
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
        warn!(
            error = %e,
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Audit log failed"
        );
    }

    Ok(StatusCode::NO_CONTENT)
}
