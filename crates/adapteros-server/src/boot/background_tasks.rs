//! Background task spawning for adapterOS control plane.
//!
//! This module contains the background task spawning logic for the boot sequence.
//! It spawns background tasks that run throughout the server lifecycle.
//!
//! ## Orphaned Training Job Cleanup (ANCHOR, AUDIT, RECTIFY)
//!
//! Periodic cleanup of training jobs that have been running for an extended period
//! without progress, indicating they are orphaned or stuck:
//!
//! - **ANCHOR**: Jobs running >24h without metrics are considered orphaned (configurable via `AOS_ORPHANED_JOB_THRESHOLD_HOURS`)
//! - **AUDIT**: Logs `ORPHANED_TRAINING_JOB_CLEANED` counter and emits warning for each cleaned job
//! - **RECTIFY**: Marks orphaned jobs as "failed" with reason "stale_no_progress_24h" for post-mortem analysis
//!
//! ## Dev Mode Optimization
//!
//! When dev bypass is enabled (`AOS_DEV_NO_AUTH=1` or `security.dev_bypass=true`),
//! only essential tasks are spawned for faster startup:
//!
//! - Status writer (UI needs it)
//! - WAL checkpoint (database health)
//! - TTL cleanup (prevents DB bloat)
//! - Log cleanup (prevents disk bloat, reduced frequency in dev)
//!
//! ## Production Mode Tasks (18 tasks)
//!
//! 1. Status writer task (5s interval)
//! 2. KV metrics alert monitor task (5s interval)
//! 3. Log cleanup task (24h interval if configured)
//! 4. TTL/expiration cleanup task (5m interval with circuit breaker)
//! 5. WAL checkpoint task (5m interval)
//! 6. Upload session cleanup task (1h interval)
//! 7. Security cleanup task (1h interval)
//! 8. Egress re-verification monitor (60s interval, configurable)
//! 9. Telemetry bundle GC task (6h interval)
//! 10. Orphaned training job cleanup task (1h interval)
//! 11. Stale worker reaper task (60s interval)
//! 12. Terminal worker purge task (1h interval)
//! 13. Rate limiter eviction task (60s interval)
//! 14. Inference cache cleanup task (5m interval)
//! 15. Idempotency store cleanup task (5m interval)
//! 16. Inference state tracker cleanup task (5m interval)
//! 17. Telemetry rate limiter cleanup task (60s interval)
//! 18. Synthetic probe runner (configurable interval, disabled by default)
//!
//! Each task uses the `BackgroundTaskSpawner` to integrate with the shutdown coordinator
//! and task tracking system.

use crate::boot::run_startup_inference_warmup;
use crate::boot::BackgroundTaskSpawner;
use crate::logging;
use crate::shutdown::ShutdownCoordinator;
use crate::status_writer;
use adapteros_db::diagnostics::SqliteDiagPersister;
use adapteros_db::kv_metrics;
use adapteros_db::{Db, ProtectedDb};
use adapteros_deterministic_exec::run_global_executor;
use adapteros_server_api::boot_state::{BootStateManager, FailureReason};
use adapteros_server_api::local_log_service::{run_local_log_service, LocalLogServiceConfig};
use adapteros_server_api::security::{
    cleanup_expired_ip_rules, cleanup_expired_revocations, cleanup_expired_sessions,
};
use adapteros_server_api::state::BackgroundTaskTracker;
use adapteros_server_api::telemetry::MetricsRegistry;
use adapteros_server_api::AppState;
use adapteros_telemetry::diagnostics::{DiagEnvelope, DiagnosticsWriter, RunTracker, WriterConfig};
use adapteros_telemetry::AlertingEngine;
use anyhow::Result;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::MissedTickBehavior;
use tracing::{debug, error, info, instrument, warn};

/// Counter for orphaned training jobs that have been cleaned up.
static ORPHANED_TRAINING_JOB_CLEANED: AtomicU64 = AtomicU64::new(0);

/// Returns the count of orphaned training jobs that have been marked as failed.
pub fn orphaned_training_job_cleaned_count() -> u64 {
    ORPHANED_TRAINING_JOB_CLEANED.load(Ordering::Relaxed)
}

#[derive(Clone)]
struct TrainingWorkerEnv {
    socket_path: std::path::PathBuf,
    database_url: String,
    datasets_root: String,
    artifacts_root: String,
}

/// Sliding-window circuit breaker for training worker crash detection.
///
/// Tracks crash timestamps and trips when `max_crashes` occur within `window`.
/// Once tripped, the supervisor stops restart attempts and writes a degraded marker.
struct WorkerCircuitBreaker {
    crash_timestamps: std::collections::VecDeque<tokio::time::Instant>,
    max_crashes: u32,
    window: Duration,
    tripped: bool,
}

impl WorkerCircuitBreaker {
    fn new(max_crashes: u32, window: Duration) -> Self {
        Self {
            crash_timestamps: std::collections::VecDeque::new(),
            max_crashes,
            window,
            tripped: false,
        }
    }

    /// Record a crash and return whether the circuit breaker has tripped.
    fn record_crash(&mut self) -> bool {
        let now = tokio::time::Instant::now();
        self.crash_timestamps.push_back(now);
        // Evict crashes outside the sliding window
        while let Some(&front) = self.crash_timestamps.front() {
            if now.duration_since(front) > self.window {
                self.crash_timestamps.pop_front();
            } else {
                break;
            }
        }
        if self.crash_timestamps.len() >= self.max_crashes as usize {
            self.tripped = true;
        }
        self.tripped
    }

    fn is_tripped(&self) -> bool {
        self.tripped
    }
}

fn resolve_training_worker_env(state: &AppState) -> Result<TrainingWorkerEnv> {
    let socket = adapteros_config::resolve_training_worker_socket_for_cp()?;
    let database_url = adapteros_config::resolve_database_url()?;
    let (datasets_root, artifacts_root) = {
        let cfg = state
            .config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        let datasets = if cfg.paths.datasets_root.is_empty() {
            adapteros_core::rebase_var_path("var/datasets")
        } else {
            adapteros_core::rebase_var_path(std::path::PathBuf::from(&cfg.paths.datasets_root))
        };
        let artifacts = if cfg.paths.artifacts_root.is_empty() {
            adapteros_core::rebase_var_path("var/artifacts")
        } else {
            adapteros_core::rebase_var_path(std::path::PathBuf::from(&cfg.paths.artifacts_root))
        };
        (datasets, artifacts)
    };

    Ok(TrainingWorkerEnv {
        socket_path: socket.path,
        database_url: database_url.path.to_string_lossy().to_string(),
        datasets_root: datasets_root.to_string_lossy().to_string(),
        artifacts_root: artifacts_root.to_string_lossy().to_string(),
    })
}

fn is_training_worker_fallback_error(message: &str) -> bool {
    message.contains("No such file or directory")
        || message.contains("os error 2")
        || message.contains("Training worker binary not found")
        || message.contains("exists but is not a file")
}

#[derive(Debug)]
enum TrainingWorkerBinaryMode {
    Managed(String),
    Fallback(String),
}

fn resolve_training_worker_binary_mode(
    config: &adapteros_config::types::PathsConfig,
    fallback_enabled: bool,
) -> Result<TrainingWorkerBinaryMode> {
    let worker_bin = match resolve_training_worker_bin(config) {
        Ok(path) => path,
        Err(error) => {
            let message = error.to_string();
            if fallback_enabled && is_training_worker_fallback_error(&message) {
                return Ok(TrainingWorkerBinaryMode::Fallback(message));
            }
            return Err(error);
        }
    };

    let bin_path = std::path::Path::new(&worker_bin);
    if !bin_path.exists() {
        let message = format!(
            "Training worker binary not found at {}. Build it: cargo build -p adapteros-training-worker",
            worker_bin
        );
        if fallback_enabled && is_training_worker_fallback_error(&message) {
            return Ok(TrainingWorkerBinaryMode::Fallback(message));
        }
        anyhow::bail!(message);
    }

    if !bin_path.is_file() {
        let message = format!(
            "Training worker path {} exists but is not a file",
            worker_bin
        );
        if fallback_enabled && is_training_worker_fallback_error(&message) {
            return Ok(TrainingWorkerBinaryMode::Fallback(message));
        }
        anyhow::bail!(message);
    }

    info!(path = %worker_bin, "Training worker binary validated at preflight");
    Ok(TrainingWorkerBinaryMode::Managed(worker_bin))
}

async fn probe_training_worker_health(socket_path: &std::path::Path) -> bool {
    let timeout = Duration::from_secs(2);
    let result = tokio::time::timeout(timeout, async {
        let mut stream = UnixStream::connect(socket_path).await?;
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: training-worker\r\n\r\n")
            .await?;
        let mut buffer = vec![0u8; 2048];
        let bytes = stream.read(&mut buffer).await?;
        Ok::<Vec<u8>, std::io::Error>(buffer[..bytes].to_vec())
    })
    .await;

    match result {
        Ok(Ok(response)) => String::from_utf8_lossy(&response).contains("200 OK"),
        _ => false,
    }
}

fn resolve_training_worker_bin(config: &adapteros_config::types::PathsConfig) -> Result<String> {
    // Priority 1: Environment variable override
    if let Ok(path) = std::env::var("AOS_TRAINING_WORKER_BIN") {
        if !path.trim().is_empty() {
            debug!(path = %path, "Resolved training worker binary from AOS_TRAINING_WORKER_BIN");
            return Ok(path);
        }
    }

    // Priority 2: Config file setting
    if let Some(ref config_path) = config.training_worker_bin {
        let candidate = std::path::Path::new(config_path);
        if candidate.exists() {
            info!(path = %config_path, "Resolved training worker binary from config");
            return Ok(config_path.clone());
        }
        warn!(
            path = %config_path,
            "Config training_worker_bin set but path does not exist; trying fallbacks"
        );
    }

    // Priority 3: Sibling to current executable
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join("aos-training-worker");
            debug!(candidate = %candidate.display(), "Checking sibling to current_exe");
            if candidate.exists() {
                let resolved = candidate.to_string_lossy().to_string();
                info!(path = %resolved, "Resolved training worker binary from sibling directory");
                return Ok(resolved);
            }
        }
    }

    // Priority 4: Workspace target directories
    let mut roots = Vec::new();

    if let Ok(var_dir) = std::env::var("AOS_VAR_DIR") {
        let path = std::path::PathBuf::from(var_dir);
        if let Some(parent) = path.parent() {
            roots.push(parent.to_path_buf());
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }

    for root in &roots {
        for candidate in [
            root.join("target/debug/aos-training-worker"),
            root.join("target/release/aos-training-worker"),
        ] {
            debug!(candidate = %candidate.display(), "Checking workspace target directory");
            if candidate.exists() {
                let resolved = candidate.to_string_lossy().to_string();
                info!(path = %resolved, "Resolved training worker binary from workspace target");
                return Ok(resolved);
            }
        }
    }

    // Priority 5: No binary found anywhere
    Err(anyhow::anyhow!(
        "Training worker binary not found at any search path. \
         Build it: cargo build -p adapteros-training-worker"
    ))
}

fn spawn_training_worker(env: &TrainingWorkerEnv, bin_path: &str) -> Result<Child> {
    let mut cmd = Command::new(bin_path);
    cmd.env(
        "AOS_TRAINING_WORKER_SOCKET",
        env.socket_path.to_string_lossy().to_string(),
    );
    cmd.env("AOS_DATABASE_URL", &env.database_url);
    cmd.env("AOS_DATASETS_DIR", &env.datasets_root);
    cmd.env("AOS_ARTIFACTS_DIR", &env.artifacts_root);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.kill_on_drop(true);
    cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "Failed to spawn aos-training-worker (bin={}, socket={}, db={}): {}",
            bin_path,
            env.socket_path.display(),
            env.database_url,
            e
        )
    })
}

async fn terminate_managed_training_worker(child: &mut Child) {
    if let Err(e) = child.start_kill() {
        warn!(error = %e, "Failed to signal training worker for shutdown");
    }
    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(status)) => info!(status = %status, "Managed training worker exited"),
        Ok(Err(e)) => warn!(error = %e, "Failed to await managed training worker exit"),
        Err(_) => warn!("Timed out waiting for managed training worker to exit"),
    }
}

fn clear_training_worker_degraded_marker(path: &std::path::Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!(
                error = %error,
                path = %path.display(),
                "Failed to clear training worker degraded marker"
            );
        }
    }
}

fn write_training_worker_degraded_marker(path: &std::path::Path, contents: String) {
    if let Err(error) = std::fs::write(path, contents) {
        warn!(
            error = %error,
            path = %path.display(),
            "Failed to write training worker degraded marker"
        );
    }
}

fn fallback_degraded_marker_contents(reason: &str, external_worker_unavailable: bool) -> String {
    let mut contents = format!("managed training worker disabled: {reason}\n");
    if external_worker_unavailable {
        contents.push_str("external worker unavailable\n");
    }
    contents
}

#[allow(clippy::too_many_arguments)]
async fn run_training_worker_supervisor(
    worker_env: TrainingWorkerEnv,
    degraded_path: std::path::PathBuf,
    metrics_registry: Arc<MetricsRegistry>,
    training_worker_bin: Option<String>,
    training_worker_fallback_reason: Option<String>,
    db: ProtectedDb,
    ready_tx: oneshot::Sender<std::result::Result<(), String>>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    probe_interval: Duration,
) {
    let mut managed_child: Option<Child> = None;
    let mut restart_count: u64 = 0;
    let mut next_spawn_at = tokio::time::Instant::now();
    let mut circuit_breaker = WorkerCircuitBreaker::new(3, Duration::from_secs(300));
    let mut spawn_disabled_due_to_fallback_error = false;
    let mut fallback_worker_observed_healthy = false;
    let mut ready_sender = Some(ready_tx);
    let mut interval = tokio::time::interval(probe_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    clear_training_worker_degraded_marker(&degraded_path);

    if probe_training_worker_health(&worker_env.socket_path).await {
        if training_worker_fallback_reason.is_some() {
            spawn_disabled_due_to_fallback_error = true;
            fallback_worker_observed_healthy = true;
        }
        info!(
            socket_path = %worker_env.socket_path.display(),
            "Adopting existing healthy training worker"
        );
        clear_training_worker_degraded_marker(&degraded_path);
        if let Some(tx) = ready_sender.take() {
            let _ = tx.send(Ok(()));
        }
    } else if let Some(reason) = training_worker_fallback_reason.as_ref() {
        spawn_disabled_due_to_fallback_error = true;
        write_training_worker_degraded_marker(
            &degraded_path,
            fallback_degraded_marker_contents(reason, false),
        );
        warn!(
            error = %reason,
            socket_path = %worker_env.socket_path.display(),
            "Disabling managed training worker respawn attempts (fallback mode)"
        );
    }

    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.recv() => {
                info!("Training worker supervisor received shutdown signal");
                if let Some(mut child) = managed_child.take() {
                    terminate_managed_training_worker(&mut child).await;
                }
                break;
            }
            _ = interval.tick() => {}
        }

        if let Some(child) = managed_child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    restart_count += 1;
                    let backoff_secs = ((restart_count + 1).min(6)) * 2;
                    next_spawn_at = tokio::time::Instant::now() + Duration::from_secs(backoff_secs);

                    match db.mark_running_jobs_failed_worker_crash().await {
                        Ok(count) if count > 0 => {
                            warn!(
                                affected_jobs = count,
                                "Marked in-flight training jobs as failed due to worker exit"
                            );
                        }
                        Err(error) => {
                            error!(
                                error = %error,
                                "Failed to mark in-flight training jobs as failed after worker exit"
                            );
                        }
                        _ => {}
                    }

                    if !status.success() {
                        warn!(
                            status = %status,
                            restart_count = restart_count,
                            backoff_secs = backoff_secs,
                            "Managed training worker crashed; scheduling restart"
                        );
                        if circuit_breaker.record_crash() {
                            write_training_worker_degraded_marker(
                                &degraded_path,
                                format!(
                                    "circuit breaker tripped: {} crashes in {} seconds\n",
                                    circuit_breaker.max_crashes,
                                    circuit_breaker.window.as_secs()
                                ),
                            );
                            error!(
                                max_crashes = circuit_breaker.max_crashes,
                                window_secs = circuit_breaker.window.as_secs(),
                                "Training worker circuit breaker tripped — stopping restart attempts, marking permanently degraded"
                            );
                        }
                    } else {
                        info!(
                            status = %status,
                            restart_count = restart_count,
                            backoff_secs = backoff_secs,
                            "Managed training worker exited gracefully; scheduling restart"
                        );
                    }

                    metrics_registry
                        .record_metric(
                            "training_worker.restarts_total".to_string(),
                            restart_count as f64,
                        )
                        .await;
                    managed_child = None;
                }
                Ok(None) => {}
                Err(error) => {
                    restart_count += 1;
                    let backoff_secs = ((restart_count + 1).min(6)) * 2;
                    next_spawn_at = tokio::time::Instant::now() + Duration::from_secs(backoff_secs);

                    match db.mark_running_jobs_failed_worker_crash().await {
                        Ok(count) if count > 0 => {
                            warn!(
                                affected_jobs = count,
                                "Marked in-flight training jobs as failed due to worker status error"
                            );
                        }
                        Err(db_error) => {
                            error!(
                                error = %db_error,
                                "Failed to mark in-flight training jobs as failed after worker status error"
                            );
                        }
                        _ => {}
                    }

                    warn!(
                        error = %error,
                        restart_count = restart_count,
                        backoff_secs = backoff_secs,
                        "Failed to inspect managed training worker status"
                    );
                    if circuit_breaker.record_crash() {
                        write_training_worker_degraded_marker(
                            &degraded_path,
                            format!(
                                "circuit breaker tripped: {} crashes in {} seconds\n",
                                circuit_breaker.max_crashes,
                                circuit_breaker.window.as_secs()
                            ),
                        );
                        error!(
                            max_crashes = circuit_breaker.max_crashes,
                            window_secs = circuit_breaker.window.as_secs(),
                            "Training worker circuit breaker tripped — stopping restart attempts, marking permanently degraded"
                        );
                    }

                    metrics_registry
                        .record_metric(
                            "training_worker.restarts_total".to_string(),
                            restart_count as f64,
                        )
                        .await;
                    managed_child = None;
                }
            }
        }

        if probe_training_worker_health(&worker_env.socket_path).await {
            clear_training_worker_degraded_marker(&degraded_path);
            if spawn_disabled_due_to_fallback_error {
                if !fallback_worker_observed_healthy {
                    info!(
                        socket_path = %worker_env.socket_path.display(),
                        "Fallback mode: healthy training worker detected; degraded marker cleared"
                    );
                }
                fallback_worker_observed_healthy = true;
            } else if circuit_breaker.is_tripped() {
                info!(
                    socket_path = %worker_env.socket_path.display(),
                    "Circuit breaker was tripped but healthy training worker detected; degraded marker cleared"
                );
            }
            if let Some(tx) = ready_sender.take() {
                let _ = tx.send(Ok(()));
            }
            continue;
        }

        if spawn_disabled_due_to_fallback_error && fallback_worker_observed_healthy {
            fallback_worker_observed_healthy = false;
            let reason = training_worker_fallback_reason
                .as_deref()
                .unwrap_or("managed training worker unavailable");
            write_training_worker_degraded_marker(
                &degraded_path,
                fallback_degraded_marker_contents(reason, true),
            );
            warn!(
                socket_path = %worker_env.socket_path.display(),
                "Fallback mode: external training worker became unavailable; degraded marker restored"
            );
        }

        if !spawn_disabled_due_to_fallback_error
            && !circuit_breaker.is_tripped()
            && managed_child.is_none()
            && tokio::time::Instant::now() >= next_spawn_at
        {
            let Some(bin_path) = training_worker_bin.as_deref() else {
                continue;
            };
            match spawn_training_worker(&worker_env, bin_path) {
                Ok(child) => {
                    info!(
                        socket_path = %worker_env.socket_path.display(),
                        restart_count = restart_count,
                        "Spawned managed training worker"
                    );
                    clear_training_worker_degraded_marker(&degraded_path);
                    managed_child = Some(child);
                }
                Err(error) => {
                    restart_count = restart_count.saturating_add(1);
                    let backoff_secs = restart_count.min(12) * 5;
                    next_spawn_at = tokio::time::Instant::now() + Duration::from_secs(backoff_secs);
                    warn!(
                        error = %error,
                        socket_path = %worker_env.socket_path.display(),
                        restart_count = restart_count,
                        backoff_secs = backoff_secs,
                        "Failed to spawn managed training worker"
                    );
                    metrics_registry
                        .record_metric("training_worker.attach_failures_total".to_string(), 1.0)
                        .await;
                    if let Some(tx) = ready_sender.take() {
                        let _ = tx.send(Err(error.to_string()));
                    }
                }
            }
        }
    }
}

/// Spawns all background tasks for the adapterOS control plane.
///
/// This function spawns background tasks that run throughout the server lifecycle.
/// Tasks are spawned using the `BackgroundTaskSpawner` which integrates with the
/// shutdown coordinator and task tracking system.
///
/// In dev mode (`is_dev_bypass_enabled()`), only essential tasks are spawned:
/// - Status writer (UI needs it)
/// - WAL checkpoint (database health)
/// - TTL cleanup (prevents DB bloat)
/// - Log cleanup (prevents disk bloat, reduced frequency in dev)
///
/// # Arguments
///
/// * `state` - Application state
/// * `db` - Database connection
/// * `shutdown_coordinator` - Shutdown coordinator for graceful shutdown
/// * `background_tasks` - Task tracker for monitoring
/// * `boot_state` - Boot state manager for reporting failures
/// * `strict_mode` - Whether to fail boot on task spawn errors
/// * `metrics_registry` - Metrics registry for KV alert monitoring
/// * `server_config` - Server configuration for log cleanup settings
///
/// # Returns
///
/// Updated shutdown coordinator and Result indicating success or failure
///
/// # Errors
///
/// Returns error if strict mode is enabled and a critical task fails to spawn
#[allow(clippy::too_many_arguments)]
#[instrument(skip_all)]
pub async fn spawn_all_background_tasks(
    state: &AppState,
    db: &Db,
    mut shutdown_coordinator: ShutdownCoordinator,
    background_tasks: Arc<BackgroundTaskTracker>,
    boot_state: &BootStateManager,
    strict_mode: bool,
    metrics_registry: Arc<MetricsRegistry>,
    server_config: Arc<std::sync::RwLock<adapteros_server_api::config::Config>>,
    diag_receiver: Option<mpsc::Receiver<DiagEnvelope>>,
) -> Result<ShutdownCoordinator> {
    // Keep the deterministic executor draining tasks so spawn_deterministic work runs.
    // This loop is intentionally lightweight and exits on shutdown.
    //
    // `run_global_executor()` performs at most one pass through the queue per call
    // (bounded by queue length), then returns so we can yield to tokio's I/O
    // reactor. Without this yield, async operations inside deterministic tasks
    // (timers, tokio::fs, RwLock) would never complete on this single-threaded
    // runtime.
    {
        let mut shutdown_rx = shutdown_coordinator.subscribe_shutdown();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build deterministic executor runtime");
            runtime.block_on(async move {
                let idle_delay = Duration::from_millis(50);
                loop {
                    tokio::select! {
                        biased;
                        _ = shutdown_rx.recv() => {
                            info!("Deterministic executor pump received shutdown signal, exiting");
                            break;
                        }
                        result = run_global_executor() => {
                            if let Err(e) = result {
                                warn!(error = %e, "Deterministic executor run failed");
                            }
                        }
                    }
                    // Yield to the tokio runtime so the I/O reactor can service
                    // pending wakers (timers, file I/O, locks) before we poll
                    // the executor again.
                    tokio::task::yield_now().await;
                    tokio::time::sleep(idle_delay).await;
                }
            });
        });
    }

    // Local action log service (UDS-only): serves bounded tail reads from var/logs/*
    // for local tooling without exposing log access on HTTP routes.
    {
        let service_config = LocalLogServiceConfig::default();
        let socket_path = service_config.socket_path.clone();
        let shutdown_rx = shutdown_coordinator.subscribe_shutdown();
        let handle = tokio::spawn(async move {
            if let Err(e) = run_local_log_service(service_config, shutdown_rx).await {
                warn!(
                    error = %e,
                    socket = %socket_path.display(),
                    "Local action log service exited with error"
                );
            }
        });
        shutdown_coordinator.set_local_log_service_handle(handle);
        background_tasks.record_spawned("Local action log service", false);
    }

    // Check if we're in dev mode - skip non-essential tasks for faster startup
    let dev_mode = adapteros_server_api::is_dev_bypass_enabled();
    if dev_mode {
        info!(
            "Dev mode enabled - spawning only essential background tasks (status writer, WAL checkpoint, TTL cleanup)"
        );
    }

    // In worker execution mode, supervise aos-training-worker via background task.
    // If a healthy worker is already bound to the socket, adopt it without spawning a duplicate.
    if adapteros_config::training_execution_mode()
        == adapteros_config::TrainingExecutionMode::Worker
    {
        let training_worker_degraded_path =
            adapteros_core::rebase_var_path("var/run/training-worker.degraded");
        let worker_env = resolve_training_worker_env(state)
            .map_err(|e| anyhow::anyhow!("Failed to resolve training worker environment: {}", e))?;
        let training_worker_fallback_enabled = adapteros_config::training_worker_fallback_enabled();

        // Preflight: resolve and validate training worker binary before spawning supervisor
        let paths_config = {
            let cfg = state
                .config
                .read()
                .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
            cfg.paths.clone()
        };
        let (training_worker_bin, training_worker_fallback_reason) =
            match resolve_training_worker_binary_mode(
                &paths_config,
                training_worker_fallback_enabled,
            )? {
                TrainingWorkerBinaryMode::Managed(path) => (Some(path), None),
                TrainingWorkerBinaryMode::Fallback(reason) => (None, Some(reason)),
            };

        let worker_env_for_supervisor = worker_env.clone();
        let degraded_path_for_supervisor = training_worker_degraded_path.clone();
        let metrics_registry_for_worker = Arc::clone(&metrics_registry);
        let training_worker_bin_for_supervisor = training_worker_bin.clone();
        let training_worker_fallback_reason_for_supervisor =
            training_worker_fallback_reason.clone();
        let db_for_supervisor = state.db.clone();
        let (ready_tx, ready_rx) = oneshot::channel::<std::result::Result<(), String>>();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if let Err(err) = spawner.spawn_with_details(
            "Training worker supervisor",
            async move {
                run_training_worker_supervisor(
                    worker_env_for_supervisor,
                    degraded_path_for_supervisor,
                    metrics_registry_for_worker,
                    training_worker_bin_for_supervisor,
                    training_worker_fallback_reason_for_supervisor,
                    db_for_supervisor,
                    ready_tx,
                    shutdown_rx,
                    Duration::from_secs(2),
                )
                .await;
            },
            "worker-mode UDS supervision",
        ) {
            if strict_mode {
                boot_state
                    .fail(FailureReason::with_component(
                        "BOOT_TRAINING_WORKER_ATTACH_FAILED",
                        format!("{} failed to spawn: {}", &err.task_name, &err.message),
                        err.task_name.clone(),
                    ))
                    .await;
                return Err(anyhow::anyhow!(err.to_string()));
            }

            boot_state
                .record_boot_warning(&err.task_name, format!("Failed to spawn: {}", &err.message));
            warn!(
                task = %err.task_name,
                error = %err.message,
                "Training worker supervisor failed to spawn; boot continues with degraded training availability"
            );
        }
        shutdown_coordinator = spawner.into_coordinator();

        let attach_timeout = Duration::from_secs(20);
        let attach_result = tokio::time::timeout(attach_timeout, ready_rx).await;
        match attach_result {
            Ok(Ok(Ok(()))) => {
                info!(
                    socket_path = %worker_env.socket_path.display(),
                    "Training worker attach verified"
                );
                metrics_registry
                    .record_metric("training_worker.attach_success_total".to_string(), 1.0)
                    .await;
            }
            Ok(Ok(Err(err_msg))) => {
                metrics_registry
                    .record_metric("training_worker.attach_failures_total".to_string(), 1.0)
                    .await;
                if training_worker_fallback_enabled && is_training_worker_fallback_error(&err_msg) {
                    warn!(
                        error = %err_msg,
                        "Training worker attach unavailable; continuing with fallback mode"
                    );
                } else {
                    if strict_mode {
                        boot_state
                            .fail(FailureReason::with_component(
                                "BOOT_TRAINING_WORKER_ATTACH_FAILED",
                                err_msg.clone(),
                                "Training worker supervisor",
                            ))
                            .await;
                        return Err(anyhow::anyhow!(err_msg));
                    }
                    boot_state.record_boot_warning("Training worker supervisor", err_msg.clone());
                    warn!(
                        error = %err_msg,
                        "Training worker attach failed in non-strict mode; continuing"
                    );
                }
            }
            Ok(Err(_closed)) => {
                let message = format!(
                    "Training worker attach did not complete within {}s",
                    attach_timeout.as_secs()
                );
                metrics_registry
                    .record_metric("training_worker.attach_failures_total".to_string(), 1.0)
                    .await;
                if strict_mode {
                    boot_state
                        .fail(FailureReason::with_component(
                            "BOOT_TRAINING_WORKER_ATTACH_FAILED",
                            message.clone(),
                            "Training worker supervisor",
                        ))
                        .await;
                    return Err(anyhow::anyhow!(message));
                }
                if !training_worker_fallback_enabled {
                    boot_state.record_boot_warning("Training worker supervisor", message.clone());
                    warn!(error = %message, "Continuing without strict training worker attach");
                } else {
                    warn!(error = %message, "Continuing without strict training worker attach (fallback mode)");
                }
            }
            Err(_) => {
                let message = format!(
                    "Training worker attach did not complete within {}s",
                    attach_timeout.as_secs()
                );
                metrics_registry
                    .record_metric("training_worker.attach_failures_total".to_string(), 1.0)
                    .await;
                if strict_mode {
                    boot_state
                        .fail(FailureReason::with_component(
                            "BOOT_TRAINING_WORKER_ATTACH_FAILED",
                            message.clone(),
                            "Training worker supervisor",
                        ))
                        .await;
                    return Err(anyhow::anyhow!(message));
                }
                if !training_worker_fallback_enabled {
                    boot_state.record_boot_warning("Training worker supervisor", message.clone());
                    warn!(error = %message, "Continuing without strict training worker attach");
                } else {
                    warn!(error = %message, "Continuing without strict training worker attach (fallback mode)");
                }
            }
        }
    }

    // Spawn status writer background task (using BackgroundTaskSpawner)
    {
        let state_clone = state.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if let Err(err) = spawner.spawn_with_details(
            "Status writer",
            async move {
                let mut interval = tokio::time::interval(Duration::from_secs(5));
                loop {
                    tokio::select! {
                        biased;
                        _ = shutdown_rx.recv() => {
                            info!("Status writer received shutdown signal, exiting gracefully");
                            break;
                        }
                        _ = interval.tick() => {
                            if let Err(e) = status_writer::write_status(&state_clone).await {
                                warn!(error = %e, "Failed to write status");
                            }
                        }
                    }
                }
            },
            "5s interval",
        ) {
            if strict_mode {
                boot_state
                    .fail(FailureReason::with_component(
                        "BOOT_BACKGROUND_TASK_FAILED",
                        format!("{} failed to spawn: {}", &err.task_name, &err.message),
                        err.task_name.clone(),
                    ))
                    .await;
                return Err(anyhow::anyhow!(err.to_string()));
            }

            // Record the warning for /readyz visibility (honest about what happened)
            boot_state
                .record_boot_warning(&err.task_name, format!("Failed to spawn: {}", &err.message));

            warn!(
                task = %err.task_name,
                error = %err.message,
                "Background task failed to spawn; boot continues but this feature will be unavailable"
            );
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn one-shot startup inference warmup task.
    // This waits for Ready, warms all resolved tenants, and promotes to FullyReady
    // only when every tenant warmup succeeds.
    {
        let state_clone = state.clone();
        let boot_state_clone = boot_state.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if let Err(err) = spawner.spawn_with_details(
            "Startup inference warmup",
            async move {
                run_startup_inference_warmup(state_clone, boot_state_clone, shutdown_rx).await;
            },
            "one-shot startup warmup task",
        ) {
            if strict_mode {
                boot_state
                    .fail(FailureReason::with_component(
                        "BOOT_BACKGROUND_TASK_FAILED",
                        format!("{} failed to spawn: {}", &err.task_name, &err.message),
                        err.task_name.clone(),
                    ))
                    .await;
                return Err(anyhow::anyhow!(err.to_string()));
            }

            boot_state
                .record_boot_warning(&err.task_name, format!("Failed to spawn: {}", &err.message));
            warn!(
                task = %err.task_name,
                error = %err.message,
                "Startup warmup task failed to spawn; FullyReady promotion will remain blocked"
            );
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn KV metrics alert monitor (drift/fallback/error/degraded)
    // SKIPPED in dev mode - production alerting only
    if !dev_mode {
        let metrics_registry = Arc::clone(&metrics_registry);
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "KV alert monitor",
                async move {
                    let mut alerting = AlertingEngine::new(100);
                    for rule in kv_metrics::kv_alert_rules() {
                        alerting.register_rule(rule);
                    }

                    let mut interval = tokio::time::interval(Duration::from_secs(5));
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("KV alert monitor received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                let snapshot = kv_metrics::global_kv_metrics().snapshot();

                                // Record KV counters into the metrics registry for dashboards
                                metrics_registry
                                    .record_metric(
                                        kv_metrics::KV_ALERT_METRIC_FALLBACKS.to_string(),
                                        snapshot.fallback_operations_total as f64,
                                    )
                                    .await;
                                metrics_registry
                                    .record_metric(
                                        kv_metrics::KV_ALERT_METRIC_ERRORS.to_string(),
                                        snapshot.errors_total as f64,
                                    )
                                    .await;
                                metrics_registry
                                    .record_metric(
                                        kv_metrics::KV_ALERT_METRIC_DRIFT.to_string(),
                                        snapshot.drift_detections_total as f64,
                                    )
                                    .await;
                                metrics_registry
                                    .record_metric(
                                        kv_metrics::KV_ALERT_METRIC_DEGRADATIONS.to_string(),
                                        snapshot.degraded_events_total as f64,
                                    )
                                    .await;

                                // Evaluate alert rules and emit warn-level logs for now (log channel only)
                                let alerts = kv_metrics::evaluate_kv_alerts(&snapshot, &mut alerting);
                                for alert in alerts {
                                    warn!(
                                        metric = %alert.metric,
                                        rule = %alert.rule_name,
                                        severity = ?alert.severity,
                                        value = alert.value,
                                        "KV alert triggered"
                                    );
                                }
                            }
                        }
                    }
                },
                "KV alerting disabled",
            )
            .is_ok()
        {
            info!("KV alert monitor started (5s interval)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn log cleanup background task
    // Runs in all modes - dev mode uses a shorter interval since logs accumulate faster
    {
        let (log_dir_opt, retention_days, max_log_files) = {
            let cfg = server_config.read().map_err(|e| {
                error!(error = %e, "Config lock poisoned during log cleanup setup");
                anyhow::anyhow!("config lock poisoned")
            })?;
            (
                cfg.logging.log_dir.clone(),
                cfg.logging.retention_days,
                cfg.logging.max_log_files,
            )
        };

        if let Some(log_dir) = log_dir_opt {
            if retention_days > 0 || max_log_files > 0 {
                let log_dir_for_info = log_dir.clone();

                // Run cleanup on startup
                if let Err(e) =
                    logging::cleanup_old_logs(&log_dir, retention_days, max_log_files).await
                {
                    error!(error = %e, "Failed to cleanup old logs on startup");
                }

                // In dev mode, run every 4 hours instead of daily
                let interval_secs = if dev_mode { 14400 } else { 86400 };

                // Spawn periodic cleanup task
                let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                    .with_task_tracker(Arc::clone(&background_tasks));
                let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
                if spawner
                    .spawn_optional(
                        "Log cleanup",
                        async move {
                            let mut interval =
                                tokio::time::interval(Duration::from_secs(interval_secs));
                            interval
                                .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                            loop {
                                tokio::select! {
                                    biased;
                                    _ = shutdown_rx.recv() => {
                                        info!("Log cleanup received shutdown signal, exiting gracefully");
                                        break;
                                    }
                                    _ = interval.tick() => {
                                        match logging::cleanup_old_logs(&log_dir, retention_days, max_log_files).await {
                                            Ok(count) => {
                                                if count > 0 {
                                                    info!(
                                                        count,
                                                        retention_days,
                                                        max_log_files,
                                                        log_dir = %log_dir,
                                                        "Cleaned up old log files"
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                error!(
                                                    error = %e,
                                                    log_dir = %log_dir,
                                                    "Failed to cleanup old logs"
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "Old logs will not be automatically deleted",
                    )
                    .is_ok()
                {
                    let interval_desc = if dev_mode { "4h (dev)" } else { "24h" };
                    info!(
                        retention_days,
                        max_log_files,
                        interval = interval_desc,
                        log_dir = %log_dir_for_info,
                        "Log cleanup task started"
                    );
                }
                shutdown_coordinator = spawner.into_coordinator();
            }
        }
    }

    // Spawn TTL cleanup background task
    // KEPT in dev mode - prevents DB bloat
    {
        let db_clone = db.clone();
        let tracker = Arc::clone(&background_tasks);
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "TTL cleanup",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                    let mut consecutive_errors = 0u32;
                    const MAX_CONSECUTIVE_ERRORS: u32 = 5;
                    const CIRCUIT_BREAKER_PAUSE_SECS: u64 = 1800; // 30 minutes

                    loop {
                        // PRD-4.8: Heartbeat for stale task detection
                        tracker.heartbeat("TTL cleanup");

                        // Check for shutdown before starting any work
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("TTL cleanup received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                // Continue with cleanup work
                            }
                        }

                        // Circuit breaker: pause if too many consecutive errors
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            error!(
                                consecutive_errors,
                                pause_duration_secs = CIRCUIT_BREAKER_PAUSE_SECS,
                                "TTL cleanup circuit breaker triggered, pausing task"
                            );
                            // Check shutdown during circuit breaker pause
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("TTL cleanup received shutdown signal during circuit breaker pause, exiting");
                                    break;
                                }
                                _ = tokio::time::sleep(Duration::from_secs(CIRCUIT_BREAKER_PAUSE_SECS)) => {}
                            }
                            consecutive_errors = 0;
                            continue;
                        }

                        let mut had_error = false;

                        // Find and clean up expired adapters
                        match db_clone.find_expired_adapters().await {
                            Ok(expired) => {
                                if !expired.is_empty() {
                                    info!(
                                        count = expired.len(),
                                        "Found expired adapters, cleaning up"
                                    );

                                    for adapter in expired {
                                        let adapter_id_display =
                                            adapter.adapter_id.as_deref().unwrap_or("unknown");
                                        let name_display = &adapter.name;

                                        info!(
                                            adapter_id = adapter_id_display,
                                            name = name_display,
                                            expired_at = ?adapter.expires_at,
                                            "Deleting expired adapter"
                                        );

                                        // Delete the expired adapter
                                        if let Err(e) = db_clone.delete_adapter(&adapter.id).await {
                                            warn!(
                                                adapter_id = adapter_id_display,
                                                error = %e,
                                                "Failed to delete expired adapter"
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                had_error = true;
                                warn!(
                                    error = %e,
                                    consecutive_errors = consecutive_errors + 1,
                                    "Failed to query for expired adapters"
                                );
                            }
                        }

                        // Also cleanup expired pins from pinned_adapters table
                        if let Err(e) = db_clone.cleanup_expired_pins().await {
                            had_error = true;
                            warn!(
                                error = %e,
                                consecutive_errors = consecutive_errors + 1,
                                "Failed to cleanup expired pins"
                            );
                        }

                        // Update error counter with exponential backoff
                        if had_error {
                            consecutive_errors += 1;
                            let backoff_secs = 2u64.pow(consecutive_errors.min(6)); // Cap at 64 seconds
                            warn!(
                                consecutive_errors,
                                backoff_secs, "TTL cleanup error, applying exponential backoff"
                            );
                            // Check shutdown during backoff
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("TTL cleanup received shutdown signal during backoff, exiting");
                                    break;
                                }
                                _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                            }
                        } else {
                            consecutive_errors = 0; // Reset on success
                        }
                    }
                },
                "Expired adapters may not be cleaned up automatically",
            )
            .is_ok()
        {
            info!("TTL cleanup task started (5 minute interval, circuit breaker enabled)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn upload session cleanup background task
    // PRD Phase 3: Cleanup ALWAYS runs - dev mode reduces frequency, never disables
    {
        let upload_manager = Arc::clone(&state.upload_session_manager);
        // In dev mode with keep_partial_uploads, run cleanup less frequently (12 hours)
        // but never disable it to prevent disk space issues
        let default_interval = if dev_mode { 43200 } else { 3600 }; // 12h dev, 1h prod
        let interval_secs = std::env::var("AOS_UPLOAD_SESSION_CLEANUP_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| {
                if v == 0 {
                    warn!("AOS_UPLOAD_SESSION_CLEANUP_SECS=0 is deprecated; using minimum of 300s");
                    300 // Minimum 5 minutes, never disable
                } else {
                    v
                }
            })
            .unwrap_or(default_interval);

        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Upload session cleanup",
                async move {
                    let mut interval =
                        tokio::time::interval(Duration::from_secs(interval_secs));
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Upload session cleanup received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                match upload_manager.cleanup_expired().await {
                                    Ok(count) => {
                                        if count > 0 {
                                            info!(count, "Cleaned up expired upload sessions");
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "Failed to cleanup expired upload sessions");
                                    }
                                }
                            }
                        }
                    }
                },
                "Expired upload sessions may accumulate",
            )
            .is_ok()
        {
            info!(
                interval_secs,
                dev_mode,
                "Upload session cleanup task started"
            );
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn security cleanup background task
    // SKIPPED in dev mode - production maintenance only
    if !dev_mode {
        let db_clone = db.clone();
        let interval_secs = std::env::var("AOS_SECURITY_CLEANUP_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600);

        if interval_secs > 0 {
            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Security cleanup",
                    async move {
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(interval_secs));
                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                        loop {
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Security cleanup received shutdown signal, exiting gracefully");
                                    break;
                                }
                                _ = interval.tick() => {
                                    let mut total_cleaned = 0usize;

                                    match cleanup_expired_sessions(&db_clone).await {
                                        Ok(count) => {
                                            total_cleaned += count;
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "Failed to cleanup expired auth sessions");
                                        }
                                    }

                                    match cleanup_expired_revocations(&db_clone).await {
                                        Ok(count) => {
                                            total_cleaned += count;
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "Failed to cleanup expired token revocations");
                                        }
                                    }

                                    match cleanup_expired_ip_rules(&db_clone).await {
                                        Ok(count) => {
                                            total_cleaned += count;
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "Failed to cleanup expired IP access rules");
                                        }
                                    }

                                    if total_cleaned > 0 {
                                        info!(total_cleaned, "Cleaned up expired security records");
                                    }
                                }
                            }
                        }
                    },
                    "Expired security records may accumulate",
                )
                .is_ok()
            {
                info!("Security cleanup task started ({}s interval)", interval_secs);
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Security cleanup disabled via AOS_SECURITY_CLEANUP_SECS=0");
        }
    }

    // Spawn egress re-verification monitor
    // SKIPPED in dev mode - production security only
    // Periodically re-checks firewall rules to detect rule changes after boot.
    // Default interval: 60s (configurable via AOS_EGRESS_MONITOR_SECS)
    if !dev_mode && !crate::boot::egress_monitor::is_monitor_disabled() {
        let require_pf_deny = {
            let cfg = server_config.read().map_err(|e| {
                error!(error = %e, "Config lock poisoned during egress monitor setup");
                anyhow::anyhow!("config lock poisoned")
            })?;
            cfg.security.require_pf_deny
        };

        if require_pf_deny {
            let interval_secs = crate::boot::egress_monitor::monitor_interval_secs();
            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Egress re-verification",
                    async move {
                        use crate::boot::egress_monitor::run_egress_check;

                        let mut interval =
                            tokio::time::interval(Duration::from_secs(interval_secs));
                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                        // First check establishes baseline
                        let mut previous = run_egress_check(None);
                        if previous.is_none() {
                            warn!(
                                "Egress monitor: pfctl/iptables unavailable, disabling periodic check"
                            );
                            return;
                        }

                        loop {
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Egress re-verification received shutdown signal, exiting gracefully");
                                    break;
                                }
                                _ = interval.tick() => {
                                    match run_egress_check(previous.as_ref()) {
                                        Some(snapshot) => {
                                            previous = Some(snapshot);
                                        }
                                        None => {
                                            warn!("Egress monitor: pfctl/iptables became unavailable, disabling");
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "Egress rules will not be monitored after boot",
                )
                .is_ok()
            {
                info!(
                    interval_secs = interval_secs,
                    "Egress re-verification monitor started"
                );
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Egress monitor skipped: require_pf_deny is false");
        }
    }

    // Spawn telemetry bundle GC background task
    // SKIPPED in dev mode - production maintenance only
    // Default interval: 6 hours (21600 seconds) per Retention Ruleset #10
    if !dev_mode {
        let telemetry_store = Arc::clone(&state.telemetry_bundle_store);
        let interval_secs = std::env::var("AOS_TELEMETRY_BUNDLE_GC_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(21600); // 6 hours default

        if interval_secs > 0 {
            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Telemetry bundle GC",
                    async move {
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(interval_secs));
                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                        loop {
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Telemetry bundle GC received shutdown signal, exiting gracefully");
                                    break;
                                }
                                _ = interval.tick() => {
                                    let store = Arc::clone(&telemetry_store);
                                    match tokio::task::spawn_blocking(move || {
                                        let mut store = store.write().unwrap_or_else(|e| {
                                            warn!(error = %e, "Telemetry bundle store lock poisoned, recovering");
                                            e.into_inner()
                                        });

                                        // Log retention policy before GC
                                        let stats_before = store.get_stats();
                                        info!(
                                            total_bundles = stats_before.total_bundles,
                                            incident_bundles = stats_before.incident_bundles,
                                            promotion_bundles = stats_before.promotion_bundles,
                                            total_bytes = stats_before.total_bytes,
                                            "Telemetry bundle GC starting"
                                        );

                                        // Run GC
                                        let gc_result = store.run_gc();

                                        // Verify protected bundles after GC
                                        if gc_result.is_ok() {
                                            let stats_after = store.get_stats();
                                            // Verify incident/promotion bundles were preserved
                                            if stats_after.incident_bundles < stats_before.incident_bundles {
                                                warn!(
                                                    before = stats_before.incident_bundles,
                                                    after = stats_after.incident_bundles,
                                                    "Incident bundles decreased during GC - policy violation!"
                                                );
                                            }
                                            if stats_after.promotion_bundles < stats_before.promotion_bundles {
                                                warn!(
                                                    before = stats_before.promotion_bundles,
                                                    after = stats_after.promotion_bundles,
                                                    "Promotion bundles decreased during GC - policy violation!"
                                                );
                                            }
                                            debug!(
                                                incident_bundles_preserved = stats_after.incident_bundles,
                                                promotion_bundles_preserved = stats_after.promotion_bundles,
                                                "Protected bundles verified after GC"
                                            );
                                        }

                                        gc_result
                                    })
                                    .await
                                    {
                                        Ok(Ok(report)) => {
                                            info!(
                                                evicted = report.evicted_bundles.len(),
                                                bytes_freed = report.bytes_freed,
                                                retained = report.retained_bundles,
                                                total_before = report.total_bundles,
                                                "Telemetry bundle GC completed"
                                            );
                                        }
                                        Ok(Err(e)) => {
                                            warn!(error = %e, "Telemetry bundle GC failed");
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "Telemetry bundle GC task failed");
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "Telemetry bundle GC is disabled",
                )
                .is_ok()
            {
                info!(
                    interval_secs = interval_secs,
                    interval_hours = interval_secs / 3600,
                    "Telemetry bundle GC task started"
                );
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Telemetry bundle GC disabled via AOS_TELEMETRY_BUNDLE_GC_SECS=0");
        }
    }

    // Spawn audit chain verification background task
    // SKIPPED in dev mode - production integrity monitoring only
    // Default interval: 1 hour (3600 seconds) per PR-001 recommendation
    if !dev_mode {
        let db_clone = db.clone();
        let metrics_reg = Arc::clone(&metrics_registry);
        let interval_secs = std::env::var("AOS_AUDIT_VERIFY_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600); // 1 hour default

        if interval_secs > 0 {
            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Audit chain verification",
                    async move {
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(interval_secs));
                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                        loop {
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Audit chain verification received shutdown signal, exiting gracefully");
                                    break;
                                }
                                _ = interval.tick() => {
                                    info!("Running periodic audit chain verification");
                                    let mut total_divergent = 0usize;
                                    let mut total_checked = 0usize;

                                    // Verify policy audit chains
                                    match db_clone.verify_all_policy_audit_chains().await {
                                        Ok(results) => {
                                            let divergent_count = results.values().filter(|r| r.divergence_detected).count();
                                            total_checked += results.len();
                                            total_divergent += divergent_count;

                                            // Record metrics
                                            metrics_reg
                                                .record_metric(
                                                    "audit_policy_chains_verified".to_string(),
                                                    results.len() as f64,
                                                )
                                                .await;
                                            metrics_reg
                                                .record_metric(
                                                    "audit_policy_chains_divergent".to_string(),
                                                    divergent_count as f64,
                                                )
                                                .await;

                                            if divergent_count > 0 {
                                                for (tenant_id, result) in results.iter().filter(|(_, r)| r.divergence_detected) {
                                                    error!(
                                                        tenant_id = %tenant_id,
                                                        first_invalid_sequence = ?result.first_invalid_sequence,
                                                        error_message = ?result.error_message,
                                                        "Policy audit chain divergence detected in periodic verification"
                                                    );
                                                    // Emit telemetry event for observability pipeline (PRD requirement)
                                                    let event = adapteros_core::telemetry::audit_chain_divergence_event(
                                                        result.error_message.clone().unwrap_or_else(|| "hash mismatch".to_string()),
                                                        result.first_invalid_sequence,
                                                        Some(tenant_id.clone()),
                                                        None,
                                                    );
                                                    adapteros_core::telemetry::emit_observability_event(&event);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!(error = %e, "Failed to verify policy audit chains");
                                            metrics_reg
                                                .record_metric(
                                                    "audit_verification_errors_total".to_string(),
                                                    1.0,
                                                )
                                                .await;
                                        }
                                    }

                                    // Verify evidence envelope chains
                                    match db_clone.verify_all_evidence_chains().await {
                                        Ok(results) => {
                                            let divergent_count = results.iter().filter(|r| r.divergence_detected).count();
                                            total_checked += results.len();
                                            total_divergent += divergent_count;

                                            // Record metrics
                                            metrics_reg
                                                .record_metric(
                                                    "audit_evidence_chains_verified".to_string(),
                                                    results.len() as f64,
                                                )
                                                .await;
                                            metrics_reg
                                                .record_metric(
                                                    "audit_evidence_chains_divergent".to_string(),
                                                    divergent_count as f64,
                                                )
                                                .await;

                                            if divergent_count > 0 {
                                                for result in results.iter().filter(|r| r.divergence_detected) {
                                                    error!(
                                                        tenant_id = %result.tenant_id,
                                                        scope = ?result.scope,
                                                        first_invalid_index = ?result.first_invalid_index,
                                                        error_message = ?result.error_message,
                                                        "Evidence envelope chain divergence detected in periodic verification"
                                                    );
                                                    // Emit telemetry event for observability pipeline (PRD requirement)
                                                    let event = adapteros_core::telemetry::audit_chain_divergence_event(
                                                        format!(
                                                            "Evidence chain {:?}: {}",
                                                            result.scope,
                                                            result.error_message.clone().unwrap_or_else(|| "chain broken".to_string())
                                                        ),
                                                        result.first_invalid_index.map(|i| i as i64),
                                                        Some(result.tenant_id.clone()),
                                                        None,
                                                    );
                                                    adapteros_core::telemetry::emit_observability_event(&event);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!(error = %e, "Failed to verify evidence envelope chains");
                                            metrics_reg
                                                .record_metric(
                                                    "audit_verification_errors_total".to_string(),
                                                    1.0,
                                                )
                                                .await;
                                        }
                                    }

                                    if total_divergent > 0 {
                                        error!(
                                            total_checked,
                                            total_divergent,
                                            "AUDIT CHAIN DIVERGENCE DETECTED - integrity compromised"
                                        );
                                    } else {
                                        info!(
                                            total_checked,
                                            "Periodic audit chain verification completed - all chains valid"
                                        );
                                    }
                                }
                            }
                        }
                    },
                    "Audit chain verification is disabled",
                )
                .is_ok()
            {
                info!(
                    interval_secs = interval_secs,
                    interval_hours = interval_secs / 3600,
                    "Audit chain verification task started"
                );
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Audit chain verification disabled via AOS_AUDIT_VERIFY_INTERVAL_SECS=0");
        }
    }

    // Spawn WAL checkpoint background task
    // KEPT in dev mode - database health
    {
        let db_clone = db.clone();
        let tracker = Arc::clone(&background_tasks);
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "WAL checkpoint",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                    loop {
                        // PRD-4.8: Heartbeat for stale task detection
                        tracker.heartbeat("WAL checkpoint");

                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("WAL checkpoint received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                match db_clone.wal_checkpoint().await {
                                    Ok(()) => {
                                        // Success - checkpoint completed
                                        debug!("WAL checkpoint completed successfully");
                                    }
                                    Err(e) => {
                                        // Log but don't fail - checkpoints are best-effort
                                        warn!(
                                            error = %e,
                                            "WAL checkpoint failed (non-fatal, will retry)"
                                        );
                                    }
                                }
                            }
                        }
                    }
                },
                "Relying on auto-checkpoint only",
            )
            .is_ok()
        {
            info!("WAL checkpoint task started (5 minute interval)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn orphaned training job cleanup background task
    // SKIPPED in dev mode - production maintenance only
    // ANCHOR: Jobs running >24h without progress are considered orphaned
    if !dev_mode {
        let db_clone = db.clone();
        let interval_secs = std::env::var("AOS_ORPHANED_JOB_CLEANUP_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600); // 1 hour default
        let threshold_hours = std::env::var("AOS_ORPHANED_JOB_THRESHOLD_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(24); // 24 hours default

        if interval_secs > 0 {
            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Orphaned training job cleanup",
                    async move {
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(interval_secs));
                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                        let staleness_threshold = Duration::from_secs(threshold_hours * 3600);

                        loop {
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Orphaned training job cleanup received shutdown signal, exiting gracefully");
                                    break;
                                }
                                _ = interval.tick() => {
                                    // ANCHOR: Find jobs that have been running too long without progress
                                    match db_clone.find_orphaned_training_jobs(staleness_threshold).await {
                                        Ok(orphaned) => {
                                            if orphaned.is_empty() {
                                                debug!("No orphaned training jobs found");
                                            } else {
                                                info!(
                                                    count = orphaned.len(),
                                                    threshold_hours = threshold_hours,
                                                    "Found orphaned training jobs, marking as failed"
                                                );

                                                for job in &orphaned {
                                                    // RECTIFY: Mark as failed with reason recorded in metadata
                                                    let reason = "stale_no_progress";
                                                    if let Err(e) = db_clone.mark_training_job_failed_orphaned(
                                                        &job.id,
                                                        reason,
                                                        threshold_hours,
                                                    ).await {
                                                        warn!(
                                                            job_id = %job.id,
                                                            error = %e,
                                                            "Failed to mark orphaned training job as failed"
                                                        );
                                                    } else {
                                                        // AUDIT: Track cleanup metrics
                                                        ORPHANED_TRAINING_JOB_CLEANED.fetch_add(1, Ordering::Relaxed);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                error = %e,
                                                "Failed to query for orphaned training jobs"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "Orphaned training jobs may accumulate",
                )
                .is_ok()
            {
                info!(
                    interval_secs = interval_secs,
                    threshold_hours = threshold_hours,
                    "Orphaned training job cleanup task started"
                );
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Orphaned training job cleanup disabled via AOS_ORPHANED_JOB_CLEANUP_SECS=0");
        }
    }

    // Spawn stale worker reaper task (60s interval)
    // SKIPPED in dev mode - production maintenance only
    // ANCHOR: Workers with non-terminal status whose PID is no longer alive are reaped
    if !dev_mode {
        let db_clone = db.clone();
        let state_clone_for_reaper = state.clone();
        let interval_secs = std::env::var("AOS_STALE_WORKER_REAPER_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);

        if interval_secs > 0 {
            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Stale worker reaper",
                    async move {
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(interval_secs));
                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                        loop {
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Stale worker reaper received shutdown signal, exiting gracefully");
                                    break;
                                }
                                _ = interval.tick() => {
                                    // ANCHOR: Query workers in non-terminal status with a PID
                                    match db_clone.list_all_workers().await {
                                        Ok(workers) => {
                                            let non_terminal: Vec<_> = workers.into_iter()
                                                .filter(|w| {
                                                    w.pid.is_some()
                                                        && w.status != "stopped"
                                                        && w.status != "error"
                                                })
                                                .collect();

                                            for worker in &non_terminal {
                                                let pid = match worker.pid {
                                                    Some(p) => p,
                                                    None => continue,
                                                };

                                                // Check PID liveness via signal 0
                                                let alive = unsafe {
                                                    libc::kill(pid, 0) == 0
                                                };

                                                if alive {
                                                    continue;
                                                }

                                                // Cross-reference UDS socket existence for safety
                                                let socket_exists = std::path::Path::new(&worker.uds_path).exists();
                                                if socket_exists {
                                                    debug!(
                                                        worker_id = %worker.id,
                                                        pid = pid,
                                                        uds_path = %worker.uds_path,
                                                        "Worker PID dead but UDS socket still exists, reaping anyway"
                                                    );
                                                }

                                                // RECTIFY: Transition dead workers to error
                                                if let Err(e) = db_clone.transition_worker_status(
                                                    &worker.id,
                                                    "error",
                                                    "stale_pid_dead",
                                                    None,
                                                ).await {
                                                    warn!(
                                                        worker_id = %worker.id,
                                                        pid = pid,
                                                        error = %e,
                                                        "Failed to transition stale worker to error"
                                                    );
                                                    continue;
                                                }

                                                // Remove from worker_runtime DashMap (fixes memory leak)
                                                state_clone_for_reaper.worker_runtime.remove(&worker.id);

                                                // AUDIT: Record incident for each reaped worker
                                                if let Err(e) = db_clone.insert_worker_incident(
                                                    &worker.id,
                                                    &worker.tenant_id,
                                                    adapteros_db::workers::WorkerIncidentType::Crash,
                                                    "stale_pid_dead: process no longer running",
                                                    None,
                                                    None,
                                                ).await {
                                                    warn!(
                                                        worker_id = %worker.id,
                                                        error = %e,
                                                        "Failed to record incident for stale worker"
                                                    );
                                                }

                                                info!(
                                                    worker_id = %worker.id,
                                                    pid = pid,
                                                    previous_status = %worker.status,
                                                    "Reaped stale worker: PID no longer alive"
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                error = %e,
                                                "Failed to query workers for stale reaper"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "Stale workers may accumulate in database",
                )
                .is_ok()
            {
                info!(
                    interval_secs = interval_secs,
                    "Stale worker reaper task started"
                );
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Stale worker reaper disabled via AOS_STALE_WORKER_REAPER_SECS=0");
        }
    }

    // Spawn terminal worker purge task (1h interval)
    // NOT skipped in dev mode - prevents DB bloat from accumulated terminal workers
    {
        let db_clone = db.clone();
        let retention_days = std::env::var("AOS_WORKER_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(7); // 7 days default

        let interval_secs = std::env::var("AOS_TERMINAL_WORKER_PURGE_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600); // 1 hour default

        if interval_secs > 0 {
            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Terminal worker purge",
                    async move {
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(interval_secs));
                        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                        loop {
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Terminal worker purge received shutdown signal, exiting gracefully");
                                    break;
                                }
                                _ = interval.tick() => {
                                    match db_clone.purge_terminal_workers(retention_days).await {
                                        Ok(purged) => {
                                            if purged > 0 {
                                                info!(
                                                    purged,
                                                    retention_days,
                                                    "Purged terminal workers older than retention period"
                                                );
                                            } else {
                                                debug!("No terminal workers to purge");
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                error = %e,
                                                retention_days,
                                                "Failed to purge terminal workers"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "Terminal workers may accumulate in database",
                )
                .is_ok()
            {
                info!(
                    interval_secs,
                    retention_days,
                    "Terminal worker purge task started"
                );
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Terminal worker purge disabled via AOS_TERMINAL_WORKER_PURGE_SECS=0");
        }
    }

    // Spawn rate limiter eviction task (60s interval)
    // SKIPPED in dev mode - production cleanup only
    if !dev_mode {
        let rate_limiter = state.rate_limiter.clone();
        let metrics_registry = Arc::clone(&metrics_registry);
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Rate limiter eviction",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(60));
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Rate limiter eviction received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                let evicted = rate_limiter.evict_stale();
                                if evicted > 0 {
                                    debug!(
                                        evicted_count = evicted,
                                        remaining_buckets = rate_limiter.bucket_count(),
                                        "Evicted stale rate limiter buckets"
                                    );
                                }
                                // Record metrics for dashboards
                                let metrics = rate_limiter.metrics();
                                metrics_registry
                                    .record_metric(
                                        "rate_limiter_bucket_count".to_string(),
                                        metrics.bucket_count as f64,
                                    )
                                    .await;
                            }
                        }
                    }
                },
                "Stale rate limiter buckets will not be evicted",
            )
            .is_ok()
        {
            info!("Rate limiter eviction task started (60s interval)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn inference cache cleanup task (5 minute interval)
    // Cleans up expired entries to reclaim memory
    {
        let inference_cache = state.inference_cache.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Inference cache cleanup",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Inference cache cleanup received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                let removed = inference_cache.cleanup_expired();
                                if removed > 0 {
                                    debug!(
                                        removed_count = removed,
                                        remaining_entries = inference_cache.len(),
                                        "Cleaned up expired inference cache entries"
                                    );
                                }
                            }
                        }
                    }
                },
                "Expired inference cache entries may accumulate",
            )
            .is_ok()
        {
            info!("Inference cache cleanup task started (5 minute interval)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn idempotency store cleanup task (5 minute interval)
    // Cleans up expired entries to reclaim memory (RESOURCE EXHAUSTION FIX)
    {
        let idempotency_store = state.idempotency_store.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Idempotency store cleanup",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Idempotency store cleanup received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                let removed = idempotency_store.cleanup_expired();
                                if removed > 0 {
                                    debug!(
                                        removed_count = removed,
                                        remaining_entries = idempotency_store.len(),
                                        "Cleaned up expired idempotency entries"
                                    );
                                }
                            }
                        }
                    }
                },
                "Expired idempotency entries may accumulate",
            )
            .is_ok()
        {
            info!("Idempotency store cleanup task started (5 minute interval)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn inference state tracker cleanup task (5 minute interval)
    // Cleans up terminal states older than TTL (RESOURCE EXHAUSTION FIX)
    if let Some(ref tracker) = state.inference_state_tracker {
        let tracker = Arc::clone(tracker);
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Inference state tracker cleanup",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Inference state tracker cleanup received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                let removed = tracker.cleanup_expired();
                                if removed > 0 {
                                    debug!(
                                        removed_count = removed,
                                        remaining_entries = tracker.count(),
                                        "Cleaned up expired inference state entries"
                                    );
                                }
                            }
                        }
                    }
                },
                "Expired inference state entries may accumulate",
            )
            .is_ok()
        {
            info!("Inference state tracker cleanup task started (5 minute interval)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn telemetry rate limiter cleanup task (60s interval)
    // Cleans up stale tenant rate limiter buckets (RESOURCE EXHAUSTION FIX)
    // SKIPPED in dev mode - production cleanup only
    if !dev_mode {
        let telemetry_buffer = state.telemetry_buffer.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Telemetry rate limiter cleanup",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(60));
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Telemetry rate limiter cleanup received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                let removed = telemetry_buffer.cleanup_stale_rate_limiters().await;
                                if removed > 0 {
                                    let remaining = telemetry_buffer.rate_limiter_count().await;
                                    debug!(
                                        removed_count = removed,
                                        remaining_buckets = remaining,
                                        "Cleaned up stale telemetry rate limiter buckets"
                                    );
                                }
                            }
                        }
                    }
                },
                "Stale telemetry rate limiter buckets may accumulate",
            )
            .is_ok()
        {
            info!("Telemetry rate limiter cleanup task started (60s interval)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn diagnostics writer if receiver is available
    if let Some(receiver) = diag_receiver {
        let persister = SqliteDiagPersister::new_arc(db.pool_result()?.clone());
        let run_tracker = Arc::new(RunTracker::new());

        // Get writer config from effective config or use defaults
        let writer_config = if let Some(eff_cfg) = adapteros_config::try_effective_config() {
            WriterConfig {
                batch_size: eff_cfg.diagnostics.batch_size,
                batch_timeout: Duration::from_millis(eff_cfg.diagnostics.batch_timeout_ms),
                max_events_per_run: eff_cfg.diagnostics.max_events_per_run,
                stale_batch_max_attempts: eff_cfg.diagnostics.stale_batch_max_attempts,
                stale_batch_max_age_secs: eff_cfg.diagnostics.stale_batch_max_age_secs,
            }
        } else {
            WriterConfig::default()
        };

        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let shutdown_rx = spawner.coordinator().subscribe_shutdown();

        if spawner
            .spawn_optional(
                "Diagnostics writer",
                async move {
                    let writer = DiagnosticsWriter::new(persister, writer_config, run_tracker);
                    writer.run(receiver, shutdown_rx).await;
                },
                "Diagnostic events will not be persisted",
            )
            .is_ok()
        {
            info!("Diagnostics writer task started");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // PRD-4.8: Spawn stale task monitor (60s interval, 5min threshold)
    // This monitors all other background tasks for health
    {
        let tracker = Arc::clone(&background_tasks);
        let metrics_registry = Arc::clone(&metrics_registry);
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Stale task monitor",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(60));
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                    let stale_threshold = Duration::from_secs(300); // 5 minutes

                    loop {
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Stale task monitor received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                let stale = tracker.stale_tasks(stale_threshold);
                                if !stale.is_empty() {
                                    warn!(
                                        stale_tasks = ?stale,
                                        threshold_secs = stale_threshold.as_secs(),
                                        "Background tasks appear stale (no heartbeat within threshold)"
                                    );
                                    // Record metric for alerting
                                    metrics_registry
                                        .record_metric(
                                            "background_tasks_stale_count".to_string(),
                                            stale.len() as f64,
                                        )
                                        .await;
                                } else {
                                    // Record zero when all tasks are healthy
                                    metrics_registry
                                        .record_metric(
                                            "background_tasks_stale_count".to_string(),
                                            0.0,
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                },
                "Stale task monitoring disabled",
            )
            .is_ok()
        {
            info!("Stale task monitor started (60s interval, 5min threshold)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn synthetic probe runner if enabled
    // SKIPPED in dev mode - production monitoring only
    if !dev_mode {
        let probe_config = adapteros_server_api::synthetic_probes::SyntheticProbeConfig::from_env();
        if probe_config.enabled {
            let (runner, probe_results) =
                adapteros_server_api::synthetic_probes::SyntheticProbeRunner::new(
                    state.clone(),
                    probe_config.clone(),
                );

            // Register the shared results handle globally so the health endpoint
            // can read probe results without requiring AppState mutation after boot.
            adapteros_server_api::synthetic_probes::register_global_results(probe_results);

            let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                .with_task_tracker(Arc::clone(&background_tasks));
            let shutdown_rx = spawner.coordinator().subscribe_shutdown();
            if spawner
                .spawn_optional(
                    "Synthetic probe runner",
                    async move {
                        runner.run(shutdown_rx).await;
                    },
                    "Synthetic probes disabled: adapters will not be continuously validated",
                )
                .is_ok()
            {
                info!(
                    cycle_interval_secs = probe_config.cycle_interval.as_secs(),
                    max_probes_per_cycle = probe_config.max_probes_per_cycle,
                    "Synthetic probe runner task started"
                );
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Synthetic probes disabled (set AOS_SYNTHETIC_PROBES_ENABLED=true to enable)");
        }
    }

    Ok(shutdown_coordinator)
}

#[cfg(test)]
mod tests {
    use super::{
        fallback_degraded_marker_contents, resolve_training_worker_binary_mode,
        run_training_worker_supervisor, TrainingWorkerBinaryMode, TrainingWorkerEnv,
    };
    use adapteros_config::types::PathsConfig;
    use adapteros_db::{Db, ProtectedDb};
    use adapteros_server_api::telemetry::MetricsRegistry;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::oneshot;

    struct FakeTrainingWorkerServer {
        socket_path: PathBuf,
        shutdown_tx: Option<oneshot::Sender<()>>,
        handle: tokio::task::JoinHandle<()>,
    }

    impl FakeTrainingWorkerServer {
        async fn start(socket_path: &Path) -> Self {
            if let Some(parent) = socket_path.parent() {
                std::fs::create_dir_all(parent).expect("create socket parent");
            }
            let _ = std::fs::remove_file(socket_path);

            let listener =
                tokio::net::UnixListener::bind(socket_path).expect("bind fake training worker");
            let owned_socket_path = socket_path.to_path_buf();
            let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
            let handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => break,
                        accepted = listener.accept() => {
                            let Ok((mut stream, _)) = accepted else {
                                break;
                            };
                            tokio::spawn(async move {
                                let mut buffer = [0u8; 1024];
                                let _ = stream.read(&mut buffer).await;
                                let _ = stream
                                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK")
                                    .await;
                            });
                        }
                    }
                }
                let _ = std::fs::remove_file(&owned_socket_path);
            });

            Self {
                socket_path: socket_path.to_path_buf(),
                shutdown_tx: Some(shutdown_tx),
                handle,
            }
        }

        async fn shutdown(mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
            let _ = self.handle.await;
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }

    fn test_paths_config(training_worker_bin: Option<String>) -> PathsConfig {
        PathsConfig {
            artifacts_root: "var/artifacts".to_string(),
            bundles_root: "var/bundles".to_string(),
            adapters_root: "var/adapters".to_string(),
            plan_dir: ".planning".to_string(),
            datasets_root: "var/datasets".to_string(),
            documents_root: "var/documents".to_string(),
            synthesis_model_path: None,
            training_worker_bin,
        }
    }

    fn test_worker_env(socket_path: &Path) -> TrainingWorkerEnv {
        TrainingWorkerEnv {
            socket_path: socket_path.to_path_buf(),
            database_url: "sqlite://worker-test.db".to_string(),
            datasets_root: "var/datasets".to_string(),
            artifacts_root: "var/artifacts".to_string(),
        }
    }

    async fn wait_for<F>(timeout: Duration, mut condition: F)
    where
        F: FnMut() -> bool,
    {
        tokio::time::timeout(timeout, async {
            loop {
                if condition() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("condition should become true before timeout");
    }

    #[test]
    fn missing_worker_binary_enters_fallback_mode_when_enabled() {
        let paths = test_paths_config(Some("/definitely/missing/aos-training-worker".to_string()));

        let mode = resolve_training_worker_binary_mode(&paths, true)
            .expect("missing worker should degrade when fallback is enabled");

        match mode {
            TrainingWorkerBinaryMode::Fallback(message) => {
                assert!(
                    message.contains("Training worker binary not found"),
                    "unexpected fallback message: {message}"
                );
            }
            TrainingWorkerBinaryMode::Managed(path) => {
                panic!("expected fallback mode, got managed path {path}");
            }
        }
    }

    #[test]
    fn missing_worker_binary_is_error_when_fallback_disabled() {
        let paths = test_paths_config(Some("/definitely/missing/aos-training-worker".to_string()));

        let error = resolve_training_worker_binary_mode(&paths, false)
            .expect_err("missing worker should fail when fallback is disabled");

        assert!(
            error
                .to_string()
                .contains("Training worker binary not found"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn existing_worker_binary_stays_managed() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let worker_path = temp_dir.path().join("aos-training-worker");
        std::fs::write(&worker_path, "#!/bin/sh\nexit 0\n").expect("write worker stub");

        let paths = test_paths_config(Some(worker_path.to_string_lossy().to_string()));
        let mode = resolve_training_worker_binary_mode(&paths, true)
            .expect("existing worker path should validate");

        match mode {
            TrainingWorkerBinaryMode::Managed(path) => {
                assert_eq!(path, worker_path.to_string_lossy());
            }
            TrainingWorkerBinaryMode::Fallback(message) => {
                panic!("expected managed worker path, got fallback: {message}");
            }
        }
    }

    #[tokio::test]
    async fn fallback_supervisor_waits_for_external_worker_before_reporting_ready() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let socket_path = temp_dir.path().join("worker.sock");
        let degraded_path = temp_dir.path().join("training-worker.degraded");
        let db = ProtectedDb::new(Db::new_in_memory().await.expect("in-memory db"));
        let metrics_registry = Arc::new(MetricsRegistry::new());
        let worker_env = test_worker_env(&socket_path);
        let fallback_reason = "Training worker binary not found at /missing/aos-training-worker";
        let (ready_tx, mut ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let supervisor = tokio::spawn(run_training_worker_supervisor(
            worker_env,
            degraded_path.clone(),
            metrics_registry,
            None,
            Some(fallback_reason.to_string()),
            db,
            ready_tx,
            shutdown_rx,
            Duration::from_millis(50),
        ));

        assert!(
            tokio::time::timeout(Duration::from_millis(200), &mut ready_rx)
                .await
                .is_err(),
            "ready signal should wait for a healthy external worker"
        );
        assert_eq!(
            std::fs::read_to_string(&degraded_path).expect("initial degraded marker"),
            fallback_degraded_marker_contents(fallback_reason, false),
        );

        let fake_worker = FakeTrainingWorkerServer::start(&socket_path).await;
        let ready = tokio::time::timeout(Duration::from_secs(2), ready_rx)
            .await
            .expect("ready signal should arrive after worker attach")
            .expect("ready channel should not close")
            .expect("external worker attach should report ready");
        assert_eq!(ready, ());

        wait_for(Duration::from_secs(2), || !degraded_path.exists()).await;

        fake_worker.shutdown().await;
        let _ = shutdown_tx.send(());
        supervisor.await.expect("supervisor task should join");
    }

    #[tokio::test]
    async fn fallback_supervisor_restores_degraded_marker_when_external_worker_disappears() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let socket_path = temp_dir.path().join("worker.sock");
        let degraded_path = temp_dir.path().join("training-worker.degraded");
        let db = ProtectedDb::new(Db::new_in_memory().await.expect("in-memory db"));
        let metrics_registry = Arc::new(MetricsRegistry::new());
        let worker_env = test_worker_env(&socket_path);
        let fallback_reason = "Training worker binary not found at /missing/aos-training-worker";
        let (ready_tx, ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let supervisor = tokio::spawn(run_training_worker_supervisor(
            worker_env,
            degraded_path.clone(),
            metrics_registry,
            None,
            Some(fallback_reason.to_string()),
            db,
            ready_tx,
            shutdown_rx,
            Duration::from_millis(50),
        ));

        let fake_worker = FakeTrainingWorkerServer::start(&socket_path).await;
        tokio::time::timeout(Duration::from_secs(2), ready_rx)
            .await
            .expect("ready signal should arrive after worker attach")
            .expect("ready channel should not close")
            .expect("external worker attach should report ready");
        wait_for(Duration::from_secs(2), || !degraded_path.exists()).await;

        fake_worker.shutdown().await;

        wait_for(Duration::from_secs(2), || degraded_path.exists()).await;
        let degraded_contents =
            std::fs::read_to_string(&degraded_path).expect("restored degraded marker");
        assert_eq!(
            degraded_contents,
            fallback_degraded_marker_contents(fallback_reason, true),
        );

        let _ = shutdown_tx.send(());
        supervisor.await.expect("supervisor task should join");
    }

    #[tokio::test]
    async fn fallback_supervisor_restores_degraded_marker_for_preexisting_external_worker() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let socket_path = temp_dir.path().join("worker.sock");
        let degraded_path = temp_dir.path().join("training-worker.degraded");
        let fake_worker = FakeTrainingWorkerServer::start(&socket_path).await;
        let db = ProtectedDb::new(Db::new_in_memory().await.expect("in-memory db"));
        let metrics_registry = Arc::new(MetricsRegistry::new());
        let worker_env = test_worker_env(&socket_path);
        let fallback_reason = "Training worker binary not found at /missing/aos-training-worker";
        let (ready_tx, ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let supervisor = tokio::spawn(run_training_worker_supervisor(
            worker_env,
            degraded_path.clone(),
            metrics_registry,
            None,
            Some(fallback_reason.to_string()),
            db,
            ready_tx,
            shutdown_rx,
            Duration::from_millis(50),
        ));

        tokio::time::timeout(Duration::from_secs(2), ready_rx)
            .await
            .expect("ready signal should arrive for preexisting worker")
            .expect("ready channel should not close")
            .expect("preexisting external worker should report ready");
        assert!(
            !degraded_path.exists(),
            "healthy preexisting worker should not leave degraded marker behind"
        );

        fake_worker.shutdown().await;

        wait_for(Duration::from_secs(2), || degraded_path.exists()).await;
        assert_eq!(
            std::fs::read_to_string(&degraded_path).expect("restored degraded marker"),
            fallback_degraded_marker_contents(fallback_reason, true),
        );

        let _ = shutdown_tx.send(());
        supervisor.await.expect("supervisor task should join");
    }
}
