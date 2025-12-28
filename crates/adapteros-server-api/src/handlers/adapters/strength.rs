// Adapter Strength Handler
//
// This module provides REST API endpoints for:
// - Updating runtime LoRA strength multiplier for adapters

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
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
) -> Result<Json<AdapterDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    if !(0.0..=2.0).contains(&req.lora_strength) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid LoRA strength value")
                    .with_code("LORA_STRENGTH_OUT_OF_RANGE")
                    .with_string_details(format!(
                        "LoRA strength must be between 0.0 and 2.0 (provided: {}). Use 1.0 for standard strength, lower values to reduce adapter influence, higher values to amplify it.",
                        req.lora_strength
                    ))
            ),
        ));
    }

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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update adapter LoRA strength")
                        .with_code("STRENGTH_UPDATE_FAILED")
                        .with_string_details(format!(
                            "Adapter '{}' LoRA strength could not be updated to {}. The adapter may be locked or in an invalid state. Technical details: {}",
                            adapter_id, req.lora_strength, e
                        )),
                ),
            )
        })?;

    let updated = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to reload adapter"
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(AdapterDetailResponse::from(updated)))
}
