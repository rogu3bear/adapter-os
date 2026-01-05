//! Global search handler
//!
//! Provides server-side search across adapters and other entities.

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Search query parameters
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search query string (min 2 characters)
    pub q: String,
    /// Scope: "all", "adapters", "pages" (default: "all")
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Max results to return (default: 20, max: 50)
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_scope() -> String {
    "all".to_string()
}

fn default_limit() -> u32 {
    20
}

/// Search result item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchResult {
    /// Result type: "adapter", "page", etc.
    pub result_type: String,
    /// Unique ID
    pub id: String,
    /// Display title
    pub title: String,
    /// Subtitle/description
    pub subtitle: Option<String>,
    /// Link/path to navigate to
    pub path: String,
    /// Relevance score (0.0 - 1.0)
    pub score: f32,
}

/// Search response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    /// Search results
    pub results: Vec<SearchResult>,
    /// Total count (may be approximate)
    pub total: u32,
    /// Query execution time in milliseconds
    pub took_ms: u64,
}

/// Global search endpoint
///
/// GET /v1/search
#[utoipa::path(
    get,
    path = "/v1/search",
    params(
        ("q" = String, Query, description = "Search query (min 2 chars)"),
        ("scope" = Option<String>, Query, description = "Search scope: all, adapters, pages"),
        ("limit" = Option<u32>, Query, description = "Max results (default: 20, max: 50)")
    ),
    responses(
        (status = 200, description = "Search results", body = SearchResponse),
        (status = 400, description = "Invalid query"),
        (status = 403, description = "Permission denied")
    ),
    tag = "search"
)]
pub async fn global_search(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    // Validate query
    if query.q.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Search query must be at least 2 characters")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    let start = std::time::Instant::now();
    let limit = query.limit.min(50) as usize;
    let mut results = Vec::new();

    // Search adapters if scope includes them
    if query.scope == "all" || query.scope == "adapters" {
        let adapter_results = search_adapters(&state, &claims.tenant_id, &query.q, limit).await?;
        results.extend(adapter_results);
    }

    // Add page search results if scope includes them
    if query.scope == "all" || query.scope == "pages" {
        let page_results = search_pages(&query.q, limit);
        results.extend(page_results);
    }

    // Sort by score and limit
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    let total = results.len() as u32;
    let took_ms = start.elapsed().as_millis() as u64;

    Ok(Json(SearchResponse {
        results,
        total,
        took_ms,
    }))
}

/// Search adapters using SQL LIKE
async fn search_adapters(
    state: &AppState,
    tenant_id: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, (StatusCode, Json<ErrorResponse>)> {
    // Use SQL LIKE for simple fuzzy matching
    let search_pattern = format!("%{}%", query.to_lowercase());

    let adapters = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        r#"
        SELECT id, adapter_id, name, intent
        FROM adapters
        WHERE tenant_id = ?
          AND active = 1
          AND (
            LOWER(name) LIKE ?
            OR LOWER(adapter_id) LIKE ?
            OR LOWER(intent) LIKE ?
          )
        ORDER BY activation_count DESC, created_at DESC
        LIMIT ?
        "#,
    )
    .bind(tenant_id)
    .bind(&search_pattern)
    .bind(&search_pattern)
    .bind(&search_pattern)
    .bind(limit as i64)
    .fetch_all(state.db.pool())
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

    let results = adapters
        .into_iter()
        .map(|(id, adapter_id, name, intent)| {
            // Calculate a simple relevance score based on match type
            let query_lower = query.to_lowercase();
            let name_lower = name.to_lowercase();
            let score = if name_lower == query_lower {
                1.0 // Exact match
            } else if name_lower.starts_with(&query_lower) {
                0.9 // Prefix match
            } else if name_lower.contains(&query_lower) {
                0.7 // Substring match
            } else {
                0.5 // Match in other fields
            };

            SearchResult {
                result_type: "adapter".to_string(),
                id: id.clone(),
                title: name,
                subtitle: intent,
                path: format!("/adapters/{}", id),
                score,
            }
        })
        .collect();

    Ok(results)
}

/// Search pages (static navigation items)
fn search_pages(query: &str, limit: usize) -> Vec<SearchResult> {
    let pages = [
        ("Dashboard", "/", "Home dashboard with system overview"),
        ("Adapters", "/adapters", "Manage LoRA adapters"),
        ("Chat", "/chat", "Interactive chat interface"),
        ("Training", "/training", "Training jobs and datasets"),
        ("Documents", "/documents", "Document management"),
        ("Repositories", "/repositories", "Code repositories"),
        ("System", "/system", "System configuration and settings"),
        ("Settings", "/settings", "User preferences and API settings"),
    ];

    let query_lower = query.to_lowercase();

    pages
        .iter()
        .filter_map(|(name, path, desc)| {
            let name_lower = name.to_lowercase();
            let desc_lower = desc.to_lowercase();

            let score = if name_lower == query_lower {
                1.0
            } else if name_lower.starts_with(&query_lower) {
                0.9
            } else if name_lower.contains(&query_lower) {
                0.7
            } else if desc_lower.contains(&query_lower) {
                0.5
            } else {
                return None;
            };

            Some(SearchResult {
                result_type: "page".to_string(),
                id: path.to_string(),
                title: name.to_string(),
                subtitle: Some(desc.to_string()),
                path: path.to_string(),
                score,
            })
        })
        .take(limit)
        .collect()
}
