use super::audit::{log_auth_event, AuthEvent};
use crate::auth::{validate_refresh_token_ed25519, validate_refresh_token_hmac};
use crate::auth_common::{
    attach_auth_cookies, issue_access_token, issue_refresh_token, AccessTokenParams, AuthConfig,
    RefreshTokenParams,
};
use crate::security::update_session_rotation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::LoginResponse;
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    Json,
};
use chrono::{Duration, Utc};
use tracing::{info, warn};
use uuid::Uuid;

fn extract_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            let prefix = format!("{name}=");
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix(&prefix) {
                    return Some(token.to_string());
                }
            }
            None
        })
}

/// Refresh access token using a valid refresh token
#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    responses(
        (status = 200, description = "Token refreshed", body = LoginResponse),
        (status = 401, description = "Invalid or expired refresh token"),
        (status = 403, description = "Account locked or disabled")
    ),
    tag = "auth"
)]
pub async fn refresh_token_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<LoginResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);

    // 1. Extract Refresh Token
    let refresh_token = extract_cookie_value(&headers, "refresh_token").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Missing refresh token").with_code("MISSING_TOKEN")),
        )
    })?;

    // 2. Validate Refresh Token
    let claims = if state.use_ed25519 {
        validate_refresh_token_ed25519(
            &refresh_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        validate_refresh_token_hmac(&refresh_token, &state.hmac_keys, &state.jwt_secret)
    }
    .map_err(|e| {
        warn!(error = %e, "Refresh token validation failed");
        log_auth_event(
            AuthEvent::TokenRefreshFailed,
            None,
            None,
            None,
            None,
            None,
            Some("token_validation_failed"),
        );
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Invalid refresh token").with_code("INVALID_TOKEN")),
        )
    })?;

    // 3. Check for Revocation/Reuse (Rotation) and Session Validity
    // Retrieve session from DB using session_id from claims
    let session_id = claims.session_id.as_str();

    // Verify session via user's session list (since get_auth_session is missing)
    // FAIL-CLOSED: Database error denies token refresh
    let sessions = state.db.get_user_sessions(&claims.sub).await.map_err(|e| {
        warn!(error = %e, user_id = %claims.sub, "Session validation failed, denying token refresh");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            // Generic message - don't leak internal details
            Json(ErrorResponse::new("Token refresh failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    if let Some(session) = sessions.iter().find(|s| s.jti == session_id) {
        if session.locked {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse::new("Session is locked").with_code("SESSION_LOCKED")),
            ));
        }
        if let Some(ref stored_rot_id) = session.rot_id {
            if stored_rot_id != &claims.rot_id {
                warn!(
                    user_id = %claims.sub,
                    session_id = %session_id,
                    stored_rot_id = %stored_rot_id,
                    token_rot_id = %claims.rot_id,
                    "Refresh token rotation id mismatch"
                );
                log_auth_event(
                    AuthEvent::TokenRefreshRotationMismatch,
                    Some(&claims.sub),
                    None,
                    Some(&claims.tenant_id),
                    None,
                    Some(session_id),
                    Some("potential_token_replay"),
                );
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(
                        ErrorResponse::new("Invalid refresh token").with_code("ROTATION_MISMATCH"),
                    ),
                ));
            }
        } else {
            warn!(
                user_id = %claims.sub,
                session_id = %session_id,
                "Session missing rot_id; skipping rotation check"
            );
        }
    } else {
        // Session not found (expired or deleted)
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Session expired or invalid").with_code("SESSION_INVALID")),
        ));
    }

    // 4. Fetch User to ensure active
    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            warn!(error = %e, user_id = %claims.sub, "Failed to fetch user for refresh");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Internal error").with_code("DATABASE_ERROR")),
            )
        })?
        .ok_or_else(|| {
            warn!(user_id = %claims.sub, "User not found during refresh");
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("User not found").with_code("USER_NOT_FOUND")),
            )
        })?;

    if user.disabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Account disabled").with_code("ACCOUNT_DISABLED")),
        ));
    }

    // 5. Issue New Access Token
    let token_ttl_seconds = auth_cfg.access_ttl();
    let roles_vec = vec![user.role.clone()];
    let admin_tenants = adapteros_db::get_user_tenant_access(&state.db, &user.id)
        .await
        .unwrap_or_default();

    // We reuse the existing session_id
    let access_params = AccessTokenParams {
        user_id: &user.id,
        email: &user.email,
        role: &user.role,
        roles: &roles_vec,
        tenant_id: &user.tenant_id,
        admin_tenants: &admin_tenants,
        device_id: claims.device_id.as_deref(),
        session_id,
        mfa_level: None,
    };
    let new_access_token = issue_access_token(&state, &access_params, Some(token_ttl_seconds))
        .map_err(|e| {
            warn!(error = %e, user_id = %user.id, "Failed to generate new access token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
            )
        })?;

    // 6. Rotate Refresh Token
    // Generate new rot_id and issue new refresh token
    let new_rot_id = Uuid::new_v4().to_string();
    let refresh_ttl = auth_cfg.effective_ttl();
    let refresh_expires_at = Utc::now() + Duration::seconds(refresh_ttl as i64);
    let session_expires_at = refresh_expires_at.timestamp();

    let refresh_params = RefreshTokenParams {
        user_id: &user.id,
        tenant_id: &user.tenant_id,
        roles: &roles_vec,
        device_id: claims.device_id.as_deref(),
        session_id,
        rot_id: &new_rot_id,
    };
    let new_refresh_token = issue_refresh_token(&state, &refresh_params, Some(refresh_ttl))
        .map_err(|e| {
            warn!(error = %e, user_id = %user.id, "Failed to generate new refresh token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
            )
        })?;

    // Hash new refresh token for storage
    let new_refresh_hash = blake3::hash(new_refresh_token.as_bytes())
        .to_hex()
        .to_string();

    // Update session with new rot_id
    if let Err(e) = update_session_rotation(
        &state.db,
        session_id,
        &new_rot_id,
        Some(&new_refresh_hash),
        &refresh_expires_at.to_rfc3339(),
        session_expires_at,
    )
    .await
    {
        warn!(error = %e, session_id = %session_id, "Failed to update session rotation");
        // Continue anyway - token is valid, just rotation tracking may be stale
    }

    log_auth_event(
        AuthEvent::TokenRefreshSuccess,
        Some(&user.id),
        None,
        Some(&user.tenant_id),
        None,
        Some(session_id),
        None,
    );

    // 7. Prepare Response
    let mut headers = HeaderMap::new();
    let csrf_token = Uuid::new_v4().to_string();
    attach_auth_cookies(
        &mut headers,
        &new_access_token,
        &new_refresh_token,
        &csrf_token,
        &auth_cfg,
        auth_cfg.effective_ttl(),
    )
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to attach cookie").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Calculate generic response fields
    // We don't return full tenant list on refresh usually.
    // LoginResponse expects tenants to be Option<Vec<TenantSummary>>.

    Ok((
        headers,
        Json(LoginResponse {
            schema_version: API_SCHEMA_VERSION.to_string(),
            token: new_access_token,
            user_id: user.id,
            tenant_id: user.tenant_id,
            role: user.role,
            expires_in: token_ttl_seconds,
            tenants: None, // Or Some(vec![])
            mfa_level: None,
        }),
    ))
}
