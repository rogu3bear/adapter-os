//! Status and configuration handlers
//!
//! Simple placeholder handlers for administrative status endpoints.

use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

/// Admin status response
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminStatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// System configuration response
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemConfigResponse {
    pub config_version: &'static str,
    pub policies_enabled: bool,
}

/// Get admin status
///
/// Returns the current administrative status of the system.
pub async fn admin_status() -> impl IntoResponse {
    Json(AdminStatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Get system configuration
///
/// Returns the current system configuration summary.
pub async fn system_config() -> impl IntoResponse {
    Json(SystemConfigResponse {
        config_version: "1.0",
        policies_enabled: true,
    })
}
