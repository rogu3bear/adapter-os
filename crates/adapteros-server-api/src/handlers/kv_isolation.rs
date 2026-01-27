use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::kv_isolation::{kv_isolation_config_from_env, run_kv_isolation_scan};
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::AosError;
use adapteros_db::users::Role;
use adapteros_db::KvIsolationScanReport;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct KvIsolationHealthResponse {
    pub last_started_at: Option<String>,
    pub last_completed_at: Option<String>,
    pub last_error: Option<String>,
    pub running: bool,
    pub last_report: Option<KvIsolationScanReport>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct KvIsolationScanRequest {
    pub sample_rate: Option<f64>,
    pub max_findings: Option<usize>,
    pub hash_seed: Option<String>,
}

#[utoipa::path(
    get,
    path = "/v1/storage/kv-isolation/health",
    responses(
        (status = 200, description = "Latest KV isolation scan snapshot", body = KvIsolationHealthResponse)
    ),
    tag = "storage",
    security(("bearer_token" = []))
)]
pub async fn get_kv_isolation_health(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<KvIsolationHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin])?;

    let snapshot = state
        .kv_isolation_snapshot
        .read()
        .map_err(|e| ApiError::internal(e.to_string()))?
        .clone();

    Ok(Json(KvIsolationHealthResponse {
        last_started_at: snapshot.last_started_at,
        last_completed_at: snapshot.last_completed_at,
        last_error: snapshot.last_error,
        running: snapshot.running,
        last_report: snapshot.last_report,
    }))
}

#[utoipa::path(
    post,
    path = "/v1/storage/kv-isolation/scan",
    request_body = KvIsolationScanRequest,
    responses(
        (status = 200, description = "KV isolation scan report", body = KvIsolationScanReport)
    ),
    tag = "storage",
    security(("bearer_token" = []))
)]
pub async fn trigger_kv_isolation_scan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<KvIsolationScanRequest>,
) -> Result<Json<KvIsolationScanReport>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin])?;

    let mut cfg = kv_isolation_config_from_env();
    if let Some(rate) = req.sample_rate {
        cfg.sample_rate = rate;
    }
    if let Some(max) = req.max_findings {
        cfg.max_findings = max;
    }
    if let Some(seed) = req.hash_seed {
        cfg.hash_seed = seed;
    }

    run_kv_isolation_scan(&state, cfg, "manual")
        .await
        .map(Json)
        .map_err(|e| ApiError::internal(e.to_string()))
}
