// Adapter Archive/Unarchive Handlers
//
// This module provides REST API endpoints for:
// - Archiving adapters
// - Unarchiving adapters
// - Getting archive status

use crate::adapter_helpers::fetch_adapter_for_tenant;
use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::ip_extraction::ClientIp;
use crate::middleware::require_any_role;
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
use tracing::{error, info};
use utoipa::ToSchema;

// ============================================================================
// Types
// ============================================================================

/// Archive adapter request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ArchiveAdapterRequest {
    /// Reason for archiving (required for audit trail)
    pub reason: String,
}

/// Archive adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct ArchiveAdapterResponse {
    pub adapter_id: String,
    pub archived: bool,
    pub reason: String,
    pub archived_by: String,
    pub archived_at: String,
}

/// Unarchive adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct UnarchiveAdapterResponse {
    pub adapter_id: String,
    pub unarchived: bool,
    pub message: String,
}

/// Archive status response
#[derive(Debug, Serialize, ToSchema)]
pub struct ArchiveStatusResponse {
    pub adapter_id: String,
    pub is_archived: bool,
    pub is_purged: bool,
    pub archive_reason: Option<String>,
    pub archived_by: Option<String>,
    pub archived_at: Option<String>,
    pub purged_at: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Archive an adapter
///
/// Archives an adapter, marking it as unavailable for inference.
/// The adapter's `.aos` file is NOT deleted until garbage collection runs.
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.archived` event.
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/archive
/// {
///   "reason": "Deprecated in favor of v2"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/archive",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = ArchiveAdapterRequest,
    responses(
        (status = 200, description = "Adapter archived successfully", body = ArchiveAdapterResponse),
        (status = 400, description = "Already archived", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn archive_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Path(adapter_id): Path<String>,
    Json(req): Json<ArchiveAdapterRequest>,
) -> ApiResult<ArchiveAdapterResponse> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    // Verify adapter exists and validate tenant isolation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Check if already archived
    if adapter.archived_at.is_some() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "ALREADY_ARCHIVED",
            "adapter is already archived",
        ));
    }

    let archived_by = claims.sub.clone();
    let archived_at = chrono::Utc::now().to_rfc3339();

    // Archive the adapter
    state
        .db
        .archive_adapter(&claims.tenant_id, &adapter_id, &archived_by, &req.reason)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to archive adapter"
            );
            ApiError::internal("failed to archive adapter").with_details(e.to_string())
        })?;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.archived",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} archived by {}", adapter_id, archived_by),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": adapter.tenant_id,
            "reason": req.reason,
            "archived_by": archived_by,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        archived_by = %archived_by,
        "Adapter archived"
    );

    // Audit log: adapter archived
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_ARCHIVE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(ArchiveAdapterResponse {
        adapter_id,
        archived: true,
        reason: req.reason,
        archived_by,
        archived_at,
    }))
}

/// Unarchive an adapter
///
/// Restores an archived adapter, making it available for inference again.
/// Cannot unarchive if the adapter has been purged (file deleted).
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.unarchived` event.
///
/// # Example
/// ```
/// DELETE /v1/adapters/{adapter_id}/archive
/// ```
#[utoipa::path(
    delete,
    path = "/v1/adapters/{adapter_id}/archive",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unarchived successfully", body = UnarchiveAdapterResponse),
        (status = 400, description = "Not archived or already purged", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn unarchive_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Path(adapter_id): Path<String>,
) -> ApiResult<UnarchiveAdapterResponse> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    // Verify adapter exists and validate tenant isolation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Check if not archived
    if adapter.archived_at.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "NOT_ARCHIVED",
            "adapter is not archived",
        ));
    }

    // Check if already purged
    if adapter.purged_at.is_some() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "ALREADY_PURGED",
            "cannot unarchive purged adapter - file has been deleted",
        ));
    }

    // Unarchive the adapter
    state
        .db
        .unarchive_adapter(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to unarchive adapter"
            );
            ApiError::internal("failed to unarchive adapter").with_details(e.to_string())
        })?;

    let unarchived_by = claims.sub.clone();

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.unarchived",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} unarchived by {}", adapter_id, unarchived_by),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": adapter.tenant_id,
            "unarchived_by": unarchived_by,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        unarchived_by = %unarchived_by,
        "Adapter unarchived"
    );

    // Audit log: adapter unarchived
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_UNARCHIVE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(UnarchiveAdapterResponse {
        adapter_id,
        unarchived: true,
        message: "Adapter restored and available for inference".to_string(),
    }))
}

/// Get archive status of an adapter
///
/// Returns the archive/purge status of an adapter.
///
/// **Permissions:** Requires `Viewer`, `Operator`, or `Admin` role.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/archive
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/archive",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Archive status retrieved", body = ArchiveStatusResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_archive_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<ArchiveStatusResponse> {
    // Require at least viewer role
    require_any_role(&claims, &[Role::Viewer, Role::Operator, Role::Admin])?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    // Fetch adapter and validate tenant isolation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    Ok(Json(ArchiveStatusResponse {
        adapter_id,
        is_archived: adapter.archived_at.is_some(),
        is_purged: adapter.purged_at.is_some(),
        archive_reason: adapter.archive_reason,
        archived_by: adapter.archived_by,
        archived_at: adapter.archived_at,
        purged_at: adapter.purged_at,
    }))
}
