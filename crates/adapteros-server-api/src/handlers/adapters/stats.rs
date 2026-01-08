// Adapter Statistics Handler
//
// This module provides REST API endpoints for:
// - Getting detailed adapter statistics

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{AdapterStatsResponse, ErrorResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use tracing::{error, warn};

// ============================================================================
// Handlers
// ============================================================================

/// Get detailed adapter statistics
///
/// Returns comprehensive statistics including activation percentage,
/// memory usage, request count, and latency metrics.
///
/// **Permissions:** Requires `AdapterView` permission (any authenticated role).
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/stats
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/stats",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter statistics", body = AdapterStatsResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_adapter_stats(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require view permission
    require_permission(&claims, Permission::AdapterView)?;

    // Get adapter from database
    let adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to fetch adapter"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!(adapter_id = %adapter_id, "Adapter not found");
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Get stats from database
    let (total_activations, selected_count, avg_gate_value) = state
        .db
        .get_adapter_stats(&claims.tenant_id, &adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    let selection_rate = if total_activations > 0 {
        (selected_count as f64 / total_activations as f64) * 100.0
    } else {
        0.0
    };

    // Calculate activation percentage from the activation count field
    let activation_percentage = if adapter.activation_count > 0 {
        // Normalize to 0-100 based on relative usage
        ((adapter.activation_count as f64).log10() * 20.0).min(100.0)
    } else {
        0.0
    };

    // Get latency metrics from performance summary table
    let (avg_latency_ms, p95_latency_ms, p99_latency_ms) = state
        .db
        .get_adapter_latency_stats(&adapter_id)
        .await
        .unwrap_or(None)
        .unwrap_or((0.0, 0.0, 0.0));

    Ok(Json(AdapterStatsResponse {
        adapter_id: adapter.adapter_id.unwrap_or(adapter.id),
        activation_percentage,
        memory_bytes: adapter.memory_bytes,
        request_count: adapter.activation_count,
        avg_latency_ms,
        p95_latency_ms,
        p99_latency_ms,
        total_activations,
        selected_count,
        avg_gate_value,
        selection_rate,
        lifecycle_state: adapter.lifecycle_state,
        last_activated: adapter.last_activated,
        created_at: adapter.created_at,
    }))
}
