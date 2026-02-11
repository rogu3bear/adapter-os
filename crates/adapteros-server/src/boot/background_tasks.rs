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
//! ## Production Mode Tasks (15 tasks)
//!
//! 1. Status writer task (5s interval)
//! 2. KV metrics alert monitor task (5s interval)
//! 3. Log cleanup task (24h interval if configured)
//! 4. TTL/expiration cleanup task (5m interval with circuit breaker)
//! 5. WAL checkpoint task (5m interval)
//! 6. Upload session cleanup task (1h interval)
//! 7. Security cleanup task (1h interval)
//! 8. Telemetry bundle GC task (6h interval)
//! 9. Orphaned training job cleanup task (1h interval)
//! 10. Stale worker reaper task (60s interval)
//! 11. Rate limiter eviction task (60s interval)
//! 12. Inference cache cleanup task (5m interval)
//! 13. Idempotency store cleanup task (5m interval)
//! 14. Inference state tracker cleanup task (5m interval)
//! 15. Telemetry rate limiter cleanup task (60s interval)
//!
//! Each task uses the `BackgroundTaskSpawner` to integrate with the shutdown coordinator
//! and task tracking system.

use crate::boot::BackgroundTaskSpawner;
use crate::logging;
use crate::shutdown::ShutdownCoordinator;
use crate::status_writer;
use adapteros_db::diagnostics::SqliteDiagPersister;
use adapteros_db::kv_metrics;
use adapteros_db::Db;
use adapteros_deterministic_exec::run_global_executor;
use adapteros_server_api::boot_state::{BootStateManager, FailureReason};
use adapteros_server_api::security::{
    cleanup_expired_ip_rules, cleanup_expired_revocations, cleanup_expired_sessions,
};
use adapteros_server_api::state::BackgroundTaskTracker;
use adapteros_server_api::telemetry::MetricsRegistry;
use adapteros_server_api::AppState;
use adapteros_telemetry::diagnostics::{DiagEnvelope, DiagnosticsWriter, RunTracker, WriterConfig};
use adapteros_telemetry::AlertingEngine;
use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tracing::{debug, error, info, instrument, warn};

/// Counter for orphaned training jobs that have been cleaned up.
static ORPHANED_TRAINING_JOB_CLEANED: AtomicU64 = AtomicU64::new(0);

/// Returns the count of orphaned training jobs that have been marked as failed.
pub fn orphaned_training_job_cleaned_count() -> u64 {
    ORPHANED_TRAINING_JOB_CLEANED.load(Ordering::Relaxed)
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
                    tokio::time::sleep(idle_delay).await;
                }
            });
        });
    }

    // Check if we're in dev mode - skip non-essential tasks for faster startup
    let dev_mode = adapteros_server_api::is_dev_bypass_enabled();
    if dev_mode {
        info!(
            "Dev mode enabled - spawning only essential background tasks (status writer, WAL checkpoint, TTL cleanup)"
        );
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
        let persister = SqliteDiagPersister::new_arc(db.pool().clone());
        let run_tracker = Arc::new(RunTracker::new());

        // Get writer config from effective config or use defaults
        let writer_config = if let Some(eff_cfg) = adapteros_config::try_effective_config() {
            WriterConfig {
                batch_size: eff_cfg.diagnostics.batch_size,
                batch_timeout: Duration::from_millis(eff_cfg.diagnostics.batch_timeout_ms),
                ..Default::default()
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

    Ok(shutdown_coordinator)
}
