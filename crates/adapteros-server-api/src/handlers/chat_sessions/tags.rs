//! Tag handlers for chat sessions
//!
//! Provides list_chat_tags, create_chat_tag, update_chat_tag, delete_chat_tag,
//! assign_tags_to_session, get_session_tags, remove_tag_from_session
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_tags】

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use adapteros_db::ChatTag;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use super::access::{ensure_session_read_access, ensure_session_write_access};
use super::types::{AssignTagsRequest, CreateTagRequest, UpdateTagRequest};

/// List all tags for the tenant
///
/// GET /v1/chat/tags
#[utoipa::path(
    get,
    path = "/v1/chat/tags",
    responses(
        (status = 200, description = "Chat tags"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn list_chat_tags(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<Vec<ChatTag>> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let tags = state
        .db
        .list_chat_tags(&claims.tenant_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(Json(tags))
}

/// Create a new tag
///
/// POST /v1/chat/tags
#[utoipa::path(
    post,
    path = "/v1/chat/tags",
    request_body = CreateTagRequest,
    responses(
        (status = 201, description = "Tag created"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn create_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTagRequest>,
) -> Result<(StatusCode, Json<ChatTag>), ApiError> {
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - requires WorkspaceManage"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("Tag name cannot be empty"));
    }

    let tag = state
        .db
        .create_chat_tag(
            &claims.tenant_id,
            &req.name,
            req.color.as_deref(),
            req.description.as_deref(),
            Some(&claims.sub),
        )
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(tag)))
}

/// Update a tag
///
/// PUT /v1/chat/tags/:tag_id
#[utoipa::path(
    put,
    path = "/v1/chat/tags/{tag_id}",
    request_body = UpdateTagRequest,
    params(
        ("tag_id" = String, Path, description = "Tag ID")
    ),
    responses(
        (status = 200, description = "Tag updated"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Tag not found")
    ),
    tag = "chat"
)]
pub async fn update_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tag_id): Path<String>,
    Json(req): Json<UpdateTagRequest>,
) -> ApiResult<ChatTag> {
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - requires WorkspaceManage"))?;
    let tag_id = crate::id_resolver::resolve_any_id(&state.db, &tag_id).await?;

    // Verify tag belongs to tenant
    let tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Tag"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &tag.tenant_id)?;

    state
        .db
        .update_chat_tag(
            &tag_id,
            req.name.as_deref(),
            req.color.as_deref(),
            req.description.as_deref(),
        )
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    let updated_tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Tag not found after update"))?;

    Ok(Json(updated_tag))
}

/// Delete a tag
///
/// DELETE /v1/chat/tags/:tag_id
#[utoipa::path(
    delete,
    path = "/v1/chat/tags/{tag_id}",
    params(
        ("tag_id" = String, Path, description = "Tag ID")
    ),
    responses(
        (status = 204, description = "Tag deleted"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Tag not found")
    ),
    tag = "chat"
)]
pub async fn delete_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tag_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - requires WorkspaceManage"))?;
    let tag_id = crate::id_resolver::resolve_any_id(&state.db, &tag_id).await?;

    // Verify tag belongs to tenant
    let tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Tag"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &tag.tenant_id)?;

    state
        .db
        .delete_chat_tag(&tag_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Assign tags to a session
///
/// POST /v1/chat/sessions/:session_id/tags
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/tags",
    request_body = AssignTagsRequest,
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Tags assigned"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn assign_tags_to_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<AssignTagsRequest>,
) -> ApiResult<Vec<ChatTag>> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    ensure_session_write_access(&state, &claims, &session).await?;

    state
        .db
        .assign_tags_to_session(&session_id, &req.tag_ids, Some(&claims.sub))
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    let tags = state
        .db
        .get_session_tags(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(Json(tags))
}

/// Get tags for a session
///
/// GET /v1/chat/sessions/:session_id/tags
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/tags",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Session tags"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn get_session_tags(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> ApiResult<Vec<ChatTag>> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    ensure_session_read_access(&state, &claims, &session).await?;

    let tags = state
        .db
        .get_session_tags(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(Json(tags))
}

/// Remove a tag from a session
///
/// DELETE /v1/chat/sessions/:session_id/tags/:tag_id
#[utoipa::path(
    delete,
    path = "/v1/chat/sessions/{session_id}/tags/{tag_id}",
    params(
        ("session_id" = String, Path, description = "Session ID"),
        ("tag_id" = String, Path, description = "Tag ID")
    ),
    responses(
        (status = 204, description = "Tag removed"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session or tag not found")
    ),
    tag = "chat"
)]
pub async fn remove_tag_from_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((session_id, tag_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;
    let tag_id = crate::id_resolver::resolve_any_id(&state.db, &tag_id).await?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    ensure_session_write_access(&state, &claims, &session).await?;

    state
        .db
        .remove_tag_from_session(&session_id, &tag_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
