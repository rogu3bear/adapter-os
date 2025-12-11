//! Orchestration configuration handlers
//!
//! Provides a minimal single-node orchestration config so the UI can render
//! without 404s. This stub is intentionally deterministic and does not persist
//! changes yet; PUT echoes the provided payload.

use crate::handlers::{AppState, Claims, ErrorResponse};
use crate::permissions::{require_any_role, require_permission, Permission};
use adapteros_api_types::orchestration::OrchestrationConfig;
use adapteros_db::users::Role;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};
use utoipa::ToSchema;

/// Request body for prompt analysis (stubbed)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PromptAnalysisRequest {
    pub prompt: String,
}

/// Get orchestration configuration (single-node stub)
#[utoipa::path(
    get,
    path = "/v1/orchestration/config",
    responses(
        (status = 200, description = "Orchestration configuration", body = adapteros_api_types::orchestration::OrchestrationConfig),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "orchestration"
)]
pub async fn get_orchestration_config(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<OrchestrationConfig>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    // Single-node deterministic config; future versions may hydrate from DB
    let config = OrchestrationConfig::default();

    Ok(Json(config))
}

/// Update orchestration configuration (echo-only stub)
#[utoipa::path(
    put,
    path = "/v1/orchestration/config",
    request_body = adapteros_api_types::orchestration::OrchestrationConfig,
    responses(
        (status = 200, description = "Updated orchestration configuration", body = adapteros_api_types::orchestration::OrchestrationConfig),
        (status = 400, description = "Invalid payload"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "orchestration"
)]
pub async fn update_orchestration_config(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<OrchestrationConfig>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterStackManage)?;

    // Echo back payload; persistence will be added once orchestration backend is wired.
    info!(
        user = %claims.sub,
        routing_strategy = %payload.routing_strategy,
        "Received orchestration config update (stub, not persisted)"
    );

    Ok((StatusCode::OK, Json(payload)))
}

/// Analyze prompt for orchestration (stubbed 501)
#[utoipa::path(
    post,
    path = "/v1/orchestration/analyze",
    request_body = PromptAnalysisRequest,
    responses(
        (status = 501, description = "Orchestration analysis not available", body = ErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    ),
    tag = "orchestration"
)]
pub async fn analyze_orchestration_prompt(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_body): Json<PromptAnalysisRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    warn!(
        user = %claims.sub,
        "orchestration analyze stub invoked; returning deterministic NOT_IMPLEMENTED"
    );

    Err::<Json<Value>, _>((
        StatusCode::NOT_IMPLEMENTED,
        Json(
            ErrorResponse::new(
                "Orchestration analysis is not available in single-node mode (stubbed)",
            )
            .with_code("NOT_IMPLEMENTED")
            .with_string_details(
                "Enable the orchestration service to receive prompt analysis responses.",
            ),
        ),
    ))
}

/// Retrieve orchestration metrics (stubbed 501)
#[utoipa::path(
    get,
    path = "/v1/orchestration/metrics",
    responses(
        (status = 501, description = "Orchestration metrics not available", body = ErrorResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    ),
    tag = "orchestration"
)]
pub async fn get_orchestration_metrics(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    warn!(
        user = %claims.sub,
        "orchestration metrics stub invoked; returning deterministic NOT_IMPLEMENTED"
    );

    Err::<Json<Value>, _>((
        StatusCode::NOT_IMPLEMENTED,
        Json(
            ErrorResponse::new(
                "Orchestration metrics are not available in single-node mode (stubbed)",
            )
            .with_code("NOT_IMPLEMENTED")
            .with_string_details(
                "Enable orchestration telemetry to expose metrics on this endpoint.",
            ),
        ),
    ))
}
