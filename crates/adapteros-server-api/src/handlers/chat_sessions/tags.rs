//! Tag handlers for chat sessions
//!
//! Provides list_chat_tags, create_chat_tag, update_chat_tag, delete_chat_tag,
//! assign_tags_to_session, get_session_tags, remove_tag_from_session
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_tags】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::ChatTag;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use super::types::{AssignTagsRequest, CreateTagRequest, UpdateTagRequest};

/// List all tags for the tenant
///
/// GET /v1/chat/tags
pub async fn list_chat_tags(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<ChatTag>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let tags = state
        .db
        .list_chat_tags(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list tags")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(tags))
}

/// Create a new tag
///
/// POST /v1/chat/tags
pub async fn create_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTagRequest>,
) -> Result<(StatusCode, Json<ChatTag>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Tag name cannot be empty").with_code("VALIDATION_ERROR")),
        ));
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
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok((StatusCode::CREATED, Json(tag)))
}

/// Update a tag
///
/// PUT /v1/chat/tags/:tag_id
pub async fn update_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tag_id): Path<String>,
    Json(req): Json<UpdateTagRequest>,
) -> Result<Json<ChatTag>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify tag belongs to tenant
    let tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Tag not found").with_code("NOT_FOUND")),
            )
        })?;

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
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let updated_tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Tag not found after update").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(updated_tag))
}

/// Delete a tag
///
/// DELETE /v1/chat/tags/:tag_id
pub async fn delete_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tag_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify tag belongs to tenant
    let tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Tag not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &tag.tenant_id)?;

    state.db.delete_chat_tag(&tag_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to delete tag")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Assign tags to a session
///
/// POST /v1/chat/sessions/:session_id/tags
pub async fn assign_tags_to_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<AssignTagsRequest>,
) -> Result<Json<Vec<ChatTag>>, (StatusCode, Json<ErrorResponse>)> {
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

    state
        .db
        .assign_tags_to_session(&session_id, &req.tag_ids, Some(&claims.sub))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to assign tags")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let tags = state.db.get_session_tags(&session_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get tags")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(tags))
}

/// Get tags for a session
///
/// GET /v1/chat/sessions/:session_id/tags
pub async fn get_session_tags(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<ChatTag>>, (StatusCode, Json<ErrorResponse>)> {
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

    let tags = state.db.get_session_tags(&session_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get tags")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(tags))
}

/// Remove a tag from a session
///
/// DELETE /v1/chat/sessions/:session_id/tags/:tag_id
pub async fn remove_tag_from_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((session_id, tag_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
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

    state
        .db
        .remove_tag_from_session(&session_id, &tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to remove tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}
