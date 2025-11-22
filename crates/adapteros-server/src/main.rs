mod assets;
mod plugin_registry;

use adapteros_core::index_snapshot::IndexSnapshot;
use adapteros_core::{derive_seed, AosError, B3Hash, PluginConfig};
use adapteros_db::Db;
use adapteros_deterministic_exec::{
    init_global_executor, select::select_2, spawn_deterministic, ExecutorConfig,
};
use adapteros_lora_worker::UmaPressureMonitor;
use adapteros_manifest::ManifestV3;
use adapteros_server::config::Config;
use adapteros_server::security::PfGuard;
use adapteros_server::status_writer;
use adapteros_server_api::{routes, AppState};
use anyhow::Result;
use clap::Parser;
use serde_json::{json, Value};
use std::collections::HashMap;
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

    /// Path to base model manifest for executor seeding
    /// Can also be set via AOS_MANIFEST_PATH environment variable
    #[arg(
        long,
        env = "AOS_MANIFEST_PATH",
        default_value = "models/qwen2.5-7b-mlx/manifest.json"
    )]
    manifest_path: PathBuf,
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

    // Load configuration early (needed for production mode check)
    info!("Loading configuration from {}", cli.config);
    let config = Arc::new(RwLock::new(Config::load(&cli.config)?));

    // Initialize deterministic executor with manifest-derived seed
    info!("Initializing deterministic executor");

    // Load manifest for deterministic seeding
    let manifest_path = &cli.manifest_path;

    let manifest_hash = if manifest_path.exists() {
        match std::fs::read_to_string(manifest_path) {
            Ok(json) => match serde_json::from_str::<ManifestV3>(&json) {
                Ok(manifest) => {
                    // Validate manifest before using for seeding
                    if let Err(e) = manifest.validate() {
                        warn!(
                            error = %e,
                            path = %manifest_path.display(),
                            "Manifest validation failed, using default seed"
                        );
                        None
                    } else {
                        match manifest.compute_hash() {
                            Ok(hash) => {
                                info!(
                                    manifest_hash = %hash.to_hex()[..16],
                                    path = %manifest_path.display(),
                                    "Loaded and validated manifest for executor seeding"
                                );
                                Some(hash)
                            }
                            Err(e) => {
                                warn!(
                                    error = %e,
                                    path = %manifest_path.display(),
                                    "Failed to compute manifest hash, using default seed"
                                );
                                None
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        path = %manifest_path.display(),
                        "Failed to parse manifest, using default seed"
                    );
                    None
                }
            },
            Err(e) => {
                warn!(
                    error = %e,
                    path = %manifest_path.display(),
                    "Failed to read manifest, using default seed"
                );
                None
            }
        }
    } else {
        warn!(
            path = %manifest_path.display(),
            "Manifest not found, using default seed (development mode)"
        );
        None
    };

    // Production mode enforcement: require valid manifest
    {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;

        if cfg.security.require_pf_deny && manifest_hash.is_none() {
            return Err(AosError::Config(
                format!(
                    "Production mode (require_pf_deny=true) requires valid manifest for executor seeding. \
                     Manifest path: {} \n\
                     Set --manifest-path or AOS_MANIFEST_PATH environment variable, or disable production mode.",
                    manifest_path.display()
                )
            ).into());
        }
    }

    // Derive executor seed using HKDF from manifest hash
    let base_seed = manifest_hash.unwrap_or_else(|| B3Hash::hash(b"default-seed-non-production"));

    let global_seed = derive_seed(&base_seed, "executor");

    info!(
        seed_hash = %B3Hash::hash(&global_seed).to_hex()[..16],
        manifest_based = manifest_hash.is_some(),
        hkdf_label = "executor",
        "Derived deterministic executor seed"
    );

    let config = ExecutorConfig {
        global_seed,
        enable_event_logging: true,
        max_ticks_per_task: 10000,
        ..Default::default()
    };
    init_global_executor(config)?;
    info!("Deterministic executor initialized with manifest-derived seed");

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
            let baseline = DeviceFingerprint::load_verified(&baseline_path, &keypair.public_key())
                .map_err(|e| {
                    AosError::Validation(format!("Failed to load baseline fingerprint: {}", e))
                })?;

            let cfg = config
                .read()
                .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;

            let evaluator = DriftEvaluator::from_policy(&cfg.policies.drift);
            let drift_report = evaluator.compare(&baseline, &current_fp).map_err(|e| {
                AosError::Validation(format!("Failed to compare fingerprints: {}", e))
            })?;

            if drift_report.should_block() {
                error!("Critical environment drift detected!");
                error!("{}", drift_report.summary());
                for field_drift in &drift_report.field_drifts {
                    error!(
                        "  {}: {} -> {}",
                        field_drift.field_name,
                        field_drift.baseline_value,
                        field_drift.current_value
                    );
                }
                return Err(AosError::PolicyViolation(
                    "Refusing to start due to critical environment drift. Run `aosctl drift-check` for details.".to_string()
                ).into());
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

    // Audit log: Executor bootstrap event
    {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;

        let metadata = serde_json::json!({
            "manifest_path": cli.manifest_path.display().to_string(),
            "manifest_based": manifest_hash.is_some(),
            "hkdf_label": "executor",
            "production_mode": cfg.security.require_pf_deny,
            "seed_source": if manifest_hash.is_some() { "manifest" } else { "default" },
        });

        if let Err(e) = db
            .log_audit(
                "system",                 // user_id
                "system",                 // user_role
                "system",                 // tenant_id
                "executor.seed_init",     // action
                "deterministic_executor", // resource_type
                None,                     // resource_id
                "success",                // status
                None,                     // error_message
                None,                     // ip_address
                Some(&serde_json::to_string(&metadata)?),
            )
            .await
        {
            warn!("Failed to log executor bootstrap audit event: {}", e);
        } else {
            info!("Executor bootstrap event logged to audit trail");
        }
    }

    // Run migrations with Ed25519 signature verification
    info!("Running database migrations...");
    db.migrate().await?;

    // Recover from any previous crash (orphaned adapters, stale state)
    info!("Running crash recovery checks...");
    db.recover_from_crash().await?;

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
        Arc::new(RwLock::new(adapteros_server_api::state::ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: cfg.metrics.enabled,
                bearer_token: cfg.metrics.bearer_token.clone(),
                system_metrics_interval_secs: 30,
                telemetry_buffer_capacity: 10000,
                telemetry_channel_capacity: 1000,
                trace_buffer_capacity: 1000,
                server_port: 9090,
                server_enabled: true,
            },
            directory_analysis_timeout_secs: 120,
            golden_gate: None,
            bundles_root: cfg.paths.bundles_root.clone(),
            repository_paths: adapteros_server_api::state::RepositoryPathsConfig::default(),
            model_load_timeout_secs: 300,
            model_unload_timeout_secs: 60,
            operation_retry: adapteros_server_api::state::OperationRetryConfig::default(),
            security: adapteros_server_api::state::SecurityConfig::default(),
            mlx: adapteros_server_api::state::MlxConfig::default(),
            production_mode: cfg.server.production_mode,
            rate_limits: adapteros_server_api::state::RateLimitsConfig::default(),
            path_policy: adapteros_server_api::state::PathPolicyConfig::default(),
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

    // Initialize status writer uptime tracking early
    status_writer::init_uptime_tracking();

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

    // Initialize Federation Daemon
    {
        info!("Initializing federation daemon");

        // Reuse telemetry and policy_watcher from above
        let federation_keypair = adapteros_crypto::Keypair::generate();
        let federation_manager = Arc::new(
            adapteros_federation::FederationManager::new(db.clone(), federation_keypair)?
        );

        // Create federation daemon config (5 minute interval per spec)
        let federation_config = adapteros_orchestrator::FederationDaemonConfig {
            interval_secs: 300, // 5 minutes
            max_hosts_per_sweep: 10,
            enable_quarantine: true,
        };

        // Create and start daemon
        let federation_daemon = Arc::new(adapteros_orchestrator::FederationDaemon::new(
            federation_manager,
            policy_watcher.clone(),
            telemetry.clone(),
            Arc::new(db.clone()),
            federation_config,
        ));

        let _federation_handle = federation_daemon.start();
        info!("Federation daemon started (300s interval)");
    }

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

    let uma_monitor = Arc::new(UmaPressureMonitor::new(15, Some(metrics_exporter.clone())));

    // Create metrics collector and registry for AppState
    let metrics_collector = Arc::new(
        adapteros_telemetry::MetricsCollector::new()
            .map_err(|e| AosError::Config(format!("Failed to create metrics collector: {}", e)))?
    );
    let metrics_registry = Arc::new(adapteros_server_api::telemetry::MetricsRegistry::new());
    uma_monitor.start_polling().await;

    // Create broadcast channel for dataset progress (capacity 100)
    let (dataset_progress_tx, _) = tokio::sync::broadcast::channel(100);

    let mut state = AppState::new(
        db.clone(),
        jwt_secret.as_bytes().to_vec(),
        api_config.clone(),
        Arc::clone(&metrics_exporter),
        Arc::clone(&metrics_collector),
        Arc::clone(&metrics_registry),
        uma_monitor.clone(),
    )
    .with_dataset_progress(dataset_progress_tx);

    state = state.with_plugin_registry(Arc::new(plugin_registry::PluginRegistry::new(db.clone())));

    let registry = state.plugin_registry.clone();

    // Git subsystem initialization
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

        // Initialize Git subsystem
        let git_subsystem = adapteros_git::GitSubsystem::new(git_config.clone(), db.clone())
            .await
            .map_err(|e| AosError::Config(format!("Failed to initialize Git subsystem: {}", e)))?;

        let git_arc = Arc::new(git_subsystem);

        // Note: GitSubsystem doesn't implement Clone, so we skip plugin registry registration.
        // The git subsystem is managed directly via AppState.with_git() instead.

        // Create broadcast channel for file change events
        let (file_change_tx, _) = tokio::sync::broadcast::channel(1000);

        state = state.with_git(git_arc, Arc::new(file_change_tx));
        info!("Git subsystem started successfully");
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

    // Spawn TTL cleanup background task
    // Citation: Agent G Stability Reinforcement Plan - Patch 2.1
    {
        let db_clone = db.clone();
        let _ = spawn_deterministic("TTL cleanup".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;

                // Find and clean up expired adapters
                match db_clone.find_expired_adapters().await {
                    Ok(expired) => {
                        if !expired.is_empty() {
                            info!(count = expired.len(), "Found expired adapters, cleaning up");

                            for adapter in expired {
                                info!(
                                    adapter_id = %adapter.adapter_id,
                                    name = %adapter.name,
                                    expired_at = ?adapter.expires_at,
                                    "Deleting expired adapter"
                                );

                                // Delete the expired adapter
                                if let Err(e) = db_clone.delete_adapter(&adapter.id).await {
                                    warn!(
                                        adapter_id = %adapter.adapter_id,
                                        error = %e,
                                        "Failed to delete expired adapter"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "Failed to query for expired adapters"
                        );
                    }
                }

                // Also cleanup expired pins from pinned_adapters table
                if let Err(e) = db_clone.cleanup_expired_pins().await {
                    warn!(
                        error = %e,
                        "Failed to cleanup expired pins"
                    );
                }
            }
        });
        info!("TTL cleanup task started (5 minute interval)");
    }

    // Spawn heartbeat recovery background task
    // Citation: Agent G Stability Reinforcement Plan - Phase 2 Heartbeat Mechanism
    {
        let db_clone = db.clone();
        let _ = spawn_deterministic("Heartbeat recovery".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;

                // Recover adapters that haven't sent heartbeat in 5 minutes
                match db_clone.recover_stale_adapters(300).await {
                    Ok(recovered) => {
                        if !recovered.is_empty() {
                            info!(
                                count = recovered.len(),
                                "Recovered stale adapters via heartbeat check"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "Failed to recover stale adapters"
                        );
                    }
                }
            }
        });
        info!("Heartbeat recovery task started (5 minute interval, 300s timeout)");
    }

    // After DB init
    let index_rebuilder = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5min
        loop {
            interval.tick().await;
            if let Err(e) = rebuild_all_indexes(&db).await {
                warn!("Index rebuild failed: {}", e);
            }
        }
    });

    // Build router with UI
    let api_routes = routes::build(state);
    let ui_routes = assets::routes();

    let app = axum::Router::new()
        .merge(ui_routes)
        .nest("/api", api_routes);

    // Bind and serve
    let (production_mode, uds_socket, port) = {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        (cfg.server.production_mode, cfg.server.uds_socket.clone(), cfg.server.port)
    };

    // Egress policy: production_mode requires UDS-only
    if production_mode {
        let socket_path = uds_socket.ok_or_else(|| {
            AosError::PolicyViolation(
                "Egress policy violation: production_mode requires uds_socket configuration".into(),
            )
        })?;

        info!("Starting control plane on UDS: {}", socket_path);
        info!("Production mode enabled - TCP binding disabled per Egress policy");

        // Remove existing socket file if present
        let _ = std::fs::remove_file(&socket_path);

        let listener = tokio::net::UnixListener::bind(&socket_path)?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    } else {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        info!("Starting control plane on {}", addr);
        info!("UI available at http://127.0.0.1:{}/", port);
        info!("API available at http://127.0.0.1:{}/api/", port);
        warn!("Development mode: TCP binding enabled. Set production_mode=true for UDS-only");

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }

    Ok(())
}

async fn rebuild_all_indexes(db: &Db) -> Result<()> {
    let tenants = db.list_tenants().await?;
    for tenant in tenants {
        let types = vec!["adapter_graph", "stacks" /* ... */];
        for typ in types {
            let snapshot = build_index_snapshot(&tenant.id, typ, db).await?;
            let hash = snapshot.compute_hash();
            db.store_index_hash(&tenant.id, typ, &hash).await?;
            // Verify
            if !db.verify_index(&tenant.id, typ).await? {
                warn!(
                    "Index verification failed for tenant {} type {}",
                    tenant.id, typ
                );
            }
        }
    }
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
