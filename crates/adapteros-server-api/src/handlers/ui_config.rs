//! UI configuration handler.

use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::{UiConfigResponse, UiProfile, API_SCHEMA_VERSION};
use axum::{extract::State, http::StatusCode, Json};
use tracing::warn;

const UI_PROFILE_ENV: &str = "AOS_UI_PROFILE";
const UI_DOCS_URL_ENV: &str = "AOS_DOCS_URL";
const DEFAULT_DOCS_URL: &str = "/docs";

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

fn resolve_docs_url() -> String {
    match std::env::var(UI_DOCS_URL_ENV) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                warn!(
                    "AOS_DOCS_URL is empty; falling back to default docs URL ({DEFAULT_DOCS_URL})"
                );
                DEFAULT_DOCS_URL.to_string()
            } else {
                trimmed.to_string()
            }
        }
        Err(_) => DEFAULT_DOCS_URL.to_string(),
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
        docs_url: resolve_docs_url(),
    };
    Ok(Json(response))
}
