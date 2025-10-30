mod assets;

use adapteros_core::AosError;
use adapteros_core::init_global_executor;
use adapteros_db::Db;
use adapteros_deterministic_exec::{
    init_global_executor, select::select_2, spawn_deterministic, ExecutorConfig,
};
use adapteros_orchestrator::{CodeJobManager, TrainingService};
use adapteros_server::config::Config;
use adapteros_server::security::PfGuard;
use adapteros_server::status_writer;
use adapteros_server_api::{routes, AppState};
use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod alerting;
mod openapi;

/// PID file lock to prevent concurrent control plane instances
struct PidFileLock {
    path: PathBuf,
}

impl PidFileLock {
    fn acquire(path: Option<PathBuf>) -> Result<Self> {
        let path = path.unwrap_or_else(|| {
            // Try system location first, fallback to local if not writable
            let system_path = PathBuf::from("/var/run/aos/cp.pid");
            if let Some(parent) = system_path.parent() {
                if std::fs::create_dir_all(parent).is_ok() {
                    return system_path;
                }
            }
            PathBuf::from("var/aos-cp.pid")
        });

        // Check if another instance is running
        if path.exists() {
            let existing_pid = std::fs::read_to_string(&path)?;
            if process_exists(existing_pid.trim()) {
                return Err(anyhow::anyhow!(
                    "Another aos-cp process is running (PID: {}). Stop it first or use --no-single-writer.",
                    existing_pid
                ));
            }
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write our PID
        std::fs::write(&path, std::process::id().to_string())?;
        info!("PID lock acquired: {}", path.display());

        Ok(Self { path })
    }
}

impl Drop for PidFileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn process_exists(pid_str: &str) -> bool {
    if let Ok(pid) = pid_str.parse::<i32>() {
        unsafe { libc::kill(pid, 0) == 0 }
    } else {
        false
    }
}

#[cfg(not(unix))]
fn process_exists(_pid_str: &str) -> bool {
    // On non-Unix systems, assume process might exist
    true
}

#[derive(Parser)]
#[command(name = "aos-cp")]
#[command(about = "AdapterOS Control Plane", long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "configs/cp.toml")]
    config: String,

    /// Run migrations only and exit
    #[arg(long)]
    migrate_only: bool,

    /// Generate OpenAPI spec and exit
    #[arg(long)]
    generate_openapi: bool,

    /// Enable single-writer mode (prevents concurrent control plane instances)
    #[arg(long, default_value_t = true)]
    single_writer: bool,

    /// Path to PID file for single-writer lock
    #[arg(long)]
    pid_file: Option<PathBuf>,

    /// Skip PF/firewall egress checks (for development only)
    #[arg(long)]
    skip_pf_check: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aos_cp=info,aos_cp_api=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    // Handle OpenAPI generation
    if cli.generate_openapi {
        info!("Generating OpenAPI specification");
        openapi::generate_openapi()?;
        info!("OpenAPI spec written to openapi.json");
        return Ok(());
    }

    // Acquire PID file lock if single-writer mode enabled
    let _pid_lock = if cli.single_writer {
        Some(PidFileLock::acquire(cli.pid_file.clone())?)
    } else {
        None
    };

    // Load configuration (wrapped in Arc<RwLock> for hot-reload)
    info!("Loading configuration from {}", cli.config);
    let config = Arc::new(RwLock::new(Config::load(&cli.config)?));

    // =========================================================================================
    // Deterministic Executor
    // =========================================================================================
    // The executor is seeded from the manifest to ensure all async tasks are deterministic.
    let seed_hex = &config.security.global_seed;
    let seed_bytes = hex::decode(seed_hex).map_err(|e| {
        AosError::Config(format!("Invalid hex for global_seed: {}", e))
    })?;

    if seed_bytes.len() != 32 {
        return Err(AosError::Config(format!(
            "global_seed must be a 32-byte hex string (got {} bytes)",
            seed_bytes.len()
        )));
    }

    let mut global_seed = [0u8; 32];
    global_seed.copy_from_slice(&seed_bytes);

    let runtime = init_global_executor(global_seed)?;
    info!("Deterministic executor initialized");

    // Security preflight: ensure egress is blocked
    info!("Running security preflight checks");
    {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        if cfg.security.require_pf_deny && !cli.skip_pf_check {
            PfGuard::preflight(&cfg.security)?;
        } else if cli.skip_pf_check {
            warn!("⚠️  PF security check skipped via --skip-pf-check flag (DEVELOPMENT ONLY)");
        }
    }

    // Environment fingerprint drift detection
    info!("Verifying environment fingerprint");
    {
        use adapteros_verify::{
            get_or_create_fingerprint_keypair, DeviceFingerprint, DriftEvaluator,
        };

        let current_fp = DeviceFingerprint::capture_current()
            .map_err(|e| AosError::Validation(format!("Failed to capture fingerprint: {}", e)))?;

        let baseline_path = std::path::PathBuf::from("var/baseline_fingerprint.json");

        if baseline_path.exists() {
            // Load baseline and compare
            let keypair = get_or_create_fingerprint_keypair().map_err(|e| {
                AosError::Crypto(format!("Failed to get fingerprint keypair: {}", e))
            })?;
            // For development, create baseline from current fingerprint if signature verification fails
            let baseline = match DeviceFingerprint::load_verified(
                &baseline_path,
                &keypair.public_key(),
            ) {
                Ok(baseline) => baseline,
                Err(_) => {
                    warn!("Baseline signature verification failed, creating new baseline for development");
                    current_fp.clone()
                }
            };

            let cfg = config
                .read()
                .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;

            let evaluator = DriftEvaluator::from_policy(&cfg.policies.drift);
            let drift_report = evaluator.compare(&baseline, &current_fp).map_err(|e| {
                AosError::Validation(format!("Failed to compare fingerprints: {}", e))
            })?;

            if drift_report.should_block() {
                warn!("Critical environment drift detected, but allowing server to start for development");
                warn!("{}", drift_report.summary());
                for field_drift in &drift_report.field_drifts {
                    warn!(
                        "  {}: {} -> {}",
                        field_drift.field_name,
                        field_drift.baseline_value,
                        field_drift.current_value
                    );
                }
                // Temporarily allow server to start despite drift for development
                // return Err(AosError::PolicyViolation(
                //     "Refusing to start due to critical environment drift. Run `aosctl drift-check` for details.".to_string()
                // ).into());
            }

            if drift_report.drift_detected {
                warn!("Environment drift detected: {}", drift_report.summary());
                for field_drift in &drift_report.field_drifts {
                    warn!(
                        "  {}: {} -> {}",
                        field_drift.field_name,
                        field_drift.baseline_value,
                        field_drift.current_value
                    );
                }
            } else {
                info!("✓ No environment drift detected");
            }
        } else {
            // First run: auto-create baseline
            warn!("No baseline fingerprint found, creating initial baseline");
            let keypair = get_or_create_fingerprint_keypair().map_err(|e| {
                AosError::Crypto(format!("Failed to get fingerprint keypair: {}", e))
            })?;

            // Ensure directory exists
            if let Some(parent) = baseline_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AosError::Io(format!("Failed to create baseline directory: {}", e))
                })?;
            }

            current_fp
                .save_signed(&baseline_path, &keypair)
                .map_err(|e| AosError::Io(format!("Failed to save baseline fingerprint: {}", e)))?;
            info!("✓ Baseline fingerprint created at {:?}", baseline_path);
        }
    }

    // Connect to database
    let db_path = config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
        .db
        .path
        .clone();
    info!("Connecting to database: {}", db_path);
    let db = Db::connect(&db_path).await?;

    // Run migrations
    info!("Running database migrations");
    db.migrate().await?;

    // Seed development data
    if let Err(e) = db.seed_dev_data().await {
        warn!("Failed to seed development data: {}", e);
    }

    if cli.migrate_only {
        info!("Migrations complete, exiting");
        return Ok(());
    }

    // Create API config (subset needed by handlers) - before SIGHUP handler
    let api_config = {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        // Map CAB golden gate config if present
        let golden_gate = cfg
            .cab
            .as_ref()
            .and_then(|c| c.golden_gate.as_ref())
            .map(|gg| adapteros_server_api::state::GoldenGateConfigApi {
                enabled: gg.enabled,
                baseline: gg.baseline.clone(),
                strictness: gg.strictness,
                skip_toolchain: gg.skip_toolchain,
                skip_signature: gg.skip_signature,
                verify_device: gg.verify_device,
                bundle_path: gg.bundle_path.clone(),
            });

        Arc::new(RwLock::new(adapteros_server_api::state::ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: cfg.metrics.enabled,
                bearer_token: cfg.metrics.bearer_token.clone(),
            },
            golden_gate,
            bundles_root: cfg.paths.bundles_root.clone(),
        }))
    };

    // Setup SIGHUP handler for config reload
    #[cfg(unix)]
    {
        let config_clone = Arc::clone(&config);
        let api_config_clone = Arc::clone(&api_config);
        let config_path = cli.config.clone();
        let _ = spawn_deterministic("SIGHUP handler".to_string(), async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sig = signal(SignalKind::hangup()).expect("Failed to setup SIGHUP handler");
            loop {
                sig.recv().await;
                info!("SIGHUP received, reloading config");
                match Config::load(&config_path) {
                    Ok(new_config) => {
                        // Update full config
                        match config_clone.write() {
                            Ok(mut cfg) => {
                                cfg.rate_limits = new_config.rate_limits.clone();
                                cfg.metrics = new_config.metrics.clone();
                                cfg.alerting = new_config.alerting.clone();
                            }
                            Err(e) => {
                                error!("Config lock poisoned during reload: {}", e);
                                continue;
                            }
                        }
                        // Update API config subset
                        match api_config_clone.write() {
                            Ok(mut api_cfg) => {
                                api_cfg.metrics.enabled = new_config.metrics.enabled;
                                api_cfg.metrics.bearer_token =
                                    new_config.metrics.bearer_token.clone();
                                // Update golden gate config
                                api_cfg.golden_gate = new_config
                                    .cab
                                    .as_ref()
                                    .and_then(|c| c.golden_gate.as_ref())
                                    .map(|gg| adapteros_server_api::state::GoldenGateConfigApi {
                                        enabled: gg.enabled,
                                        baseline: gg.baseline.clone(),
                                        strictness: gg.strictness,
                                        skip_toolchain: gg.skip_toolchain,
                                        skip_signature: gg.skip_signature,
                                        verify_device: gg.verify_device,
                                        bundle_path: gg.bundle_path.clone(),
                                    });
                                api_cfg.bundles_root = new_config.paths.bundles_root.clone();
                            }
                            Err(e) => {
                                error!("API config lock poisoned during reload: {}", e);
                                continue;
                            }
                        }
                        info!("Config reloaded successfully");
                    }
                    Err(e) => error!("Failed to reload config: {}", e),
                }
            }
        });
    }

    // Initialize status writer start time
    status_writer::init_start_time();

    // Spawn alert watcher if enabled
    {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        if cfg.alerting.enabled {
            info!("Starting alert watcher");
            alerting::spawn_alert_watcher(db.clone(), cfg.alerting.clone())?;
        }
    }

    // Initialize policy hash watcher (continuous monitoring)
    {
        info!("Initializing policy hash watcher");

        // Create telemetry writer
        let bundles_path = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
            .paths
            .bundles_root
            .clone();

        std::fs::create_dir_all(&bundles_path)
            .map_err(|e| AosError::Io(format!("Failed to create bundles directory: {}", e)))?;

        let telemetry = Arc::new(adapteros_telemetry::TelemetryWriter::new(
            &bundles_path,
            10000,            // max_events_per_bundle
            50 * 1024 * 1024, // max_bundle_size (50MB)
        )?);

        // Create policy hash watcher
        let policy_watcher = Arc::new(adapteros_policy::PolicyHashWatcher::new(
            Arc::new(db.clone()),
            telemetry,
            None, // cpid - will be set per-tenant
        ));

        // Load baseline hashes from database
        if let Err(e) = policy_watcher.load_cache().await {
            warn!("Failed to load policy hash cache: {}", e);
        }

        // Start background watcher (60 second interval)
        let policy_hashes = Arc::new(RwLock::new(std::collections::HashMap::new()));
        let _watcher_handle = policy_watcher
            .clone()
            .start_background_watcher(Duration::from_secs(60), policy_hashes.clone());

        info!("Policy hash watcher started (60s interval)");
    }

    // Initialize UDS metrics exporter (zero-network metrics per Egress Ruleset #1)
    {
        info!("Initializing UDS metrics exporter");

        let socket_path = PathBuf::from("var/run/metrics.sock");

        // Ensure directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AosError::Io(format!("Failed to create metrics socket directory: {}", e))
            })?;
        }

        let mut uds_exporter = adapteros_telemetry::UdsMetricsExporter::new(socket_path.clone())?;

        // Register default metrics
        uds_exporter.register_metric(adapteros_telemetry::MetricMetadata {
            name: "adapteros_inference_requests_total".to_string(),
            help: "Total inference requests".to_string(),
            metric_type: "counter".to_string(),
            labels: std::collections::HashMap::new(),
            value: adapteros_telemetry::MetricValue::Counter(0.0),
        });

        uds_exporter.register_metric(adapteros_telemetry::MetricMetadata {
            name: "adapteros_memory_usage_bytes".to_string(),
            help: "Current memory usage".to_string(),
            metric_type: "gauge".to_string(),
            labels: std::collections::HashMap::new(),
            value: adapteros_telemetry::MetricValue::Gauge(0.0),
        });

        uds_exporter.register_metric(adapteros_telemetry::MetricMetadata {
            name: "adapteros_quarantine_active".to_string(),
            help: "System quarantine status (1 = active, 0 = not active)".to_string(),
            metric_type: "gauge".to_string(),
            labels: std::collections::HashMap::new(),
            value: adapteros_telemetry::MetricValue::Gauge(0.0),
        });

        // Bind and start serving in background
        uds_exporter.bind().await?;

        let exporter_socket_path = socket_path.clone();
        tokio::spawn(async move {
            if let Err(e) = uds_exporter.serve().await {
                error!("UDS metrics exporter error: {}", e);
            }
        });

        info!(
            "UDS metrics exporter started on {}",
            exporter_socket_path.display()
        );
        info!(
            "Test with: socat - UNIX-CONNECT:{}",
            exporter_socket_path.display()
        );
    }

    // Telemetry bundle retention GC loop (runs every 6 hours)
    {
        use adapteros_telemetry::bundle_store::{BundleStore, EvictionStrategy, RetentionPolicy};
        let cfg_guard = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        let bundles_root = cfg_guard.paths.bundles_root.clone();
        // Use defaults for now (config knobs can be added as needed)
        let keep_bundles_per_cpid: usize = 12;
        let keep_incident_bundles = true;
        let keep_promotion_bundles = true;
        drop(cfg_guard);

        let _ = spawn_deterministic("Telemetry GC".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(6 * 60 * 60));
            loop {
                interval.tick().await;
                let policy = RetentionPolicy {
                    keep_bundles_per_cpid,
                    keep_incident_bundles,
                    keep_promotion_bundles,
                    evict_strategy: EvictionStrategy::OldestFirstSafe,
                };
                match BundleStore::new(&bundles_root, policy) {
                    Ok(mut store) => match store.run_gc() {
                        Ok(report) => info!(
                            "Telemetry GC: evicted={} freed={} retained={}",
                            report.evicted_bundles.len(),
                            report.bytes_freed,
                            report.retained_bundles
                        ),
                        Err(e) => warn!("Telemetry GC run failed: {}", e),
                    },
                    Err(e) => warn!("Telemetry GC init failed: {}", e),
                }
            }
        });
        info!("Telemetry retention GC loop scheduled (6h interval)");
    }

    // Ephemeral adapter GC loop (runs every hour)
    {
        let db_clone = db.clone();
        let paths_config = config.read().unwrap().paths.clone();
        let orchestrator_config = config.read().unwrap().orchestrator.clone();
        let _ = spawn_deterministic("Ephemeral Adapter GC".to_string(), async move {
            let job_manager = CodeJobManager::new(db_clone, paths_config, orchestrator_config);
            let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
            loop {
                interval.tick().await;
                if let Err(e) = job_manager.run_ephemeral_adapter_gc().await {
                    warn!("Ephemeral adapter GC run failed: {}", e);
                }
            }
        });
        info!("Ephemeral adapter GC loop scheduled (1h interval)");
    }

    // TODO: Start Federation Daemon once dependencies are fixed
    // NOTE: Federation daemon code exists in adapteros-orchestrator/src/federation_daemon.rs
    // but cannot be started due to missing dependencies (adapteros-secd, parking_lot, etc.)
    //
    // Once fixed, uncomment this block:
    // {
    //     info!("Initializing federation daemon");
    //
    //     // Reuse telemetry and policy_watcher from above (would need to move out of scope)
    //     // Create federation manager
    //     let federation_keypair = adapteros_crypto::Keypair::generate();
    //     let federation_manager = Arc::new(
    //         adapteros_federation::FederationManager::new(db.clone(), federation_keypair)?
    //     );
    //
    //     // Create federation daemon config (5 minute interval per spec)
    //     let federation_config = adapteros_orchestrator::FederationDaemonConfig {
    //         interval_secs: 300, // 5 minutes
    //         max_hosts_per_sweep: 10,
    //         enable_quarantine: true,
    //     };
    //
    //     // Create and start daemon
    //     let federation_daemon = Arc::new(adapteros_orchestrator::FederationDaemon::new(
    //         federation_manager,
    //         policy_watcher.clone(),
    //         telemetry.clone(),
    //         Arc::new(db.clone()),
    //         federation_config,
    //     ));
    //
    //     let _federation_handle = federation_daemon.start();
    //     info!("Federation daemon started (300s interval)");
    // }

    // Create metrics exporter
    let metrics_exporter = {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        Arc::new(adapteros_metrics_exporter::MetricsExporter::new(
            cfg.metrics.histogram_buckets.clone(),
        )?)
    };

    // Build application state
    let jwt_secret = config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
        .security
        .jwt_secret
        .clone();

    let orchestrator_base_model = {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        cfg.orchestrator.base_model.clone()
    };

    let training_service = Arc::new(TrainingService::new_with_base_model(orchestrator_base_model));

    let mut state = AppState::new(
        db.clone(),
        jwt_secret.as_bytes().to_vec(),
        api_config.clone(),
        Arc::clone(&metrics_exporter),
        Arc::clone(&training_service),
    );

    // Optionally initialize LifecycleManager with mmap/hot-swap
    {
        use adapteros_lora_lifecycle::LifecycleManager;
        use adapteros_manifest::Policies;
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        let adapters_path = std::path::PathBuf::from(cfg.paths.adapters_root.clone());
        let mut lifecycle = LifecycleManager::new(
            Vec::new(),
            &Policies::default(),
            adapters_path.clone(),
            None,
            3,
        );
        if cfg.server.enable_mmap_adapters {
            lifecycle = lifecycle.with_mmap_loader(adapters_path.clone(), cfg.server.mmap_cache_size_mb);
        }
        if cfg.server.enable_hot_swap {
            lifecycle = lifecycle.with_hot_swap();
        }
        state = state.with_lifecycle(Arc::new(tokio::sync::Mutex::new(lifecycle)));
    }

    // Optionally initialize LifecycleManager with mmap/hot-swap per config
    {
        use adapteros_manifest::Policies;
        use adapteros_lora_lifecycle::LifecycleManager;

        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        let adapters_path = std::path::PathBuf::from(&cfg.paths.adapters_root);
        let enable_mmap = cfg.server.enable_mmap_adapters;
        let mmap_mb = cfg.server.mmap_cache_size_mb;
        let enable_hot_swap = cfg.server.enable_hot_swap;
        drop(cfg);

        // Collect adapter names from DB
        let adapter_rows = db.list_adapters().await.unwrap_or_default();
        let adapter_names: Vec<String> = adapter_rows
            .into_iter()
            .map(|a| a.adapter_id)
            .collect();

        let policies = Policies::default();
        let mut lifecycle = LifecycleManager::new(
            adapter_names,
            &policies,
            adapters_path.clone(),
            None,
            3,
        );
        if enable_mmap {
            lifecycle = lifecycle.with_mmap_loader(adapters_path.clone(), mmap_mb);
        }
        if enable_hot_swap {
            lifecycle = lifecycle.with_hot_swap();
        }

        state = state.with_lifecycle(Arc::new(tokio::sync::Mutex::new(lifecycle)));
    }

    let paths_config = config.read().unwrap().paths.clone();
    let orchestrator_config = config.read().unwrap().orchestrator.clone();
    let code_job_manager = Arc::new(CodeJobManager::new(db.clone(), paths_config, orchestrator_config));
    state = state.with_code_jobs(code_job_manager);

    // Configure JWT mode from config (HMAC by default, EdDSA optional)
    {
        use adapteros_server_api::state::JwtMode;
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        let mode = match cfg.security.jwt_mode.as_deref() {
            Some("eddsa") => JwtMode::EdDsa,
            _ => JwtMode::Hmac,
        };
        let pem = cfg.security.jwt_public_key_pem.clone();
        state.set_jwt_mode(mode, pem);
    }

    // Initialize Git subsystem (optional, only if enabled in config)
    let git_enabled = config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
        .git
        .as_ref()
        .map(|c| c.enabled)
        .unwrap_or(false);

    if git_enabled {
        info!("Initializing Git subsystem");
        let git_config = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
            .git
            .clone()
            .unwrap_or_default();

        match adapteros_git::GitSubsystem::new(git_config, db.clone()).await {
            Ok(mut git_subsystem) => {
                // Start git subsystem
                if let Err(e) = git_subsystem.start().await {
                    error!("Failed to start Git subsystem: {}", e);
                } else {
                    // Create broadcast channel for file change events
                    let (file_change_tx, _) = tokio::sync::broadcast::channel(1000);

                    state = state.with_git(Arc::new(git_subsystem), Arc::new(file_change_tx));
                    info!("Git subsystem started successfully");
                }
            }
            Err(e) => {
                error!("Failed to initialize Git subsystem: {}", e);
            }
        }
    } else {
        info!("Git subsystem disabled in configuration");
    }

    // Spawn status writer background task
    {
        let state_clone = state.clone();
        let _ = spawn_deterministic("Status writer".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Err(e) = status_writer::write_status(&state_clone).await {
                    warn!("Failed to write status: {}", e);
                }
            }
        });
        info!("Status writer started (5s interval)");
    }

    // Build router with UI
    let api_routes = routes::build(state);
    let ui_routes = assets::routes();

    let app = axum::Router::new()
        .merge(ui_routes)
        .nest("/api", api_routes);

    // Bind and serve
    let port = config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
        .server
        .port;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("Starting control plane on {}", addr);
    info!("UI available at http://127.0.0.1:{}/", port);
    info!("API available at http://127.0.0.1:{}/api/", port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // Use deterministic select instead of tokio::select!
    // Left (ctrl_c) has priority over Right (terminate)
    let _ = select_2(ctrl_c, terminate).await;

    info!("Shutdown signal received");
}
