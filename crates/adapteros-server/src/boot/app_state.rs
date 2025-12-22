//! AppState construction and initialization.
//!
//! This module handles Phase 10a of the boot sequence: building the AppState
//! with all its dependencies including services, monitoring, and subsystems.

use adapteros_core::AosError;
use adapteros_db::Db;
use adapteros_deterministic_exec::global_ledger::GlobalTickLedger;
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_orchestrator::{FederationDaemon, TrainingService};
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::runtime_mode::RuntimeMode;
use adapteros_server_api::state::BackgroundTaskTracker;
use adapteros_server_api::storage_reconciler::spawn_storage_reconciler;
use adapteros_server_api::worker_health::WorkerHealthMonitor;
use adapteros_server_api::{ApiConfig, AppState};
use adapteros_telemetry::MetricsCollector;
use anyhow::Result;
use ed25519_dalek::SigningKey;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

use crate::boot::BackgroundTaskSpawner;
use crate::shutdown::ShutdownCoordinator;

const DEFAULT_MANIFEST_HASH: &str =
    "756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e";

/// Build the AppState with all its dependencies.
///
/// This function handles:
/// - WorkerHealthMonitor initialization (30s polling, latency thresholds)
/// - MetricsCollector and MetricsRegistry creation
/// - Dataset progress broadcast channel
/// - TrainingService setup
/// - AppState builder chain (.with_* methods)
/// - Manifest hash resolution (env var fallback hierarchy)
/// - Plugin registry initialization
/// - Self-hosting agent launch (if enabled)
/// - Adapter registry initialization (SQLite registry.db)
/// - Git subsystem initialization (if enabled)
#[allow(clippy::too_many_arguments)]
pub async fn build_app_state(
    db: Db,
    api_config: Arc<RwLock<ApiConfig>>,
    server_config: Arc<RwLock<Config>>,
    federation_daemon: Arc<FederationDaemon>,
    metrics_exporter: Arc<MetricsExporter>,
    uma_monitor: Arc<UmaPressureMonitor>,
    jwt_secret: Vec<u8>,
    worker_signing_keypair: Option<SigningKey>,
    mut shutdown_coordinator: ShutdownCoordinator,
    background_tasks: Arc<BackgroundTaskTracker>,
    boot_state: &BootStateManager,
    runtime_mode: RuntimeMode,
    tick_ledger: Arc<GlobalTickLedger>,
    manifest_hash: Option<adapteros_core::B3Hash>,
    strict_mode: bool,
) -> Result<(AppState, ShutdownCoordinator)> {
    info!(target: "boot", phase = 10, name = "services", "═══ BOOT PHASE 10/12: Service Initialization ═══");

    info!("Initializing worker health monitor");
    let health_monitor = Arc::new(WorkerHealthMonitor::with_defaults(db.clone()));
    {
        let monitor_clone = Arc::clone(&health_monitor);
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
            .with_task_tracker(Arc::clone(&background_tasks));
        if spawner
            .spawn_optional(
                "Worker health monitor",
                async move {
                    monitor_clone.run_polling_loop().await;
                },
                "Worker health checks unavailable",
            )
            .is_ok()
        {
            info!(
                polling_interval_secs = 30,
                latency_threshold_ms = 5000,
                consecutive_slow_count = 5,
                "Worker health monitor started"
            );
        }
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Create metrics collector and registry for AppState
    let metrics_collector = Arc::new(MetricsCollector::new(
        adapteros_telemetry::MetricsConfig::default(),
    ));
    let metrics_registry = Arc::new(adapteros_server_api::telemetry::MetricsRegistry::new());

    // Create broadcast channel for dataset progress (capacity 100)
    let (dataset_progress_tx, _) = tokio::sync::broadcast::channel(100);

    // Wire training service to DB + dataset storage so training uses real datasets (not synthetic).
    let training_storage_root = {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        PathBuf::from(&cfg.paths.datasets_root)
    };
    if let Err(e) = std::fs::create_dir_all(&training_storage_root) {
        warn!(
            error = %e,
            path = %training_storage_root.display(),
            "Failed to ensure training storage root exists; training may fail"
        );
    }
    let training_service = Arc::new(TrainingService::with_db(
        db.clone(),
        training_storage_root.clone(),
    ));
    info!(
        path = %training_storage_root.display(),
        "Training service initialized with DB-backed storage root"
    );

    let federation_daemon_for_state = federation_daemon.clone();

    let mut state = AppState::new(
        db.clone(),
        jwt_secret,
        api_config.clone(),
        Arc::clone(&metrics_exporter),
        Arc::clone(&metrics_collector),
        Arc::clone(&metrics_registry),
        uma_monitor.clone(),
    )
    .with_training_service(training_service)
    .with_dataset_progress(dataset_progress_tx)
    .with_boot_state(boot_state.clone())
    .with_runtime_mode(runtime_mode)
    .with_strict_mode(strict_mode)
    .with_tick_ledger(tick_ledger.clone())
    .with_health_monitor(health_monitor.clone())
    .with_background_task_tracker(Arc::clone(&background_tasks))
    .with_federation(federation_daemon_for_state);

    // Wire worker signing keypair for CP->Worker authentication
    if let Some(ref keypair) = worker_signing_keypair {
        state = state.with_worker_signing_keypair(keypair.clone());
    }

    // Require manifest hash to keep worker routing aligned.
    // Prefer the hash computed from the loaded manifest; fall back to env when provided.
    let computed_manifest_hash = manifest_hash.as_ref().map(|h| h.to_hex());
    let env_manifest_hash = std::env::var("AOS_MANIFEST_HASH")
        .ok()
        .filter(|s| !s.is_empty());

    let manifest_hash = match (env_manifest_hash, computed_manifest_hash) {
        (Some(env_hash), Some(computed)) => {
            if env_hash != computed {
                warn!(
                    env_manifest_hash = %env_hash,
                    computed_manifest_hash = %computed,
                    "AOS_MANIFEST_HASH differs from computed manifest hash; continuing with env value"
                );
            }
            env_hash
        }
        (Some(env_hash), None) => env_hash,
        (None, Some(computed)) => {
            // Auto-export so downstream components (and logs) see the canonical hash.
            std::env::set_var("AOS_MANIFEST_HASH", &computed);
            computed
        }
        (None, None) => {
            let is_production = api_config
                .read()
                .map(|c| c.server.production_mode)
                .unwrap_or(false);

            if is_production {
                return Err(AosError::Config(
                    "AOS_MANIFEST_HASH must be set to enable manifest-bound routing".to_string(),
                )
                .into());
            }

            warn!(
                default_hash = DEFAULT_MANIFEST_HASH,
                "AOS_MANIFEST_HASH not set and manifest hash unavailable; using default (development only)"
            );
            DEFAULT_MANIFEST_HASH.to_string()
        }
    };

    // Ensure env reflects the hash we actually use for routing.
    std::env::set_var("AOS_MANIFEST_HASH", &manifest_hash);
    let backend_name = std::env::var("AOS_MODEL_BACKEND").unwrap_or_else(|_| "mlx".to_string());
    state = state.with_manifest_info(manifest_hash, backend_name);

    state = state.with_plugin_registry(Arc::new(adapteros_server_api::PluginRegistry::new(
        db.clone(),
    )));

    // Start self-hosting agent if enabled
    let _self_hosting_handle =
        adapteros_server_api::self_hosting::spawn_self_hosting_agent(state.clone());

    // Initialize Registry for adapter management
    {
        let adapters_root: PathBuf = {
            let cfg = api_config
                .read()
                .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
            let paths = adapteros_core::paths::AdapterPaths::from_config(Some(
                cfg.paths.adapters_root.as_str(),
            ));
            let root = paths.root().to_path_buf();
            info!(path = %root.display(), "Resolved adapters root");
            root
        };

        let registry_path = adapters_root.join("registry.db");

        // Create adapters directory if it doesn't exist
        if let Some(parent) = registry_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(
                    error = %e,
                    path = %parent.display(),
                    "Failed to create adapters directory, registry disabled"
                );
            }
        }

        match adapteros_registry::Registry::open(&registry_path) {
            Ok(registry) => {
                info!(
                    path = %registry_path.display(),
                    "Registry initialized successfully"
                );
                state = state.with_registry(Arc::new(registry));
            }
            Err(e) => {
                warn!(
                    error = %e,
                    path = %registry_path.display(),
                    "Failed to initialize registry, adapter registration disabled"
                );
            }
        }
    }

    // Spawn storage reconciler in the background to detect missing/orphaned bytes.
    spawn_storage_reconciler(Arc::new(state.clone()));

    // Git subsystem initialization
    let git_enabled = server_config
        .read()
        .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
        .git
        .as_ref()
        .map(|c| c.enabled)
        .unwrap_or(false);

    if git_enabled {
        info!("Initializing Git subsystem");
        let git_config = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
            .git
            .clone()
            .unwrap_or_default();

        // Initialize Git subsystem
        let git_subsystem = adapteros_git::GitSubsystem::new(git_config.clone(), db.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize Git subsystem: {}", e))?;

        let git_arc = Arc::new(git_subsystem);

        // Create broadcast channel for file change events
        let (file_change_tx, _) = tokio::sync::broadcast::channel(1000);

        state = state.with_git(git_arc, Arc::new(file_change_tx));
        info!("Git subsystem started successfully");
    } else {
        info!("Git subsystem disabled in configuration");
    }

    Ok((state, shutdown_coordinator))
}
