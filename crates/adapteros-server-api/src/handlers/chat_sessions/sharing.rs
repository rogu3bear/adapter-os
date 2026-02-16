//! Sharing handlers for chat sessions
//!
//! Provides share_session, get_session_shares, revoke_session_share, get_sessions_shared_with_me
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_sharing】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::{ChatSessionWithStatus, SessionShare};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Serialize;

use super::types::{ListArchivedQuery, ShareSessionRequest};

/// Single share entry matching the UI's `SessionShareInfo` shape.
#[derive(Debug, Clone, Serialize)]
pub struct SessionShareInfo {
    pub share_id: String,
    pub user_id: String,
    pub permission: String,
    pub shared_at: String,
}

/// Wrapped response matching the UI's `SessionSharesResponse` shape.
#[derive(Debug, Clone, Serialize)]
pub struct SessionSharesResponse {
    pub shares: Vec<SessionShareInfo>,
}

/// Single shared-session entry matching the UI's `SharedSessionInfo` shape.
#[derive(Debug, Clone, Serialize)]
pub struct SharedSessionInfo {
    pub session_id: String,
    pub name: String,
    pub shared_by: String,
    pub permission: String,
    pub shared_at: String,
}

/// Wrapped response matching the UI's `SharedWithMeResponse` shape.
#[derive(Debug, Clone, Serialize)]
pub struct SharedWithMeResponse {
    pub sessions: Vec<SharedSessionInfo>,
}

/// Share a session
///
/// POST /v1/chat/sessions/:session_id/shares
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/shares",
    request_body = ShareSessionRequest,
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 201, description = "Session shared"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn share_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ShareSessionRequest>,
) -> Result<(StatusCode, Json<SessionSharesResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceResourceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    let shared_at = chrono::Utc::now().to_rfc3339();
    let mut shares = Vec::new();

    // Share with workspace
    if let Some(workspace_id) = &req.workspace_id {
        let id = state
            .db
            .share_session_with_workspace(
                &session_id,
                workspace_id,
                &req.permission,
                &claims.sub,
                req.expires_at.as_deref(),
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to share with workspace")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        shares.push(SessionShareInfo {
            share_id: id,
            user_id: workspace_id.clone(),
            permission: req.permission.clone(),
            shared_at: shared_at.clone(),
        });
    }

    // Share with users
    if let Some(user_ids) = &req.user_ids {
        for user_id in user_ids {
            let id = state
                .db
                .share_session_with_user(
                    &session_id,
                    user_id,
                    &claims.tenant_id,
                    &req.permission,
                    &claims.sub,
                    req.expires_at.as_deref(),
                )
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to share with user")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
            shares.push(SessionShareInfo {
                share_id: id,
                user_id: user_id.clone(),
                permission: req.permission.clone(),
                shared_at: shared_at.clone(),
            });
        }
    }

    Ok((StatusCode::CREATED, Json(SessionSharesResponse { shares })))
}

/// Get shares for a session
///
/// GET /v1/chat/sessions/:session_id/shares
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/shares",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Session shares"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn get_session_shares(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionSharesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    let db_shares = state
        .db
        .get_session_shares(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get shares")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let shares = db_shares
        .into_iter()
        .map(|s| SessionShareInfo {
            share_id: s.id,
            user_id: s.shared_with_user_id.unwrap_or_default(),
            permission: s.permission,
            shared_at: s.shared_at,
        })
        .collect();

    Ok(Json(SessionSharesResponse { shares }))
}

/// Revoke a session share
///
/// DELETE /v1/chat/sessions/:session_id/shares/:share_id
#[utoipa::path(
    delete,
    path = "/v1/chat/sessions/{session_id}/shares/{share_id}",
    params(
        ("session_id" = String, Path, description = "Session ID"),
        ("share_id" = String, Path, description = "Share ID")
    ),
    responses(
        (status = 204, description = "Share revoked"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Share not found")
    ),
    tag = "chat"
)]
pub async fn revoke_session_share(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((session_id, share_id)): Path<(String, String)>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceResourceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    let share_id = crate::id_resolver::resolve_any_id(&state.db, &share_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    let share_type = params.get("type").map(|s| s.as_str()).unwrap_or("user");

    state
        .db
        .revoke_session_share(&share_id, share_type)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to revoke share")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get sessions shared with the current user
///
/// GET /v1/chat/sessions/shared-with-me
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/shared-with-me",
    params(
        ListArchivedQuery
    ),
    responses(
        (status = 200, description = "Shared sessions"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn get_sessions_shared_with_me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListArchivedQuery>,
) -> Result<Json<SharedWithMeResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let db_sessions = state
        .db
        .get_sessions_shared_with_user(&claims.sub, &claims.tenant_id, query.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get shared sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let sessions = db_sessions
        .into_iter()
        .map(|s| SharedSessionInfo {
            session_id: s.id,
            name: s.name,
            shared_by: s.user_id.unwrap_or_default(),
            permission: "read".to_owned(),
            shared_at: s.created_at,
        })
        .collect();

    Ok(Json(SharedWithMeResponse { sessions }))
}
