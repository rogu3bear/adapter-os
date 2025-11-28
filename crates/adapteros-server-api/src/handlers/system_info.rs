//! System information and metadata handlers

use crate::types::*;
use axum::Json;

/// Get system metadata
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/meta",
    responses(
        (status = 200, description = "System metadata", body = MetaResponse)
    )
)]
pub async fn meta() -> Json<MetaResponse> {
    // Note: environment, production_mode, and dev_login_enabled are runtime config values
    // that require AppState. For now, we provide defaults.
    // TODO: Update this handler to take State if these values are needed.
    Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_hash: option_env!("BUILD_HASH").unwrap_or("dev").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("unknown").to_string(),
        environment: "dev".to_string(),
        production_mode: false,
        dev_login_enabled: false,
    })
}
