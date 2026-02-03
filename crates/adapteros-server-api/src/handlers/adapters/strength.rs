// Adapter Strength Handler
//
// This module provides REST API endpoints for:
// - Updating runtime LoRA strength multiplier for adapters

use crate::adapter_helpers::fetch_adapter_for_tenant;
use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::{Path, State},
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use tracing::error;
use utoipa::ToSchema;

// Re-use AdapterDetailResponse from lineage module
use super::lineage::AdapterDetailResponse;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdateAdapterStrengthRequest {
    /// Runtime LoRA strength multiplier (scales adapter effect)
    pub lora_strength: f32,
}

// ============================================================================
// Handlers
// ============================================================================

/// Update runtime LoRA strength multiplier for an adapter
#[utoipa::path(
    patch,
    path = "/v1/adapters/{adapter_id}/strength",
    request_body = UpdateAdapterStrengthRequest,
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter strength updated", body = AdapterDetailResponse),
        (status = 400, description = "Invalid strength value", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn update_adapter_strength(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<UpdateAdapterStrengthRequest>,
) -> ApiResult<AdapterDetailResponse> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    if !(0.0..=2.0).contains(&req.lora_strength) {
        return Err(ApiError::bad_request("Invalid LoRA strength value").with_details(format!(
            "LoRA strength must be between 0.0 and 2.0 (provided: {}). Use 1.0 for standard strength, lower values to reduce adapter influence, higher values to amplify it.",
            req.lora_strength
        )));
    }

    // Fetch adapter with tenant isolation validation
    let _adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // Update adapter strength
    state
        .db
        .update_adapter_strength(&adapter_id, req.lora_strength)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to update adapter strength"
            );
            ApiError::internal("Failed to update adapter LoRA strength").with_details(format!(
                "Adapter '{}' LoRA strength could not be updated to {}. The adapter may be locked or in an invalid state. Technical details: {}",
                adapter_id, req.lora_strength, e
            ))
        })?;

    // Reload adapter to return updated state
    let updated = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    Ok(Json(AdapterDetailResponse::from(updated)))
}
