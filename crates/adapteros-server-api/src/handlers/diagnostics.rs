//! Diagnostics API handlers for PRD G2
//!
//! Endpoints:
//! - GET /v1/diagnostics/determinism-status - Get determinism check status
//! - GET /v1/diagnostics/quarantine-status - Get quarantine status
//! - GET /v1/diag/runs - List diagnostic runs (tenant-safe, paginated)
//! - GET /v1/diag/runs/{trace_id} - Get diagnostic run by trace_id
//! - GET /v1/diag/runs/{trace_id}/events - List events for a run (paginated)
//! - POST /v1/diag/diff - Compare two diagnostic runs
//! - POST /v1/diag/export - Export diagnostic data

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::diagnostics::{
    AnchorComparison, DeterminismFreshnessReason, DeterminismFreshnessStatus,
    DeterminismStatusResponse, DiagDiffRequest, DiagDiffResponse, DiagDiffSummary,
    DiagEventResponse, DiagExportRequest, DiagExportResponse, DiagRunResponse, EventDiff,
    ExportMetadata, FirstDivergence, ListDiagEventsQuery, ListDiagEventsResponse,
    ListDiagRunsQuery, ListDiagRunsResponse, RouterStepDiff, StageTiming, TimingDiff,
};
use adapteros_db::diagnostics::{
    get_all_diag_events_for_run, get_diag_run_by_id, get_diag_run_by_trace_id,
    get_router_step_events, get_stage_timing_summary, list_diag_events_paginated,
    list_diag_runs_paginated, DiagEventRecord, DiagRunRecord,
};
use adapteros_db::users::Role;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use sqlx::Row;
use tracing::{debug, warn};
use utoipa::ToSchema;

const DETERMINISM_STATUS_STALE_AFTER_SECS: i64 = 60 * 60;

#[derive(Debug, Clone, Copy)]
struct DeterminismFreshnessEvaluation {
    status: DeterminismFreshnessStatus,
    reason: DeterminismFreshnessReason,
    age_seconds: Option<i64>,
}

fn parse_determinism_last_run(last_run: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(last_run) {
        return Some(parsed.with_timezone(&chrono::Utc));
    }

    chrono::NaiveDateTime::parse_from_str(last_run, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(last_run, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .map(|parsed| {
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(parsed, chrono::Utc)
        })
}

fn evaluate_determinism_freshness(
    last_run: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> DeterminismFreshnessEvaluation {
    let Some(last_run) = last_run else {
        return DeterminismFreshnessEvaluation {
            status: DeterminismFreshnessStatus::Unknown,
            reason: DeterminismFreshnessReason::MissingLastRun,
            age_seconds: None,
        };
    };

    let Some(parsed_last_run) = parse_determinism_last_run(last_run) else {
        return DeterminismFreshnessEvaluation {
            status: DeterminismFreshnessStatus::Unknown,
            reason: DeterminismFreshnessReason::InvalidLastRunFormat,
            age_seconds: None,
        };
    };

    let age_seconds = now.signed_duration_since(parsed_last_run).num_seconds();
    if age_seconds < 0 {
        return DeterminismFreshnessEvaluation {
            status: DeterminismFreshnessStatus::Unknown,
            reason: DeterminismFreshnessReason::FutureLastRun,
            age_seconds: None,
        };
    }

    if age_seconds <= DETERMINISM_STATUS_STALE_AFTER_SECS {
        DeterminismFreshnessEvaluation {
            status: DeterminismFreshnessStatus::Fresh,
            reason: DeterminismFreshnessReason::RecentRun,
            age_seconds: Some(age_seconds),
        }
    } else {
        DeterminismFreshnessEvaluation {
            status: DeterminismFreshnessStatus::Stale,
            reason: DeterminismFreshnessReason::StaleLastRun,
            age_seconds: Some(age_seconds),
        }
    }
}

/// GET /v1/diagnostics/determinism-status - Get determinism check status
#[utoipa::path(
    get,
    path = "/v1/diagnostics/determinism-status",
    responses(
        (status = 200, description = "Determinism check status", body = DeterminismStatusResponse)
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn get_determinism_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<DeterminismStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    debug!("Querying determinism check status");

    // Query for last determinism check run
    // For MVP, we'll store this in a simple table or use a file-based approach
    // In production, this would be stored in a diagnostics_runs table
    let query_result = sqlx::query(
        "SELECT last_run, result, runs, divergences 
         FROM determinism_checks 
         ORDER BY last_run DESC 
         LIMIT 1",
    )
    .fetch_optional(state.db.pool())
    .await;

    match query_result {
        Ok(Some(row)) => {
            let last_run: Option<String> = row.try_get("last_run").unwrap_or_else(|e| {
                warn!("Failed to get last_run from determinism check row: {}", e);
                None
            });
            let check_result: Option<String> = row.try_get("result").unwrap_or_else(|e| {
                warn!("Failed to get result from determinism check row: {}", e);
                None
            });
            let runs: Option<i64> = row.try_get("runs").unwrap_or_else(|e| {
                warn!("Failed to get runs from determinism check row: {}", e);
                None
            });
            let divergences: Option<i64> = row.try_get("divergences").unwrap_or_else(|e| {
                warn!(
                    "Failed to get divergences from determinism check row: {}",
                    e
                );
                None
            });
            let freshness = evaluate_determinism_freshness(last_run.as_deref(), chrono::Utc::now());

            Ok(Json(DeterminismStatusResponse {
                last_run,
                result: check_result,
                runs: runs.map(|r| r as usize),
                divergences: divergences.map(|d| d as usize),
                freshness_status: freshness.status,
                freshness_reason: freshness.reason,
                freshness_age_seconds: freshness.age_seconds,
                stale_after_seconds: DETERMINISM_STATUS_STALE_AFTER_SECS,
            }))
        }
        Ok(None) => {
            // No previous check
            Ok(Json(DeterminismStatusResponse {
                last_run: None,
                result: None,
                runs: None,
                divergences: None,
                freshness_status: DeterminismFreshnessStatus::Unknown,
                freshness_reason: DeterminismFreshnessReason::NoDeterminismChecks,
                freshness_age_seconds: None,
                stale_after_seconds: DETERMINISM_STATUS_STALE_AFTER_SECS,
            }))
        }
        Err(e) => {
            // Table might not exist yet - return empty status
            debug!("Determinism checks table not found or error: {}", e);
            Ok(Json(DeterminismStatusResponse {
                last_run: None,
                result: None,
                runs: None,
                divergences: None,
                freshness_status: DeterminismFreshnessStatus::Unknown,
                freshness_reason: DeterminismFreshnessReason::QueryError,
                freshness_age_seconds: None,
                stale_after_seconds: DETERMINISM_STATUS_STALE_AFTER_SECS,
            }))
        }
    }
}

/// Quarantine status response
#[derive(Debug, Serialize, ToSchema)]
pub struct QuarantineStatusResponse {
    pub is_quarantined: bool,
    pub reason: Option<String>,
    pub quarantined_adapters: Vec<String>,
    pub active_quarantined_adapters: Vec<(String, String)>, // (adapter_id, stack_id)
    pub last_checked: Option<String>,
}

/// Quarantined adapter info
#[derive(Debug, Serialize, ToSchema)]
pub struct QuarantinedAdapter {
    pub id: String,
    pub reason: String,
    pub created_at: String,
}

/// GET /v1/diagnostics/quarantine-status - Get quarantine status
#[utoipa::path(
    get,
    path = "/v1/diagnostics/quarantine-status",
    responses(
        (status = 200, description = "Quarantine status", body = QuarantineStatusResponse)
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn get_quarantine_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<QuarantineStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    debug!("Querying quarantine status");

    // Query active quarantines from database (PRD G2)
    let quarantines = sqlx::query(
        "SELECT id, reason, created_at, violation_type, cpid, metadata 
         FROM active_quarantine 
         ORDER BY created_at DESC",
    )
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to query quarantines");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to query quarantine status")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Extract adapter IDs from quarantine records (improved logic from diag.rs)
    let mut quarantined_adapter_ids = Vec::new();
    let mut system_quarantined = false;
    let mut quarantine_reason: Option<String> = None;

    for row in &quarantines {
        let id: String = row.try_get("id").unwrap_or_else(|e| {
            warn!("Failed to get id from quarantine row: {}", e);
            String::new()
        });
        let reason: String = row.try_get("reason").unwrap_or_else(|e| {
            warn!("Failed to get reason from quarantine row: {}", e);
            String::new()
        });
        let metadata: Option<String> = row.try_get("metadata").unwrap_or_else(|e| {
            warn!("Failed to get metadata from quarantine row: {}", e);
            None
        });

        // Check if this is a system-wide quarantine (not adapter-specific)
        if reason.contains("policy hash") || reason.contains("system") || id == "system" {
            system_quarantined = true;
            if quarantine_reason.is_none() {
                quarantine_reason = Some(reason.clone());
            }
        }

        // Try to extract adapter ID from metadata JSON
        let mut adapter_id: Option<String> = None;
        if let Some(ref meta_str) = metadata {
            if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(meta_str) {
                if let Some(serde_json::Value::String(adapter_id_str)) = meta_json.get("adapter_id")
                {
                    adapter_id = Some(adapter_id_str.clone());
                } else if let Some(serde_json::Value::String(adapter_id_str)) =
                    meta_json.get("adapter")
                {
                    adapter_id = Some(adapter_id_str.clone());
                }
            }
        }

        // Fall back to extracting from reason
        if adapter_id.is_none() {
            for part in reason.split_whitespace() {
                if part.starts_with("adapter:") || part.starts_with("Adapter:") {
                    if let Some(id_part) = part.split(':').nth(1) {
                        adapter_id = Some(id_part.trim().to_string());
                        break;
                    }
                }
            }
            if adapter_id.is_none() && !reason.contains(' ') && reason.len() > 5 {
                adapter_id = Some(reason.clone());
            }
        }

        if let Some(adapter_id_str) = adapter_id {
            if !quarantined_adapter_ids.contains(&adapter_id_str) {
                quarantined_adapter_ids.push(adapter_id_str);
            }
        }
    }

    // Check for quarantined adapters in active stacks
    let active_stacks = sqlx::query(
        "SELECT id, name, adapter_ids_json 
         FROM adapter_stacks 
         WHERE active = 1",
    )
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to list active stacks for quarantine check");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to list active stacks").with_code("DATABASE_ERROR")),
        )
    })?;

    let mut active_quarantined_adapters = Vec::new();
    for stack_row in &active_stacks {
        let stack_id: String = stack_row.try_get("id").unwrap_or_else(|e| {
            warn!("Failed to get id from stack row: {}", e);
            String::new()
        });
        let adapter_ids_json: Option<String> =
            stack_row.try_get("adapter_ids_json").unwrap_or_else(|e| {
                warn!("Failed to get adapter_ids_json from stack row: {}", e);
                None
            });

        if let Some(ref json_str) = adapter_ids_json {
            if let Ok(adapter_ids) = serde_json::from_str::<Vec<String>>(json_str) {
                for adapter_id in &adapter_ids {
                    if quarantined_adapter_ids.contains(adapter_id) {
                        active_quarantined_adapters.push((adapter_id.clone(), stack_id.clone()));
                    }
                }
            }
        }
    }

    Ok(Json(QuarantineStatusResponse {
        is_quarantined: system_quarantined || !quarantined_adapter_ids.is_empty(),
        reason: quarantine_reason,
        quarantined_adapters: quarantined_adapter_ids,
        active_quarantined_adapters,
        last_checked: Some(chrono::Utc::now().to_rfc3339()),
    }))
}

// ============================================================================
// Tenant-safe diagnostic run endpoints
// ============================================================================

/// Convert a DB record to API response (sanitized - no prompt/output content).
fn run_record_to_response(record: &DiagRunRecord) -> DiagRunResponse {
    let duration_ms = record
        .completed_at_unix_ms
        .map(|end| end - record.started_at_unix_ms);

    DiagRunResponse {
        id: record.id.clone(),
        trace_id: record.trace_id.clone(),
        status: record.status.clone(),
        started_at_unix_ms: record.started_at_unix_ms,
        completed_at_unix_ms: record.completed_at_unix_ms,
        request_hash: record.request_hash.clone(),
        request_hash_verified: None,
        manifest_hash: record.manifest_hash.clone(),
        manifest_hash_verified: None,
        total_events_count: record.total_events_count,
        dropped_events_count: record.dropped_events_count,
        duration_ms,
        created_at: record.created_at.clone(),
    }
}

/// Sensitive field names that should be removed from diagnostic payloads.
/// These fields may contain user prompts, model outputs, or other PII.
const SENSITIVE_FIELDS: &[&str] = &[
    "prompt",
    "prompts",
    "system_prompt",
    "user_prompt",
    "output",
    "outputs",
    "completion",
    "generated_text",
    "input",
    "response",
    "content",
    "text",
    "message",
    "messages",
];

/// Recursively sanitize a JSON value by removing sensitive fields.
fn sanitize_recursive(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(obj) => {
            // Remove all sensitive fields from this object
            for field in SENSITIVE_FIELDS {
                obj.remove(*field);
            }
            // Recurse into remaining values
            for (_, v) in obj.iter_mut() {
                sanitize_recursive(v);
            }
        }
        serde_json::Value::Array(arr) => {
            // Recurse into each array element
            for item in arr.iter_mut() {
                sanitize_recursive(item);
            }
        }
        // Other types (strings, numbers, bools, null) don't need sanitization
        _ => {}
    }
}

/// Sanitize event payload to remove any prompt/output content.
///
/// This ensures we never leak sensitive inference content through diagnostics.
fn sanitize_event_payload(payload_json: &str) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(payload_json) {
        Ok(mut value) => {
            sanitize_recursive(&mut value);
            value
        }
        Err(_) => serde_json::json!({"error": "invalid_payload"}),
    }
}

/// Convert a DB event record to API response (sanitized).
fn event_record_to_response(record: &DiagEventRecord) -> DiagEventResponse {
    DiagEventResponse {
        seq: record.seq,
        mono_us: record.mono_us,
        event_type: record.event_type.clone(),
        severity: record.severity.clone(),
        payload: sanitize_event_payload(&record.payload_json),
    }
}

/// GET /v1/diag/runs - List diagnostic runs for the tenant
///
/// Returns a paginated list of diagnostic runs. All queries are tenant-scoped.
#[utoipa::path(
    get,
    path = "/v1/diag/runs",
    params(ListDiagRunsQuery),
    responses(
        (status = 200, description = "List of diagnostic runs", body = ListDiagRunsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn list_diag_runs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListDiagRunsQuery>,
) -> Result<Json<ListDiagRunsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let tenant_id = &claims.tenant_id;
    let limit = query.limit.unwrap_or(50).min(200);

    debug!(
        tenant_id = %tenant_id,
        since = ?query.since,
        limit = limit,
        "Listing diagnostic runs"
    );

    let (runs, total_count) = list_diag_runs_paginated(
        state.db.pool(),
        tenant_id,
        query.since,
        limit,
        query.after.as_deref(),
        query.status.as_deref(),
    )
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to list diagnostic runs");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to list diagnostic runs")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let has_more = runs.len() as u32 >= limit;
    let next_cursor = if has_more {
        runs.last().map(|r| r.id.clone())
    } else {
        None
    };

    let run_responses: Vec<DiagRunResponse> = runs.iter().map(run_record_to_response).collect();

    Ok(Json(ListDiagRunsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        runs: run_responses,
        total_count,
        next_cursor,
        has_more,
    }))
}

/// GET /v1/diag/runs/{trace_id} - Get a specific diagnostic run by trace_id
///
/// Returns the run details if it exists and belongs to the tenant.
#[utoipa::path(
    get,
    path = "/v1/diag/runs/{trace_id}",
    params(
        ("trace_id" = String, Path, description = "Trace ID of the diagnostic run")
    ),
    responses(
        (status = 200, description = "Diagnostic run details", body = DiagRunResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn get_diag_run(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(trace_id): Path<String>,
) -> Result<Json<DiagRunResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;
    let trace_id = crate::id_resolver::resolve_any_id(&state.db, &trace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    let tenant_id = &claims.tenant_id;

    debug!(
        tenant_id = %tenant_id,
        trace_id = %trace_id,
        "Getting diagnostic run"
    );

    let run = get_diag_run_by_trace_id(state.db.pool(), tenant_id, &trace_id)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to get diagnostic run");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get diagnostic run")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    match run {
        Some(record) => Ok(Json(run_record_to_response(&record))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Diagnostic run not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("trace_id: {}", trace_id)),
            ),
        )),
    }
}

/// GET /v1/diag/runs/{trace_id}/events - List events for a diagnostic run
///
/// Returns a paginated list of events. Sequence-based cursor pagination.
#[utoipa::path(
    get,
    path = "/v1/diag/runs/{trace_id}/events",
    params(
        ("trace_id" = String, Path, description = "Trace ID of the diagnostic run"),
        ListDiagEventsQuery
    ),
    responses(
        (status = 200, description = "List of diagnostic events", body = ListDiagEventsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn list_diag_events(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(trace_id): Path<String>,
    Query(query): Query<ListDiagEventsQuery>,
) -> Result<Json<ListDiagEventsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;
    let trace_id = crate::id_resolver::resolve_any_id(&state.db, &trace_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    let tenant_id = &claims.tenant_id;
    let limit = query.limit.unwrap_or(100).min(1000);

    debug!(
        tenant_id = %tenant_id,
        trace_id = %trace_id,
        after_seq = ?query.after_seq,
        limit = limit,
        "Listing diagnostic events"
    );

    // First get the run to get the run_id
    let run = get_diag_run_by_trace_id(state.db.pool(), tenant_id, &trace_id)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to get diagnostic run for events");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get diagnostic run")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let run = run.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Diagnostic run not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("trace_id: {}", trace_id)),
            ),
        )
    })?;

    let mut events = list_diag_events_paginated(
        state.db.pool(),
        tenant_id,
        &run.id,
        query.after_seq,
        limit,
        query.event_type.as_deref(),
        query.severity.as_deref(),
    )
    .await
    .map_err(|e| {
        warn!(error = %e, "Failed to list diagnostic events");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to list diagnostic events")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Check if there are more results (we fetched limit + 1)
    let has_more = events.len() as u32 > limit;
    if has_more {
        events.pop();
    }

    let last_seq = events.last().map(|e| e.seq);
    let event_responses: Vec<DiagEventResponse> =
        events.iter().map(event_record_to_response).collect();

    Ok(Json(ListDiagEventsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        events: event_responses,
        last_seq,
        has_more,
    }))
}

/// POST /v1/diag/diff - Compare two diagnostic runs
///
/// Computes differences between two runs for debugging.
#[utoipa::path(
    post,
    path = "/v1/diag/diff",
    request_body = DiagDiffRequest,
    responses(
        (status = 200, description = "Diff result", body = DiagDiffResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "One or both runs not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn diff_diag_runs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<DiagDiffRequest>,
) -> Result<Json<DiagDiffResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = &claims.tenant_id;

    debug!(
        tenant_id = %tenant_id,
        trace_id_a = %request.trace_id_a,
        trace_id_b = %request.trace_id_b,
        "Comparing diagnostic runs"
    );

    // Get both runs
    let run_a = get_diag_run_by_trace_id(state.db.pool(), tenant_id, &request.trace_id_a)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get run A")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Run A not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("trace_id: {}", request.trace_id_a)),
                ),
            )
        })?;

    let run_b = get_diag_run_by_trace_id(state.db.pool(), tenant_id, &request.trace_id_b)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get run B")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Run B not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("trace_id: {}", request.trace_id_b)),
                ),
            )
        })?;

    // Compute summary
    let status_match = run_a.status == run_b.status;
    let event_count_match = run_a.total_events_count == run_b.total_events_count;

    let duration_a = run_a
        .completed_at_unix_ms
        .map(|end| end - run_a.started_at_unix_ms);
    let duration_b = run_b
        .completed_at_unix_ms
        .map(|end| end - run_b.started_at_unix_ms);
    let duration_diff_ms = match (duration_a, duration_b) {
        (Some(a), Some(b)) => Some(b - a),
        _ => None,
    };

    let mut event_diffs = None;
    let mut timing_diffs = None;
    let mut event_type_mismatches = 0i64;
    let mut severity_changes = 0i64;

    // Compute event-level diffs if requested
    if request.include_events {
        let events_a =
            get_all_diag_events_for_run(state.db.pool(), tenant_id, &run_a.id, 1000).await;
        let events_b =
            get_all_diag_events_for_run(state.db.pool(), tenant_id, &run_b.id, 1000).await;

        if let (Ok(ea), Ok(eb)) = (events_a, events_b) {
            let mut diffs = Vec::new();
            let max_seq = ea.len().max(eb.len());

            for seq in 0..max_seq {
                let event_a = ea.iter().find(|e| e.seq as usize == seq);
                let event_b = eb.iter().find(|e| e.seq as usize == seq);

                match (event_a, event_b) {
                    (Some(a), Some(b)) => {
                        if a.event_type != b.event_type {
                            event_type_mismatches += 1;
                            diffs.push(EventDiff {
                                seq: seq as i64,
                                diff_type: "changed".to_string(),
                                event_type_a: Some(a.event_type.clone()),
                                event_type_b: Some(b.event_type.clone()),
                                description: format!(
                                    "Event type changed: {} -> {}",
                                    a.event_type, b.event_type
                                ),
                            });
                        }
                        if a.severity != b.severity {
                            severity_changes += 1;
                        }
                    }
                    (Some(a), None) => {
                        diffs.push(EventDiff {
                            seq: seq as i64,
                            diff_type: "removed".to_string(),
                            event_type_a: Some(a.event_type.clone()),
                            event_type_b: None,
                            description: format!("Event removed: {}", a.event_type),
                        });
                    }
                    (None, Some(b)) => {
                        diffs.push(EventDiff {
                            seq: seq as i64,
                            diff_type: "added".to_string(),
                            event_type_a: None,
                            event_type_b: Some(b.event_type.clone()),
                            description: format!("Event added: {}", b.event_type),
                        });
                    }
                    (None, None) => {}
                }
            }
            event_diffs = Some(diffs);
        }
    }

    // Compute timing diffs if requested
    if request.include_timing {
        let timing_a = get_stage_timing_summary(state.db.pool(), tenant_id, &run_a.id).await;
        let timing_b = get_stage_timing_summary(state.db.pool(), tenant_id, &run_b.id).await;

        if let (Ok(ta), Ok(tb)) = (timing_a, timing_b) {
            let mut diffs = Vec::new();

            // Create a map of stage timings from A
            let stage_map_a: std::collections::HashMap<_, _> =
                ta.into_iter().map(|(s, _, d, _)| (s, d)).collect();
            let stage_map_b: std::collections::HashMap<_, _> =
                tb.into_iter().map(|(s, _, d, _)| (s, d)).collect();

            // Combine all stage names
            let all_stages: std::collections::HashSet<_> = stage_map_a
                .keys()
                .chain(stage_map_b.keys())
                .cloned()
                .collect();

            for stage in all_stages {
                let dur_a = stage_map_a.get(&stage).copied().flatten();
                let dur_b = stage_map_b.get(&stage).copied().flatten();

                let diff_us = match (dur_a, dur_b) {
                    (Some(a), Some(b)) => Some(b - a),
                    _ => None,
                };

                let percent_change = match (dur_a, dur_b) {
                    (Some(a), Some(b)) if a > 0 => Some(((b - a) as f64 / a as f64) * 100.0),
                    _ => None,
                };

                diffs.push(TimingDiff {
                    stage,
                    duration_us_a: dur_a,
                    duration_us_b: dur_b,
                    diff_us,
                    percent_change,
                });
            }
            timing_diffs = Some(diffs);
        }
    }

    // Compute router step diffs if requested
    let mut router_step_diffs = None;
    let mut first_divergence: Option<FirstDivergence> = None;

    if request.include_router_steps {
        let steps_a = get_router_step_events(state.db.pool(), tenant_id, &run_a.id).await;
        let steps_b = get_router_step_events(state.db.pool(), tenant_id, &run_b.id).await;

        if let (Ok(sa), Ok(sb)) = (steps_a, steps_b) {
            let mut diffs = Vec::new();
            let max_steps = sa.len().max(sb.len());
            let mut found_first_divergence = false;

            for idx in 0..max_steps {
                let step_a = sa.get(idx);
                let step_b = sb.get(idx);

                let matches = match (step_a, step_b) {
                    (Some(a), Some(b)) => {
                        a.selected_stable_ids == b.selected_stable_ids
                            && a.gates_q15 == b.gates_q15
                            && a.decision_hash == b.decision_hash
                    }
                    _ => false,
                };

                let is_first_divergence = !matches && !found_first_divergence;
                if is_first_divergence {
                    found_first_divergence = true;
                    first_divergence = Some(FirstDivergence {
                        category: "router_step".to_string(),
                        stage: None,
                        router_step: Some(idx as u32),
                        description: format!("Router step {} diverged", idx),
                        value_a: step_a.map(|s| {
                            serde_json::json!({
                                "selected_ids": s.selected_stable_ids,
                                "decision_hash": s.decision_hash
                            })
                        }),
                        value_b: step_b.map(|s| {
                            serde_json::json!({
                                "selected_ids": s.selected_stable_ids,
                                "decision_hash": s.decision_hash
                            })
                        }),
                    });
                }

                diffs.push(RouterStepDiff {
                    step_idx: idx as u32,
                    matches,
                    is_first_divergence,
                    selected_ids_a: step_a
                        .map(|s| s.selected_stable_ids.clone())
                        .unwrap_or_default(),
                    selected_ids_b: step_b
                        .map(|s| s.selected_stable_ids.clone())
                        .unwrap_or_default(),
                    scores_q15_a: step_a.map(|s| s.gates_q15.clone()).unwrap_or_default(),
                    scores_q15_b: step_b.map(|s| s.gates_q15.clone()).unwrap_or_default(),
                    decision_hash_a: step_a.and_then(|s| s.decision_hash.clone()),
                    decision_hash_b: step_b.and_then(|s| s.decision_hash.clone()),
                });
            }
            router_step_diffs = Some(diffs);
        }
    }

    // Build anchor comparison using decision chain hashes from diag_runs table
    let request_hash_match = run_a.request_hash == run_b.request_hash;
    let manifest_hash_match = run_a.manifest_hash == run_b.manifest_hash;
    let decision_chain_hash_match = run_a.decision_chain_hash == run_b.decision_chain_hash;
    let backend_identity_hash_match = run_a.backend_identity_hash == run_b.backend_identity_hash;
    let model_identity_hash_match = run_a.model_identity_hash == run_b.model_identity_hash;
    let all_anchors_match = request_hash_match
        && manifest_hash_match
        && decision_chain_hash_match
        && backend_identity_hash_match
        && model_identity_hash_match;

    let anchor_comparison = AnchorComparison {
        request_hash_match,
        manifest_hash_match,
        decision_chain_hash_match,
        backend_identity_hash_match,
        model_identity_hash_match,
        all_anchors_match,
        request_hash_a: run_a.request_hash.clone(),
        request_hash_b: run_b.request_hash.clone(),
        decision_chain_hash_a: run_a.decision_chain_hash.clone(),
        decision_chain_hash_b: run_b.decision_chain_hash.clone(),
    };

    // Check for anchor-level divergence
    if first_divergence.is_none() && !all_anchors_match {
        first_divergence = Some(FirstDivergence {
            category: "anchor".to_string(),
            stage: None,
            router_step: None,
            description: if !request_hash_match {
                "Request hash mismatch - different inputs".to_string()
            } else {
                "Manifest hash mismatch - different model/config".to_string()
            },
            value_a: Some(serde_json::json!({
                "request_hash": run_a.request_hash,
                "manifest_hash": run_a.manifest_hash
            })),
            value_b: Some(serde_json::json!({
                "request_hash": run_b.request_hash,
                "manifest_hash": run_b.manifest_hash
            })),
        });
    }

    let equivalent = status_match
        && event_count_match
        && event_type_mismatches == 0
        && severity_changes == 0
        && all_anchors_match
        && first_divergence.is_none();

    let divergence_reason = if !equivalent {
        Some(
            first_divergence
                .as_ref()
                .map(|d| d.description.clone())
                .unwrap_or_else(|| "Unknown divergence".to_string()),
        )
    } else {
        None
    };

    Ok(Json(DiagDiffResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        run_a: run_record_to_response(&run_a),
        run_b: run_record_to_response(&run_b),
        anchor_comparison,
        first_divergence,
        summary: DiagDiffSummary {
            status_match,
            event_count_match,
            duration_diff_ms,
            event_type_mismatches,
            severity_changes,
            equivalent,
            divergence_reason,
        },
        event_diffs,
        timing_diffs,
        router_step_diffs,
    }))
}

/// POST /v1/diag/export - Export diagnostic data
///
/// Exports a run's events and timing for offline analysis.
#[utoipa::path(
    post,
    path = "/v1/diag/export",
    request_body = DiagExportRequest,
    responses(
        (status = 200, description = "Export result", body = DiagExportResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "diagnostics",
    security(("bearer_token" = []))
)]
pub async fn export_diag_run(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<DiagExportRequest>,
) -> Result<Json<DiagExportResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = &claims.tenant_id;
    let max_events = request.max_events.unwrap_or(10000).min(50000);

    debug!(
        tenant_id = %tenant_id,
        trace_id = %request.trace_id,
        format = %request.format,
        max_events = max_events,
        "Exporting diagnostic run"
    );

    // Get the run.
    //
    // Note: callers historically pass either trace_id (preferred) or diag run_id.
    // Preserve compatibility by falling back to run_id lookup.
    let run = match get_diag_run_by_trace_id(state.db.pool(), tenant_id, &request.trace_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get diagnostic run")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })? {
        Some(record) => record,
        None => get_diag_run_by_id(state.db.pool(), tenant_id, &request.trace_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to get diagnostic run")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Diagnostic run not found")
                            .with_code("NOT_FOUND")
                            .with_string_details(format!("trace_id: {}", request.trace_id)),
                    ),
                )
            })?,
    };

    // Get events if requested
    let (events, events_exported, truncated) = if request.include_events {
        let all_events =
            get_all_diag_events_for_run(state.db.pool(), tenant_id, &run.id, max_events)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to get events")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;

        let count = all_events.len() as i64;
        let truncated = count >= max_events as i64;
        let responses: Vec<DiagEventResponse> =
            all_events.iter().map(event_record_to_response).collect();
        (Some(responses), count, truncated)
    } else {
        (None, 0, false)
    };

    // Get timing summary if requested
    let timing_summary = if request.include_timing {
        let timing = get_stage_timing_summary(state.db.pool(), tenant_id, &run.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to get timing summary")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        Some(
            timing
                .into_iter()
                .map(|(stage, start, duration, success)| StageTiming {
                    stage,
                    start_us: start,
                    end_us: duration.map(|d| start + d),
                    duration_us: duration,
                    success,
                })
                .collect(),
        )
    } else {
        None
    };

    // Build metadata if requested
    let metadata = if request.include_metadata {
        Some(ExportMetadata {
            exported_at: chrono::Utc::now().to_rfc3339(),
            events_exported,
            events_total: run.total_events_count,
            truncated,
        })
    } else {
        None
    };

    Ok(Json(DiagExportResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        format: request.format,
        run: run_record_to_response(&run),
        events,
        timing_summary,
        metadata,
    }))
}
