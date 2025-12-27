//! Health check and system status handlers

use crate::boot_state::BootState;
use crate::state::{AppState, BackgroundTaskSnapshot};
use crate::supervisor_client;
use crate::types::*;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::query;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use utoipa::ToSchema;

/// Health check endpoint
///
/// Returns 503 during boot to allow orchestrators to wait for startup.
/// Returns 200 once the service is ready or in maintenance/draining states.
#[utoipa::path(
    tag = "system",
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
        (status = 503, description = "Service is booting", body = HealthResponse)
    )
)]
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    // Check boot state - return 503 if still booting or failed
    let (status_code, status_str) = if let Some(ref boot_state) = state.boot_state {
        let current = boot_state.current_state();
        if current.is_failed() {
            // Failed state - return 503 with failure details
            if let Some(failure_reason) = boot_state.get_failure_reason() {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!(
                        "failed: [{}] {}",
                        failure_reason.code, failure_reason.message
                    ),
                )
            } else {
                (StatusCode::SERVICE_UNAVAILABLE, "failed".to_string())
            }
        } else if current.is_degraded() {
            // Degraded is operational but with reduced functionality
            (StatusCode::OK, "degraded".to_string())
        } else if boot_state.is_ready() {
            (StatusCode::OK, "healthy".to_string())
        } else if current.is_maintenance() {
            // Maintenance is a valid "alive" state
            (StatusCode::OK, "maintenance".to_string())
        } else if current.is_draining() || current.is_shutting_down() {
            // Draining/stopping means the service is still alive but winding down
            (StatusCode::OK, format!("draining: {}", current))
        } else {
            // Still booting - return 503
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("booting: {}", current),
            )
        }
    } else {
        // No boot state configured - assume healthy (backwards compatibility)
        (StatusCode::OK, "healthy".to_string())
    };

    (
        status_code,
        Json(HealthResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            status: status_str,
            version: env!("CARGO_PKG_VERSION").to_string(),
            models: None,
        }),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadyzCheck {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ReadyMetrics>,
    pub boot_trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<crate::boot_state::PhaseStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct ReadyMetrics {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boot_phases_ms: Vec<BootPhaseDuration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models_latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BootPhaseDuration {
    pub state: String,
    pub elapsed_ms: u64,
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
#[allow(unused_assignments)] // ready is initialized but always overwritten before use
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let mut ready = true;

    let mut db_check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: None,
    };
    let mut worker_check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: None,
    };
    let mut models_seeded_check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: None,
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
                metrics: None,
                boot_trace_id: String::new(),
                last_error_code: None,
                phases: Vec::new(),
            }),
        );
    };

    let mut current = boot_state.current_state();

    // During early startup, treat an uninitialized state as booting to avoid
    // reporting "stopped" in readiness responses.
    if matches!(current, BootState::Stopped) {
        current = BootState::Starting;
    }

    if current.is_failed() {
        // Failed state - system is not ready
        worker_check.ok = false;
        if let Some(failure_reason) = boot_state.get_failure_reason() {
            worker_check.hint = Some(format!(
                "failed: [{}] {}",
                failure_reason.code, failure_reason.message
            ));
        } else {
            worker_check.hint = Some("failed".to_string());
        }
    } else if current.is_degraded() {
        // Degraded state - system is operational but should not advertise ready
        worker_check.ok = false;
        worker_check.hint = Some("degraded".to_string());
    } else if current.is_maintenance() {
        worker_check.ok = false;
        worker_check.hint = Some("maintenance".to_string());
    } else if current.is_draining() || current.is_shutting_down() {
        worker_check.ok = false;
        worker_check.hint = Some("draining".to_string());
    } else if !boot_state.is_ready() {
        worker_check.ok = false;
        worker_check.hint = Some(format!("booting: {}", current));
    }

    // Check database connectivity
    // Use configured timeout, fallback to 2 seconds (2000ms) if not configured
    const DB_TIMEOUT_FALLBACK_MS: u64 = 2000;
    let timeout_ms = {
        // STABILITY: Use poison-safe lock access
        let cfg = state.config.read().unwrap_or_else(|e| {
            tracing::warn!("Config lock poisoned in health check, recovering");
            e.into_inner()
        });
        cfg.server.health_check_db_timeout_ms
    };
    let db_timeout = if timeout_ms > 0 {
        Duration::from_millis(timeout_ms)
    } else {
        Duration::from_millis(DB_TIMEOUT_FALLBACK_MS)
    };

    let db_start = Instant::now();
    let db_probe = timeout(db_timeout, async {
        let mut conn = state.db.pool().acquire().await?;
        query("SELECT 1").execute(&mut *conn).await?;
        Ok::<(), sqlx::Error>(())
    })
    .await;
    let db_latency = db_start.elapsed().as_millis() as u64;

    match db_probe {
        Ok(Ok(())) => {
            db_check.latency_ms = Some(db_latency);
        }
        Ok(Err(_)) => {
            db_check.ok = false;
            db_check.hint = Some("db unreachable".to_string());
            db_check.latency_ms = Some(db_latency);
        }
        Err(_) => {
            db_check.ok = false;
            db_check.hint = Some("db timeout".to_string());
            db_check.latency_ms = Some(db_latency);
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
            let worker_timeout_ms = {
                // STABILITY: Use poison-safe lock access
                let cfg = state.config.read().unwrap_or_else(|e| {
                    tracing::warn!("Config lock poisoned in health worker check, recovering");
                    e.into_inner()
                });
                cfg.server.health_check_worker_timeout_ms
            };
            let worker_timeout = if worker_timeout_ms > 0 {
                Duration::from_millis(worker_timeout_ms)
            } else {
                Duration::from_millis(2000)
            };

            let worker_start = Instant::now();
            let worker_probe = timeout(worker_timeout, state.db.count_active_workers()).await;
            let worker_latency = worker_start.elapsed().as_millis() as u64;

            match worker_probe {
                Ok(Ok(count)) if count > 0 => {
                    worker_check.latency_ms = Some(worker_latency);
                }
                Ok(Ok(_)) => {
                    worker_check.ok = false;
                    worker_check.hint = Some("no workers registered".to_string());
                    worker_check.latency_ms = Some(worker_latency);
                }
                Ok(Err(_)) => {
                    worker_check.ok = false;
                    worker_check.hint = Some("failed to query workers".to_string());
                    worker_check.latency_ms = Some(worker_latency);
                }
                Err(_) => {
                    worker_check.ok = false;
                    worker_check.hint = Some("worker check timeout".to_string());
                    worker_check.latency_ms = Some(worker_latency);
                }
            }
        }

        let models_timeout_ms = {
            // STABILITY: Use poison-safe lock access
            let cfg = state.config.read().unwrap_or_else(|e| {
                tracing::warn!("Config lock poisoned in health models check, recovering");
                e.into_inner()
            });
            cfg.server.health_check_models_timeout_ms
        };
        let models_timeout = if models_timeout_ms > 0 {
            Duration::from_millis(models_timeout_ms)
        } else {
            Duration::from_millis(2000)
        };

        let models_start = Instant::now();
        let models_probe = timeout(
            models_timeout,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM models").fetch_one(state.db.pool()),
        )
        .await;
        let models_latency = models_start.elapsed().as_millis() as u64;

        match models_probe {
            Ok(Ok(count)) if count > 0 => {
                models_seeded_check.latency_ms = Some(models_latency);
            }
            Ok(Ok(_)) => {
                models_seeded_check.ok = false;
                models_seeded_check.hint = Some("no models seeded".to_string());
                models_seeded_check.latency_ms = Some(models_latency);
            }
            Ok(Err(_)) => {
                models_seeded_check.ok = false;
                models_seeded_check.hint = Some("failed to query models".to_string());
                models_seeded_check.latency_ms = Some(models_latency);
            }
            Err(_) => {
                models_seeded_check.ok = false;
                models_seeded_check.hint = Some("models check timeout".to_string());
                models_seeded_check.latency_ms = Some(models_latency);
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

    // Emit readiness metrics and boot phase durations for observability
    let registry = state.metrics_registry.clone();
    if let Some(latency) = db_check.latency_ms {
        registry
            .set_gauge("readyz_db_latency_ms".to_string(), latency as f64)
            .await;
    }
    if let Some(latency) = worker_check.latency_ms {
        registry
            .set_gauge("readyz_worker_latency_ms".to_string(), latency as f64)
            .await;
    }
    if let Some(latency) = models_seeded_check.latency_ms {
        registry
            .set_gauge("readyz_models_latency_ms".to_string(), latency as f64)
            .await;
    }

    let boot_phase_metrics = boot_state
        .transition_history()
        .into_iter()
        .map(|t| BootPhaseDuration {
            state: t.to.as_str().to_string(),
            elapsed_ms: t.elapsed.as_millis() as u64,
        })
        .collect::<Vec<_>>();

    for phase in &boot_phase_metrics {
        registry
            .set_gauge(
                format!("boot_phase_duration_ms.{}", phase.state),
                phase.elapsed_ms as f64,
            )
            .await;
    }

    let db_latency_ms = db_check.latency_ms;
    let worker_latency_ms = worker_check.latency_ms;
    let models_latency_ms = models_seeded_check.latency_ms;

    (
        status_code,
        Json(ReadyzResponse {
            ready,
            checks: ReadyzChecks {
                db: db_check,
                worker: worker_check,
                models_seeded: models_seeded_check,
            },
            metrics: Some(ReadyMetrics {
                boot_phases_ms: boot_phase_metrics,
                db_latency_ms,
                worker_latency_ms,
                models_latency_ms,
            }),
            boot_trace_id: boot_state.boot_trace_id(),
            last_error_code: boot_state.last_error_code(),
            phases: boot_state.phase_statuses(),
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
    #[serde(default)]
    pub background_tasks: BackgroundTaskSnapshot,
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
        background_tasks: state.background_task_snapshot(),
    })
}

fn map_boot_state(state: &AppState) -> String {
    use crate::boot_state::BootState;
    if let Some(ref boot_state) = state.boot_state {
        match boot_state.current_state() {
            BootState::Stopped => "stopped",
            // All booting states (new granular states + legacy aliases)
            BootState::Starting
            | BootState::DbConnecting
            | BootState::Migrating
            | BootState::Seeding
            | BootState::LoadingPolicies
            | BootState::StartingBackend
            | BootState::LoadingBaseModels
            | BootState::LoadingAdapters
            | BootState::WorkerDiscovery
            | BootState::Booting
            | BootState::InitializingDb => "booting",
            BootState::Ready | BootState::FullyReady => "ready",
            BootState::Degraded => "degraded",
            BootState::Failed => "failed",
            BootState::Maintenance => "maintenance",
            BootState::Draining => "draining",
            BootState::Stopping => "stopping",
        }
        .to_string()
    } else {
        "unknown".to_string()
    }
}
