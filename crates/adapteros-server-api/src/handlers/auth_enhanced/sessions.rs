//! Session management handlers
//!
//! Contains handlers for listing and revoking sessions.

use crate::auth::Claims;
use crate::security::{get_user_sessions, lock_session, revoke_token};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::auth_sessions_kv::AuthSessionKvRepository;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Duration, Utc};
use tracing::{info, warn};

use super::helpers::log_auth_event;
use super::types::{LogoutResponse, SessionInfo, SessionsResponse};

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
