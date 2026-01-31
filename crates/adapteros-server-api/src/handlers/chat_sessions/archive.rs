//! Archive/restore handlers for chat sessions
//!
//! Provides archive_session, restore_session, hard_delete_session,
//! list_archived_sessions, list_deleted_sessions
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_archive】

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use adapteros_db::ChatSessionWithStatus;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use tracing::info;

use super::types::{ArchiveSessionRequest, ListArchivedQuery};

/// Archive a session
///
/// POST /v1/chat/sessions/:session_id/archive
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/archive",
    request_body = ArchiveSessionRequest,
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session archived"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn archive_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ArchiveSessionRequest>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    state
        .db
        .archive_session(&session_id, &claims.sub, req.reason.as_deref())
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Restore a deleted or archived session (admin-only)
///
/// POST /v1/chat/sessions/:session_id/restore
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/restore",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session restored"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn restore_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Admin-only: requires WorkspaceManage
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - restore requires WorkspaceManage"))?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    state
        .db
        .restore_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    info!(session_id = %session_id, user = %claims.sub, "Session restored");
    Ok(StatusCode::NO_CONTENT)
}

/// Permanently delete a session
///
/// DELETE /v1/chat/sessions/:session_id/permanent
#[utoipa::path(
    delete,
    path = "/v1/chat/sessions/{session_id}/permanent",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session deleted"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn hard_delete_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Admin-only
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - requires WorkspaceManage"))?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    state
        .db
        .hard_delete_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    info!(session_id = %session_id, user = %claims.sub, "Session permanently deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// List archived sessions
///
/// GET /v1/chat/sessions/archived
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/archived",
    params(
        ListArchivedQuery
    ),
    responses(
        (status = 200, description = "Archived sessions"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn list_archived_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListArchivedQuery>,
) -> ApiResult<Vec<ChatSessionWithStatus>> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let sessions = state
        .db
        .list_archived_sessions(&claims.tenant_id, Some(&claims.sub), query.limit)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(Json(sessions))
}

/// List deleted sessions (trash)
///
/// GET /v1/chat/sessions/trash
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/trash",
    params(
        ListArchivedQuery
    ),
    responses(
        (status = 200, description = "Deleted sessions"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn list_deleted_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListArchivedQuery>,
) -> ApiResult<Vec<ChatSessionWithStatus>> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let sessions = state
        .db
        .list_deleted_sessions(&claims.tenant_id, Some(&claims.sub), query.limit)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(Json(sessions))
}
