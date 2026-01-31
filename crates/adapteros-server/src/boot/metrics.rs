//! Metrics and monitoring initialization for adapterOS.
//!
//! This module handles the initialization of:
//! - UDS (Unix Domain Socket) metrics exporter for zero-network metrics
//! - MetricsExporter for histogram-based metrics
//! - UMA (Unified Memory Architecture) pressure monitor
//! - JWT secret preparation and validation
//!
//! # Boot Phase
//!
//! This is part of Boot Phase 9c - Metrics initialization, which occurs after
//! database setup and before service initialization.

use crate::cli::normalize_jwt_mode;
use crate::shutdown::ShutdownCoordinator;
use adapteros_core::{rebase_var_path, AosError};
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::state::BackgroundTaskTracker;
use anyhow::Result;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{error, info};

/// Context containing initialized metrics components.
pub struct MetricsContext {
    /// Metrics exporter for histogram-based metrics
    pub metrics_exporter: Arc<MetricsExporter>,
    /// UMA pressure monitor for memory pressure detection
    pub uma_monitor: Arc<UmaPressureMonitor>,
    /// JWT secret (HMAC mode) or empty Vec (EdDSA mode)
    pub jwt_secret: Vec<u8>,
}

/// Initialize metrics infrastructure.
///
/// # Arguments
///
/// * `config` - Server configuration (contains metrics settings, JWT mode, etc.)
/// * `shutdown_coordinator` - Coordinator for graceful shutdown
/// * `background_tasks` - Tracker for background tasks
/// * `boot_state` - Boot state manager for recording warnings
/// * `production_mode` - Whether running in production mode
///
/// # Returns
///
/// Returns `MetricsContext` containing the initialized metrics components.
///
/// # Errors
///
/// Returns an error if:
/// - UDS metrics exporter directory creation fails
/// - MetricsExporter creation fails
/// - JWT secret validation fails (production mode with HMAC, or missing EdDSA key)
pub async fn initialize_metrics(
    config: Arc<RwLock<Config>>,
    shutdown_coordinator: &mut ShutdownCoordinator,
    background_tasks: Arc<BackgroundTaskTracker>,
    boot_state: &BootStateManager,
    _production_mode: bool,
) -> Result<MetricsContext> {
    // Initialize UDS metrics exporter (zero-network metrics per Egress Ruleset #1)
    {
        info!("Initializing UDS metrics exporter");

        let socket_path = rebase_var_path("var/run/metrics.sock");

        // Ensure directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create metrics socket directory: {}", e))?;
        }

        let mut uds_exporter = adapteros_telemetry::UdsMetricsExporter::new(socket_path.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create UDS metrics exporter: {}", e))?;

        // Register default metrics
        uds_exporter
            .register_metric(adapteros_telemetry::MetricMetadata {
                name: "adapteros_inference_requests_total".to_string(),
                help: "Total inference requests".to_string(),
                metric_type: "counter".to_string(),
                labels: std::collections::HashMap::new(),
                value: adapteros_telemetry::MetricValue::Counter(0.0),
            })
            .await;

        uds_exporter
            .register_metric(adapteros_telemetry::MetricMetadata {
                name: "adapteros_memory_usage_bytes".to_string(),
                help: "Current memory usage".to_string(),
                metric_type: "gauge".to_string(),
                labels: std::collections::HashMap::new(),
                value: adapteros_telemetry::MetricValue::Gauge(0.0),
            })
            .await;

        uds_exporter
            .register_metric(adapteros_telemetry::MetricMetadata {
                name: "adapteros_quarantine_active".to_string(),
                help: "System quarantine status (1 = active, 0 = not active)".to_string(),
                metric_type: "gauge".to_string(),
                labels: std::collections::HashMap::new(),
                value: adapteros_telemetry::MetricValue::Gauge(0.0),
            })
            .await;

        // PRD-4.8: Telemetry drop counter for backpressure observability
        uds_exporter
            .register_metric(adapteros_telemetry::MetricMetadata {
                name: "adapteros_telemetry_dropped_total".to_string(),
                help: "Total telemetry events dropped due to channel backpressure".to_string(),
                metric_type: "counter".to_string(),
                labels: std::collections::HashMap::new(),
                value: adapteros_telemetry::MetricValue::Counter(0.0),
            })
            .await;

        // KV metrics gauges (counters are exported as gauges for snapshots)
        for (name, help) in [
            (
                adapteros_db::kv_metrics::KV_ALERT_METRIC_FALLBACKS,
                "KV SQL fallback operations total",
            ),
            (
                adapteros_db::kv_metrics::KV_ALERT_METRIC_ERRORS,
                "KV backend/error total",
            ),
            (
                adapteros_db::kv_metrics::KV_ALERT_METRIC_DRIFT,
                "KV drift detections total",
            ),
            (
                adapteros_db::kv_metrics::KV_ALERT_METRIC_DEGRADATIONS,
                "KV degraded events total",
            ),
            (
                "kv.operations_total",
                "KV operations total (reads+writes+deletes+scans)",
            ),
        ] {
            uds_exporter
                .register_metric(adapteros_telemetry::MetricMetadata {
                    name: name.to_string(),
                    help: help.to_string(),
                    metric_type: "gauge".to_string(),
                    labels: std::collections::HashMap::new(),
                    value: adapteros_telemetry::MetricValue::Gauge(0.0),
                })
                .await;
        }

        // Bind and start serving in background
        match uds_exporter.bind().await {
            Ok(()) => {
                let exporter_socket_path = socket_path.clone();
                let uds_exporter = Arc::new(uds_exporter);
                let shutdown_rx = shutdown_coordinator.subscribe_shutdown();
                let uds_handle = {
                    let exporter = uds_exporter.clone();
                    tokio::spawn(async move {
                        if let Err(e) = exporter.serve(shutdown_rx).await {
                            error!(error = %e, "UDS metrics exporter error");
                        }
                    })
                };

                shutdown_coordinator.set_uds_metrics_handle(uds_handle);
                background_tasks.record_spawned("UDS metrics exporter", false);

                // Background task: publish KV metrics snapshot to UDS gauges
                {
                    let exporter = uds_exporter.clone();
                    let mut kv_shutdown_rx = shutdown_coordinator.subscribe_shutdown();
                    tokio::spawn(async move {
                        let mut ticker = tokio::time::interval(Duration::from_secs(15));
                        loop {
                            tokio::select! {
                                _ = ticker.tick() => {
                                    let snapshot = adapteros_db::kv_metrics::global_kv_metrics().snapshot();
                                    // Ignore update errors to keep loop resilient
                                    let _ = exporter
                                        .set_gauge(adapteros_db::kv_metrics::KV_ALERT_METRIC_FALLBACKS, snapshot.fallback_operations_total as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge(adapteros_db::kv_metrics::KV_ALERT_METRIC_ERRORS, snapshot.errors_total as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge(adapteros_db::kv_metrics::KV_ALERT_METRIC_DRIFT, snapshot.drift_detections_total as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge(adapteros_db::kv_metrics::KV_ALERT_METRIC_DEGRADATIONS, snapshot.degraded_events_total as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge("kv.operations_total", snapshot.operations_total as f64)
                                        .await;

                                    // Collect and publish seed metrics
                                    let seed_metrics = adapteros_deterministic_exec::seed::SeedMetrics::collect();
                                    let _ = exporter
                                        .set_gauge("adapteros_seed_collision_total", seed_metrics.collision_count as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge("adapteros_seed_propagation_failure_total", seed_metrics.propagation_failure_count as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge("adapteros_seed_active_threads", seed_metrics.active_threads as f64)
                                        .await;

                                    // Collect and publish observability event metrics
                                    let _ = exporter
                                        .set_gauge("adapteros_determinism_violation_total", adapteros_core::telemetry::determinism_violation_count() as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge("adapteros_strict_violation_total", adapteros_core::telemetry::strict_violation_count() as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge("adapteros_receipt_mismatch_total", adapteros_core::telemetry::receipt_mismatch_count() as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge("adapteros_audit_divergence_total", adapteros_core::telemetry::audit_divergence_count() as f64)
                                        .await;
                                    let _ = exporter
                                        .set_gauge("adapteros_policy_override_total", adapteros_core::telemetry::policy_override_count() as f64)
                                        .await;

                                    // PRD-4.8: Export telemetry drop counter
                                    let _ = exporter
                                        .set_gauge("adapteros_telemetry_dropped_total", adapteros_telemetry::dropped_event_count() as f64)
                                        .await;
                                }
                                _ = kv_shutdown_rx.recv() => {
                                    info!("KV metrics exporter loop shutting down");
                                    break;
                                }
                            }
                        }
                    });
                    background_tasks.record_spawned("KV metrics publisher", false);
                }

                info!(
                    "UDS metrics exporter started on {}",
                    exporter_socket_path.display()
                );
                info!(
                    "Test with: socat - UNIX-CONNECT:{}",
                    exporter_socket_path.display()
                );
            }
            Err(e) => {
                // PRD-4.8: Make UDS bind failure louder - metrics loss is a critical observability gap
                background_tasks.record_failed("UDS metrics exporter", &e.to_string(), true);
                boot_state.record_boot_warning(
                    "metrics-uds",
                    format!(
                        "UDS metrics exporter failed to bind at {}: {}",
                        socket_path.display(),
                        e
                    ),
                );
                error!(
                    error = %e,
                    socket_path = %socket_path.display(),
                    "CRITICAL: UDS metrics exporter failed to bind - metrics export disabled. \
                     Check socket permissions and ensure no other process is using the socket."
                );
            }
        }
    }

    // Create metrics exporter
    let metrics_exporter = {
        let cfg = config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        Arc::new(MetricsExporter::new(cfg.metrics.histogram_buckets.clone())?)
    };

    // Build JWT secret
    let jwt_secret = {
        let cfg = config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        let mode = cfg
            .security
            .jwt_mode
            .clone()
            .unwrap_or_else(|| normalize_jwt_mode("eddsa"));

        if mode == "hmac" || mode == "hs256" {
            #[cfg(debug_assertions)]
            {
                let dev_secret = std::env::var("AOS_DEV_JWT_SECRET").map_err(|_| {
                    anyhow::anyhow!("AOS_DEV_JWT_SECRET must be set in debug HMAC mode")
                })?;
                if dev_secret.is_empty() {
                    return Err(AosError::Config(
                        "AOS_DEV_JWT_SECRET is empty in HMAC mode".to_string(),
                    )
                    .into());
                }
                info!("Using AOS_DEV_JWT_SECRET for JWT signing (debug build only)");
                dev_secret.into_bytes()
            }
            #[cfg(not(debug_assertions))]
            {
                return Err(AosError::Config(
                    "HMAC JWT mode is not allowed in release builds".to_string(),
                )
                .into());
            }
        } else {
            // Ed25519 path: ensure key file exists (CryptoState will load)
            let keys_dir = cfg
                .security
                .key_file_path
                .clone()
                .unwrap_or_else(|| "var/keys".to_string());
            let keys_dir = rebase_var_path(keys_dir);
            let jwt_key_path = keys_dir.join("jwt_signing.key");
            if !jwt_key_path.exists() {
                return Err(AosError::Config(format!(
                    "Ed25519 JWT key missing at {}",
                    jwt_key_path.display()
                ))
                .into());
            }
            Vec::new()
        }
    };

    // UMA monitor for memory pressure detection
    // Start polling before wrapping in Arc since start_polling requires &mut self
    let mut uma_monitor = UmaPressureMonitor::new(15, None);
    uma_monitor.start_polling().await;
    let uma_monitor = Arc::new(uma_monitor);

    Ok(MetricsContext {
        metrics_exporter,
        uma_monitor,
        jwt_secret,
    })
}
