//! Routing Decisions API handlers for PRD-04
//!
//! Endpoints:
//! - GET /v1/routing/decisions - Query routing decisions with filters
//! - GET /v1/routing/decisions/:id - Get specific routing decision
//! - POST /v1/telemetry/routing - Ingest router decision events (internal)

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::users::Role;
use adapteros_db::{
    routing_decision_chain::ChainVerification, RouterCandidate as DbRouterCandidate,
    RoutingDecision as DbRoutingDecision, RoutingDecisionChainRecord, RoutingDecisionFilters,
};
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::{response::IntoResponse, Json};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};
use utoipa::ToSchema;
use uuid::Uuid;

// ===== Stub Response Types =====

/// Simple ID response for ingestion endpoints
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IdResponse {
    /// Created resource ID
    pub id: String,
}

/// Response for not-implemented endpoints
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NotImplementedResponse {
    /// Status indicating not implemented
    pub status: String,
    /// Description message
    pub message: String,
}

/// Empty routing history response (placeholder)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct RoutingHistoryResponse {
    /// History entries (currently empty)
    pub entries: Vec<serde_json::Value>,
}

// ===== Request Types =====

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
    pub source_type: Option<String>,
    pub min_entropy: Option<f64>,
    pub max_overhead_pct: Option<f64>,
    pub anomalies_only: Option<bool>,
}

/// Response containing routing decisions
#[derive(Debug, Serialize, ToSchema)]
pub struct RoutingDecisionsResponse {
    pub items: Vec<RoutingDecisionResponse>,
}

/// Query parameters for routing decision chain
#[derive(Debug, Deserialize)]
pub struct RoutingChainQuery {
    pub tenant: String,
    pub inference_id: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[serde(default)]
    pub verify: bool,
}

/// Routing decision chain response wrapper
#[derive(Debug, Serialize, ToSchema)]
pub struct RoutingDecisionChainResponse {
    pub items: Vec<RoutingDecisionChainItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<ChainVerificationSchema>,
}

/// API schema version of chain verification (avoids pulling utoipa into DB types)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ChainVerificationSchema {
    pub is_valid: bool,
    pub entries_checked: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_invalid_step: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl From<ChainVerification> for ChainVerificationSchema {
    fn from(v: ChainVerification) -> Self {
        Self {
            is_valid: v.is_valid,
            entries_checked: v.entries_checked,
            first_invalid_step: v.first_invalid_step,
            error: v.error,
        }
    }
}

/// Routing decision chain item
#[derive(Debug, Serialize, ToSchema)]
pub struct RoutingDecisionChainItem {
    pub step: i64,
    pub input_token_id: Option<i64>,
    pub adapter_indices: Vec<u16>,
    pub adapter_ids: Vec<String>,
    pub gates_q15: Vec<i16>,
    pub entropy: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    pub entry_hash: String,
    pub created_at: String,
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
        (status = 200, description = "Decision ingested successfully", body = IdResponse),
        (status = 500, description = "Failed to ingest decision", body = ErrorResponse)
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn ingest_router_decision(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<IngestRouterDecisionRequest>,
) -> Result<Json<IdResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;
    validate_tenant_isolation(&claims, &request.tenant_id)?;

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

    // Return 200 OK (consistent with other ingestion endpoints)
    // Status code can be set via response builder if 201 is required
    Ok(Json(IdResponse { id }))
}

/// GET /v1/routing/chain - Fetch cryptographically chained per-token routing entries
#[utoipa::path(
    get,
    path = "/v1/routing/chain",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("inference_id" = String, Query, description = "Inference/request ID")
    ),
    responses(
        (status = 200, description = "Routing decision chain", body = RoutingDecisionChainResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Failed to fetch routing decision chain")
    ),
    tag = "routing",
    security(("bearer_token" = []))
)]
pub async fn get_routing_decision_chain(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<RoutingChainQuery>,
) -> Result<Json<RoutingDecisionChainResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;
    validate_tenant_isolation(&claims, &query.tenant)?;

    let records = state
        .db
        .get_routing_decision_chain(
            &query.tenant,
            &query.inference_id,
            query.limit,
            query.offset,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to fetch routing decision chain")
                        .with_code("ROUTING_CHAIN_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let items: Vec<RoutingDecisionChainItem> =
        records.into_iter().map(chain_record_to_response).collect();

    let verification = if query.verify {
        match state
            .db
            .verify_routing_decision_chain(&query.tenant, &query.inference_id)
            .await
        {
            Ok(v) => Some(v.into()),
            Err(e) => {
                warn!(error = %e, "Failed to verify routing decision chain");
                None
            }
        }
    } else {
        None
    };

    Ok(Json(RoutingDecisionChainResponse {
        items,
        verification,
    }))
}

fn chain_record_to_response(rec: RoutingDecisionChainRecord) -> RoutingDecisionChainItem {
    fn parse_vec<T: DeserializeOwned>(raw: &str) -> Vec<T> {
        // Safe to use default on parse failure: chain records may have empty/malformed JSON from legacy data.
        // Empty vec is semantically correct for missing data in display contexts.
        serde_json::from_str(raw).unwrap_or_else(|e| {
            warn!(error = %e, raw = %raw, "Failed to parse chain record JSON array, using empty vec");
            Vec::default()
        })
    }

    let decision_hash = rec
        .decision_hash_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<Value>(json).ok());

    RoutingDecisionChainItem {
        step: rec.step,
        input_token_id: rec.input_token_id,
        adapter_indices: parse_vec::<u16>(&rec.adapter_indices),
        adapter_ids: parse_vec::<String>(&rec.adapter_ids),
        gates_q15: parse_vec::<i16>(&rec.gates_q15),
        entropy: rec.entropy,
        decision_hash,
        previous_hash: rec.previous_hash,
        entry_hash: rec.entry_hash,
        created_at: rec.created_at,
    }
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
        ("source_type" = Option<String>, Query, description = "Filter by chat source_type via session request_id"),
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
    validate_tenant_isolation(&claims, &query.tenant)?;

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
        source_type: query.source_type.clone(),
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
        .map(convert_decision_to_response)
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

    validate_tenant_isolation(&claims, &decision.tenant_id)?;

    Ok(Json(convert_decision_to_response(decision)))
}

/// Adapter usage statistics response
#[derive(Debug, Serialize, ToSchema)]
pub struct AdapterUsageResponse {
    pub adapter_id: String,
    pub call_count: i64,
    pub average_gate_value: f64,
    pub last_used: Option<String>,
}

/// Adapter fired in a step
#[derive(Debug, Serialize, ToSchema)]
pub struct AdapterFired {
    pub adapter_idx: u16,
    pub gate_value: f32,
    pub selected: bool,
}

/// Step in a chat session
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionStep {
    pub step: i64,
    pub timestamp: String,
    pub input_token_id: Option<i64>,
    pub adapters_fired: Vec<AdapterFired>,
    pub entropy: f64,
    pub tau: f64,
}

/// Chat session router view response
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionRouterViewResponse {
    pub request_id: String,
    pub stack_id: Option<String>,
    pub stack_hash: Option<String>,
    pub steps: Vec<SessionStep>,
    pub total_steps: usize,
}

/// GET /v1/adapters/:adapter_id/usage - Get adapter usage statistics
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/usage",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter usage statistics", body = AdapterUsageResponse),
        (status = 404, description = "Adapter not found")
    ),
    tag = "adapters",
    security(("bearer_token" = []))
)]
pub async fn get_adapter_usage(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    debug!(adapter_id = %adapter_id, "Querying adapter usage statistics");

    // PRD-RECT-001: Use tenant-scoped query to prevent cross-tenant enumeration.
    // Returns 404 for both missing and cross-tenant adapters.
    let _adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            warn!(error = %e, adapter_id = %adapter_id, "Failed to query adapter");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to query adapter")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            // Returns same error for both "not found" and "cross-tenant" cases
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter '{}' not found", adapter_id)),
                ),
            )
        })?;
    // No validate_tenant_isolation() call needed - query is already tenant-scoped

    // Get usage statistics from routing decisions
    let (call_count, avg_gate, last_used) = state
        .db
        .get_adapter_usage_stats(&adapter_id)
        .await
        .map_err(|e| {
            warn!(error = %e, adapter_id = %adapter_id, "Failed to get adapter usage stats");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get adapter usage statistics")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(AdapterUsageResponse {
        adapter_id,
        call_count,
        average_gate_value: avg_gate,
        last_used,
    }))
}

/// GET /v1/routing/sessions/:request_id - Get router decisions for a chat session
#[utoipa::path(
    get,
    path = "/v1/routing/sessions/{request_id}",
    params(
        ("request_id" = String, Path, description = "Request ID (session identifier)")
    ),
    responses(
        (status = 200, description = "Session router view", body = SessionRouterViewResponse),
        (status = 404, description = "Session not found")
    ),
    tag = "routing",
    security(("bearer_token" = []))
)]
pub async fn get_session_router_view(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(request_id): Path<String>,
) -> Result<Json<SessionRouterViewResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    debug!(request_id = %request_id, "Querying session router view");

    // Get routing decisions for this session
    let decisions = state
        .db
        .get_session_routing_decisions(&request_id, Some(1000))
        .await
        .map_err(|e| {
            warn!(error = %e, request_id = %request_id, "Failed to query session routing decisions");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to query session routing decisions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if decisions.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Session not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!(
                        "No routing decisions found for request_id '{}'",
                        request_id
                    )),
            ),
        ));
    }

    // Validate tenant isolation using the first decision's tenant_id
    // All decisions in a session should have the same tenant_id
    if let Some(first_decision) = decisions.first() {
        validate_tenant_isolation(&claims, &first_decision.tenant_id)?;
    }

    // Extract stack_id from first decision (all should have same stack_id for a session)
    let stack_id = decisions.first().and_then(|d| d.stack_id.clone());
    let stack_hash = decisions.first().and_then(|d| d.stack_hash.clone());

    // Convert decisions to steps, ordered by step ASC
    let mut steps: Vec<SessionStep> = decisions
        .into_iter()
        .map(|decision| {
            // Parse candidate adapters to extract adapters fired
            let adapters_fired: Vec<AdapterFired> =
                serde_json::from_str(&decision.candidate_adapters)
                    .ok()
                    .map(|candidates: Vec<DbRouterCandidate>| {
                        // Determine which candidates are selected (top-K)
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
                                AdapterFired {
                                    adapter_idx: c.adapter_idx,
                                    gate_value: gate_float,
                                    selected,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        // Safe to use empty vec on parse failure: allows session view to render with partial data.
                        // Empty adapters_fired indicates corrupted/missing routing data for this step.
                        warn!(
                            step = decision.step,
                            raw = %decision.candidate_adapters,
                            "Failed to parse candidate_adapters JSON, using empty vec"
                        );
                        Vec::default()
                    });

            SessionStep {
                step: decision.step,
                timestamp: decision.timestamp,
                input_token_id: decision.input_token_id,
                adapters_fired,
                entropy: decision.entropy,
                tau: decision.tau,
            }
        })
        .collect();

    // Sort by step ASC for timeline view
    steps.sort_by(|a, b| a.step.cmp(&b.step));

    Ok(Json(SessionRouterViewResponse {
        request_id,
        stack_id,
        stack_hash,
        total_steps: steps.len(),
        steps,
    }))
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
            .unwrap_or_else(|| {
                // Safe to use empty vec on parse failure: allows response to return with partial data.
                // Empty candidates indicates corrupted/missing routing data in database.
                warn!(
                    decision_id = %decision.id,
                    raw = %decision.candidate_adapters,
                    "Failed to parse candidate_adapters JSON, using empty vec"
                );
                Vec::default()
            });

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

/// Debug routing logic
#[utoipa::path(
    post,
    path = "/v1/routing/debug",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Routing debug info (not implemented)", body = NotImplementedResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn debug_routing(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(_payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Stub implementation
    Json(NotImplementedResponse {
        status: "not_implemented".to_string(),
        message: "Debug routing endpoint is not yet implemented".to_string(),
    })
    .into_response()
}

/// Get routing history
#[utoipa::path(
    get,
    path = "/v1/routing/history",
    responses(
        (status = 200, description = "Routing history", body = RoutingHistoryResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn get_routing_history(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> impl IntoResponse {
    // Stub implementation
    Json(RoutingHistoryResponse::default()).into_response()
}
