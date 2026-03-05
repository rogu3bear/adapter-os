//! Collection binding handler for chat sessions
//!
//! Provides update_session_collection handler.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_collection】

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::ChatSession;
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use tracing::info;

use super::access::ensure_session_write_access;
use super::types::UpdateCollectionRequest;

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
) -> ApiResult<ChatSession> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_e| ApiError::forbidden("Permission denied"))?;
    let session_id = crate::id_resolver::resolve_any_id(&state.db, &session_id).await?;

    // Verify session exists and tenant has access
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    ensure_session_write_access(&state, &claims, &session).await?;

    // Update collection binding
    state
        .db
        .update_session_collection(&session_id, request.collection_id.clone())
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    // Retrieve updated session
    let updated_session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::internal("Session not found after update"))?;

    info!(
        session_id = %session_id,
        collection_id = ?request.collection_id,
        tenant_id = %claims.tenant_id,
        "Chat session collection updated"
    );

    Ok(Json(updated_session))
}
