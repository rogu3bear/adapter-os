//! System integrity handler for standalone deployments.
//!
//! This endpoint intentionally reports local-only mode to keep the MVP focused
//! on single-node performance without federation complexity.

use crate::api_error::ApiResult;
use crate::auth::Claims;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{extract::State, Extension, Json};
use serde::Serialize;

/// Lightweight integrity response for the control plane.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SystemIntegrityResponse {
    /// API schema version for compatibility checks.
    pub schema_version: String,
    /// High-level status indicator.
    pub status: String,
    /// Human-readable mode label (e.g., "local").
    pub mode: String,
    /// Short description of current integrity posture.
    pub message: String,
    /// Whether federation is enabled for this node.
    pub is_federated: bool,
}

/// Report system integrity posture.
///
/// This deliberately returns `is_federated = false` and a "Local Mode" message
/// so the UI can present a complete experience without requiring a federation
/// daemon during the MVP.
#[utoipa::path(
    get,
    path = "/v1/system/integrity",
    responses(
        (status = 200, description = "System integrity status", body = SystemIntegrityResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "system"
)]
pub async fn get_system_integrity(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> ApiResult<SystemIntegrityResponse> {
    Ok(Json(SystemIntegrityResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        status: "ok".to_string(),
        mode: "local".to_string(),
        message: "Local Mode: Secure.".to_string(),
        is_federated: false,
    }))
}
