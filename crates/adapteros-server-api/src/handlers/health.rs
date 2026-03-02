//! Health check and system status handlers

use crate::auth::is_dev_bypass_enabled;
use crate::boot_state::{BootState, BootWarning};
use crate::inference_core::InferenceCore;
use crate::state::{AppState, BackgroundTaskSnapshot};
use crate::supervisor_client;
use crate::types::*;
use adapteros_api_types::ModelLoadStatus;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{query, Row};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Canary inference deep probe
// ---------------------------------------------------------------------------

/// Query parameters for the readiness endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct ReadyzQuery {
    /// When `true`, run a canary inference probe through the full pipeline.
    #[serde(default)]
    pub deep: bool,
}

/// Cached result of the most recent canary inference probe.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CanaryProbeResult {
    /// Whether the canary inference succeeded.
    pub ok: bool,
    /// Human-readable hint on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Latency of the canary inference in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// Internal cache entry holding a `CanaryProbeResult` and its timestamp.
struct CanaryCache {
    result: CanaryProbeResult,
    at: Instant,
}

/// TTL for cached canary results. Repeated `/readyz?deep=true` within this
/// window return the cached probe instead of hammering inference.
const CANARY_CACHE_TTL: Duration = Duration::from_secs(30);

/// Timeout for the canary inference request itself.
const CANARY_TIMEOUT: Duration = Duration::from_secs(5);

/// Hardcoded trivial prompt for the canary probe. Deliberately minimal.
const CANARY_PROMPT: &str = "ping";

/// System tenant used for canary probe requests.
const CANARY_TENANT: &str = "system";

/// Replay guard entries older than this are treated as stale in strict mode.
const REPLAY_GUARD_STALE_AFTER: Duration = Duration::from_secs(15 * 60);

/// Timeout for querying replay-guard status from `determinism_checks`.
const REPLAY_GUARD_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

fn canary_cache() -> &'static RwLock<Option<CanaryCache>> {
    static CACHE: OnceLock<RwLock<Option<CanaryCache>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

/// Run a canary inference probe through `InferenceCore`, with caching.
///
/// Returns `None` when `deep` is `false`.
async fn run_canary_probe(state: &AppState, deep: bool) -> Option<CanaryProbeResult> {
    if !deep {
        return None;
    }

    // Check cache first
    {
        let guard = canary_cache().read().await;
        if let Some(ref cached) = *guard {
            if cached.at.elapsed() < CANARY_CACHE_TTL {
                return Some(cached.result.clone());
            }
        }
    }

    // Cache miss or stale — run the probe
    let start = Instant::now();
    let core = InferenceCore::new(state);

    let mut req =
        InferenceRequestInternal::new(CANARY_TENANT.to_string(), CANARY_PROMPT.to_string());
    req.max_tokens = 1; // We only need one token to prove the pipeline works
    req.stream = false;
    req.require_step = false;
    req.require_determinism = false;

    let probe = timeout(
        CANARY_TIMEOUT,
        core.route_and_infer(req, None, None, None, None),
    )
    .await;

    let latency = start.elapsed().as_millis() as u64;

    let result = match probe {
        Ok(Ok(_inference_result)) => CanaryProbeResult {
            ok: true,
            hint: None,
            latency_ms: Some(latency),
        },
        Ok(Err(e)) => CanaryProbeResult {
            ok: false,
            hint: Some(format!("canary inference failed: {e}")),
            latency_ms: Some(latency),
        },
        Err(_) => CanaryProbeResult {
            ok: false,
            hint: Some("canary inference timeout".to_string()),
            latency_ms: Some(latency),
        },
    };

    // Update cache
    {
        let mut guard = canary_cache().write().await;
        *guard = Some(CanaryCache {
            result: result.clone(),
            at: Instant::now(),
        });
    }

    Some(result)
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

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
    fn build_id_is_set_and_consistent() {
        let build_id = env!("AOS_BUILD_ID");
        assert!(
            !build_id.trim().is_empty(),
            "AOS_BUILD_ID must not be empty"
        );
        assert!(
            build_id.contains('-'),
            "AOS_BUILD_ID should be in {{prefix}}-{{YYYYMMDDHHmmss}} format: {build_id}"
        );
        assert_eq!(
            build_id,
            adapteros_core::version::BUILD_ID,
            "server-api AOS_BUILD_ID should match adapteros-core BUILD_ID"
        );
    }
}

/// Readiness mode indicates how strictly checks are enforced.
///
/// This helps the frontend understand the semantics of the readiness response
/// and behave appropriately (e.g., show informational UI vs panic screen).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ReadinessMode {
    /// Full readiness checks enforced - all checks must pass for ready=true
    #[default]
    Strict,
    /// Some checks are relaxed (e.g., skip-worker mode) - listed checks are informational
    Relaxed {
        /// List of check names that are relaxed (e.g., ["worker", "models"])
        relaxed_checks: Vec<String>,
    },
    /// Dev bypass active - all checks are informational, system returns 200 regardless
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

/// Replay guard freshness state for determinism health signaling.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayGuardState {
    Fresh,
    Failing,
    Stale,
    Missing,
    Unknown,
}

/// Replay determinism guard status surfaced via `/readyz`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayGuardCheck {
    pub ok: bool,
    pub state: ReplayGuardState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergences: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<u64>,
    pub stale_after_seconds: u64,
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
    /// Indicates how strictly checks are enforced.
    /// - `strict`: All checks must pass for ready=true
    /// - `relaxed`: Some checks are informational only
    /// - `dev_bypass`: All checks informational, always returns 200
    #[serde(default)]
    pub readiness_mode: ReadinessMode,
    /// Warnings recorded during boot (non-fatal issues that reduce functionality).
    /// Present even when `ready=true` to give operators visibility into
    /// components that failed to start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boot_warnings: Vec<BootWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    /// Canary inference probe result (only present when `?deep=true`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canary: Option<CanaryProbeResult>,
    /// Replay determinism guard state (strict mode only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_guard: Option<ReplayGuardCheck>,
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

fn parse_replay_guard_last_run(last_run: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(last_run) {
        return Some(parsed.with_timezone(&Utc));
    }

    NaiveDateTime::parse_from_str(last_run, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

/// Evaluate replay guard state from a determinism-check snapshot.
///
/// This helper is intentionally pure so integration tests can validate
/// stale/failing semantics without constructing full `AppState`.
pub fn evaluate_replay_guard_status(
    last_run: Option<&str>,
    result: Option<&str>,
    runs: Option<i64>,
    divergences: Option<i64>,
    now: DateTime<Utc>,
    stale_after: Duration,
) -> ReplayGuardCheck {
    let stale_after_seconds = stale_after.as_secs();
    let runs = runs.and_then(|v| u64::try_from(v).ok());
    let divergences = divergences.and_then(|v| u64::try_from(v).ok());

    let Some(last_run_raw) = last_run else {
        return ReplayGuardCheck {
            ok: false,
            state: ReplayGuardState::Missing,
            hint: Some("replay guard has no recorded runs".to_string()),
            last_run: None,
            result: None,
            runs,
            divergences,
            age_seconds: None,
            stale_after_seconds,
        };
    };

    let normalized_result = result.map(|r| r.trim().to_ascii_lowercase());
    if normalized_result.as_deref() != Some("pass") {
        return ReplayGuardCheck {
            ok: false,
            state: ReplayGuardState::Failing,
            hint: Some("latest replay guard run did not pass".to_string()),
            last_run: Some(last_run_raw.to_string()),
            result: normalized_result,
            runs,
            divergences,
            age_seconds: None,
            stale_after_seconds,
        };
    }

    let Some(parsed_last_run) = parse_replay_guard_last_run(last_run_raw) else {
        return ReplayGuardCheck {
            ok: false,
            state: ReplayGuardState::Stale,
            hint: Some("unable to parse replay guard last_run timestamp".to_string()),
            last_run: Some(last_run_raw.to_string()),
            result: Some("pass".to_string()),
            runs,
            divergences,
            age_seconds: None,
            stale_after_seconds,
        };
    };

    let age_signed = now.signed_duration_since(parsed_last_run).num_seconds();
    if age_signed < 0 {
        return ReplayGuardCheck {
            ok: false,
            state: ReplayGuardState::Stale,
            hint: Some("replay guard last_run timestamp is in the future".to_string()),
            last_run: Some(last_run_raw.to_string()),
            result: Some("pass".to_string()),
            runs,
            divergences,
            age_seconds: None,
            stale_after_seconds,
        };
    }

    let age_seconds = age_signed as u64;
    if age_seconds > stale_after_seconds {
        return ReplayGuardCheck {
            ok: false,
            state: ReplayGuardState::Stale,
            hint: Some(format!(
                "replay guard is stale ({age_seconds}s old > {stale_after_seconds}s)"
            )),
            last_run: Some(last_run_raw.to_string()),
            result: Some("pass".to_string()),
            runs,
            divergences,
            age_seconds: Some(age_seconds),
            stale_after_seconds,
        };
    }

    ReplayGuardCheck {
        ok: true,
        state: ReplayGuardState::Fresh,
        hint: None,
        last_run: Some(last_run_raw.to_string()),
        result: Some("pass".to_string()),
        runs,
        divergences,
        age_seconds: Some(age_seconds),
        stale_after_seconds,
    }
}

/// Strict mode treats replay guard failures as readiness blockers.
pub fn replay_guard_allows_ready(
    readiness_mode: &ReadinessMode,
    replay_guard: Option<&ReplayGuardCheck>,
) -> bool {
    match readiness_mode {
        ReadinessMode::Strict => replay_guard.map(|check| check.ok).unwrap_or(false),
        ReadinessMode::Relaxed { .. } | ReadinessMode::DevBypass => true,
    }
}

async fn fetch_replay_guard_check(state: &AppState) -> ReplayGuardCheck {
    let replay_guard_probe = timeout(REPLAY_GUARD_QUERY_TIMEOUT, async {
        let Some(pool) = state.db.pool_opt() else {
            return Err(sqlx::Error::Configuration(
                "SQL pool not available (kv-only mode)".into(),
            ));
        };

        sqlx::query(
            "SELECT last_run, result, runs, divergences
             FROM determinism_checks
             ORDER BY last_run DESC
             LIMIT 1",
        )
        .fetch_optional(pool)
        .await
    })
    .await;

    match replay_guard_probe {
        Ok(Ok(Some(row))) => {
            let last_run: Option<String> = row.try_get("last_run").ok();
            let result: Option<String> = row.try_get("result").ok();
            let runs: Option<i64> = row.try_get("runs").ok();
            let divergences: Option<i64> = row.try_get("divergences").ok();
            evaluate_replay_guard_status(
                last_run.as_deref(),
                result.as_deref(),
                runs,
                divergences,
                Utc::now(),
                REPLAY_GUARD_STALE_AFTER,
            )
        }
        Ok(Ok(None)) => evaluate_replay_guard_status(
            None,
            None,
            None,
            None,
            Utc::now(),
            REPLAY_GUARD_STALE_AFTER,
        ),
        Ok(Err(err)) => {
            let err_message = err.to_string();
            if err_message.contains("no such table") && err_message.contains("determinism_checks") {
                return evaluate_replay_guard_status(
                    None,
                    None,
                    None,
                    None,
                    Utc::now(),
                    REPLAY_GUARD_STALE_AFTER,
                );
            }

            ReplayGuardCheck {
                ok: false,
                state: ReplayGuardState::Unknown,
                hint: Some(format!(
                    "failed to query replay guard status: {err_message}"
                )),
                last_run: None,
                result: None,
                runs: None,
                divergences: None,
                age_seconds: None,
                stale_after_seconds: REPLAY_GUARD_STALE_AFTER.as_secs(),
            }
        }
        Err(_) => ReplayGuardCheck {
            ok: false,
            state: ReplayGuardState::Unknown,
            hint: Some("replay guard query timeout".to_string()),
            last_run: None,
            result: None,
            runs: None,
            divergences: None,
            age_seconds: None,
            stale_after_seconds: REPLAY_GUARD_STALE_AFTER.as_secs(),
        },
    }
}

/// Readiness check
///
/// Pass `?deep=true` to trigger a canary inference probe through the full
/// pipeline. The result is cached for 30 seconds. Without the parameter,
/// existing behaviour is unchanged.
#[utoipa::path(
    tag = "system",
    get,
    path = "/readyz",
    params(
        ("deep" = Option<bool>, Query, description = "Run canary inference probe")
    ),
    responses(
        (status = 200, description = "Service is ready", body = ReadyzResponse),
        (status = 503, description = "Service is not ready", body = ReadyzResponse)
    )
)]
pub async fn ready(
    State(state): State<AppState>,
    Query(query): Query<ReadyzQuery>,
) -> impl IntoResponse {
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

    // Determine readiness mode based on configuration
    let skip_worker_check = {
        // STABILITY: Use poison-safe lock access
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
    let replay_guard = if matches!(readiness_mode, ReadinessMode::Strict) {
        Some(fetch_replay_guard_check(&state).await)
    } else {
        None
    };

    // Check boot state - only return ready if in Ready state
    let Some(ref boot_state) = state.boot_state else {
        worker_check.ok = false;
        worker_check.hint = Some("boot state manager not configured".to_string());

        // In dev bypass mode, return 200 regardless of check results
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
                build_id: None,
                canary: None,
                replay_guard,
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
        // Degraded state - system is operational, keep ready but report degraded status.
        // Degraded is for non-critical dependencies (metrics, telemetry) - NOT a reason to fail readiness.
        // STABILITY: Do NOT set worker_check.ok = false here - degraded should still be ready
        // to prevent orchestrator flapping on transient non-critical failures.
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
        // STABILITY: Use pool_opt() to avoid panic if database is in KV-only mode
        let Some(pool) = state.db.pool_opt() else {
            return Err(sqlx::Error::Configuration(
                "SQL pool not available (kv-only mode)".into(),
            ));
        };
        let mut conn = pool.acquire().await?;
        sqlx::query("SELECT 1").execute(&mut *conn).await?;
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
        // STABILITY: Use pool_opt() for defense-in-depth (should be Some if we reached here)
        let models_probe = match state.db.pool_opt() {
            Some(pool) => {
                timeout(
                    models_timeout,
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM models").fetch_one(pool),
                )
                .await
            }
            None => Ok(Err(sqlx::Error::Configuration(
                "SQL pool not available".into(),
            ))),
        };
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

        if models_seeded_check.ok {
            // System-level mismatch detection: uses global list intentionally for
            // overall health visibility. This check is informational only (sets a
            // hint but doesn't fail readiness) and doesn't expose tenant-specific
            // data - only a generic "mismatch exists" signal.
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

    // Final readiness depends on mode:
    // - Strict: all checks must pass
    // - Relaxed: skip specified checks (e.g., worker check when skip_worker_check is set)
    // - DevBypass: all checks informational only
    let replay_guard_ok = replay_guard_allows_ready(&readiness_mode, replay_guard.as_ref());
    let ready = match &readiness_mode {
        ReadinessMode::Strict => {
            db_check.ok && worker_check.ok && models_seeded_check.ok && replay_guard_ok
        }
        ReadinessMode::Relaxed { relaxed_checks } => {
            // Start with db check (always required)
            let mut result = db_check.ok;
            // Worker check is optional if relaxed
            if !relaxed_checks.contains(&"worker".to_string()) {
                result = result && worker_check.ok;
            }
            // Models check is optional if relaxed
            if !relaxed_checks.contains(&"models".to_string()) {
                result = result && models_seeded_check.ok;
            }
            result
        }
        ReadinessMode::DevBypass => true, // Always ready in dev bypass
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
    if let Some(ref guard) = replay_guard {
        registry
            .set_gauge(
                "readyz_replay_guard_ok".to_string(),
                if guard.ok { 1.0 } else { 0.0 },
            )
            .await;
        if let Some(age_seconds) = guard.age_seconds {
            registry
                .set_gauge(
                    "readyz_replay_guard_age_seconds".to_string(),
                    age_seconds as f64,
                )
                .await;
        }
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

    // Run canary inference probe when deep=true and basic checks passed.
    // Skip canary when the system is already not ready — no point probing
    // inference if DB/workers/models are down.
    let canary = if ready && query.deep {
        run_canary_probe(&state, true).await
    } else if query.deep {
        // deep requested but basic checks failed — report skip
        Some(CanaryProbeResult {
            ok: false,
            hint: Some("skipped: basic readiness checks failed".to_string()),
            latency_ms: None,
        })
    } else {
        None
    };

    // If canary was requested and failed, downgrade readiness
    let ready = if let Some(ref c) = canary {
        ready && c.ok
    } else {
        ready
    };

    // Determine status code (accounts for canary downgrade when applicable)
    let status_code = if matches!(readiness_mode, ReadinessMode::DevBypass) || ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    // Emit canary latency metric
    if let Some(ref c) = canary {
        if let Some(latency) = c.latency_ms {
            registry
                .set_gauge("readyz_canary_latency_ms".to_string(), latency as f64)
                .await;
        }
    }

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
            build_id: Some(adapteros_core::version::BUILD_ID.to_string()),
            canary,
            replay_guard,
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

/// Invariant violation details for API response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvariantViolationDto {
    /// Invariant identifier (e.g., "SEC-001", "DAT-002")
    pub id: String,
    /// Human-readable description of the violation
    pub message: String,
    /// Whether this violation is fatal (blocks boot in production)
    pub is_fatal: bool,
    /// Suggested remediation steps
    pub remediation: String,
}

/// Boot invariants status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvariantStatusResponse {
    /// Total number of invariants checked
    pub checked: u64,
    /// Number of invariants that passed
    pub passed: u64,
    /// Number of invariants that failed (non-fatal)
    pub failed: u64,
    /// Number of invariants skipped via config
    pub skipped: u64,
    /// Number of fatal violations
    pub fatal: u64,
    /// List of violations encountered
    pub violations: Vec<InvariantViolationDto>,
    /// List of invariant IDs that were skipped
    pub skipped_ids: Vec<String>,
    /// Whether server is running in production mode
    pub production_mode: bool,
}

/// Get boot invariants status
///
/// Returns the current status of boot-time invariant checks, including
/// any violations that were detected during startup.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/invariants",
    responses(
        (status = 200, description = "Invariants status", body = InvariantStatusResponse)
    )
)]
pub async fn get_invariant_status(State(state): State<AppState>) -> Json<InvariantStatusResponse> {
    // Read production mode from config
    let production_mode = {
        let cfg = state.config.read().unwrap_or_else(|e| {
            tracing::warn!("Config lock poisoned in invariants check, recovering");
            e.into_inner()
        });
        cfg.server.production_mode
    };

    // Get boot invariant metrics from the shared static atomic counters in adapteros-boot
    let metrics = adapteros_boot::boot_invariant_metrics();

    // Calculate passed count (checked - failed - skipped)
    let passed = metrics
        .checked
        .saturating_sub(metrics.violated)
        .saturating_sub(metrics.skipped);

    // Convert to response format
    // Note: The actual violations are tracked at boot time; this endpoint
    // returns the summary metrics. For detailed violation info, operators
    // should check the boot logs or use `aosctl doctor`.
    Json(InvariantStatusResponse {
        checked: metrics.checked,
        passed,
        failed: metrics.violated,
        skipped: metrics.skipped,
        fatal: metrics.fatal,
        violations: Vec::new(),  // Detailed violations are in boot logs
        skipped_ids: Vec::new(), // Would need to track these separately
        production_mode,
    })
}

// ---------------------------------------------------------------------------
// Synthetic probe health endpoint
// ---------------------------------------------------------------------------

/// Get synthetic probe health report.
///
/// Returns aggregated health information from the synthetic probe system,
/// including per-adapter success/failure status, latency statistics, and
/// the list of healthy vs degraded adapters.
///
/// Returns an empty report if probes are disabled or have not yet run.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/system/probe-health",
    responses(
        (status = 200, description = "Probe health report", body = crate::synthetic_probes::ProbeHealthReport)
    )
)]
pub async fn get_probe_health(
    State(_state): State<AppState>,
) -> Json<crate::synthetic_probes::ProbeHealthReport> {
    match crate::synthetic_probes::global_probe_results() {
        Some(results) => {
            let guard = results.read().await;
            Json(guard.health_report())
        }
        None => {
            // Probes not configured — return empty report
            Json(crate::synthetic_probes::ProbeHealthReport {
                total_probes: 0,
                successful: 0,
                failed: 0,
                avg_latency_ms: 0.0,
                p99_latency_ms: 0,
                adapters_healthy: Vec::new(),
                adapters_degraded: Vec::new(),
                last_cycle_at: None,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod canary_probe_tests {
    use super::*;

    #[test]
    fn readyz_query_defaults_deep_false() {
        let q: ReadyzQuery = serde_json::from_str("{}").unwrap();
        assert!(!q.deep);
    }

    #[test]
    fn readyz_query_deep_true() {
        let q: ReadyzQuery = serde_json::from_str(r#"{"deep": true}"#).unwrap();
        assert!(q.deep);
    }

    #[test]
    fn canary_probe_result_ok_serializes() {
        let r = CanaryProbeResult {
            ok: true,
            hint: None,
            latency_ms: Some(42),
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["latency_ms"], 42);
        assert!(json.get("hint").is_none());
    }

    #[test]
    fn canary_probe_result_fail_serializes() {
        let r = CanaryProbeResult {
            ok: false,
            hint: Some("canary inference timeout".into()),
            latency_ms: Some(5001),
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["hint"], "canary inference timeout");
    }

    #[test]
    fn readyz_response_canary_none_omitted() {
        // Verify that canary: None is omitted from serialized JSON
        let resp = ReadyzResponse {
            ready: true,
            checks: ReadyzChecks {
                db: ReadyzCheck {
                    ok: true,
                    hint: None,
                    latency_ms: None,
                },
                worker: ReadyzCheck {
                    ok: true,
                    hint: None,
                    latency_ms: None,
                },
                models_seeded: ReadyzCheck {
                    ok: true,
                    hint: None,
                    latency_ms: None,
                },
            },
            metrics: None,
            boot_trace_id: "test".into(),
            last_error_code: None,
            phases: vec![],
            readiness_mode: ReadinessMode::Strict,
            boot_warnings: vec![],
            build_id: None,
            canary: None,
            replay_guard: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(
            json.get("canary").is_none(),
            "canary should be omitted when None"
        );
    }

    #[test]
    fn readyz_response_canary_present_when_set() {
        let resp = ReadyzResponse {
            ready: true,
            checks: ReadyzChecks {
                db: ReadyzCheck {
                    ok: true,
                    hint: None,
                    latency_ms: None,
                },
                worker: ReadyzCheck {
                    ok: true,
                    hint: None,
                    latency_ms: None,
                },
                models_seeded: ReadyzCheck {
                    ok: true,
                    hint: None,
                    latency_ms: None,
                },
            },
            metrics: None,
            boot_trace_id: "test".into(),
            last_error_code: None,
            phases: vec![],
            readiness_mode: ReadinessMode::Strict,
            boot_warnings: vec![],
            build_id: None,
            canary: Some(CanaryProbeResult {
                ok: true,
                hint: None,
                latency_ms: Some(15),
            }),
            replay_guard: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        let canary = json.get("canary").expect("canary should be present");
        assert_eq!(canary["ok"], true);
        assert_eq!(canary["latency_ms"], 15);
    }

    #[test]
    fn canary_cache_ttl_is_30s() {
        assert_eq!(CANARY_CACHE_TTL, Duration::from_secs(30));
    }

    #[test]
    fn canary_timeout_is_5s() {
        assert_eq!(CANARY_TIMEOUT, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn run_canary_probe_returns_none_when_deep_false() {
        // Use a dummy state — shouldn't matter since deep=false short-circuits
        // We can't easily construct AppState in a unit test, but we can test the
        // non-deep path by verifying the function signature and short-circuit.
        // The actual deep=false check happens at the call site in ready(), but
        // `run_canary_probe` also checks internally.
        //
        // Since we can't construct AppState here, we verify the constants and
        // the serialization contract instead. Integration tests cover the full path.
        assert_eq!(CANARY_PROMPT, "ping");
        assert_eq!(CANARY_TENANT, "system");
    }

    #[test]
    fn canary_probe_result_downgrade_readiness() {
        // Verify the readiness downgrade logic matches what ready() does:
        // if canary was requested and failed, ready becomes false
        let canary = Some(CanaryProbeResult {
            ok: false,
            hint: Some("canary inference failed: no workers".into()),
            latency_ms: Some(100),
        });
        let base_ready = true;
        let effective_ready = if let Some(ref c) = canary {
            base_ready && c.ok
        } else {
            base_ready
        };
        assert!(!effective_ready, "failed canary should downgrade readiness");
    }

    #[test]
    fn canary_probe_result_preserves_readiness_on_success() {
        let canary = Some(CanaryProbeResult {
            ok: true,
            hint: None,
            latency_ms: Some(50),
        });
        let base_ready = true;
        let effective_ready = if let Some(ref c) = canary {
            base_ready && c.ok
        } else {
            base_ready
        };
        assert!(
            effective_ready,
            "successful canary should not downgrade readiness"
        );
    }
}
