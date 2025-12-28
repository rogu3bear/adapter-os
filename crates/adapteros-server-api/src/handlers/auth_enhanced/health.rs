//! Auth health handler
//!
//! Contains the health check endpoint for the auth subsystem.

use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{extract::State, http::StatusCode, Json};
use tracing::warn;

use super::types::AuthHealthResponse;

/// Auth subsystem health check
#[utoipa::path(
    get,
    path = "/v1/auth/health",
    responses(
        (status = 200, description = "Auth health", body = AuthHealthResponse),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn auth_health_handler(
    State(state): State<AppState>,
) -> Result<Json<AuthHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    let db_status = if let Some(pool) = state.db.pool_opt() {
        match sqlx::query("SELECT 1").fetch_one(pool).await {
            Ok(_) => "ok".to_string(),
            Err(e) => {
                warn!(error = %e, "DB health check failed for auth health");
                "unhealthy".to_string()
            }
        }
    } else {
        "unknown".to_string()
    };

    let signing_keys = if state.use_ed25519 {
        "eddsa".to_string()
    } else {
        "hmac".to_string()
    };

    Ok(Json(AuthHealthResponse {
        status: "ok".to_string(),
        db: db_status,
        signing_keys,
        idp_configured: false,
    }))
}
