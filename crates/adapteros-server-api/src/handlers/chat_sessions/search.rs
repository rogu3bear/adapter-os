//! Search handler for chat sessions
//!
//! Provides search_chat_sessions handler.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_search】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::ChatSearchResult;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};

use super::types::SearchSessionsQuery;

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

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

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
