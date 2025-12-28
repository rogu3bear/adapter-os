//! Background task spawning for AdapterOS control plane.
//!
//! This module contains the background task spawning logic for the boot sequence.
//! It spawns 11 different background tasks that run throughout the server lifecycle:
//!
//! 1. Status writer task (5s interval)
//! 2. KV isolation scanner task (configurable interval, default 900s)
//! 3. KV metrics alert monitor task (5s interval)
//! 4. Log cleanup task (24h interval if configured)
//! 5. TTL/expiration cleanup task (5m interval with circuit breaker)
//! 6. WAL checkpoint task (5m interval)
//! 7. DB index health monitor task
//! 8. Heartbeat recovery task (5m interval with circuit breaker)
//! 9. Upload session cleanup task (1h interval)
//! 10. Security cleanup task (1h interval)
//! 11. Telemetry bundle GC task (24h interval)
//!
//! Each task uses the `BackgroundTaskSpawner` to integrate with the shutdown coordinator
//! and task tracking system.

use crate::boot::BackgroundTaskSpawner;
use crate::db_index_monitor;
use crate::logging;
use crate::shutdown::ShutdownCoordinator;
use crate::status_writer;
use adapteros_db::kv_metrics;
use adapteros_db::Db;
use adapteros_server_api::boot_state::{BootStateManager, FailureReason};
use adapteros_server_api::kv_isolation;
use adapteros_server_api::security::{
    cleanup_expired_ip_rules, cleanup_expired_revocations, cleanup_expired_sessions,
};
use adapteros_server_api::state::BackgroundTaskTracker;
use adapteros_server_api::telemetry::MetricsRegistry;
use adapteros_server_api::AppState;
use adapteros_telemetry::AlertingEngine;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tracing::{debug, error, info, warn};

/// Spawns all background tasks for the AdapterOS control plane.
///
/// This function spawns 11 background tasks that run throughout the server lifecycle.
/// Tasks are spawned using the `BackgroundTaskSpawner` which integrates with the
/// shutdown coordinator and task tracking system.
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
pub async fn spawn_all_background_tasks(
    state: &AppState,
    db: &Db,
    mut shutdown_coordinator: ShutdownCoordinator,
    background_tasks: Arc<BackgroundTaskTracker>,
    boot_state: &BootStateManager,
    strict_mode: bool,
    metrics_registry: Arc<MetricsRegistry>,
    server_config: Arc<std::sync::RwLock<adapteros_server_api::config::Config>>,
) -> Result<ShutdownCoordinator> {
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

            warn!(
                task = %err.task_name,
                error = %err.message,
                "Critical background task failed to spawn; boot will continue in degraded state"
            );
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn KV isolation scan background task (using BackgroundTaskSpawner)
    {
        let state_clone = state.clone();
        let base_config = kv_isolation::kv_isolation_config_from_env();
        let interval_secs = std::env::var("AOS_KV_ISOLATION_SCAN_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(900);

        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if let Err(err) = spawner.spawn_with_details(
            "KV isolation scan",
            async move {
                let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
                interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        biased;
                        _ = shutdown_rx.recv() => {
                            info!("KV isolation scan received shutdown signal, exiting gracefully");
                            break;
                        }
                        _ = interval.tick() => {
                            if let Err(e) = kv_isolation::run_kv_isolation_scan(
                                &state_clone,
                                base_config.clone(),
                                "scheduled",
                            )
                            .await
                            {
                                warn!(error = %e, "KV isolation scan failed");
                            }
                        }
                    }
                }
            },
            &format!(
                "{}s interval, read-only, deterministic ordering",
                interval_secs
            ),
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

            warn!(
                task = %err.task_name,
                error = %err.message,
                "Critical background task failed to spawn; boot will continue in degraded state"
            );
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn KV metrics alert monitor (drift/fallback/error/degraded)
    {
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
    {
        let (log_dir_opt, retention_days) = {
            let cfg = server_config.read().map_err(|e| {
                error!(error = %e, "Config lock poisoned during log cleanup setup");
                anyhow::anyhow!("config lock poisoned")
            })?;
            (cfg.logging.log_dir.clone(), cfg.logging.retention_days)
        };

        if let Some(log_dir) = log_dir_opt {
            if retention_days > 0 {
                let log_dir_for_info = log_dir.clone();

                // Run cleanup on startup
                if let Err(e) = logging::cleanup_old_logs(&log_dir, retention_days).await {
                    error!(error = %e, "Failed to cleanup old logs on startup");
                }

                // Spawn daily cleanup task
                let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
                    .with_task_tracker(Arc::clone(&background_tasks));
                let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
                if spawner
                    .spawn_optional(
                        "Log cleanup",
                        async move {
                            let mut interval = tokio::time::interval(Duration::from_secs(86400)); // 24 hours
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
                                        match logging::cleanup_old_logs(&log_dir, retention_days).await {
                                            Ok(count) => {
                                                if count > 0 {
                                                    info!(
                                                        count,
                                                        retention_days,
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
                    info!(
                        retention_days,
                        log_dir = %log_dir_for_info,
                        "Log cleanup task started (daily interval)"
                    );
                }
                shutdown_coordinator = spawner.into_coordinator();
            }
        }
    }

    // Spawn TTL cleanup background task
    {
        let db_clone = db.clone();
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
    {
        let upload_manager = Arc::clone(&state.upload_session_manager);
        let interval_secs = std::env::var("AOS_UPLOAD_SESSION_CLEANUP_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600);

        if interval_secs > 0 {
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
                info!("Upload session cleanup task started ({}s interval)", interval_secs);
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Upload session cleanup disabled via AOS_UPLOAD_SESSION_CLEANUP_SECS=0");
        }
    }

    // Spawn security cleanup background task
    {
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
    {
        let telemetry_store = Arc::clone(&state.telemetry_bundle_store);
        let interval_secs = std::env::var("AOS_TELEMETRY_BUNDLE_GC_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(86400);

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
                                        store.run_gc()
                                    })
                                    .await
                                    {
                                        Ok(Ok(report)) => {
                                            if !report.evicted_bundles.is_empty() {
                                                info!(
                                                    evicted = report.evicted_bundles.len(),
                                                    bytes_freed = report.bytes_freed,
                                                    retained = report.retained_bundles,
                                                    "Telemetry bundle GC completed"
                                                );
                                            }
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
                info!("Telemetry bundle GC task started ({}s interval)", interval_secs);
            }
            shutdown_coordinator = spawner.into_coordinator();
        } else {
            info!("Telemetry bundle GC disabled via AOS_TELEMETRY_BUNDLE_GC_SECS=0");
        }
    }

    // Spawn WAL checkpoint background task
    {
        let db_clone = db.clone();
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

    // Spawn DB index health monitor + maintenance automation
    {
        let state_clone = state.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        if spawner
            .spawn_optional(
                "DB index monitor",
                async move {
                    db_index_monitor::run_db_index_monitor(state_clone).await;
                },
                "Index health monitoring disabled",
            )
            .is_ok()
        {
            info!("DB index monitor started");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn heartbeat recovery background task
    {
        let db_clone = db.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        let mut shutdown_rx = spawner.coordinator().subscribe_shutdown();
        if spawner
            .spawn_optional(
                "Heartbeat recovery",
                async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                    let mut consecutive_errors = 0u32;
                    const MAX_CONSECUTIVE_ERRORS: u32 = 5;
                    const CIRCUIT_BREAKER_PAUSE_SECS: u64 = 1800; // 30 minutes

                    loop {
                        // Check for shutdown before starting any work
                        tokio::select! {
                            biased;
                            _ = shutdown_rx.recv() => {
                                info!("Heartbeat recovery received shutdown signal, exiting gracefully");
                                break;
                            }
                            _ = interval.tick() => {
                                // Continue with recovery work
                            }
                        }

                        // Circuit breaker: pause if too many consecutive errors
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            error!(
                                consecutive_errors,
                                pause_duration_secs = CIRCUIT_BREAKER_PAUSE_SECS,
                                "Heartbeat recovery circuit breaker triggered, pausing task"
                            );
                            // Check shutdown during circuit breaker pause
                            tokio::select! {
                                biased;
                                _ = shutdown_rx.recv() => {
                                    info!("Heartbeat recovery received shutdown signal during circuit breaker pause, exiting");
                                    break;
                                }
                                _ = tokio::time::sleep(Duration::from_secs(CIRCUIT_BREAKER_PAUSE_SECS)) => {}
                            }
                            consecutive_errors = 0;
                            continue;
                        }

                        // Recover adapters that haven't sent heartbeat in 5 minutes
                        match db_clone.recover_stale_adapters(300).await {
                            Ok(recovered) => {
                                if !recovered.is_empty() {
                                    info!(
                                        count = recovered.len(),
                                        "Recovered stale adapters via heartbeat check"
                                    );
                                }
                                consecutive_errors = 0; // Reset on success
                            }
                            Err(e) => {
                                consecutive_errors += 1;
                                let backoff_secs = 2u64.pow(consecutive_errors.min(6)); // Cap at 64 seconds
                                warn!(
                                    error = %e,
                                    consecutive_errors,
                                    backoff_secs,
                                    "Failed to recover stale adapters, applying exponential backoff"
                                );
                                // Check shutdown during backoff
                                tokio::select! {
                                    biased;
                                    _ = shutdown_rx.recv() => {
                                        info!("Heartbeat recovery received shutdown signal during backoff, exiting");
                                        break;
                                    }
                                    _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                                }
                            }
                        }
                    }
                },
                "Stale adapters may not be recovered automatically",
            )
            .is_ok()
        {
            info!("Heartbeat recovery task started (5 minute interval, 300s timeout, circuit breaker enabled)");
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    Ok(shutdown_coordinator)
}
