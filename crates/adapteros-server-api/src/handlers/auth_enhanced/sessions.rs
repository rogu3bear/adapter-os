use crate::auth::Claims;
use crate::auth_common::{clear_auth_cookies, AuthConfig};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::SessionInfo;
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct SessionsResponse {
    pub schema_version: String,
    pub sessions: Vec<SessionInfo>,
}
use tracing::{info, warn};

/// List active sessions for the current user
#[utoipa::path(
    get,
    path = "/v1/auth/sessions",
    responses(
        (status = 200, description = "List of active sessions", body = SessionsResponse),
        (status = 500, description = "Database error")
    ),
    tag = "auth"
)]
pub async fn list_sessions_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SessionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let sessions = state.db.get_user_sessions(&claims.sub).await.map_err(|e| {
        warn!(error = %e, user_id = %claims.sub, "Failed to list user sessions");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to list sessions").with_code("DATABASE_ERROR")),
        )
    })?;

    let session_infos = sessions
        .into_iter()
        .map(|s| SessionInfo {
            jti: s.jti,
            created_at: s.created_at,
            ip_address: s.ip_address,
            user_agent: s.user_agent,
            last_activity: s.last_activity,
        })
        .collect();

    Ok(Json(SessionsResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        sessions: session_infos,
    }))
}

/// Revoke a specific session
#[utoipa::path(
    delete,
    path = "/v1/auth/sessions/{session_id}",
    responses(
        (status = 200, description = "Session revoked"),
        (status = 403, description = "Cannot revoke this session"),
        (status = 500, description = "Database error")
    ),
    tag = "auth"
)]
pub async fn revoke_session_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // 1. Check ownership (or admin status)
    // For now, allow users to revoke only their own sessions.
    // We fetch the session first to check owner.
    // Actually `get_user_sessions` filters by user_id.
    // We can also use `delete_auth_session` directly but that might allow deleting others if we don't check.

    // Safer approach: Fetch user sessions and verify `session_id` is among them.
    // OR enforce RLS at DB level. Here we do application-level check.

    let sessions = state.db.get_user_sessions(&claims.sub).await.map_err(|e| {
        warn!(error = %e, "Failed to fetch sessions for ownership check");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Internal error")),
        )
    })?;

    let is_owner = sessions.iter().any(|s| s.jti == session_id);
    if !is_owner {
        return Ok(StatusCode::OK);
    }

    state
        .db
        .delete_auth_session(&session_id)
        .await
        .map_err(|e| {
            warn!(error = %e, session_id = %session_id, "Failed to revoke session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to revoke session").with_code("DATABASE_ERROR")),
            )
        })?;

    info!(user_id = %claims.sub, session_id = %session_id, "Session revoked");
    Ok(StatusCode::OK)
}

/// User logout
///
/// Clears httpOnly cookies and revokes the session server-side.
/// Clients should also clear any local auth state.
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    responses(
        (status = 200, description = "Logged out"),
    ),
    tag = "auth"
)]
pub async fn logout_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<(HeaderMap, StatusCode), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);

    // Revoke the JTI of the current token to prevent replay until expiry.
    // Use session_id if available, otherwise fallback to jti
    let sid = claims.session_id.as_deref().unwrap_or(&claims.jti);
    if let Err(e) = state.db.delete_auth_session(sid).await {
        // Log but don't fail logout
        warn!(error = %e, session_id = %sid, "Failed to revoke session on logout");
    }

    // Clear httpOnly cookies to fully log out browser clients
    let mut headers = HeaderMap::new();
    if let Err(e) = clear_auth_cookies(&mut headers, &auth_cfg) {
        warn!(error = %e, user_id = %claims.sub, "Failed to clear auth cookies on logout");
        // Non-fatal, session is already deleted
    }

    info!(user_id = %claims.sub, "User logged out");
    Ok((headers, StatusCode::OK))
}
