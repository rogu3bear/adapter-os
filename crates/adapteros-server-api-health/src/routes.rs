//! Health check routes
//!
//! Router configuration for health check endpoints.

use adapteros_server_api::state::AppState;
use axum::{routing::get, Router};

use crate::handlers::{get_invariant_status, get_status, health, ready, startup_health};

/// Build the health check router
///
/// Returns a router with the following endpoints:
/// - `GET /healthz` - Liveness probe
/// - `GET /healthz/startup` - Startup orchestration status + operator guidance
/// - `GET /readyz` - Readiness probe
/// - `GET /v1/status` - Lifecycle status
/// - `GET /v1/invariants` - Boot invariants status
pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(health))
        .route("/healthz/startup", get(startup_health))
        .route("/readyz", get(ready))
        .route("/v1/status", get(get_status))
        .route("/v1/invariants", get(get_invariant_status))
}
