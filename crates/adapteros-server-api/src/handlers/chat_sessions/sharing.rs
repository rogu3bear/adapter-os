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

use super::types::{ListArchivedQuery, ShareSessionRequest};

/// Share a session
///
/// POST /v1/chat/sessions/:session_id/shares
pub async fn share_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ShareSessionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceResourceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

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

    let mut share_ids = Vec::new();

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
        share_ids.push(serde_json::json!({"type": "workspace", "id": id}));
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
            share_ids.push(serde_json::json!({"type": "user", "id": id, "user_id": user_id}));
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"shares": share_ids})),
    ))
}

/// Get shares for a session
///
/// GET /v1/chat/sessions/:session_id/shares
pub async fn get_session_shares(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<SessionShare>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

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

    let shares = state
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

    Ok(Json(shares))
}

/// Revoke a session share
///
/// DELETE /v1/chat/sessions/:session_id/shares/:share_id
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
pub async fn get_sessions_shared_with_me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListArchivedQuery>,
) -> Result<Json<Vec<ChatSessionWithStatus>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let sessions = state
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

    Ok(Json(sessions))
}
