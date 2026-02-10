// Adapter Duplication Handler
//
// This module provides REST API endpoints for:
// - Duplicating adapters

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use utoipa::ToSchema;

// Re-use AdapterDetailResponse from lineage module
use super::lineage::AdapterDetailResponse;

// ============================================================================
// Types
// ============================================================================

/// Request to duplicate an adapter
#[derive(Debug, Clone, serde::Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DuplicateAdapterRequest {
    /// Optional name for the duplicate adapter (defaults to "{original_name} (copy)")
    #[serde(default)]
    pub name: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Duplicate an existing adapter
///
/// Creates a copy of an adapter with a new ID. The duplicate will have
/// the original adapter set as its parent and fork_type="duplicate".
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/duplicate",
    request_body = DuplicateAdapterRequest,
    params(
        ("adapter_id" = String, Path, description = "Adapter ID to duplicate")
    ),
    responses(
        (status = 201, description = "Adapter duplicated successfully", body = AdapterDetailResponse),
        (status = 404, description = "Source adapter not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn duplicate_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<DuplicateAdapterRequest>,
) -> Result<(StatusCode, Json<AdapterDetailResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Duplicate the adapter with tenant validation
    let new_adapter = state
        .db
        .duplicate_adapter_for_tenant(&claims.tenant_id, &adapter_id, req.name.as_deref())
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("not found") || error_str.contains("NotFound") {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(format!("Adapter not found: {}", adapter_id))
                            .with_code("NOT_FOUND"),
                    ),
                )
            } else {
                tracing::error!(
                    tenant_id = %claims.tenant_id,
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to duplicate adapter"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new(format!("Failed to duplicate adapter: {}", e))
                            .with_code("DATABASE_ERROR"),
                    ),
                )
            }
        })?;

    let new_adapter_id = new_adapter
        .adapter_id
        .clone()
        .unwrap_or_else(|| new_adapter.id.clone());

    tracing::info!(
        source_adapter_id = %adapter_id,
        new_adapter_id = %new_adapter_id,
        tenant_id = %claims.tenant_id,
        "Duplicated adapter"
    );

    // Audit log
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_REGISTER,
        crate::audit_helper::resources::ADAPTER,
        Some(&new_adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok((
        StatusCode::CREATED,
        Json(AdapterDetailResponse::from(new_adapter)),
    ))
}
