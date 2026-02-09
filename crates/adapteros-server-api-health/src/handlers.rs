//! Health check and system status handlers
//!
//! Split from adapteros-server-api for faster incremental builds.

use adapteros_api_types::ModelLoadStatus;
use adapteros_server_api::auth::is_dev_bypass_enabled;
use adapteros_server_api::boot_state::{BootState, BootWarning};
use adapteros_server_api::state::{AppState, BackgroundTaskSnapshot};
use adapteros_server_api::supervisor_client;
use adapteros_server_api::types::HealthResponse;
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
        Json({
            let provenance = adapteros_core::version::BuildProvenance::cached();
            let manifest = &provenance.crate_manifest;
            HealthResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                status: status_str,
                version: env!("CARGO_PKG_VERSION").to_string(),
                build_id: option_env!("AOS_BUILD_ID").map(|s| s.to_string()),
                models: None,
                crate_manifest: Some(manifest.crates.clone()),
                crate_manifest_digest: manifest.digest.as_ref().map(|d| d.to_hex()),
            }
        }),
    )
}

#[cfg(test)]
mod build_id_tests {
    #[test]
    fn build_id_is_set_and_canonical() {
        let build_id = option_env!("AOS_BUILD_ID").expect("AOS_BUILD_ID must be set by build.rs");
        assert!(
            !build_id.trim().is_empty(),
            "AOS_BUILD_ID must not be empty"
        );
        let (_prefix, ts) = build_id
            .rsplit_once('-')
            .expect("AOS_BUILD_ID should contain '-' separating timestamp");
        assert!(
            ts.len() == 14 && ts.chars().all(|c| c.is_ascii_digit()),
            "AOS_BUILD_ID should end with YYYYMMDDHHmmss: {build_id}"
        );
    }
}

/// Readiness mode indicates how strictly checks are enforced.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ReadinessMode {
    /// Full readiness checks enforced - all checks must pass for ready=true
    #[default]
    Strict,
    /// Some checks are relaxed (e.g., skip-worker mode)
    Relaxed { relaxed_checks: Vec<String> },
    /// Dev bypass active - all checks are informational
    DevBypass,
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
    pub phases: Vec<adapteros_server_api::boot_state::PhaseStatus>,
    #[serde(default)]
    pub readiness_mode: ReadinessMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boot_warnings: Vec<BootWarning>,
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
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
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

    // Determine readiness mode
    let skip_worker_check = {
        let cfg = state.config.read().unwrap_or_else(|e| {
            tracing::warn!("Config lock poisoned in readiness mode check, recovering");
            e.into_inner()
        });
        cfg.server.skip_worker_check
    };
    let readiness_mode = if is_dev_bypass_enabled() {
        ReadinessMode::DevBypass
    } else if skip_worker_check {
        ReadinessMode::Relaxed {
            relaxed_checks: vec!["worker".to_string()],
        }
    } else {
        ReadinessMode::Strict
    };

    // Check boot state
    let Some(ref boot_state) = state.boot_state else {
        worker_check.ok = false;
        worker_check.hint = Some("boot state manager not configured".to_string());

        let status_code = if matches!(readiness_mode, ReadinessMode::DevBypass) {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };

        return (
            status_code,
            Json(ReadyzResponse {
                ready: false,
                checks: ReadyzChecks {
                    db: db_check,
                    worker: worker_check,
                    models_seeded: models_seeded_check,
                },
                metrics: None,
                boot_trace_id: String::new(),
                last_error_code: None,
                phases: Vec::new(),
                readiness_mode,
                boot_warnings: Vec::new(),
            }),
        );
    };

    let mut current = boot_state.current_state();

    if matches!(current, BootState::Stopped) {
        current = BootState::Starting;
    }

    if current.is_failed() {
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
        worker_check.hint = Some("degraded (non-critical)".to_string());
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
    const DB_TIMEOUT_FALLBACK_MS: u64 = 2000;
    let timeout_ms = {
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
        // Worker check
        if worker_check.ok {
            let worker_timeout_ms = {
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

        // Models check
        let models_timeout_ms = {
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

        // Model mismatch detection
        if models_seeded_check.ok {
            if let (Ok(active_states), Ok(statuses)) = (
                state.db.list_workspace_active_states().await,
                state.db.list_base_model_statuses().await,
            ) {
                let has_mismatch = active_states.iter().any(|active| {
                    let Some(model_id) = active.active_base_model_id.as_deref() else {
                        return false;
                    };

                    let ready = statuses
                        .iter()
                        .find(|s| s.tenant_id == active.tenant_id && s.model_id == model_id)
                        .map(|s| ModelLoadStatus::parse_status(&s.status).is_ready())
                        .unwrap_or(false);

                    !ready
                });

                if has_mismatch && models_seeded_check.hint.is_none() {
                    models_seeded_check.hint =
                        Some("active model mismatch (not loaded)".to_string());
                }
            }
        }
    }

    // Final readiness
    let ready = match &readiness_mode {
        ReadinessMode::Strict => db_check.ok && worker_check.ok && models_seeded_check.ok,
        ReadinessMode::Relaxed { relaxed_checks } => {
            let mut result = db_check.ok;
            if !relaxed_checks.contains(&"worker".to_string()) {
                result = result && worker_check.ok;
            }
            if !relaxed_checks.contains(&"models".to_string()) {
                result = result && models_seeded_check.ok;
            }
            result
        }
        ReadinessMode::DevBypass => true,
    };

    let status_code = if matches!(readiness_mode, ReadinessMode::DevBypass) || ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    // Emit metrics
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
            readiness_mode,
            boot_warnings: boot_state.get_boot_warnings(),
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

    let (components, _boot_elapsed_ms) =
        adapteros_server_api::health::gather_system_ready_components(state.clone()).await;
    let critical_components = ["server", "db", "router"];
    let mut critical_degraded = Vec::new();
    let mut non_critical_degraded = Vec::new();

    for comp in components.iter() {
        if comp.status == adapteros_server_api::health::ComponentStatus::Healthy {
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
    if let Some(ref boot_state) = state.boot_state {
        match boot_state.current_state() {
            BootState::Stopped => "stopped",
            BootState::Starting
            | BootState::SecurityInit
            | BootState::ExecutorInit
            | BootState::Preflight
            | BootState::BootInvariants
            | BootState::DbConnecting
            | BootState::Migrating
            | BootState::PostDbInvariants
            | BootState::StartupRecovery
            | BootState::Seeding
            | BootState::LoadingPolicies
            | BootState::StartingBackend
            | BootState::LoadingBaseModels
            | BootState::LoadingAdapters
            | BootState::WorkerDiscovery
            | BootState::RouterBuild
            | BootState::Finalize
            | BootState::Bind => "booting",
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

// =============================================================================
// Boot Invariants Status Endpoint
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvariantViolationDto {
    pub id: String,
    pub message: String,
    pub is_fatal: bool,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvariantStatusResponse {
    pub checked: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub fatal: u64,
    pub violations: Vec<InvariantViolationDto>,
    pub skipped_ids: Vec<String>,
    pub production_mode: bool,
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/invariants",
    responses(
        (status = 200, description = "Invariants status", body = InvariantStatusResponse)
    )
)]
pub async fn get_invariant_status(State(state): State<AppState>) -> Json<InvariantStatusResponse> {
    let production_mode = {
        let cfg = state.config.read().unwrap_or_else(|e| {
            tracing::warn!("Config lock poisoned in invariants check, recovering");
            e.into_inner()
        });
        cfg.server.production_mode
    };

    let metrics = adapteros_boot::boot_invariant_metrics();

    let passed = metrics
        .checked
        .saturating_sub(metrics.violated)
        .saturating_sub(metrics.skipped);

    Json(InvariantStatusResponse {
        checked: metrics.checked,
        passed,
        failed: metrics.violated,
        skipped: metrics.skipped,
        fatal: metrics.fatal,
        violations: Vec::new(),
        skipped_ids: Vec::new(),
        production_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readyz_check_serde() {
        let check = ReadyzCheck {
            ok: true,
            hint: Some("all good".to_string()),
            latency_ms: Some(42),
        };
        let json = serde_json::to_string(&check).unwrap();
        let parsed: ReadyzCheck = serde_json::from_str(&json).unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.hint, Some("all good".to_string()));
        assert_eq!(parsed.latency_ms, Some(42));
    }

    #[test]
    fn test_readyz_check_skips_none() {
        let check = ReadyzCheck {
            ok: false,
            hint: None,
            latency_ms: None,
        };
        let json = serde_json::to_string(&check).unwrap();
        assert!(!json.contains("hint"));
        assert!(!json.contains("latency_ms"));
    }

    #[test]
    fn test_readyz_checks_serde() {
        let checks = ReadyzChecks {
            db: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(5),
            },
            worker: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(10),
            },
            models_seeded: ReadyzCheck {
                ok: false,
                hint: Some("no models".to_string()),
                latency_ms: None,
            },
        };
        let json = serde_json::to_string(&checks).unwrap();
        let parsed: ReadyzChecks = serde_json::from_str(&json).unwrap();
        assert!(parsed.db.ok);
        assert!(parsed.worker.ok);
        assert!(!parsed.models_seeded.ok);
    }

    #[test]
    fn test_readiness_mode_strict_serde() {
        let mode = ReadinessMode::Strict;
        let json = serde_json::to_string(&mode).unwrap();
        assert!(json.contains("strict"));
        let parsed: ReadinessMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ReadinessMode::Strict));
    }

    #[test]
    fn test_readiness_mode_relaxed_serde() {
        let mode = ReadinessMode::Relaxed {
            relaxed_checks: vec!["worker".to_string(), "models".to_string()],
        };
        let json = serde_json::to_string(&mode).unwrap();
        assert!(json.contains("relaxed"));
        assert!(json.contains("worker"));
        let parsed: ReadinessMode = serde_json::from_str(&json).unwrap();
        if let ReadinessMode::Relaxed { relaxed_checks } = parsed {
            assert_eq!(relaxed_checks.len(), 2);
        } else {
            panic!("Expected Relaxed variant");
        }
    }

    #[test]
    fn test_readiness_mode_dev_bypass_serde() {
        let mode = ReadinessMode::DevBypass;
        let json = serde_json::to_string(&mode).unwrap();
        assert!(json.contains("dev_bypass"));
        let parsed: ReadinessMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ReadinessMode::DevBypass));
    }

    #[test]
    fn test_boot_phase_duration_serde() {
        let phase = BootPhaseDuration {
            state: "Migrating".to_string(),
            elapsed_ms: 1500,
        };
        let json = serde_json::to_string(&phase).unwrap();
        let parsed: BootPhaseDuration = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.state, "Migrating");
        assert_eq!(parsed.elapsed_ms, 1500);
    }

    #[test]
    fn test_ready_metrics_serde() {
        let metrics = ReadyMetrics {
            boot_phases_ms: vec![
                BootPhaseDuration {
                    state: "Starting".to_string(),
                    elapsed_ms: 100,
                },
                BootPhaseDuration {
                    state: "Ready".to_string(),
                    elapsed_ms: 50,
                },
            ],
            db_latency_ms: Some(5),
            worker_latency_ms: Some(10),
            models_latency_ms: None,
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let parsed: ReadyMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.boot_phases_ms.len(), 2);
        assert_eq!(parsed.db_latency_ms, Some(5));
    }

    #[test]
    fn test_ready_metrics_default() {
        let metrics = ReadyMetrics::default();
        assert!(metrics.boot_phases_ms.is_empty());
        assert!(metrics.db_latency_ms.is_none());
    }

    #[test]
    fn test_drain_section_serde() {
        let drain = DrainSection {
            active: true,
            in_flight_requests: Some(5),
            in_flight_jobs: Some(2),
            started_at: Some("2025-01-27T00:00:00Z".to_string()),
            deadline_at: Some("2025-01-27T00:05:00Z".to_string()),
        };
        let json = serde_json::to_string(&drain).unwrap();
        let parsed: DrainSection = serde_json::from_str(&json).unwrap();
        assert!(parsed.active);
        assert_eq!(parsed.in_flight_requests, Some(5));
    }

    #[test]
    fn test_maintenance_section_serde() {
        let maintenance = MaintenanceSection {
            active: true,
            reason: Some("scheduled maintenance".to_string()),
            actor: Some("admin".to_string()),
        };
        let json = serde_json::to_string(&maintenance).unwrap();
        let parsed: MaintenanceSection = serde_json::from_str(&json).unwrap();
        assert!(parsed.active);
        assert_eq!(parsed.reason, Some("scheduled maintenance".to_string()));
    }

    #[test]
    fn test_restart_section_serde() {
        let restart = RestartSection {
            supervisor_hook_configured: true,
            restart_counter: 3,
            last_restart_at: Some("2025-01-27T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&restart).unwrap();
        let parsed: RestartSection = serde_json::from_str(&json).unwrap();
        assert!(parsed.supervisor_hook_configured);
        assert_eq!(parsed.restart_counter, 3);
    }

    #[test]
    fn test_telemetry_section_serde() {
        let telemetry = TelemetrySection {
            last_registration_heartbeat_at: Some("2025-01-27T00:00:00Z".to_string()),
            last_error: Some("connection timeout".to_string()),
            last_error_at: Some("2025-01-26T23:59:00Z".to_string()),
        };
        let json = serde_json::to_string(&telemetry).unwrap();
        let parsed: TelemetrySection = serde_json::from_str(&json).unwrap();
        assert!(parsed.last_error.is_some());
    }

    #[test]
    fn test_system_ready_section_serde() {
        let section = SystemReadySection {
            ready: false,
            critical_degraded: vec!["db".to_string()],
            non_critical_degraded: vec!["metrics".to_string()],
            maintenance: false,
            reason: "critical degraded: [\"db\"]".to_string(),
        };
        let json = serde_json::to_string(&section).unwrap();
        let parsed: SystemReadySection = serde_json::from_str(&json).unwrap();
        assert!(!parsed.ready);
        assert_eq!(parsed.critical_degraded.len(), 1);
    }

    #[test]
    fn test_invariant_violation_dto_serde() {
        let violation = InvariantViolationDto {
            id: "INV-001".to_string(),
            message: "Seed not configured".to_string(),
            is_fatal: true,
            remediation: "Set AOS_GLOBAL_SEED environment variable".to_string(),
        };
        let json = serde_json::to_string(&violation).unwrap();
        let parsed: InvariantViolationDto = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "INV-001");
        assert!(parsed.is_fatal);
    }

    #[test]
    fn test_invariant_status_response_serde() {
        let response = InvariantStatusResponse {
            checked: 10,
            passed: 8,
            failed: 1,
            skipped: 1,
            fatal: 0,
            violations: vec![InvariantViolationDto {
                id: "INV-002".to_string(),
                message: "Minor issue".to_string(),
                is_fatal: false,
                remediation: "Check configuration".to_string(),
            }],
            skipped_ids: vec!["INV-SKIP-001".to_string()],
            production_mode: false,
        };
        let json = serde_json::to_string(&response).unwrap();
        let parsed: InvariantStatusResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.checked, 10);
        assert_eq!(parsed.passed, 8);
        assert_eq!(parsed.violations.len(), 1);
    }

    #[test]
    fn test_normalize_service_status() {
        assert_eq!(normalize_service_status("failed"), "error");
        assert_eq!(normalize_service_status("restarting"), "starting");
        assert_eq!(normalize_service_status("running"), "running");
        assert_eq!(normalize_service_status("stopped"), "stopped");
        assert_eq!(normalize_service_status("unknown_state"), "unknown");
    }
}
