//! Health check and system status handlers

use crate::state::AppState;
use crate::supervisor_client;
use crate::types::*;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::warn;
use utoipa::ToSchema;

/// Health check endpoint
#[utoipa::path(
    tag = "system",
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        models: None,
    })
}

/// Readiness check
#[utoipa::path(
    tag = "system",
    get,
    path = "/readyz",
    responses(
        (status = 200, description = "Service is ready", body = HealthResponse),
        (status = 503, description = "Service is not ready", body = HealthResponse)
    )
)]
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    // Check boot state - only return ready if in Ready state
    if let Some(ref boot_state) = state.boot_state {
        if !boot_state.is_ready() {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                    status: format!("booting: {}", boot_state.current_state()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    models: None,
                }),
            );
        }
    }

    // Check database connectivity
    match state.db.pool().acquire().await {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                status: "ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                models: None,
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                status: "not ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                models: None,
            }),
        ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_id: Option<String>,
    pub inference_count: u64,
    pub total_tokens: u64,
    pub avg_latency_ms: f64,
    pub error_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance: Option<HashMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterServiceStatus {
    pub id: String,
    pub name: String,
    pub status: String,
    pub state: String,
    pub restart_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl From<supervisor_client::ServiceStatus> for AdapterServiceStatus {
    fn from(service: supervisor_client::ServiceStatus) -> Self {
        let state = service.state;
        let status = normalize_service_status(&state);

        AdapterServiceStatus {
            id: service.id,
            name: service.name,
            status: status.to_string(),
            state,
            restart_count: service.restart_count,
            last_error: service.last_error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterOSStatusResponse {
    pub adapter_id: String,
    pub os_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_health_check: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<AdapterMetrics>,
    #[serde(default)]
    pub services: Vec<AdapterServiceStatus>,
}

fn normalize_service_status(raw_state: &str) -> &'static str {
    match raw_state {
        "failed" => "error",
        "restarting" => "starting",
        "running" => "running",
        "stopped" => "stopped",
        "starting" => "starting",
        "error" => "error",
        "active" => "active",
        "inactive" => "inactive",
        _ => "unknown",
    }
}

fn determine_overall_status(
    services: &[AdapterServiceStatus],
    supervisor_available: bool,
) -> &'static str {
    if !supervisor_available || services.iter().any(|s| s.state == "failed") {
        "error"
    } else if services.iter().any(|s| s.status == "running") {
        "active"
    } else {
        "inactive"
    }
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/status",
    responses(
        (status = 200, description = "AdapterOS status snapshot", body = AdapterOSStatusResponse)
    )
)]
pub async fn get_status(State(state): State<AppState>) -> Json<AdapterOSStatusResponse> {
    let (service_statuses, supervisor_available) =
        match supervisor_client::SupervisorClient::from_env()
            .get_services()
            .await
        {
            Ok(services) => (
                services
                    .into_iter()
                    .map(AdapterServiceStatus::from)
                    .collect::<Vec<_>>(),
                true,
            ),
            Err(err) => {
                warn!(error = %err, "Failed to fetch service status from supervisor");
                (Vec::new(), false)
            }
        };

    let runtime_mode_id = state
        .runtime_mode
        .map(|mode| format!("adapteros-{}", mode.as_str()))
        .unwrap_or_else(|| "adapteros".to_string());

    let os_status = determine_overall_status(&service_statuses, supervisor_available).to_string();

    Json(AdapterOSStatusResponse {
        adapter_id: runtime_mode_id,
        os_status,
        last_health_check: Some(Utc::now().to_rfc3339()),
        metrics: None,
        services: service_statuses,
    })
}
