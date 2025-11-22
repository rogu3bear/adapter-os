//! Routing Decisions API handlers for PRD-04
//!
//! Endpoints:
//! - GET /v1/routing/decisions - Query routing decisions with filters
//! - GET /v1/routing/decisions/:id - Get specific routing decision
//! - POST /v1/telemetry/routing - Ingest router decision events (internal)

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::users::Role;
use adapteros_db::{
    RouterCandidate as DbRouterCandidate, RoutingDecision as DbRoutingDecision,
    RoutingDecisionFilters,
};
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use utoipa::ToSchema;
use uuid::Uuid;

/// Router candidate for API schema
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RouterCandidateRequest {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,
}

/// Request to ingest a router decision event (internal endpoint)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestRouterDecisionRequest {
    pub tenant_id: String,
    pub request_id: Option<String>,
    pub step: usize,
    pub input_token_id: Option<u32>,
    pub candidate_adapters: Vec<RouterCandidateRequest>,
    pub entropy: f32,
    pub tau: f32,
    pub entropy_floor: f32,
    pub stack_hash: Option<String>,
    pub stack_id: Option<String>,
    pub router_latency_us: Option<u64>,
    pub total_inference_latency_us: Option<u64>,
}

/// Query parameters for routing decisions endpoint
#[derive(Debug, Deserialize)]
pub struct RoutingDecisionsQuery {
    pub tenant: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub stack_id: Option<String>,
    pub adapter_id: Option<String>,
    pub min_entropy: Option<f64>,
    pub max_overhead_pct: Option<f64>,
    pub anomalies_only: Option<bool>,
}

/// Response containing routing decisions
#[derive(Debug, Serialize, ToSchema)]
pub struct RoutingDecisionsResponse {
    pub items: Vec<RoutingDecisionResponse>,
}

/// Routing decision response (enriched with parsed candidates)
#[derive(Debug, Serialize, ToSchema)]
pub struct RoutingDecisionResponse {
    pub id: String,
    pub tenant_id: String,
    pub timestamp: String,
    pub request_id: Option<String>,
    pub step: i64,
    pub input_token_id: Option<i64>,
    pub stack_id: Option<String>,
    pub stack_hash: Option<String>,
    pub entropy: f64,
    pub tau: f64,
    pub entropy_floor: f64,
    pub k_value: Option<i64>,
    pub candidates: Vec<RouterCandidateResponse>,
    pub router_latency_us: Option<i64>,
    pub total_inference_latency_us: Option<i64>,
    pub overhead_pct: Option<f64>,
}

/// Router candidate response with computed fields
#[derive(Debug, Serialize, ToSchema)]
pub struct RouterCandidateResponse {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,
    pub gate_float: f32,
    pub selected: bool,
}

/// POST /v1/telemetry/routing - Ingest router decision event
///
/// This endpoint is called internally by the router to persist decision events.
/// It's non-blocking from the router's perspective (fire-and-forget).
#[utoipa::path(
    post,
    path = "/v1/telemetry/routing",
    request_body = IngestRouterDecisionRequest,
    responses(
        (status = 201, description = "Decision ingested successfully"),
        (status = 500, description = "Failed to ingest decision")
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn ingest_router_decision(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<IngestRouterDecisionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    debug!(
        tenant_id = %request.tenant_id,
        step = request.step,
        entropy = request.entropy,
        "Ingesting router decision event"
    );

    // Convert to database record
    let db_candidates: Vec<DbRouterCandidate> = request
        .candidate_adapters
        .iter()
        .map(|c| DbRouterCandidate {
            adapter_idx: c.adapter_idx,
            raw_score: c.raw_score,
            gate_q15: c.gate_q15,
        })
        .collect();

    let candidates_json = serde_json::to_string(&db_candidates).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Failed to serialize candidates")
                    .with_code("SERIALIZATION_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let selected_adapter_ids: Vec<String> = db_candidates
        .iter()
        .filter(|c| c.gate_q15 > 0) // Consider selected if gate > 0
        .map(|c| c.adapter_idx.to_string())
        .collect();

    let k_value = selected_adapter_ids.len() as i64;
    let overhead_pct = if let (Some(router_latency), Some(total_latency)) = (
        request.router_latency_us,
        request.total_inference_latency_us,
    ) {
        if total_latency > 0 {
            Some((router_latency as f64 / total_latency as f64) * 100.0)
        } else {
            None
        }
    } else {
        None
    };

    let decision = DbRoutingDecision {
        id: Uuid::new_v4().to_string(),
        tenant_id: request.tenant_id.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        request_id: request.request_id.clone(),
        step: request.step as i64,
        input_token_id: request.input_token_id.map(|v| v as i64),
        stack_id: request.stack_id.clone(),
        stack_hash: request.stack_hash.clone(),
        entropy: request.entropy as f64,
        tau: request.tau as f64,
        entropy_floor: request.entropy_floor as f64,
        k_value: Some(k_value),
        candidate_adapters: candidates_json,
        selected_adapter_ids: Some(selected_adapter_ids.join(",")),
        router_latency_us: request.router_latency_us.map(|v| v as i64),
        total_inference_latency_us: request.total_inference_latency_us.map(|v| v as i64),
        overhead_pct,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Insert into database
    let id = state
        .db
        .insert_routing_decision(&decision)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to insert routing decision");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to insert routing decision")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// GET /v1/routing/decisions - Query routing decisions with filters
#[utoipa::path(
    get,
    path = "/v1/routing/decisions",
    params(
        ("tenant" = String, Query, description = "Tenant ID (required)"),
        ("limit" = Option<usize>, Query, description = "Maximum number of results (default 50)"),
        ("offset" = Option<usize>, Query, description = "Offset for pagination"),
        ("since" = Option<String>, Query, description = "Start time (ISO-8601)"),
        ("until" = Option<String>, Query, description = "End time (ISO-8601)"),
        ("stack_id" = Option<String>, Query, description = "Filter by stack ID"),
        ("adapter_id" = Option<String>, Query, description = "Filter by adapter ID"),
        ("min_entropy" = Option<f64>, Query, description = "Minimum entropy threshold"),
        ("max_overhead_pct" = Option<f64>, Query, description = "Maximum overhead percentage"),
        ("anomalies_only" = Option<bool>, Query, description = "Show only anomalies (low entropy or high overhead)"),
    ),
    responses(
        (status = 200, description = "Routing decisions retrieved", body = RoutingDecisionsResponse)
    ),
    tag = "routing",
    security(("bearer_token" = []))
)]
pub async fn get_routing_decisions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<RoutingDecisionsQuery>,
) -> Result<Json<RoutingDecisionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    debug!(
        tenant_id = %query.tenant,
        limit = ?query.limit,
        "Querying routing decisions"
    );

    // Build filters
    let mut filters = RoutingDecisionFilters {
        tenant_id: Some(query.tenant.clone()),
        limit: query.limit,
        offset: query.offset,
        since: query.since.clone(),
        until: query.until.clone(),
        stack_id: query.stack_id.clone(),
        adapter_id: query.adapter_id.clone(),
        request_id: None,
        min_entropy: query.min_entropy,
        max_overhead_pct: query.max_overhead_pct,
    };

    // If anomalies_only is true, apply thresholds
    if query.anomalies_only.unwrap_or(false) {
        if filters.min_entropy.is_none() {
            filters.min_entropy = Some(0.0);
        }
        if filters.max_overhead_pct.is_none() {
            filters.max_overhead_pct = Some(8.0); // Budget threshold
        }
    }

    // Query database
    let decisions = state
        .db
        .query_routing_decisions(&filters)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to query routing decisions");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to query routing decisions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Convert to response format
    let items: Vec<RoutingDecisionResponse> = decisions
        .into_iter()
        .map(|d| convert_decision_to_response(d))
        .collect();

    Ok(Json(RoutingDecisionsResponse { items }))
}

/// GET /v1/routing/decisions/:id - Get specific routing decision
#[utoipa::path(
    get,
    path = "/v1/routing/decisions/{id}",
    params(
        ("id" = String, Path, description = "Routing decision ID")
    ),
    responses(
        (status = 200, description = "Routing decision found", body = RoutingDecisionResponse),
        (status = 404, description = "Routing decision not found")
    ),
    tag = "routing",
    security(("bearer_token" = []))
)]
pub async fn get_routing_decision_by_id(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<RoutingDecisionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    let decision = state.db.get_routing_decision(&id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Routing decision not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(convert_decision_to_response(decision)))
}

/// Convert database routing decision to API response
fn convert_decision_to_response(decision: DbRoutingDecision) -> RoutingDecisionResponse {
    // Parse candidate adapters from JSON
    let candidates: Vec<RouterCandidateResponse> =
        serde_json::from_str(&decision.candidate_adapters)
            .ok()
            .map(|candidates: Vec<DbRouterCandidate>| {
                // Determine which candidates are selected (top-K with highest gates)
                let mut sorted_candidates = candidates.clone();
                sorted_candidates.sort_by(|a, b| b.gate_q15.cmp(&a.gate_q15));

                let k = decision.k_value.unwrap_or(0) as usize;
                let selected_indices: std::collections::HashSet<u16> = sorted_candidates
                    .iter()
                    .take(k)
                    .map(|c| c.adapter_idx)
                    .collect();

                candidates
                    .into_iter()
                    .map(|c| {
                        let selected = selected_indices.contains(&c.adapter_idx);
                        let gate_float = (c.gate_q15 as f32) / 32767.0;
                        RouterCandidateResponse {
                            adapter_idx: c.adapter_idx,
                            raw_score: c.raw_score,
                            gate_q15: c.gate_q15,
                            gate_float,
                            selected,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

    RoutingDecisionResponse {
        id: decision.id,
        tenant_id: decision.tenant_id,
        timestamp: decision.timestamp,
        request_id: decision.request_id,
        step: decision.step,
        input_token_id: decision.input_token_id,
        stack_id: decision.stack_id,
        stack_hash: decision.stack_hash,
        entropy: decision.entropy,
        tau: decision.tau,
        entropy_floor: decision.entropy_floor,
        k_value: decision.k_value,
        candidates,
        router_latency_us: decision.router_latency_us,
        total_inference_latency_us: decision.total_inference_latency_us,
        overhead_pct: decision.overhead_pct,
    }
}
