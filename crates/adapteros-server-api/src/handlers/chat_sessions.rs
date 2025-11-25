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
    AddMessageParams, ChatMessage, ChatSession, ChatSessionTrace, CreateChatSessionParams,
    InferenceEvidence,
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
        (status = 201, description = "Message added", body = ChatMessage),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn add_chat_message(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<AddChatMessageRequest>,
) -> Result<(StatusCode, Json<ChatMessage>), (StatusCode, Json<ErrorResponse>)> {
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

    Ok((StatusCode::CREATED, Json(message)))
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
        (status = 200, description = "Messages retrieved", body = Vec<ChatMessage>),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_chat_messages(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<ChatMessage>>, (StatusCode, Json<ErrorResponse>)> {
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

    Ok(Json(messages))
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

/// Delete a chat session
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
        (status = 204, description = "Session deleted"),
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

    // Delete session
    state
        .db
        .delete_chat_session(&session_id)
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
        "Chat session deleted"
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
