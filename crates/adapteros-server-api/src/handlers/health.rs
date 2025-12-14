//! Health check and system status handlers

use crate::boot_state::BootState;
use crate::state::AppState;
use crate::supervisor_client;
use crate::types::*;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::query;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadyzCheck {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadyzChecks {
    pub db: ReadyzCheck,
    pub worker: ReadyzCheck,
    pub models_seeded: ReadyzCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadyzResponse {
    pub ready: bool,
    pub checks: ReadyzChecks,
}

/// Readiness check
#[utoipa::path(
    tag = "system",
    get,
    path = "/readyz",
    responses(
        (status = 200, description = "Service is ready", body = ReadyzResponse),
        (status = 503, description = "Service is not ready", body = ReadyzResponse)
    )
)]
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let mut ready = true;

    let mut db_check = ReadyzCheck {
        ok: true,
        hint: None,
    };
    let mut worker_check = ReadyzCheck {
        ok: true,
        hint: None,
    };
    let mut models_seeded_check = ReadyzCheck {
        ok: true,
        hint: None,
    };

    // Check boot state - only return ready if in Ready state
    let Some(ref boot_state) = state.boot_state else {
        ready = false;
        worker_check.ok = false;
        worker_check.hint = Some("boot state manager not configured".to_string());

        let status_code = StatusCode::SERVICE_UNAVAILABLE;
        return (
            status_code,
            Json(ReadyzResponse {
                ready,
                checks: ReadyzChecks {
                    db: db_check,
                    worker: worker_check,
                    models_seeded: models_seeded_check,
                },
            }),
        );
    };

    let mut current = boot_state.current_state();

    // During early startup, treat an uninitialized state as booting to avoid
    // reporting "stopped" in readiness responses.
    if matches!(current, BootState::Stopped) {
        current = BootState::Booting;
    }

    if current.is_maintenance() {
        ready = false;
        worker_check.ok = false;
        worker_check.hint = Some("maintenance".to_string());
    } else if current.is_draining() || current.is_shutting_down() {
        ready = false;
        worker_check.ok = false;
        worker_check.hint = Some("draining".to_string());
    } else if !boot_state.is_ready() {
        ready = false;
        worker_check.ok = false;
        worker_check.hint = Some(format!("booting: {}", current));
    }

    // Check database connectivity
    const DB_TIMEOUT: Duration = Duration::from_secs(1);
    let db_probe = timeout(DB_TIMEOUT, async {
        let mut conn = state.db.pool().acquire().await?;
        query("SELECT 1").execute(&mut *conn).await?;
        Ok::<(), sqlx::Error>(())
    })
    .await;

    match db_probe {
        Ok(Ok(())) => {}
        Ok(Err(_)) => {
            ready = false;
            db_check.ok = false;
            db_check.hint = Some("db unreachable".to_string());
        }
        Err(_) => {
            ready = false;
            db_check.ok = false;
            db_check.hint = Some("db timeout".to_string());
        }
    }

    if !db_check.ok {
        worker_check.ok = false;
        if worker_check.hint.is_none() {
            worker_check.hint = Some("database unavailable (cannot check workers)".to_string());
        }
        models_seeded_check.ok = false;
        models_seeded_check.hint = Some("database unavailable (cannot check models)".to_string());
    } else {
        // Report worker presence as informational readiness checks.
        if worker_check.ok {
            match state.db.count_active_workers().await {
                Ok(count) if count > 0 => {}
                Ok(_) => {
                    worker_check.ok = false;
                    worker_check.hint = Some("no workers registered".to_string());
                }
                Err(_) => {
                    worker_check.ok = false;
                    worker_check.hint = Some("failed to query workers".to_string());
                }
            }
        }

        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM models")
            .fetch_one(state.db.pool())
            .await
        {
            Ok(count) if count > 0 => {}
            Ok(_) => {
                models_seeded_check.ok = false;
                models_seeded_check.hint = Some("no models seeded".to_string());
            }
            Err(_) => {
                models_seeded_check.ok = false;
                models_seeded_check.hint = Some("failed to query models".to_string());
            }
        }
    }

    // Final readiness is the conjunction of the individual checks.
    ready = db_check.ok && worker_check.ok && models_seeded_check.ok;

    let status_code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadyzResponse {
            ready,
            checks: ReadyzChecks {
                db: db_check,
                worker: worker_check,
                models_seeded: models_seeded_check,
            },
        }),
    )
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
pub struct LifecycleStatusResponse {
    pub role: String,
    pub lifecycle: String,
    #[serde(default)]
    pub flags: Vec<String>,
    pub environment: String,
    pub ready: bool,
    pub system_ready: SystemReadySection,
    pub drain: DrainSection,
    pub maintenance: MaintenanceSection,
    pub restart: RestartSection,
    pub telemetry: TelemetrySection,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemReadySection {
    pub ready: bool,
    pub critical_degraded: Vec<String>,
    pub non_critical_degraded: Vec<String>,
    pub maintenance: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DrainSection {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_flight_requests: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_flight_jobs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MaintenanceSection {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RestartSection {
    pub supervisor_hook_configured: bool,
    pub restart_counter: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_restart_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetrySection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_registration_heartbeat_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_at: Option<String>,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn determine_overall_status(
    services: &[AdapterServiceStatus],
    supervisor_available: bool,
) -> &'static str {
    // Fail only when a service is explicitly failed
    if services.iter().any(|s| s.state == "failed") {
        return "error";
    }

    let any_running = services
        .iter()
        .any(|s| s.status == "running" || s.status == "active");

    if any_running {
        "active"
    } else if supervisor_available {
        "inactive"
    } else {
        // Supervisor absent but no failures: treat as active for single-node setups
        "active"
    }
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/status",
    responses(
        (status = 200, description = "Lifecycle status snapshot", body = LifecycleStatusResponse)
    )
)]
pub async fn get_status(State(state): State<AppState>) -> Json<LifecycleStatusResponse> {
    let runtime_mode = state
        .runtime_mode
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "dev".to_string());

    let lifecycle = map_boot_state(&state);

    let (components, _boot_elapsed_ms) =
        crate::health::gather_system_ready_components(state.clone()).await;
    let critical_components = ["server", "db", "router"];
    let mut critical_degraded = Vec::new();
    let mut non_critical_degraded = Vec::new();

    for comp in components.iter() {
        if comp.status == crate::health::ComponentStatus::Healthy {
            continue;
        }
        if critical_components.contains(&comp.component.as_str()) {
            critical_degraded.push(comp.component.clone());
        } else {
            non_critical_degraded.push(comp.component.clone());
        }
    }

    let maintenance_active = state
        .boot_state
        .as_ref()
        .map(|b| b.is_maintenance())
        .unwrap_or(false);
    let ready = critical_degraded.is_empty() && !maintenance_active;
    let reason = if maintenance_active {
        "maintenance".to_string()
    } else if !critical_degraded.is_empty() {
        format!("critical degraded: {:?}", critical_degraded)
    } else if !non_critical_degraded.is_empty() {
        format!("non-critical degraded: {:?}", non_critical_degraded)
    } else {
        "ready".to_string()
    };

    let drain_active = state
        .boot_state
        .as_ref()
        .map(|b| b.is_draining())
        .unwrap_or(false);
    let in_flight = state
        .in_flight_requests
        .load(std::sync::atomic::Ordering::Relaxed) as i64;

    let mut lifecycle = map_boot_state(&state);
    if lifecycle == "ready" && !critical_degraded.is_empty() {
        lifecycle = "degraded".to_string();
    }

    Json(LifecycleStatusResponse {
        role: "control-plane".to_string(),
        lifecycle,
        flags: Vec::new(),
        environment: runtime_mode,
        ready,
        system_ready: SystemReadySection {
            ready,
            critical_degraded,
            non_critical_degraded,
            maintenance: maintenance_active,
            reason: reason.clone(),
        },
        drain: DrainSection {
            active: drain_active,
            in_flight_requests: Some(in_flight),
            in_flight_jobs: None,
            started_at: None,
            deadline_at: None,
        },
        maintenance: MaintenanceSection {
            active: maintenance_active,
            reason: if maintenance_active {
                Some(reason)
            } else {
                None
            },
            actor: None,
        },
        restart: RestartSection {
            supervisor_hook_configured: false,
            restart_counter: 0,
            last_restart_at: None,
        },
        telemetry: TelemetrySection {
            last_registration_heartbeat_at: None,
            last_error: None,
            last_error_at: None,
        },
    })
}

fn map_boot_state(state: &AppState) -> String {
    if let Some(ref boot_state) = state.boot_state {
        match boot_state.current_state() {
            crate::boot_state::BootState::Stopped => "stopped",
            crate::boot_state::BootState::Booting
            | crate::boot_state::BootState::InitializingDb
            | crate::boot_state::BootState::LoadingPolicies
            | crate::boot_state::BootState::StartingBackend
            | crate::boot_state::BootState::LoadingBaseModels
            | crate::boot_state::BootState::LoadingAdapters => "booting",
            crate::boot_state::BootState::Ready | crate::boot_state::BootState::FullyReady => {
                "ready"
            }
            crate::boot_state::BootState::Maintenance => "maintenance",
            crate::boot_state::BootState::Draining => "draining",
            crate::boot_state::BootState::Stopping => "stopping",
        }
        .to_string()
    } else {
        "unknown".to_string()
    }
}
