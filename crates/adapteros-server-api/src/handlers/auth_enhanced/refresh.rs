//! Token refresh handler
//!
//! Contains the refresh token endpoint for session renewal.

use crate::auth::{
    issue_access_token_ed25519, issue_access_token_hmac, issue_refresh_token_ed25519,
    issue_refresh_token_ed25519_with_kv, issue_refresh_token_hmac,
    issue_refresh_token_hmac_with_kv, validate_refresh_token_ed25519, validate_refresh_token_hmac,
};
use crate::auth_common::{
    attach_auth_cookie, attach_csrf_cookie, attach_refresh_cookie, AuthConfig,
};
use crate::security::{
    get_session_by_id, get_tenant_token_baseline, is_token_revoked, upsert_user_session,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::auth_sessions_kv::AuthSessionKvRepository;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use blake3;
use chrono::{Duration, TimeZone, Utc};
use tracing::{info, warn};
use uuid::Uuid;

use super::helpers::{
    emit_auth_event, emit_refresh_failure, extract_cookie_token, log_auth_event,
    log_refresh_failure,
};
use super::types::RefreshResponse;

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

    // Apply clock skew tolerance for session expiry check
    let clock_skew = {
        let config = state.config.read().unwrap_or_else(|e| {
            warn!("Config lock was poisoned in refresh handler, recovering");
            e.into_inner()
        });
        config.security.clock_skew_seconds as i64
    };
    let now_with_skew = now_ts - clock_skew;

    if refresh_claims.exp <= now_with_skew || session_exp_ts <= now_with_skew {
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
