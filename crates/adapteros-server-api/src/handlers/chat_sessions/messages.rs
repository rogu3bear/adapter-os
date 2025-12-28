//! Message handlers for chat sessions
//!
//! Provides add_chat_message, get_chat_messages, get_session_summary, get_message_evidence
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_messages】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::{AddMessageParams, InferenceEvidence};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use tracing::debug;

use super::types::{AddChatMessageRequest, ChatMessageResponse};

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
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
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

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    // Generate message ID
    let message_id = format!("msg-{}", uuid::Uuid::new_v4());

    // Add message
    let params = AddMessageParams {
        id: message_id.clone(),
        session_id: session_id.clone(),
        tenant_id: Some(session.tenant_id.clone()),
        role: req.role,
        content: req.content,
        sequence: None,
        created_at: None,
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
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
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

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

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
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
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

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

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
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Get evidence from database with tenant isolation
    let evidence = state
        .db
        .get_evidence_by_message(&claims.tenant_id, &message_id)
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
