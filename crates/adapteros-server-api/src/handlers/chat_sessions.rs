//! Chat session API handlers
//!
//! Provides endpoints for managing persistent chat sessions with stack context
//! and trace linkage for the workspace experience.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_handlers】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::{
    AddMessageParams, ChatCategory, ChatMessage, ChatSearchResult, ChatSession,
    ChatSessionWithStatus, ChatTag, CreateChatSessionParams, InferenceEvidence, SessionShare,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use utoipa::{IntoParams, ToSchema};

/// Request to create a new chat session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatSessionRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// Response for chat session creation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatSessionResponse {
    pub session_id: String,
    pub tenant_id: String,
    pub name: String,
    pub created_at: String,
}

/// Request to add a message to a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddChatMessageRequest {
    pub role: String, // 'user', 'assistant', 'system'
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// API response wrapper for ChatMessage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatMessageResponse {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

impl From<ChatMessage> for ChatMessageResponse {
    fn from(msg: ChatMessage) -> Self {
        Self {
            id: msg.id,
            session_id: msg.session_id,
            role: msg.role,
            content: msg.content,
            timestamp: msg.timestamp,
            metadata_json: msg.metadata_json,
        }
    }
}

/// Query parameters for listing sessions
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListSessionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

/// Create a new chat session
///
/// POST /v1/chat/sessions
#[utoipa::path(
    post,
    path = "/v1/chat/sessions",
    tag = "chat",
    request_body = CreateChatSessionRequest,
    responses(
        (status = 201, description = "Session created", body = CreateChatSessionResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn create_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateChatSessionRequest>,
) -> Result<(StatusCode, Json<CreateChatSessionResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Permission check: InferenceExecute allows chat sessions
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Validate session name
    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Session name cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }

    // Generate session ID
    let session_id = format!("session-{}", uuid::Uuid::new_v4());

    // Create session parameters
    let params = CreateChatSessionParams {
        id: session_id.clone(),
        tenant_id: claims.tenant_id.clone(),
        user_id: Some(claims.sub.clone()),
        stack_id: req.stack_id,
        collection_id: req.collection_id,
        name: req.name,
        metadata_json: req.metadata_json,
    };

    // Create session in database
    state.db.create_chat_session(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to create session")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Retrieve created session
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Session not found after creation")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    info!(
        session_id = %session_id,
        tenant_id = %claims.tenant_id,
        user_id = %claims.sub,
        "Chat session created"
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateChatSessionResponse {
            session_id: session.id,
            tenant_id: session.tenant_id,
            name: session.name,
            created_at: session.created_at,
        }),
    ))
}

/// List chat sessions for the current user/tenant
///
/// GET /v1/chat/sessions
#[utoipa::path(
    get,
    path = "/v1/chat/sessions",
    tag = "chat",
    params(ListSessionsQuery),
    responses(
        (status = 200, description = "Sessions retrieved", body = Vec<ChatSession>),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn list_chat_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<Vec<ChatSession>>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // List sessions - filter by user_id if provided, otherwise show all for tenant
    let user_filter = query.user_id.or(Some(claims.sub.clone()));
    let sessions = state
        .db
        .list_chat_sessions(&claims.tenant_id, user_filter.as_deref(), query.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    debug!(
        tenant_id = %claims.tenant_id,
        count = sessions.len(),
        "Listed chat sessions"
    );

    Ok(Json(sessions))
}

/// Get a specific chat session
///
/// GET /v1/chat/sessions/:session_id
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Session retrieved", body = ChatSession),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<ChatSession>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Get session
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

    // Verify tenant access
    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    Ok(Json(session))
}

/// Add a message to a chat session
///
/// POST /v1/chat/sessions/:session_id/messages
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/messages",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    request_body = AddChatMessageRequest,
    responses(
        (status = 201, description = "Message added", body = ChatMessageResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn add_chat_message(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<AddChatMessageRequest>,
) -> Result<(StatusCode, Json<ChatMessageResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session exists and tenant has access
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    // Generate message ID
    let message_id = format!("msg-{}", uuid::Uuid::new_v4());

    // Add message
    let params = AddMessageParams {
        id: message_id.clone(),
        session_id: session_id.clone(),
        role: req.role,
        content: req.content,
        metadata_json: req.metadata_json,
    };

    state.db.add_chat_message(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to add message")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Retrieve added message
    let messages = state
        .db
        .get_chat_messages(&session_id, Some(1))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve message")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let message = messages.into_iter().last().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Message not found after creation").with_code("INTERNAL_ERROR"),
            ),
        )
    })?;

    Ok((StatusCode::CREATED, Json(message.into())))
}

/// Get messages for a chat session
///
/// GET /v1/chat/sessions/:session_id/messages
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/messages",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID"),
        ("limit" = Option<i64>, Query, description = "Maximum messages to return")
    ),
    responses(
        (status = 200, description = "Messages retrieved", body = Vec<ChatMessageResponse>),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_chat_messages(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<ChatMessageResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session exists and tenant has access
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    // Get limit from query
    let limit = query.get("limit").and_then(|s| s.parse::<i64>().ok());

    // Get messages
    let messages = state
        .db
        .get_chat_messages(&session_id, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get messages")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Convert to API response type
    let response: Vec<ChatMessageResponse> = messages.into_iter().map(|m| m.into()).collect();

    Ok(Json(response))
}

/// Get session summary with trace counts
///
/// GET /v1/chat/sessions/:session_id/summary
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/summary",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Session summary", body = serde_json::Value),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_session_summary(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session exists and tenant has access
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    // Get summary
    let summary = state
        .db
        .get_session_summary(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session summary")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(summary))
}

/// Soft delete a chat session (moves to trash)
///
/// DELETE /v1/chat/sessions/:session_id
#[utoipa::path(
    delete,
    path = "/v1/chat/sessions/{session_id}",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session moved to trash"),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn delete_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session exists and tenant has access
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    // Soft delete session (moves to trash)
    state
        .db
        .soft_delete_session(&session_id, &claims.sub)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to delete session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        session_id = %session_id,
        tenant_id = %claims.tenant_id,
        "Chat session soft deleted"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// Get evidence for a chat message
///
/// GET /v1/chat/messages/:message_id/evidence
#[utoipa::path(
    get,
    path = "/v1/chat/messages/{message_id}/evidence",
    tag = "chat",
    params(
        ("message_id" = String, Path, description = "Message ID")
    ),
    responses(
        (status = 200, description = "Evidence retrieved", body = Vec<InferenceEvidence>),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_message_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(message_id): Path<String>,
) -> Result<Json<Vec<InferenceEvidence>>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Get evidence from database
    let evidence = state
        .db
        .get_evidence_by_message(&message_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get message evidence")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    debug!(
        message_id = %message_id,
        evidence_count = evidence.len(),
        "Retrieved message evidence"
    );

    Ok(Json(evidence))
}

/// Request to update collection binding for a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCollectionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
}

/// Update the collection binding for a chat session
///
/// PUT /v1/chat/sessions/:session_id/collection
#[utoipa::path(
    put,
    path = "/v1/chat/sessions/{session_id}/collection",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    request_body = UpdateCollectionRequest,
    responses(
        (status = 200, description = "Collection updated", body = ChatSession),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn update_session_collection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateCollectionRequest>,
) -> Result<Json<ChatSession>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session exists and tenant has access
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    // Update collection binding
    state
        .db
        .update_session_collection(&session_id, request.collection_id.clone())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update collection")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Retrieve updated session
    let updated_session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve updated session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Session not found after update")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    info!(
        session_id = %session_id,
        collection_id = ?request.collection_id,
        tenant_id = %claims.tenant_id,
        "Chat session collection updated"
    );

    Ok(Json(updated_session))
}

// =============================================================================
// Tags API
// =============================================================================

/// Request to create a new tag
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to update a tag
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateTagRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to assign tags to a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssignTagsRequest {
    pub tag_ids: Vec<String>,
}

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

    if tag.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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
        .unwrap();

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

    if tag.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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

// =============================================================================
// Categories API
// =============================================================================

/// Request to create a category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to update a category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to set session category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SetCategoryRequest {
    pub category_id: Option<String>,
}

/// List all categories for the tenant
///
/// GET /v1/chat/categories
pub async fn list_chat_categories(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<ChatCategory>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    let categories = state
        .db
        .list_chat_categories(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list categories")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(categories))
}

/// Create a new category
///
/// POST /v1/chat/categories
pub async fn create_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<ChatCategory>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Category name cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }

    let category = state
        .db
        .create_chat_category(
            &claims.tenant_id,
            &req.name,
            req.parent_id.as_deref(),
            req.icon.as_deref(),
            req.color.as_deref(),
        )
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("depth cannot exceed") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new("Failed to create category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok((StatusCode::CREATED, Json(category)))
}

/// Update a category
///
/// PUT /v1/chat/categories/:category_id
pub async fn update_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
    Json(req): Json<UpdateCategoryRequest>,
) -> Result<Json<ChatCategory>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Category not found").with_code("NOT_FOUND")),
            )
        })?;

    if category.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    state
        .db
        .update_chat_category(
            &category_id,
            req.name.as_deref(),
            req.icon.as_deref(),
            req.color.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let updated = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .unwrap();

    Ok(Json(updated))
}

/// Delete a category
///
/// DELETE /v1/chat/categories/:category_id
pub async fn delete_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
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

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Category not found").with_code("NOT_FOUND")),
            )
        })?;

    if category.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    state
        .db
        .delete_chat_category(&category_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("Cannot delete category") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new("Failed to delete category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Set the category for a session
///
/// PUT /v1/chat/sessions/:session_id/category
pub async fn set_session_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<SetCategoryRequest>,
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    state
        .db
        .set_session_category(&session_id, req.category_id.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to set category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// Archive / Restore API
// =============================================================================

/// Request to archive a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArchiveSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Archive a session
///
/// POST /v1/chat/sessions/:session_id/archive
pub async fn archive_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ArchiveSessionRequest>,
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    state
        .db
        .archive_session(&session_id, &claims.sub, req.reason.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to archive session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Restore a deleted or archived session (admin-only)
///
/// POST /v1/chat/sessions/:session_id/restore
pub async fn restore_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Admin-only: requires WorkspaceManage
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - restore requires WorkspaceManage")
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    state.db.restore_session(&session_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to restore session")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    info!(session_id = %session_id, user = %claims.sub, "Session restored");
    Ok(StatusCode::NO_CONTENT)
}

/// Permanently delete a session
///
/// DELETE /v1/chat/sessions/:session_id/permanent
pub async fn hard_delete_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Admin-only
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

    state
        .db
        .hard_delete_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to delete session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(session_id = %session_id, user = %claims.sub, "Session permanently deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// Query parameters for listing archived sessions
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListArchivedQuery {
    pub limit: Option<i64>,
}

/// List archived sessions
///
/// GET /v1/chat/sessions/archived
pub async fn list_archived_sessions(
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

    let sessions = state
        .db
        .list_archived_sessions(&claims.tenant_id, Some(&claims.sub), query.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list archived sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(sessions))
}

/// List deleted sessions (trash)
///
/// GET /v1/chat/sessions/trash
pub async fn list_deleted_sessions(
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

    let sessions = state
        .db
        .list_deleted_sessions(&claims.tenant_id, Some(&claims.sub), query.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list deleted sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(sessions))
}

// =============================================================================
// Search API
// =============================================================================

/// Query parameters for session search
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct SearchSessionsQuery {
    pub q: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub category_id: Option<String>,
    pub tags: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_scope() -> String {
    "all".to_string()
}
fn default_limit() -> i64 {
    20
}

/// Search chat sessions and messages
///
/// GET /v1/chat/sessions/search
pub async fn search_chat_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<SearchSessionsQuery>,
) -> Result<Json<Vec<ChatSearchResult>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    if query.q.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Search query must be at least 2 characters")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    let tag_ids: Option<Vec<String>> = query
        .tags
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    let results = state
        .db
        .search_chat_sessions(
            &claims.tenant_id,
            &query.q,
            &query.scope,
            query.category_id.as_deref(),
            tag_ids.as_deref(),
            query.include_archived,
            query.limit,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Search failed")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(results))
}

// =============================================================================
// Sharing API
// =============================================================================

/// Request to share a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShareSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub permission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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

    if session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        ));
    }

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
