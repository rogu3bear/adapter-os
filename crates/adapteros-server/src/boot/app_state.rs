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
use adapteros_policy::PolicyHashWatcher;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::handlers::datasets::{
    resolve_dataset_root_lenient_from_strings, ENV_DATASETS_DIR,
};
use adapteros_server_api::handlers::workspaces::reconcile_active_models;
use adapteros_server_api::pause_tracker::ServerPauseTracker;
use adapteros_server_api::runtime_mode::RuntimeMode;
use adapteros_server_api::state::BackgroundTaskTracker;
use adapteros_server_api::storage_reconciler::spawn_storage_reconciler;
use adapteros_server_api::worker_health::WorkerHealthMonitor;
use adapteros_server_api::worker_reconciler::spawn_worker_reconciler;
use adapteros_server_api::{ApiConfig, AppState};
use adapteros_telemetry::diagnostics::{DiagEnvelope, DiagnosticsConfig, DiagnosticsService};
use adapteros_telemetry::MetricsCollector;
use anyhow::Result;
use ed25519_dalek::SigningKey;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, mpsc};
use tracing::{info, instrument, warn};

use crate::boot::BackgroundTaskSpawner;
use crate::shutdown::ShutdownCoordinator;

const DEFAULT_MANIFEST_HASH: &str =
    "07578bfa5014183755ff8fb1ae2d91cf3544a28903bf5c483f38292d6a0fadf7";

#[cfg(feature = "embeddings")]
fn init_embedding_model_and_status(state: AppState) -> AppState {
    use adapteros_config::{resolve_embedding_model_path, resolve_tokenizer_path};
    use adapteros_ingest_docs::{load_tokenizer, EmbeddingModel, ProductionEmbeddingModel};
    use adapteros_server_api::state::RagStatus;

    // Tokenizer is required even for the simple embedding fallback.
    let tokenizer_path = match resolve_tokenizer_path(None) {
        Ok(p) => p,
        Err(e) => {
            let reason = format!("Tokenizer not configured: {e}");
            warn!(reason = %reason, "RAG/embeddings disabled");
            return state.with_rag_status(RagStatus::Disabled { reason });
        }
    };

    let tokenizer = match load_tokenizer(&tokenizer_path) {
        Ok(tok) => tok,
        Err(e) => {
            let reason = format!(
                "Failed to load tokenizer ({}): {e}",
                tokenizer_path.display()
            );
            warn!(reason = %reason, "RAG/embeddings disabled");
            return state.with_rag_status(RagStatus::Disabled { reason });
        }
    };

    let model_path = resolve_embedding_model_path().ok().map(|rp| rp.path);
    let model = ProductionEmbeddingModel::load(model_path.as_ref(), tokenizer);
    let model_hash = model.model_hash().to_hex();
    let dimension = model.dimension();

    info!(
        embedding_model_hash = %model_hash,
        embedding_dimension = dimension,
        tokenizer_path = %tokenizer_path.display(),
        "Embedding model initialized"
    );

    state
        .with_embedding_model(Arc::new(model))
        .with_rag_status(RagStatus::Enabled {
            model_hash,
            dimension,
        })
}

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
#[instrument(skip_all)]
pub async fn build_app_state(
    db: Db,
    api_config: Arc<RwLock<ApiConfig>>,
    server_config: Arc<RwLock<Config>>,
    federation_daemon: Arc<FederationDaemon>,
    policy_watcher: Arc<PolicyHashWatcher>,
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
) -> Result<(
    AppState,
    ShutdownCoordinator,
    Option<mpsc::Receiver<DiagEnvelope>>,
    broadcast::Receiver<()>,
)> {
    info!(target: "boot", phase = 10, name = "services", "═══ BOOT PHASE 10/12: Service Initialization ═══");

    // Resolve manifest hash BEFORE spawning background tasks.
    // std::env::set_var is not thread-safe after spawning async tasks.
    let (resolved_manifest_hash, resolved_backend_name) = {
        let computed_manifest_hash = manifest_hash.as_ref().map(|h| h.to_hex());
        let env_manifest_hash = std::env::var("AOS_MANIFEST_HASH")
            .ok()
            .filter(|s| !s.is_empty());

        let hash = match (env_manifest_hash, computed_manifest_hash) {
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
            (None, Some(computed)) => computed,
            (None, None) => {
                let is_production = api_config
                    .read()
                    .map(|c| c.server.production_mode)
                    .unwrap_or(false);

                if is_production {
                    return Err(AosError::Config(
                        "AOS_MANIFEST_HASH must be set to enable manifest-bound routing"
                            .to_string(),
                    )
                    .into());
                }

                warn!(
                    default_hash = DEFAULT_MANIFEST_HASH,
                    "AOS_MANIFEST_HASH not set and manifest hash unavailable; \
                     using default (development only)"
                );
                DEFAULT_MANIFEST_HASH.to_string()
            }
        };

        std::env::set_var("AOS_MANIFEST_HASH", &hash);
        let backend = std::env::var("AOS_MODEL_BACKEND").unwrap_or_else(|_| "mlx".to_string());
        (hash, backend)
    };

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
    let (shutdown_tx, shutdown_rx) = broadcast::channel(4);

    // Wire training service to DB + dataset storage so training uses real datasets (not synthetic).
    let (training_storage_root, training_artifacts_root) = {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        let config_root = if cfg.paths.datasets_root.is_empty() {
            None
        } else {
            Some(cfg.paths.datasets_root.clone())
        };
        let env_root = std::env::var(ENV_DATASETS_DIR).ok();
        let datasets_root = resolve_dataset_root_lenient_from_strings(env_root, config_root)
            .map_err(|e| anyhow::anyhow!(
                "Failed to resolve datasets root: {}. \
                 Please ensure AOS_DATASETS_DIR or paths.datasets_root points to a valid, persistent directory.",
                e
            ))?;
        let artifacts_root = if cfg.paths.artifacts_root.is_empty() {
            None
        } else {
            Some(PathBuf::from(cfg.paths.artifacts_root.clone()))
        };
        (datasets_root, artifacts_root)
    };
    if let Err(e) = std::fs::create_dir_all(&training_storage_root) {
        warn!(
            error = %e,
            path = %training_storage_root.display(),
            "Failed to ensure training storage root exists; training may fail"
        );
    }
    let mut training_service = TrainingService::with_db(db.clone(), training_storage_root.clone());
    if let Some(root) = training_artifacts_root {
        training_service.set_artifacts_root(root);
    }
    let training_service = Arc::new(training_service);
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
    .with_shutdown_signal(Arc::new(shutdown_tx))
    .with_federation(federation_daemon_for_state)
    .with_policy_watcher(policy_watcher)
    .with_pause_tracker(Arc::new(ServerPauseTracker::new()))
    .with_inference_state_tracker(Arc::new(
        adapteros_server_api::inference_state_tracker::InferenceStateTracker::new(),
    ));

    // Wire worker signing keypair for CP->Worker authentication
    if let Some(ref keypair) = worker_signing_keypair {
        state = state.with_worker_signing_keypair(keypair.clone());
    }

    // Apply pre-resolved manifest hash (resolved before background task spawns)
    state = state.with_manifest_info(resolved_manifest_hash.clone(), resolved_backend_name);

    // Phase 5: Advisory manifest hash validation against database
    match state.db.get_manifest_by_hash(&resolved_manifest_hash).await {
        Ok(Some(_)) => info!(
            manifest_hash = %resolved_manifest_hash,
            "Manifest hash validated against database"
        ),
        Ok(None) => warn!(
            manifest_hash = %resolved_manifest_hash,
            "Manifest hash not found in database \
             (will be created on first worker registration in dev mode)"
        ),
        Err(e) => warn!(
            manifest_hash = %resolved_manifest_hash,
            error = %e,
            "Failed to validate manifest hash against database (non-fatal)"
        ),
    }

    let plugin_registry = Arc::new(adapteros_server_api::PluginRegistry::new(state.db.clone()));
    state = state.with_plugin_registry(plugin_registry);

    // Embeddings are used by:
    // - document ingestion/indexing (for later dataset construction),
    // - RAG retrieval (vector search),
    // and can indirectly affect worker/model loading via UMA memory pressure.
    //
    // Without this, dataset upload/indexing will fail at runtime with
    // "Embedding model not configured" even when the `embeddings` feature is enabled.
    #[cfg(feature = "embeddings")]
    {
        state = init_embedding_model_and_status(state);
    }

    // Start self-hosting agent if enabled
    let _self_hosting_handle =
        adapteros_server_api::self_hosting::spawn_self_hosting_agent(state.clone());

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

    // Initialize Registry for adapter management
    {
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

        match adapteros_model_hub::registry::Registry::open(&registry_path) {
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

    // Ingest topology graph from adapters/catalog.json into the DB for routing.
    crate::topology_loader::ingest_catalog_topology(&state.db, &adapters_root)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to ingest topology catalog: {}", e))?;

    // Spawn storage reconciler in the background to detect missing/orphaned bytes.
    spawn_storage_reconciler(Arc::new(state.clone()));

    // Reconcile active workspace state to surface model/worker mismatches on startup.
    reconcile_active_models(&state).await;

    // Reconcile stale worker_model_state rows from crashed/stopped workers
    match state.db.reconcile_worker_model_states_at_startup().await {
        Ok(count) if count > 0 => info!(
            reconciled = count,
            "Reconciled stale worker_model_state rows at startup"
        ),
        Ok(_) => {}
        Err(e) => warn!(
            error = %e,
            "Failed to reconcile worker model states at startup \
             (non-fatal, table may not exist pre-migration)"
        ),
    }

    // Keep worker/workspace state reconciliation running beyond startup.
    spawn_worker_reconciler(Arc::new(state.clone()));

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

    // Initialize diagnostics service if enabled in config
    let diag_receiver = if let Some(eff_cfg) = adapteros_config::try_effective_config() {
        if eff_cfg.diagnostics.enabled {
            // Convert config DiagLevel to diagnostics DiagLevel via string representation
            let level_str = format!("{:?}", eff_cfg.diagnostics.level);
            let diag_level = adapteros_telemetry::diagnostics::DiagLevel::from_str_lossy(
                &level_str.to_lowercase(),
            );

            let diag_config = DiagnosticsConfig {
                enabled: eff_cfg.diagnostics.enabled,
                level: diag_level,
                channel_capacity: eff_cfg.diagnostics.channel_capacity,
                max_events_per_run: eff_cfg.diagnostics.max_events_per_run,
                batch_size: eff_cfg.diagnostics.batch_size,
                batch_timeout_ms: eff_cfg.diagnostics.batch_timeout_ms,
            };
            let (service, receiver) = DiagnosticsService::new(diag_config);
            info!(
                level = ?diag_level,
                channel_capacity = eff_cfg.diagnostics.channel_capacity,
                max_events_per_run = eff_cfg.diagnostics.max_events_per_run,
                batch_size = eff_cfg.diagnostics.batch_size,
                batch_timeout_ms = eff_cfg.diagnostics.batch_timeout_ms,
                "Diagnostics service initialized"
            );
            state = state.with_diagnostics_service(Arc::new(service));
            Some(receiver)
        } else {
            info!("Diagnostics service disabled in configuration");
            None
        }
    } else {
        info!("Diagnostics service disabled (no effective config)");
        None
    };

    Ok((state, shutdown_coordinator, diag_receiver, shutdown_rx))
}
