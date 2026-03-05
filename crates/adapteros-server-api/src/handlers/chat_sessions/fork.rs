//! Fork handler for chat sessions
//!
//! Provides fork_chat_session handler.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_fork】

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use tracing::info;

use super::access::ensure_session_read_access;
use super::types::{ForkChatSessionRequest, ForkChatSessionResponse, ForkedFromInfo};

/// Fork an existing chat session
///
/// Creates a copy of a chat session with a new ID. Optionally copies
/// all messages from the source session.
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/fork",
    request_body = ForkChatSessionRequest,
    params(
        ("session_id" = String, Path, description = "Session ID to fork")
    ),
    responses(
        (status = 201, description = "Session forked successfully", body = ForkChatSessionResponse),
        (status = 404, description = "Source session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "chat"
)]
pub async fn fork_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ForkChatSessionRequest>,
) -> Result<(StatusCode, Json<ForkChatSessionResponse>), ApiError> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;

    // First get the source session name for the response
    let source_session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, session_id = %session_id, "Failed to get source session");
            ApiError::db_error(&e)
        })?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    ensure_session_read_access(&state, &claims, &source_session).await?;

    let source_name = source_session.name.clone();

    // Fork the session
    let new_session = state
        .db
        .fork_session(
            &claims.tenant_id,
            &session_id,
            req.name.as_deref(),
            req.include_messages,
        )
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("not found") || error_str.contains("NotFound") {
                ApiError::not_found("Session")
            } else {
                tracing::error!(error = %e, session_id = %session_id, "Failed to fork session");
                ApiError::db_error(&e).with_details(format!("Failed to fork session: {}", e))
            }
        })?;

    info!(
        source_session_id = %session_id,
        new_session_id = %new_session.id,
        tenant_id = %claims.tenant_id,
        include_messages = req.include_messages,
        "Forked chat session"
    );

    Ok((
        StatusCode::CREATED,
        Json(ForkChatSessionResponse {
            session_id: new_session.id,
            name: new_session.name,
            created_at: new_session.created_at,
            forked_from: ForkedFromInfo {
                session_id,
                name: source_name,
            },
        }),
    ))
}
