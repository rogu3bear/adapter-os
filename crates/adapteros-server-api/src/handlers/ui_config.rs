//! UI configuration handler.

use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::{UiConfigResponse, UiProfile, API_SCHEMA_VERSION};
use axum::{extract::State, http::StatusCode, Json};
use tracing::warn;

const UI_PROFILE_ENV: &str = "AOS_UI_PROFILE";

fn resolve_ui_profile() -> UiProfile {
    match std::env::var(UI_PROFILE_ENV) {
        Ok(value) => match value.parse::<UiProfile>() {
            Ok(profile) => profile,
            Err(_) => {
                warn!(
                    ui_profile = %value,
                    "Invalid AOS_UI_PROFILE value; falling back to primary"
                );
                UiProfile::Primary
            }
        },
        Err(_) => UiProfile::Primary,
    }
}

/// Get public UI configuration.
///
/// Returns UI settings used by the frontend to shape navigation and surfaces.
#[utoipa::path(
    get,
    path = "/v1/ui/config",
    responses(
        (status = 200, description = "UI configuration", body = UiConfigResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "ui"
)]
pub async fn get_ui_config(
    State(_state): State<AppState>,
) -> Result<Json<UiConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let response = UiConfigResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        ui_profile: resolve_ui_profile(),
    };
    Ok(Json(response))
}
