//! Messaging handlers
//!
//! Provides API endpoints for workspace-scoped messaging with thread support.

use crate::handlers::{AppState, Claims, ErrorResponse};
use adapteros_db::workspaces::WorkspaceRole;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Debug, Deserialize)]
pub struct CreateMessageRequest {
    pub content: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub workspace_id: String,
    pub from_user_id: String,
    pub from_tenant_id: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub created_at: String,
    pub edited_at: Option<String>,
}

/// Send a message to a workspace
pub async fn create_message(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check workspace access - must be member or owner
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    match role {
        Some(WorkspaceRole::Owner) | Some(WorkspaceRole::Member) => {
            // Allowed
        }
        _ => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Only owners and members can send messages")
                        .with_code("FORBIDDEN"),
                ),
            ));
        }
    }

    let message_id = state
        .db
        .create_message(
            &workspace_id,
            &claims.sub,
            &claims.tenant_id,
            &req.content,
            req.thread_id.as_deref(),
        )
        .await
        .map_err(|e| {
            error!("Failed to create message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create message")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let message = state
        .db
        .get_message(&message_id)
        .await
        .map_err(|e| {
            error!("Failed to get created message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve created message")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Message not found after creation").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(MessageResponse {
        id: message.id,
        workspace_id: message.workspace_id,
        from_user_id: message.from_user_id,
        from_tenant_id: message.from_tenant_id,
        content: message.content,
        thread_id: message.thread_id,
        created_at: message.created_at,
        edited_at: message.edited_at,
    }))
}

/// List messages in a workspace
pub async fn list_workspace_messages(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<MessageResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Check workspace access
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    if role.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to workspace").with_code("FORBIDDEN")),
        ));
    }

    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(50);
    let offset = params
        .get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let messages = state
        .db
        .list_workspace_messages(&workspace_id, Some(limit), Some(offset))
        .await
        .map_err(|e| {
            error!("Failed to list workspace messages: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to list messages").with_code("INTERNAL_ERROR")),
            )
        })?;

    let responses: Vec<MessageResponse> = messages
        .into_iter()
        .map(|m| MessageResponse {
            id: m.id,
            workspace_id: m.workspace_id,
            from_user_id: m.from_user_id,
            from_tenant_id: m.from_tenant_id,
            content: m.content,
            thread_id: m.thread_id,
            created_at: m.created_at,
            edited_at: m.edited_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Get a message thread
pub async fn get_message_thread(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, thread_id)): Path<(String, String)>,
) -> Result<Json<Vec<MessageResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Check workspace access
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    if role.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to workspace").with_code("FORBIDDEN")),
        ));
    }

    let messages = state
        .db
        .list_message_thread(&thread_id, Some(100), Some(0))
        .await
        .map_err(|e| {
            error!("Failed to get message thread: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get message thread").with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    let responses: Vec<MessageResponse> = messages
        .into_iter()
        .map(|m| MessageResponse {
            id: m.id,
            workspace_id: m.workspace_id,
            from_user_id: m.from_user_id,
            from_tenant_id: m.from_tenant_id,
            content: m.content,
            thread_id: m.thread_id,
            created_at: m.created_at,
            edited_at: m.edited_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Edit a message
pub async fn edit_message(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, message_id)): Path<(String, String)>,
    Json(req): Json<CreateMessageRequest>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check workspace access
    let role = state
        .db
        .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to check workspace access: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check workspace access")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    if role.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to workspace").with_code("FORBIDDEN")),
        ));
    }

    // Verify message belongs to user
    let message = state
        .db
        .get_message(&message_id)
        .await
        .map_err(|e| {
            error!("Failed to get message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to get message").with_code("INTERNAL_ERROR")),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Message not found").with_code("NOT_FOUND")),
            )
        })?;

    if message.from_user_id != claims.sub {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Can only edit own messages").with_code("FORBIDDEN")),
        ));
    }

    state
        .db
        .edit_message(&message_id, &req.content)
        .await
        .map_err(|e| {
            error!("Failed to edit message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to edit message").with_code("INTERNAL_ERROR")),
            )
        })?;

    let updated_message = state
        .db
        .get_message(&message_id)
        .await
        .map_err(|e| {
            error!("Failed to get updated message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve updated message")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Message not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(MessageResponse {
        id: updated_message.id,
        workspace_id: updated_message.workspace_id,
        from_user_id: updated_message.from_user_id,
        from_tenant_id: updated_message.from_tenant_id,
        content: updated_message.content,
        thread_id: updated_message.thread_id,
        created_at: updated_message.created_at,
        edited_at: updated_message.edited_at,
    }))
}
