//! Embedding benchmark handlers
//!
//! API handlers for listing embedding benchmark reports.

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::embeddings::{
    EmbeddingBenchmarkReport, EmbeddingBenchmarksQuery, EmbeddingBenchmarksResponse,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};

/// List embedding benchmark reports
///
/// Returns benchmark results for the current tenant, ordered by timestamp descending.
#[utoipa::path(
    get,
    path = "/v1/embeddings/benchmarks",
    tag = "Embeddings",
    security(("bearer_auth" = [])),
    params(EmbeddingBenchmarksQuery),
    responses(
        (status = 200, description = "List of embedding benchmarks", body = EmbeddingBenchmarksResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_embedding_benchmarks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<EmbeddingBenchmarksQuery>,
) -> Result<Json<EmbeddingBenchmarksResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::MetricsView)?;

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    // Fetch from database
    let rows = state
        .db
        .list_embedding_benchmarks(
            &claims.tenant_id,
            query.model_name.as_deref(),
            limit,
            offset,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to fetch benchmarks")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let total = state
        .db
        .count_embedding_benchmarks(&claims.tenant_id, query.model_name.as_deref())
        .await
        .unwrap_or(0) as usize;

    // Convert to API types
    let benchmarks: Vec<EmbeddingBenchmarkReport> = rows
        .into_iter()
        .map(|row| EmbeddingBenchmarkReport {
            report_id: row.report_id,
            timestamp: row.timestamp,
            model_name: row.model_name,
            model_hash: row.model_hash,
            is_finetuned: row.is_finetuned,
            corpus_version: row.corpus_version,
            num_chunks: row.num_chunks as usize,
            recall_at_10: row.recall_at_10,
            ndcg_at_10: row.ndcg_at_10,
            mrr_at_10: row.mrr_at_10,
            determinism_pass: row.determinism_pass,
            determinism_runs: row.determinism_runs as usize,
        })
        .collect();

    Ok(Json(EmbeddingBenchmarksResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        benchmarks,
        total,
    }))
}
