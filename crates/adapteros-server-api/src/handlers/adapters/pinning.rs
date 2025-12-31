// Adapter Pinning Handlers
//
// This module provides REST API endpoints for:
// - Pinning adapters to prevent eviction
// - Unpinning adapters to allow eviction
// - Getting pin status

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use utoipa::ToSchema;

// ============================================================================
// Types
// ============================================================================

/// Pin adapter request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PinAdapterRequest {
    /// Reason for pinning (required for audit trail)
    pub reason: String,
    /// Optional TTL timestamp (ISO 8601 format, e.g., "2099-12-31T23:59:59Z")
    pub pinned_until: Option<String>,
}

/// Pin adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct PinAdapterResponse {
    pub adapter_id: String,
    pub pinned: bool,
    pub reason: String,
    pub pinned_by: String,
    pub pinned_at: String,
    pub pinned_until: Option<String>,
}

/// Unpin adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct UnpinAdapterResponse {
    pub adapter_id: String,
    pub unpinned: bool,
    pub message: String,
}

/// Pin status response
#[derive(Debug, Serialize, ToSchema)]
pub struct PinStatusResponse {
    pub adapter_id: String,
    pub is_pinned: bool,
    pub reason: Option<String>,
    pub pinned_by: Option<String>,
    pub pinned_at: Option<String>,
    pub pinned_until: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Pin an adapter to prevent eviction
///
/// Pinned adapters are protected from automatic eviction due to memory pressure
/// or TTL expiration. Use this for production-critical adapters.
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.pinned` event.
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/pin
/// {
///   "reason": "Production-critical adapter",
///   "pinned_until": "2099-12-31T23:59:59Z"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/pin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = PinAdapterRequest,
    responses(
        (status = 200, description = "Adapter pinned successfully", body = PinAdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn pin_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<PinAdapterRequest>,
) -> Result<Json<PinAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Verify adapter exists
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

    let tenant_id = adapter.tenant_id.clone();
    let pinned_by = claims.sub.clone();
    let pinned_at = chrono::Utc::now().to_rfc3339();

    // === ISSUE 3: Validate pin TTL is not in the past (API layer) ===
    if let Some(ref until_str) = req.pinned_until {
        use chrono::{DateTime, Utc};
        match DateTime::parse_from_rfc3339(until_str) {
            Ok(parsed) => {
                if parsed.with_timezone(&Utc) <= Utc::now() {
                    warn!(
                        tenant_id = %claims.tenant_id,
                        adapter_id = %adapter_id,
                        pinned_until = %until_str,
                        "Rejected pin request: TTL is in the past"
                    );
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(
                            ErrorResponse::new("Adapter pin TTL is in the past")
                                .with_code("TTL_IN_PAST"),
                        ),
                    ));
                }
            }
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new(format!(
                            "Invalid pinned_until timestamp format: {}. Expected RFC3339 (e.g., 2099-12-31T23:59:59Z)",
                            e
                        ))
                        .with_code("INVALID_TTL_FORMAT"),
                    ),
                ));
            }
        }
    }

    // Pin the adapter
    state
        .db
        .pin_adapter(
            &tenant_id,
            &adapter_id,
            req.pinned_until.as_deref(),
            &req.reason,
            Some(&pinned_by),
        )
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to pin adapter"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to pin adapter to prevent eviction")
                        .with_code("ADAPTER_PIN_FAILED")
                        .with_string_details(format!(
                            "Adapter '{}' could not be pinned. The adapter may already be pinned or in an incompatible state. Technical details: {}",
                            adapter_id, e
                        )),
                ),
            )
        })?;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.pinned",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} pinned by {}", adapter_id, pinned_by),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": tenant_id,
            "reason": req.reason,
            "pinned_by": pinned_by,
            "pinned_until": req.pinned_until,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        pinned_by = %pinned_by,
        "Adapter pinned"
    );

    // Audit log: adapter pinned
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_PIN,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(PinAdapterResponse {
        adapter_id,
        pinned: true,
        reason: req.reason,
        pinned_by,
        pinned_at,
        pinned_until: req.pinned_until,
    }))
}

/// Unpin an adapter to allow eviction
///
/// Removes pin protection from an adapter, allowing it to be evicted
/// during memory pressure or TTL expiration.
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.unpinned` event.
///
/// # Example
/// ```
/// DELETE /v1/adapters/{adapter_id}/pin
/// ```
#[utoipa::path(
    delete,
    path = "/v1/adapters/{adapter_id}/pin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unpinned successfully", body = UnpinAdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn unpin_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<UnpinAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Verify adapter exists
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

    let tenant_id = adapter.tenant_id.clone();
    let actor = claims.sub.clone();

    // Unpin the adapter
    state
        .db
        .unpin_adapter(&tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to unpin adapter"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to unpin adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.unpinned",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} unpinned by {}", adapter_id, actor),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": tenant_id,
            "actor": actor,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        actor = %actor,
        "Adapter unpinned"
    );

    // Audit log: adapter unpinned
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_UNPIN,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(UnpinAdapterResponse {
        adapter_id,
        unpinned: true,
        message: "Adapter unpinned successfully".to_string(),
    }))
}

/// Get adapter pin status
///
/// Returns the current pin status of an adapter including pin reason,
/// pinned_by user, and TTL information.
///
/// **Permissions:** Any authenticated user can view pin status.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/pin
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/pin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Pin status retrieved", body = PinStatusResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_pin_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<PinStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    // Verify adapter exists
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

    let tenant_id = adapter.tenant_id.clone();

    // Check if pinned
    let is_pinned = state
        .db
        .is_pinned(&tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to check pin status"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to check pin status")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get pin details if pinned
    let (reason, pinned_by, pinned_at, pinned_until) = if is_pinned {
        let pinned_adapters = state
            .db
            .list_pinned_adapters(&tenant_id)
            .await
            .map_err(|e| {
                error!(
                    tenant_id = %tenant_id,
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to list pinned adapters"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to get pin details")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        pinned_adapters
            .into_iter()
            .find(|p| p.adapter_id.as_deref() == Some(adapter_id.as_str()))
            .map(|p| {
                (
                    Some(p.reason),
                    p.pinned_by,
                    Some(p.pinned_at),
                    p.pinned_until,
                )
            })
            .unwrap_or((None, None, None, None))
    } else {
        (None, None, None, None)
    };

    Ok(Json(PinStatusResponse {
        adapter_id,
        is_pinned,
        reason,
        pinned_by,
        pinned_at,
        pinned_until,
    }))
}
