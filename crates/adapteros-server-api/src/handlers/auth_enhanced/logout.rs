//! Logout handler
//!
//! Contains the logout endpoint for revoking sessions.

use crate::auth::Claims;
use crate::auth_common::{clear_auth_cookies, AuthConfig};
use crate::security::{lock_session, revoke_token};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::auth_sessions_kv::AuthSessionKvRepository;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::{Duration, Utc};
use tracing::{info, warn};

use super::helpers::emit_auth_event;
use super::types::LogoutResponse;

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
