//! Registry status handlers
//!
//! Provides endpoints for checking registry availability and health.

use crate::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Registry status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegistryStatus {
    /// Whether the registry is available
    pub available: bool,
    /// Additional status message
    pub message: String,
}

/// Get registry status
///
/// Returns whether the registry is initialized and available for adapter management.
#[utoipa::path(
    get,
    path = "/v1/registry/status",
    responses(
        (status = 200, description = "Registry status", body = RegistryStatus)
    ),
    tag = "Registry"
)]
pub async fn get_registry_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = if state.registry.is_some() {
        RegistryStatus {
            available: true,
            message: "Registry is initialized and ready".to_string(),
        }
    } else {
        RegistryStatus {
            available: false,
            message: "Registry is not initialized (adapter registration disabled)".to_string(),
        }
    };

    (StatusCode::OK, Json(status))
}
