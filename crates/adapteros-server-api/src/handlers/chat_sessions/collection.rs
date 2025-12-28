//! Collection binding handler for chat sessions
//!
//! Provides update_session_collection handler.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_collection】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::ChatSession;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use tracing::info;

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
) -> Result<Json<ChatSession>, (StatusCode, Json<ErrorResponse>)> {
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
