//! Search handler for chat sessions
//!
//! Provides search_chat_sessions handler.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_search】

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use adapteros_db::ChatSearchResult;
use axum::{
    extract::{Query, State},
    Extension, Json,
};

use super::types::SearchSessionsQuery;

/// Search chat sessions and messages
///
/// GET /v1/chat/sessions/search
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/search",
    params(
        SearchSessionsQuery
    ),
    responses(
        (status = 200, description = "Search results"),
        (status = 400, description = "Invalid query"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn search_chat_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<SearchSessionsQuery>,
) -> ApiResult<Vec<ChatSearchResult>> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if query.q.len() < 2 {
        return Err(ApiError::bad_request(
            "Search query must be at least 2 characters",
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
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(Json(results))
}
