//! In-flight adapters handler
//!
//! Returns the set of adapter IDs currently being used for inference.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::Json};
use serde::Serialize;
use utoipa::ToSchema;

/// Response for in-flight adapters endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct InFlightAdaptersResponse {
    /// Adapter IDs currently in use for inference
    pub adapter_ids: Vec<String>,
    /// Total count of in-flight inferences
    pub inference_count: usize,
}

/// GET /v1/adapters/in-flight
///
/// Returns adapter IDs currently being used for active inference requests.
#[utoipa::path(
    get,
    path = "/v1/adapters/in-flight",
    responses(
        (status = 200, description = "In-flight adapters", body = InFlightAdaptersResponse),
    ),
    tag = "adapters"
)]
pub async fn get_in_flight_adapters(
    State(state): State<AppState>,
) -> Result<Json<InFlightAdaptersResponse>, StatusCode> {
    let (adapter_ids, inference_count) = if let Some(ref tracker) = state.inference_state_tracker {
        let ids: Vec<String> = tracker.adapters_in_flight().into_iter().collect();
        let count = tracker.count_active();
        (ids, count)
    } else {
        (vec![], 0)
    };

    Ok(Json(InFlightAdaptersResponse {
        adapter_ids,
        inference_count,
    }))
}
