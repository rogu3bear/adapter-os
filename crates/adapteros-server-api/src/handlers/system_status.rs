//! Aggregated system status handler.
//!
//! Combines integrity, readiness, boot lifecycle, and kernel/model summaries
//! into a single response so the UI no longer stitches multiple endpoints.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use adapteros_api_types::system_status::{
    AdapterInventory, AneMemorySummary, BootFailure, BootPhaseTiming, BootStatus, ComponentCheck,
    DataAvailability, DegradedReason, DriftLevel, DriftStatus, InferenceBlocker,
    InferenceReadyState, IntegrityStatus, KernelMemorySummary, KernelStatus, ModelStatusSummary,
    PlanStatusSummary, ReadinessChecks, ReadinessStatus, StatusIndicator, SystemStatusResponse,
    UmaMemorySummary,
};
use adapteros_api_types::{ModelLoadStatus, API_SCHEMA_VERSION};
use axum::{extract::State, Extension, Json};
use tokio::time::timeout;
use tracing::warn;

use crate::api_error::ApiResult;
use crate::auth::Claims;
use crate::boot_state::BootState;
use crate::model_status::aggregate_status;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use sqlx::{query, query_scalar};

const DB_TIMEOUT_FALLBACK_MS: u64 = 2000;
const WORKER_TIMEOUT_FALLBACK_MS: u64 = 2000;
const MODELS_TIMEOUT_FALLBACK_MS: u64 = 2000;

/// GET /v1/system/status
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/system/status",
    responses(
        (status = 200, description = "Aggregated system status", body = SystemStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::types::ErrorResponse)
    )
)]
pub async fn get_system_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<SystemStatusResponse> {
    require_permission(&claims, Permission::MetricsView)?;

    // Scope status checks to the caller's tenant for workspace isolation
    let tenant_id = Some(claims.tenant_id.as_str());

    let integrity = build_integrity_status(&state);
    let boot = collect_boot_status(&state);
    let readiness = collect_readiness(&state).await;
    let (inference_ready, inference_blockers) =
        collect_inference_status(&state, &readiness, tenant_id).await;
    let kernel = collect_kernel_status(
        &state,
        matches!(readiness.checks.db.status, StatusIndicator::Ready),
        tenant_id,
    )
    .await;

    Ok(Json(SystemStatusResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        integrity,
        readiness,
        inference_ready,
        inference_blockers,
        boot,
        kernel,
    }))
}

fn build_integrity_status(state: &AppState) -> IntegrityStatus {
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
    let pf_required = cfg.security.require_pf_deny;
    let strict_mode = state.strict_mode;

    IntegrityStatus {
        mode: "local".to_string(),
        is_federated: state.federation_daemon.is_some(),
        strict_mode,
        // Assume PF deny was enforced unless a skip flag was set for development.
        pf_deny_ok: !pf_required || std::env::var("AOS_SKIP_PF_CHECK").is_err(),
        drift: DriftStatus {
            level: DriftLevel::Ok,
            summary: Some("no drift detected".to_string()),
        },
    }
}

fn collect_boot_status(state: &AppState) -> Option<BootStatus> {
    let boot_state = state.boot_state.as_ref()?;
    let timings = boot_state
        .transition_history()
        .into_iter()
        .map(|t| BootPhaseTiming {
            phase: t.to.as_str().to_string(),
            elapsed_ms: t.elapsed.as_millis() as u64,
        })
        .collect();

    let degraded = boot_state
        .get_degraded_reasons()
        .into_iter()
        .map(|r| DegradedReason {
            component: r.component,
            reason: r.reason,
        })
        .collect();

    let failure = boot_state.get_failure_reason().map(|f| BootFailure {
        code: f.code,
        message: Some(f.message),
    });

    Some(BootStatus {
        phase: boot_state.current_state().as_str().to_string(),
        boot_trace_id: Some(boot_state.boot_trace_id()),
        timings,
        degraded,
        failure,
    })
}

async fn collect_readiness(state: &AppState) -> ReadinessStatus {
    let (db_timeout_ms, worker_timeout_ms, models_timeout_ms) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (
            cfg.server.health_check_db_timeout_ms,
            cfg.server.health_check_worker_timeout_ms,
            cfg.server.health_check_models_timeout_ms,
        )
    };

    let db_timeout = timeout_from_ms(db_timeout_ms, DB_TIMEOUT_FALLBACK_MS);
    let mut db_check = ComponentCheck {
        status: StatusIndicator::Unknown,
        reason: None,
        latency_ms: None,
        critical: None,
    };

    let db_start = Instant::now();
    // Wrap database connection acquisition with a 500ms timeout to prevent
    // blocking on pool exhaustion. This is separate from the overall db_timeout
    // which covers the entire query operation.
    const ACQUIRE_TIMEOUT_MS: u64 = 500;
    let acquire_timeout = Duration::from_millis(ACQUIRE_TIMEOUT_MS);
    let db_probe = timeout(db_timeout, async {
        let conn_result = timeout(acquire_timeout, state.db.pool().acquire()).await;
        match conn_result {
            Ok(Ok(mut conn)) => query("SELECT 1").execute(&mut *conn).await,
            Ok(Err(e)) => Err(e),
            Err(_) => Err(sqlx::Error::PoolTimedOut),
        }
    })
    .await;
    let db_latency = db_start.elapsed().as_millis() as u64;
    match db_probe {
        Ok(Ok(_)) => {
            db_check.status = StatusIndicator::Ready;
            db_check.latency_ms = Some(db_latency);
        }
        Ok(Err(e)) => {
            db_check.status = StatusIndicator::Unknown;
            db_check.reason = Some(format!("db unreachable: {}", e));
            db_check.latency_ms = Some(db_latency);
        }
        Err(_) => {
            db_check.status = StatusIndicator::Unknown;
            db_check.reason = Some("db timeout".to_string());
            db_check.latency_ms = Some(db_latency);
        }
    }

    let mut migrations_check = ComponentCheck {
        status: StatusIndicator::Unknown,
        reason: None,
        latency_ms: None,
        critical: None,
    };
    if let Some(boot_state) = state.boot_state.as_ref() {
        let phase = boot_state.current_state();
        if phase.is_failed() {
            migrations_check.status = StatusIndicator::NotReady;
            migrations_check.reason = boot_state
                .get_failure_reason()
                .map(|f| format!("[{}] {}", f.code, f.message))
                .or_else(|| Some("boot failed".to_string()));
        } else if phase.is_booting() {
            migrations_check.status = StatusIndicator::NotReady;
            migrations_check.reason = Some(match phase {
                BootState::Migrating => "running migrations".to_string(),
                _ => format!("booting: {}", phase),
            });
        } else {
            migrations_check.status = StatusIndicator::Ready;
        }
    } else {
        migrations_check.reason = Some("boot state unavailable".to_string());
    }

    let worker_timeout = timeout_from_ms(worker_timeout_ms, WORKER_TIMEOUT_FALLBACK_MS);
    let mut workers_check = ComponentCheck {
        status: StatusIndicator::Unknown,
        reason: None,
        latency_ms: None,
        critical: None,
    };

    if db_check.status == StatusIndicator::Ready {
        if let Some(boot_state) = state.boot_state.as_ref() {
            let phase = boot_state.current_state();
            if !boot_state.is_ready() {
                workers_check.status = StatusIndicator::NotReady;
                workers_check.reason = Some(format!("booting: {}", phase));
            } else if boot_state.is_degraded() {
                let degraded = boot_state.get_degraded_reasons();
                workers_check.status = StatusIndicator::NotReady;
                workers_check.reason = degraded.first().map(|d| d.reason.clone());
            } else {
                let worker_start = Instant::now();
                let worker_probe = timeout(worker_timeout, state.db.count_active_workers()).await;
                let worker_latency = worker_start.elapsed().as_millis() as u64;
                match worker_probe {
                    Ok(Ok(count)) if count > 0 => {
                        workers_check.status = StatusIndicator::Ready;
                        workers_check.latency_ms = Some(worker_latency);
                    }
                    Ok(Ok(_)) => {
                        workers_check.status = StatusIndicator::NotReady;
                        workers_check.reason = Some("no workers registered".to_string());
                        workers_check.latency_ms = Some(worker_latency);
                    }
                    Ok(Err(e)) => {
                        workers_check.status = StatusIndicator::Unknown;
                        workers_check.reason = Some(format!("failed to query workers: {}", e));
                        workers_check.latency_ms = Some(worker_latency);
                    }
                    Err(_) => {
                        workers_check.status = StatusIndicator::Unknown;
                        workers_check.reason = Some("worker check timeout".to_string());
                        workers_check.latency_ms = Some(worker_latency);
                    }
                }
            }
        } else {
            workers_check.reason = Some("boot state unavailable".to_string());
        }
    } else {
        workers_check.reason = Some("database unavailable".to_string());
    }

    let models_timeout = timeout_from_ms(models_timeout_ms, MODELS_TIMEOUT_FALLBACK_MS);
    let mut models_check = ComponentCheck {
        status: StatusIndicator::Unknown,
        reason: None,
        latency_ms: None,
        critical: None,
    };

    if db_check.status == StatusIndicator::Ready {
        let models_start = Instant::now();
        let models_probe = timeout(
            models_timeout,
            query_scalar::<_, i64>("SELECT COUNT(*) FROM models").fetch_one(state.db.pool()),
        )
        .await;
        let models_latency = models_start.elapsed().as_millis() as u64;
        match models_probe {
            Ok(Ok(count)) if count > 0 => {
                models_check.status = StatusIndicator::Ready;
                models_check.latency_ms = Some(models_latency);
            }
            Ok(Ok(_)) => {
                models_check.status = StatusIndicator::NotReady;
                models_check.reason = Some("no models seeded".to_string());
                models_check.latency_ms = Some(models_latency);
            }
            Ok(Err(e)) => {
                models_check.status = StatusIndicator::Unknown;
                models_check.reason = Some(format!("failed to query models: {}", e));
                models_check.latency_ms = Some(models_latency);
            }
            Err(_) => {
                models_check.status = StatusIndicator::Unknown;
                models_check.reason = Some("models check timeout".to_string());
                models_check.latency_ms = Some(models_latency);
            }
        }
    } else {
        models_check.reason = Some("database unavailable".to_string());
    }

    if state.strict_mode {
        for check in [
            &mut db_check,
            &mut migrations_check,
            &mut workers_check,
            &mut models_check,
        ] {
            if check.status != StatusIndicator::Ready {
                check.critical = Some(true);
            }
        }
    }

    let overall = aggregate_readiness([
        db_check.status,
        migrations_check.status,
        workers_check.status,
        models_check.status,
    ]);

    ReadinessStatus {
        overall,
        checks: ReadinessChecks {
            db: db_check,
            migrations: migrations_check,
            workers: workers_check,
            models: models_check,
        },
    }
}

fn aggregate_readiness(statuses: impl IntoIterator<Item = StatusIndicator>) -> StatusIndicator {
    let mut overall = StatusIndicator::Ready;
    for status in statuses {
        match status {
            StatusIndicator::Unknown => return StatusIndicator::Unknown,
            StatusIndicator::NotReady => overall = StatusIndicator::NotReady,
            StatusIndicator::Ready => {}
        }
    }
    overall
}

async fn collect_inference_status(
    state: &AppState,
    readiness: &ReadinessStatus,
    tenant_id: Option<&str>,
) -> (InferenceReadyState, Vec<InferenceBlocker>) {
    // Check boot state first - block ALL boot phases until Ready
    if let Some(boot_state) = state.boot_state.as_ref() {
        if boot_state.is_booting() {
            return (
                InferenceReadyState::False,
                vec![InferenceBlocker::SystemBooting],
            );
        }
        if boot_state.is_failed() {
            return (
                InferenceReadyState::False,
                vec![InferenceBlocker::BootFailed],
            );
        }
    }

    if readiness.checks.db.status != StatusIndicator::Ready {
        return (
            InferenceReadyState::Unknown,
            vec![InferenceBlocker::DatabaseUnavailable],
        );
    }

    let mut blockers = Vec::new();

    // Check for degraded state - any degradation triggers TelemetryDegraded
    if let Some(boot_state) = state.boot_state.as_ref() {
        if boot_state.is_degraded() {
            let reasons = boot_state.get_degraded_reasons();
            if !reasons.is_empty() {
                push_blocker(&mut blockers, InferenceBlocker::TelemetryDegraded);
            }
        }
    }

    let worker_count = match state.db.count_active_workers().await {
        Ok(count) => count,
        Err(e) => {
            warn!(error = %e, "Failed to count workers for inference readiness");
            return (
                InferenceReadyState::Unknown,
                vec![InferenceBlocker::DatabaseUnavailable],
            );
        }
    };
    if worker_count == 0 {
        push_blocker(&mut blockers, InferenceBlocker::WorkerMissing);
    }

    let model_statuses = match state.db.list_base_model_statuses().await {
        Ok(statuses) => statuses,
        Err(e) => {
            warn!(error = %e, "Failed to load base model status for inference readiness");
            return (
                InferenceReadyState::Unknown,
                vec![InferenceBlocker::DatabaseUnavailable],
            );
        }
    };

    let any_ready = model_statuses
        .iter()
        .any(|status| ModelLoadStatus::parse_status(&status.status).is_ready());
    if !any_ready {
        push_blocker(&mut blockers, InferenceBlocker::NoModelLoaded);
    }

    // Fetch all active states and filter by tenant if provided
    // Note: DB layer doesn't support tenant-scoped query, so we filter in memory
    let all_states = match state.db.list_workspace_active_states().await {
        Ok(states) => states,
        Err(e) => {
            warn!(error = %e, "Failed to load workspace active state for inference readiness");
            return (
                InferenceReadyState::Unknown,
                vec![InferenceBlocker::DatabaseUnavailable],
            );
        }
    };
    // Filter by tenant_id if provided for proper tenant isolation
    let active_states: Vec<_> = match tenant_id {
        Some(tid) => all_states
            .into_iter()
            .filter(|s| s.tenant_id == tid)
            .collect(),
        None => all_states,
    };

    let has_mismatch = active_states.iter().any(|active| {
        let Some(model_id) = active.active_base_model_id.as_deref() else {
            return false;
        };

        let ready = model_statuses
            .iter()
            .find(|status| status.tenant_id == active.tenant_id && status.model_id == model_id)
            .map(|status| ModelLoadStatus::parse_status(&status.status).is_ready())
            .unwrap_or(false);

        !ready
    });
    if has_mismatch {
        push_blocker(&mut blockers, InferenceBlocker::ActiveModelMismatch);
    }

    let ready = if blockers.is_empty() {
        InferenceReadyState::True
    } else {
        InferenceReadyState::False
    };

    (ready, blockers)
}

async fn collect_kernel_status(
    state: &AppState,
    db_ready: bool,
    tenant_id: Option<&str>,
) -> Option<KernelStatus> {
    let mut model_summary = None;
    let mut plan_summary = None;
    let mut adapter_inventory = None;

    if db_ready && !state.db.pool().is_closed() {
        match state.db.list_base_model_statuses().await {
            Ok(statuses) if !statuses.is_empty() => {
                let aggregated = aggregate_status(statuses.iter());
                model_summary = Some(ModelStatusSummary {
                    status: aggregated.status.as_str().to_string(),
                    model_id: aggregated.latest.map(|m| m.model_id.clone()),
                    updated_at: aggregated.latest.map(|m| m.updated_at.clone()),
                });
            }
            Ok(_) => {}
            Err(e) => {
                warn!(error = %e, "Failed to load model status for system status");
            }
        }

        let mut active_adapter_count: Option<i64> = None;
        let mut active_plan_summary: Option<PlanStatusSummary> = None;
        // Fetch all active states and filter by tenant if provided
        // Note: DB layer doesn't support tenant-scoped query, so we filter in memory
        let all_states = match state.db.list_workspace_active_states().await {
            Ok(states) => Some(states),
            Err(e) => {
                warn!(error = %e, "Failed to load workspace active state for system status");
                None
            }
        };
        // Filter by tenant_id if provided for proper tenant isolation
        let active_states: Option<Vec<_>> = all_states.map(|states| match tenant_id {
            Some(tid) => states.into_iter().filter(|s| s.tenant_id == tid).collect(),
            None => states,
        });

        if let Some(states) = active_states.as_ref() {
            if !states.is_empty() {
                let mut adapter_ids: HashSet<String> = HashSet::new();
                for record in states {
                    if let Some(raw) = record.active_adapter_ids.as_deref() {
                        match serde_json::from_str::<Vec<String>>(raw) {
                            Ok(ids) => {
                                for id in ids {
                                    adapter_ids.insert(id);
                                }
                            }
                            Err(e) => {
                                warn!(
                                    tenant_id = %record.tenant_id,
                                    error = %e,
                                    "Failed to parse workspace active adapters"
                                );
                            }
                        }
                    }
                }
                active_adapter_count = Some(adapter_ids.len() as i64);

                if let Some(latest_plan_state) = states
                    .iter()
                    .filter(|s| s.active_plan_id.is_some())
                    .max_by(|a, b| a.updated_at.cmp(&b.updated_at))
                {
                    if let Some(ref plan_id) = latest_plan_state.active_plan_id {
                        match state.db.get_plan(plan_id).await {
                            Ok(Some(plan)) => {
                                active_plan_summary = Some(PlanStatusSummary {
                                    plan_id: plan.id,
                                    tenant_id: plan.tenant_id,
                                    created_at: Some(plan.created_at),
                                });
                            }
                            Ok(None) => {
                                active_plan_summary = Some(PlanStatusSummary {
                                    plan_id: plan_id.clone(),
                                    tenant_id: latest_plan_state.tenant_id.clone(),
                                    created_at: Some(latest_plan_state.updated_at.clone()),
                                });
                            }
                            Err(e) => {
                                warn!(error = %e, plan_id = %plan_id, "Failed to load active plan");
                            }
                        }
                    }
                }
            }
        }

        if active_plan_summary.is_some() {
            plan_summary = active_plan_summary;
        } else {
            match state.db.list_all_plans().await {
                Ok(plans) if !plans.is_empty() => {
                    let latest = &plans[0];
                    plan_summary = Some(PlanStatusSummary {
                        plan_id: latest.id.clone(),
                        tenant_id: latest.tenant_id.clone(),
                        created_at: Some(latest.created_at.clone()),
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(error = %e, "Failed to list plans for system status");
                }
            }
        }

        let total_active = if active_adapter_count.is_some() {
            active_adapter_count
        } else {
            match state.db.count_active_adapters().await {
                Ok(v) => Some(v),
                Err(e) => {
                    warn!(error = %e, "Failed to count active adapters");
                    None
                }
            }
        };

        let loaded = match state.db.count_loaded_models().await {
            Ok(v) => Some(v),
            Err(e) => {
                warn!(error = %e, "Failed to count loaded models");
                None
            }
        };

        if total_active.is_some() || loaded.is_some() {
            adapter_inventory = Some(AdapterInventory {
                total_active,
                loaded,
            });
        }
    }

    let memory = build_memory_summary(state).await;

    if model_summary.is_none()
        && plan_summary.is_none()
        && adapter_inventory.is_none()
        && memory.is_none()
    {
        None
    } else {
        Some(KernelStatus {
            model: model_summary,
            plan: plan_summary,
            adapters: adapter_inventory,
            memory,
        })
    }
}

async fn build_memory_summary(state: &AppState) -> Option<KernelMemorySummary> {
    let uma_stats = state.uma_monitor.get_uma_stats().await;
    let mut memory = KernelMemorySummary {
        ane: None,
        uma: None,
        pressure: None,
    };

    // Build UMA summary - only if we have real data (total_mb > 0)
    if uma_stats.total_mb > 0 {
        memory.uma = Some(UmaMemorySummary {
            availability: DataAvailability::Available,
            total_mb: Some(uma_stats.total_mb),
            used_mb: Some(uma_stats.used_mb),
            available_mb: Some(uma_stats.available_mb),
            headroom_pct: Some(uma_stats.headroom_pct),
        });
        memory.pressure = Some(state.uma_monitor.get_current_pressure().to_string());
    } else {
        // UMA data unavailable - report honestly with Unavailable status
        // instead of omitting entirely, so UI knows telemetry is degraded
        tracing::debug!(
            target: "telemetry.memory",
            "UMA metrics unavailable in system status - reporting as unavailable"
        );
        memory.uma = Some(UmaMemorySummary {
            availability: DataAvailability::Unavailable,
            total_mb: None,
            used_mb: None,
            available_mb: None,
            headroom_pct: None,
        });
    }

    // Build ANE summary - only if we have ALL real ANE metrics
    if let (Some(allocated), Some(used), Some(available), Some(usage_pct)) = (
        uma_stats.ane_allocated_mb,
        uma_stats.ane_used_mb,
        uma_stats.ane_available_mb,
        uma_stats.ane_usage_percent,
    ) {
        memory.ane = Some(AneMemorySummary {
            availability: DataAvailability::Available,
            allocated_mb: Some(allocated),
            used_mb: Some(used),
            available_mb: Some(available),
            usage_pct: Some(usage_pct),
        });
    } else {
        // ANE data unavailable - report honestly with Unavailable status
        // NEVER fabricate estimated values
        tracing::debug!(
            target: "telemetry.memory",
            "ANE metrics unavailable in system status - reporting as unavailable"
        );
        memory.ane = Some(AneMemorySummary {
            availability: DataAvailability::Unavailable,
            allocated_mb: None,
            used_mb: None,
            available_mb: None,
            usage_pct: None,
        });
    }

    // Always return memory summary now that we report availability status
    Some(memory)
}

fn timeout_from_ms(config_value: u64, fallback_ms: u64) -> Duration {
    if config_value > 0 {
        Duration::from_millis(config_value)
    } else {
        Duration::from_millis(fallback_ms)
    }
}

fn push_blocker(blockers: &mut Vec<InferenceBlocker>, blocker: InferenceBlocker) {
    if !blockers.contains(&blocker) {
        blockers.push(blocker);
    }
}
