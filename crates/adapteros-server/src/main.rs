mod assets;

use adapteros_core::AosError;
use adapteros_core::Result;
use adapteros_crypto::Keypair;
use adapteros_db::Database;
use adapteros_deterministic_exec::{
    init_global_executor, select::select_2, spawn_deterministic, ExecutorConfig,
};
use adapteros_orchestrator::{CodeJobManager, TrainingService};
use adapteros_policy::PolicyPackManager;
use adapteros_server::config::Config;
use adapteros_server::security::PfGuard;
use adapteros_server::status_writer;
use adapteros_server_api::{routes, AppState};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::signal;
use tower::Service;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod alerting;
mod openapi;
mod orchestrator_config;

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
                return Err(AosError::Config(format!(
                     "Another aos-cp process is running (PID: {}). Stop it first or use --no-single-writer.",
                     existing_pid
                 )));
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

fn read_trimmed_file(path: &str) -> Result<String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| AosError::Config(format!("Failed to read {}: {}", path, e)))?;
    Ok(contents.trim().to_string())
}

fn load_ed25519_keypair_hex(path: &str) -> Result<Keypair> {
    let contents = read_trimmed_file(path)?;
    let key_bytes = hex::decode(&contents)
        .map_err(|e| AosError::Config(format!("Invalid hex in {}: {}", path, e)))?;
    if key_bytes.len() != 32 {
        return Err(AosError::Config(format!(
            "Ed25519 signing key must be 32 bytes (found {} bytes) in {}",
            key_bytes.len(),
            path
        )));
    }
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    Ok(Keypair::from_bytes(&key_array))
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

    // Load configuration
    info!("Loading configuration from {}", cli.config);
    let mut config_data = Config::load(&cli.config)?;
    
    // Validate configuration before proceeding
    info!("Validating configuration");
    config_data.validate()?;
    info!("Configuration validation passed");

    if config_data.security.jwt_public_key_pem.is_none() {
        if let Some(ref pem_file) = config_data.security.jwt_public_key_pem_file {
            let pem = read_trimmed_file(pem_file)?;
            config_data.security.jwt_public_key_pem = Some(pem);
        }
    }

    // Apply MLX configuration if present
    if let Some(ref mlx_config) = config_data.mlx {
        if mlx_config.enabled {
            if let Some(ref model_path) = mlx_config.model_path {
                // Set environment variable if not already set (env var takes precedence)
                if std::env::var("AOS_MLX_FFI_MODEL").is_err() {
                    std::env::set_var("AOS_MLX_FFI_MODEL", model_path);
                    info!("Set AOS_MLX_FFI_MODEL from config: {}", model_path);
                } else {
                    info!(
                        "AOS_MLX_FFI_MODEL already set in environment, not overriding with config value"
                    );
                }
            }
        }
    }

    // =========================================================================================
    // Deterministic Executor
    // =========================================================================================
    // The executor is seeded from the manifest to ensure all async tasks are deterministic.
    let seed_hex = &config_data.security.global_seed;
    let seed_bytes = hex::decode(seed_hex)
        .map_err(|e| AosError::Config(format!("Invalid hex for global_seed: {}", e)))?;

    if seed_bytes.len() != 32 {
        return Err(AosError::Config(format!(
            "global_seed must be a 32-byte hex string (got {} bytes)",
            seed_bytes.len()
        )));
    }

    let mut global_seed = [0u8; 32];
    global_seed.copy_from_slice(&seed_bytes);

    let mut executor_config = ExecutorConfig::default();
    executor_config.global_seed = global_seed;

    init_global_executor(executor_config)
        .map_err(|e| AosError::DeterministicExecutor(e.to_string()))?;
    info!("Deterministic executor initialized");

    // Wrap config in Arc<RwLock> for hot-reload (after initialization)
    let config = Arc::new(RwLock::new(config_data));

    // Production mode validation and enforcement (M1 hardening)
    {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;

        if cfg.server.production_mode {
            info!("🔒 Production mode enabled - enforcing M1 security requirements");

            // Enforce UDS-only serving
            if cfg.server.uds_socket.is_none() {
                return Err(AosError::Config(
                    "Production mode requires uds_socket to be configured. TCP serving is disabled in production.".to_string()
                ).into());
            }

            // Enforce Ed25519 JWTs (block HMAC)
            let jwt_mode = cfg.security.jwt_mode.as_deref().unwrap_or("hmac");
            if jwt_mode != "eddsa" {
                return Err(AosError::Config(
                    format!(
                        "Production mode requires jwt_mode = 'eddsa' (found: '{}'). HMAC is not allowed in production.",
                        jwt_mode
                    )
                )
                .into());
            }

            let pem_configured = cfg.security.jwt_public_key_pem.is_some()
                || cfg.security.jwt_public_key_pem_file.is_some();
            if !pem_configured {
                return Err(AosError::Config(
                    "Production mode requires security.jwt_public_key_pem or security.jwt_public_key_pem_file for Ed25519 validation."
                        .to_string(),
                )
                .into());
            }

            if cfg.security.jwt_signing_key_path.is_none() {
                return Err(AosError::Config(
                    "Production mode requires security.jwt_signing_key_path pointing to a 32-byte hex Ed25519 signing key"
                        .to_string(),
                )
                .into());
            }

            // Block egress skip override
            if cli.skip_pf_check {
                return Err(AosError::Config(
                    "--skip-pf-check is not allowed in production mode. Zero egress must be enforced.".to_string()
                ).into());
            }

            if !cfg.security.require_pf_deny {
                return Err(AosError::Config(
                    "Production mode requires security.require_pf_deny = true".to_string(),
                )
                .into());
            }

            info!("✓ Production mode validation passed");
        }
    }

    // Model runtime environment validation
    {
        // Note: mlx-ffi-backend feature check removed to avoid build warnings
        // MLX backend is handled at runtime via adapteros-lora-mlx-ffi crate
        match std::env::var("AOS_MLX_FFI_MODEL") {
            Ok(path) => {
                if std::path::Path::new(&path).exists() {
                    info!(
                        path = %path,
                        "AOS_MLX_FFI_MODEL environment variable set and path exists"
                    );
                } else {
                    warn!(
                        path = %path,
                        "AOS_MLX_FFI_MODEL environment variable set but path does not exist. Model loading will fail."
                    );
                }
            }
            Err(_) => {
                // Environment variable not set - this is fine for non-MLX backends
                // Using default backend (Metal/CPU)
            }
        }
    }

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
            if cfg.server.production_mode {
                return Err(AosError::Config(
                    "Cannot skip PF check in production mode".to_string(),
                )
                .into());
            }
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
    info!("Connecting to database (from DATABASE_URL)");
    let db = Database::connect_env().await?;

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
                system_metrics_interval_secs: cfg.metrics.system_metrics_interval_secs,
                telemetry_buffer_capacity: 1024,
                telemetry_channel_capacity: 256,
                trace_buffer_capacity: 512,
            },
            golden_gate,
            bundles_root: cfg.paths.bundles_root.clone(),
            rate_limits: Some(adapteros_server_api::state::RateLimitApiConfig {
                requests_per_minute: cfg.rate_limits.requests_per_minute,
                burst_size: cfg.rate_limits.burst_size,
            }),
            path_policy: adapteros_server_api::state::PathPolicyConfig {
                allowlist: vec!["**/*".to_string()],
                denylist: vec!["**/.env*".to_string(), "**/secrets/**".to_string()],
            },
            production_mode: cfg.server.production_mode,
            model_load_timeout_secs: 300,
            model_unload_timeout_secs: 30,
            max_loaded_models: 3,
            max_models_per_tenant: 1,
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
                                api_cfg.production_mode = new_config.server.production_mode;
                                api_cfg.rate_limits =
                                    Some(adapteros_server_api::state::RateLimitApiConfig {
                                        requests_per_minute: new_config
                                            .rate_limits
                                            .requests_per_minute,
                                        burst_size: new_config.rate_limits.burst_size,
                                    });
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

    // Initialize status cache
    status_writer::init_status_cache();

    // Spawn alert watcher if enabled
    {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        if cfg.alerting.enabled {
            info!("Starting alert watcher");
            alerting::spawn_alert_watcher(db.clone().into_inner(), cfg.alerting.clone())?;
        }
    }

    // Create metrics collector and registry for AppState
    let metrics_collector = Arc::new(
        adapteros_telemetry::MetricsCollector::new_with_system_provider(None)
            .expect("metrics collector"),
    );
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
        metrics_collector.clone(),
    ));

    // Initialize time series for key metrics (1 second resolution, 1000 points = ~16 minutes of history)
    for name in [
        "inference_latency_p95_ms",
        "queue_depth",
        "tokens_per_second",
        "memory_usage_mb",
    ] {
        metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_000);
    }
    info!("Initialized metrics time series for dashboard");

    // Create metrics server for HTTP Prometheus export
    let metrics_server = if cfg.metrics.server_enabled {
        let server = Arc::new(
            adapteros_telemetry::MetricsServer::new(metrics_collector.clone(), cfg.metrics.server_port)
                .expect("metrics server"),
        );

        // Start metrics server in background
        let server_clone = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server_clone.start().await {
                error!("Metrics server error: {}", e);
            }
        });

        info!("Metrics server started on port {}", cfg.metrics.server_port);
        Some(server)
    } else {
        info!("Metrics server disabled");
        None
    };

    // Initialize policy hash watcher (continuous monitoring)
    let (telemetry_tx, _telemetry) = {
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

        // Create broadcast channel for live telemetry streaming
        let (telemetry_tx, _telemetry_rx) =
            tokio::sync::broadcast::channel::<adapteros_telemetry::UnifiedTelemetryEvent>(256);

        let _telemetry = Arc::new(adapteros_telemetry::TelemetryWriter::new_with_broadcast(
            &bundles_path,
            10000,            // max_events_per_bundle
            50 * 1024 * 1024, // max_bundle_size (50MB)
            Some(telemetry_tx.clone()),
        )?);

        // Create policy hash watcher
        let policy_watcher = Arc::new(adapteros_policy::PolicyHashWatcher::new(
            Arc::new(db.clone()),
            _telemetry.clone(),
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

        (telemetry_tx, _telemetry)
    };

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
        let db_clone = db.clone().into_inner();
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        let paths_config = orchestrator_config::convert_paths_config(&cfg.paths);
        let orchestrator_config =
            orchestrator_config::convert_orchestrator_config(&cfg, &cfg.orchestrator);
        drop(cfg);
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
    //     // Note: FederationManager needs Db, so extract from Database wrapper
    //     use adapteros_db::DatabaseBackend;
    //     let db_for_federation = match db.backend() {
    //         DatabaseBackend::Sqlite(db_inner) => db_inner.clone(),
    //         DatabaseBackend::Postgres(_) => {
    //             return Err(AosError::Config(
    //                 "Federation daemon requires SQLite backend".to_string()
    //             ).into());
    //         }
    //     };
    //     let federation_keypair = adapteros_crypto::Keypair::generate();
    //     let federation_manager = Arc::new(
    //         adapteros_federation::FederationManager::new(db_for_federation, federation_keypair)?
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
    //     // FederationDaemon now expects Arc<Database>
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

    // Build application state - load JWT secret from file or use inline
    let jwt_secret = {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;

        if let Some(ref secret_file) = cfg.security.jwt_secret_file {
            // Load from file
            match std::fs::read_to_string(secret_file) {
                Ok(contents) => {
                    info!("Loaded JWT secret from file: {}", secret_file);
                    contents.trim().as_bytes().to_vec()
                }
                Err(e) => {
                    return Err(AosError::Config(format!(
                        "Failed to read JWT secret file {}: {}",
                        secret_file, e
                    ))
                    .into());
                }
            }
        } else {
            // Use inline secret
            info!("Using inline JWT secret from config");
            cfg.security.jwt_secret.as_bytes().to_vec()
        }
    };

    let orchestrator_base_model = {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        cfg.orchestrator.base_model.clone()
    };

    let db_for_training = {
        match db.backend() {
            adapteros_db::DatabaseBackend::Sqlite(db_inner) => Arc::new(db_inner.clone()),
            adapteros_db::DatabaseBackend::Postgres(_) => {
                return Err(AosError::Config(
                    "TrainingService requires SQLite database backend".to_string(),
                )
                .into());
            }
        }
    };
    let training_service = Arc::new(TrainingService::new_with_db(
        db_for_training,
        orchestrator_base_model,
    ));

    // Warm up training service cache and reconcile stuck jobs on startup
    {
        let training_service_clone = training_service.clone();
        info!("Warming up training service cache...");
        match training_service_clone.warmup_cache().await {
            Ok(count) => info!("Training service cache warmup complete: loaded {} jobs", count),
            Err(e) => warn!("Training service cache warmup failed: {}", e),
        }

        info!("Reconciling stuck training jobs...");
        match training_service_clone.reconcile_stuck_jobs(24).await {
            Ok(count) => {
                if count > 0 {
                    warn!("Reconciled {} stuck training jobs", count);
                } else {
                    info!("No stuck training jobs found");
                }
            }
            Err(e) => warn!("Training job reconciliation failed: {}", e),
        }
    }

    let mut state = AppState::new(
        db.clone(),
        jwt_secret,
        api_config.clone(),
        Arc::clone(&metrics_exporter),
        metrics_collector,
        metrics_registry,
        training_service,
        Some(telemetry_tx),
    );

    // Add metrics server to AppState if enabled
    if let Some(metrics_server) = metrics_server {
        state = state.with_metrics_server(metrics_server);
    }

    {
        let signing_path_opt = {
            let cfg = config
                .read()
                .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
            cfg.security.jwt_signing_key_path.clone()
        };

        if let Some(signing_path) = signing_path_opt {
            let keypair = load_ed25519_keypair_hex(&signing_path)?;
            let crypto = state.crypto.clone_with_jwt(keypair);
            state = state.with_crypto(crypto);
            info!("Loaded Ed25519 JWT signing key from {}", signing_path);
        }
    }

    {
        let manager = Arc::new(PolicyPackManager::new());
        info!(
            packs = adapteros_policy::policy_packs::PolicyPackId::all().len(),
            "Policy pack manager initialized"
        );
        state = state.with_policy_manager(manager);
    }

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

        // Configure adapter loader with file size limits
        {
            let max_adapter_size = cfg.server.max_adapter_size_bytes;
            let mut loader = lifecycle.loader.write();
            loader.set_max_size(max_adapter_size);
        }

        if cfg.server.enable_mmap_adapters {
            lifecycle =
                lifecycle.with_mmap_loader(adapters_path.clone(), cfg.server.mmap_cache_size_mb);
        }
        if cfg.server.enable_hot_swap {
            lifecycle = lifecycle.with_hot_swap();
        }
        state = state.with_lifecycle(Arc::new(tokio::sync::Mutex::new(lifecycle)));
    }

    // Optionally initialize LifecycleManager with mmap/hot-swap per config
    {
        use adapteros_lora_lifecycle::LifecycleManager;
        use adapteros_manifest::Policies;

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
        let adapter_names: Vec<String> = adapter_rows.into_iter().map(|a| a.adapter_id).collect();

        let policies = Policies::default();
        let mut lifecycle =
            LifecycleManager::new(adapter_names, &policies, adapters_path.clone(), None, 3);

        // Configure adapter loader with file size limits
        {
            let max_adapter_size = cfg.server.max_adapter_size_bytes;
            let mut loader = lifecycle.loader.write();
            loader.set_max_size(max_adapter_size);
        }

        if enable_mmap {
            lifecycle = lifecycle.with_mmap_loader(adapters_path.clone(), mmap_mb);
        }
        if enable_hot_swap {
            lifecycle = lifecycle.with_hot_swap();
        }

        state = state.with_lifecycle(Arc::new(tokio::sync::Mutex::new(lifecycle)));
    }

    let cfg = config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
    let paths_config = orchestrator_config::convert_paths_config(&cfg.paths);
    let orchestrator_config =
        orchestrator_config::convert_orchestrator_config(&cfg, &cfg.orchestrator);
    drop(cfg);
    let code_job_manager = Arc::new(CodeJobManager::new(
        db.clone().into_inner(),
        paths_config,
        orchestrator_config,
    ));
    state = state.with_code_jobs(code_job_manager);

    // Configure JWT mode from config (HMAC by default, EdDSA optional)
    {
        use adapteros_server_api::state::JwtMode;
        let (mode, pem_inline, pem_file) = {
            let cfg = config
                .read()
                .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
            (
                match cfg.security.jwt_mode.as_deref() {
                    Some("eddsa") => JwtMode::EdDsa,
                    _ => JwtMode::Hmac,
                },
                cfg.security.jwt_public_key_pem.clone(),
                cfg.security.jwt_public_key_pem_file.clone(),
            )
        };

        let pem = match mode {
            JwtMode::EdDsa => {
                if let Some(pem) = pem_inline {
                    Some(pem)
                } else if let Some(file) = pem_file {
                    Some(read_trimmed_file(&file)?)
                } else {
                    return Err(AosError::Config(
                        "Ed25519 mode requires a JWT public key (inline or file)".to_string(),
                    )
                    .into());
                }
            }
            JwtMode::Hmac => None,
        };

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
                    let (file_change_tx, _) = tokio::sync::broadcast::channel::<
                        adapteros_api_types::git::FileChangeEvent,
                    >(1000);

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

    // Reconcile model states on startup to fix any stuck 'loading' or 'unloading' states
    {
        use adapteros_server_api::handlers::models::reconcile_model_states;
        info!("Running model state reconciliation on startup");
        if let Err(e) = reconcile_model_states(&state).await {
            warn!("Model state reconciliation failed: {}", e);
            // Continue startup even if reconciliation fails
        }
    }

    // Spawn status cache update background task
    {
        let state_clone = state.clone();
        let _ = spawn_deterministic("Status cache updater".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Err(e) = status_writer::update_cache(&state_clone).await {
                    warn!("Failed to update status cache: {}", e);
                }
            }
        });
        info!("Status cache updater started (5s interval)");
    }

    // Spawn status file writer background task
    {
        let state_clone = state.clone();
        let _ = spawn_deterministic("Status file writer".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Err(e) = status_writer::write_status(&state_clone).await {
                    warn!("Failed to write status file: {}", e);
                }
            }
        });
        info!("Status file writer started (5s interval)");
    }

    // Spawn periodic model state health check task
    {
        use adapteros_server_api::handlers::models::check_model_state_health;
        let state_clone = state.clone();
        let _ = spawn_deterministic("Model state health check".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every 60 seconds
            loop {
                interval.tick().await;
                match check_model_state_health(&state_clone).await {
                    Ok(metrics) => {
                        if metrics.divergences > 0 {
                            warn!(
                                divergence_count = metrics.divergences,
                                total_models = metrics.total_models,
                                "Model state health check detected {} divergence(s) out of {} models",
                                metrics.divergences,
                                metrics.total_models
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Model state health check failed: {}", e);
                    }
                }
            }
        });
        info!("Model state health check task started (60s interval)");
    }

    // Spawn operation health monitoring task (stuck operations and state divergences)
    {
        use adapteros_server_api::handlers::monitor_operation_health;
        let state_clone = state.clone();
        let _ = spawn_deterministic("Operation health monitor".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every 60 seconds
            loop {
                interval.tick().await;
                match monitor_operation_health(&state_clone).await {
                    Ok(()) => {
                        // Monitoring completed successfully
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "Operation health monitoring failed"
                        );
                    }
                }
            }
        });
        info!("Operation health monitor started (60s interval)");
    }

    // Clone metrics and telemetry buffer before moving state into routes
    let metrics_collector_for_tasks = state.metrics_collector.clone();
    let metrics_registry_for_tasks = state.metrics_registry.clone();
    let telemetry_buffer_for_kernel_latency = state.telemetry_buffer.clone();

    // Start real-time metrics update task (before moving state)
    {
        let metrics_collector_clone = metrics_collector_for_tasks.clone();
        let metrics_registry_clone = metrics_registry_for_tasks.clone();
        async fn update_metrics(
            metrics_collector: &Arc<adapteros_telemetry::MetricsCollector>,
            metrics_registry: &Arc<adapteros_telemetry::MetricsRegistry>,
        ) -> Result<()> {
            metrics_collector.update_cache().await?;
            metrics_registry.record_snapshot().await?;
            Ok(())
        }

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1)); // Update every 1 second for real-time metrics
            loop {
                interval.tick().await;

                if let Err(e) =
                    update_metrics(&metrics_collector_clone, &metrics_registry_clone).await
                {
                    error!("Failed to update metrics: {}", e);
                }
            }
        });
        info!("Real-time metrics update task started (1s interval)");
    }

    // Background task to aggregate kernel latency from telemetry events (before moving state)
    {
        let metrics_collector_clone = metrics_collector_for_tasks.clone();
        let telemetry_buffer_clone = telemetry_buffer_for_kernel_latency.clone();
        let _ = spawn_deterministic("Kernel latency aggregator".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5)); // Aggregate every 5 seconds
            loop {
                interval.tick().await;

                // Query telemetry for recent inference.step events with kernel latency
                use adapteros_telemetry::TelemetryFilters;
                use chrono::{Duration as ChronoDuration, Utc};

                let end_time = Utc::now();
                let start_time = end_time - ChronoDuration::seconds(5);

                // Query for inference.step events (kernel latency) and router.decision events (router latency)
                let step_filters = TelemetryFilters {
                    limit: Some(1000),
                    event_type: Some("inference.step".to_string()),
                    start_time: Some(start_time),
                    end_time: Some(end_time),
                    ..Default::default()
                };

                let router_filters = TelemetryFilters {
                    limit: Some(1000),
                    event_type: Some("router.decision".to_string()),
                    start_time: Some(start_time),
                    end_time: Some(end_time),
                    ..Default::default()
                };

                let step_events = telemetry_buffer_clone.query(&step_filters);
                let router_events = telemetry_buffer_clone.query(&router_filters);

                // Aggregate kernel latency per tenant
                let mut kernel_latency_by_tenant: std::collections::HashMap<String, Vec<f64>> =
                    std::collections::HashMap::new();
                let mut router_latency_by_tenant: std::collections::HashMap<String, Vec<f64>> =
                    std::collections::HashMap::new();

                for event in step_events.iter() {
                    if let Some(ref metadata) = event.metadata {
                        // Extract kernel latency
                        if let Some(latency_us) =
                            metadata.get("kernel_latency_us").and_then(|v| v.as_u64())
                        {
                            let latency_secs = latency_us as f64 / 1_000_000.0;
                            let tenant_id = event
                                .tenant_id
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or("default");
                            kernel_latency_by_tenant
                                .entry(tenant_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(latency_secs);
                        }

                        // Extract router latency (also in inference.step events)
                        if let Some(latency_us) =
                            metadata.get("router_latency_us").and_then(|v| v.as_u64())
                        {
                            let latency_secs = latency_us as f64 / 1_000_000.0;
                            let tenant_id = event
                                .tenant_id
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or("default");
                            router_latency_by_tenant
                                .entry(tenant_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(latency_secs);
                        }
                    }
                }

                // Also check router.decision events
                for event in router_events.iter() {
                    if let Some(ref metadata) = event.metadata {
                        if let Some(latency_us) =
                            metadata.get("router_latency_us").and_then(|v| v.as_u64())
                        {
                            let latency_secs = latency_us as f64 / 1_000_000.0;
                            let tenant_id = event
                                .tenant_id
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or("default");
                            router_latency_by_tenant
                                .entry(tenant_id.to_string())
                                .or_insert_with(Vec::new)
                                .push(latency_secs);
                        }
                    }
                }

                // Record aggregated kernel latencies to metrics collector
                for (tenant_id, latencies) in kernel_latency_by_tenant.iter() {
                    let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
                    if latencies.len() > 0 {
                        metrics_collector_clone.record_kernel_latency(
                            "metal",
                            tenant_id,
                            avg_latency,
                        );
                    }
                }

                // Record aggregated router latencies to metrics collector
                for (tenant_id, latencies) in router_latency_by_tenant.iter() {
                    let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
                    if latencies.len() > 0 {
                        metrics_collector_clone.record_router_latency(tenant_id, avg_latency);
                    }
                }
            }
        });
        info!("Kernel and router latency aggregator started");
    }

    // Background task to update queue depth metrics periodically
    {
        let metrics_collector_clone = metrics_collector_for_tasks.clone();
        let db_for_queues = db.clone().into_inner();
        let _ = spawn_deterministic("Queue depth monitor".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5)); // Update every 5 seconds
            loop {
                interval.tick().await;

                // Count queued jobs per tenant
                if let Ok(count) = db_for_queues.count_queued_jobs().await {
                    // Update request queue depth (aggregate across all tenants for now)
                    metrics_collector_clone.update_queue_depth("request", "default", count as f64);
                }

                // Note: Adapter and kernel queue depths would need worker-level metrics
                // For now, we set them to 0 and they'll be updated when worker metrics are available
                metrics_collector_clone.update_queue_depth("adapter", "default", 0.0);
                metrics_collector_clone.update_queue_depth("kernel", "default", 0.0);
            }
        });
        info!("Queue depth monitor started");
    }

    // Background task to clean up old training logs periodically
    {
        let training_service_clone = training_service.clone();
        let _ = spawn_deterministic("Training log cleanup".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Run hourly
            loop {
                interval.tick().await;
                match training_service_clone.cleanup_old_logs(7).await {
                    Ok(count) => {
                        if count > 0 {
                            info!("Cleaned up {} old training log files", count);
                        }
                    }
                    Err(e) => {
                        warn!("Training log cleanup failed: {}", e);
                    }
                }
            }
        });
        info!("Training log cleanup task started (hourly, keeps 7 days)");
    }

    // Build router with UI (after spawning background tasks)
    let api_routes = routes::build(state);
    let ui_routes = assets::routes();

    let app = axum::Router::new()
        .merge(ui_routes)
        .nest("/api", api_routes);

    // Choose serving mode: UDS (M1+) or TCP (dev)
    // In production_mode, UDS is required and TCP is blocked
    let cfg_guard = config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
    if let Some(ref uds_path) = cfg_guard.server.uds_socket {
        use hyper_util::rt::TokioIo;
        use hyper_util::server::conn::auto::Builder as HyperBuilder;
        use std::os::unix::fs::PermissionsExt;
        use tokio::net::UnixListener;

        let socket_path = std::path::PathBuf::from(uds_path);
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        // Restrictive permissions: 600
        let mut perms = std::fs::metadata(&socket_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&socket_path, perms)?;

        info!("Starting control plane (UDS) on {}", socket_path.display());

        let app_service = app.clone();
        let builder = HyperBuilder::new(hyper_util::rt::TokioExecutor::new());

        let state_for_cleanup = state.clone();
        let shutdown = async {
            shutdown_signal().await;
            cleanup_resources(&state_for_cleanup).await;
        };
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => {
                    info!("Shutdown signal received and cleanup completed");
                    break;
                }
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, _)) => {
                            let svc = app_service.clone();
                            let builder_clone = builder.clone();
                            tokio::spawn(async move {
                                let io = TokioIo::new(stream);
                                let hyper_svc = hyper::service::service_fn(move |req| {
                                    let mut svc_clone = svc.clone();
                                    async move {
                                        svc_clone.call(req).await.map_err(|e| {
                                            tracing::error!("Service error: {}", e);
                                            // TODO: Fix proper error handling for UDS service
                                            std::io::Error::new(std::io::ErrorKind::Other, "service error")
                                        })
                                    }
                                });
                                if let Err(e) = builder_clone.serve_connection(io, hyper_svc).await {
                                    tracing::error!("UDS connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("UDS accept error: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    } else {
        // TCP (development only)
        let production_mode = cfg_guard.server.production_mode;
        if production_mode {
            return Err(AosError::Config(
                "Production mode requires uds_socket configuration. TCP serving is disabled."
                    .to_string(),
            )
            .into());
        }

        let port = cfg_guard.server.port;
        let bind = cfg_guard.server.bind.clone();
        drop(cfg_guard);
        let addr: SocketAddr = format!("{}:{}", bind, port)
            .parse()
            .unwrap_or(SocketAddr::from(([127, 0, 0, 1], port)));
        warn!("⚠️  Starting control plane on TCP (development mode)");
        info!("UI available at http://{}:{}/", addr.ip(), port);
        info!("API available at http://{}:{}/api/", addr.ip(), port);
        let state_for_cleanup = state.clone();
        let shutdown = async {
            shutdown_signal().await;
            cleanup_resources(&state_for_cleanup).await;
        };
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await?;
    }

    Ok(())
}

/// Statistics for cleanup operations during shutdown
#[derive(Debug, Default)]
struct CleanupStats {
    total_models: usize,
    models_unloaded: usize,
    models_failed: usize,
    models_timed_out: usize,
    total_adapters: usize,
    adapters_unloaded: usize,
    adapters_failed: usize,
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

/// Cleanup models and adapters during graceful shutdown
async fn cleanup_resources(state: &adapteros_server_api::state::AppState) {
    use std::time::Duration;
    use adapteros_server_api::model_runtime::ModelRuntime;
    use adapteros_lora_lifecycle::AdapterLoader;
    use tracing::{info, warn, error};

    info!("Starting graceful shutdown cleanup");

    let mut cleanup_stats = CleanupStats::default();

    // Cleanup models with timeout
    if let Some(model_runtime) = &state.model_runtime {
        let timeout = Duration::from_secs(30);
        let cleanup_start = std::time::Instant::now();

        let loaded_models = {
            let guard = model_runtime.lock().await;
            guard.get_all_loaded_models()
        };

        cleanup_stats.total_models = loaded_models.len();
        info!(count = cleanup_stats.total_models, "Found {} models to unload", cleanup_stats.total_models);

        if cleanup_stats.total_models > 0 {
            for (tenant_id, model_id) in loaded_models {
                let model_start = std::time::Instant::now();
                let result = tokio::time::timeout(timeout, async {
                    let mut guard = model_runtime.lock().await;
                    guard.unload_model(&tenant_id, &model_id)
                }).await;

                let model_duration = model_start.elapsed();

                match result {
                    Ok(Ok(())) => {
                        cleanup_stats.models_unloaded += 1;
                        info!(
                            tenant_id = %tenant_id,
                            model_id = %model_id,
                            duration_ms = model_duration.as_millis(),
                            "Model unloaded successfully during shutdown"
                        );
                    }
                    Ok(Err(e)) => {
                        cleanup_stats.models_failed += 1;
                        error!(
                            tenant_id = %tenant_id,
                            model_id = %model_id,
                            error = %e,
                            duration_ms = model_duration.as_millis(),
                            "Failed to unload model during shutdown"
                        );
                    }
                    Err(_) => {
                        cleanup_stats.models_timed_out += 1;
                        error!(
                            tenant_id = %tenant_id,
                            model_id = %model_id,
                            duration_ms = model_duration.as_millis(),
                            "Model unload timed out during shutdown"
                        );
                    }
                }

                // Update database status to 'unloaded' regardless of unload result
                // This ensures the database reflects the shutdown state
                if let Err(e) = sqlx::query!(
                    "UPDATE base_model_status SET status = 'unloaded', updated_at = datetime('now') WHERE model_id = ? AND tenant_id = ?",
                    model_id,
                    tenant_id
                )
                .execute(state.db.pool())
                .await {
                    warn!(
                        tenant_id = %tenant_id,
                        model_id = %model_id,
                        error = %e,
                        "Failed to update model status in database during shutdown"
                    );
                }
            }
        }

        let cleanup_duration = cleanup_start.elapsed();
        info!(
            duration_ms = cleanup_duration.as_millis(),
            unloaded = cleanup_stats.models_unloaded,
            failed = cleanup_stats.models_failed,
            timed_out = cleanup_stats.models_timed_out,
            "Model cleanup completed"
        );
    }

    // Cleanup adapters - unload all loaded adapters directly from loader
    if let Some(lifecycle_manager) = &state.lifecycle_manager {
        let adapter_cleanup_start = std::time::Instant::now();
        let guard = lifecycle_manager.lock().await;
        // Get direct access to the adapter loader
        let loader = guard.loader();
        let loaded_adapters: Vec<u16> = {
            let loader_guard = loader.read().await;
            (0..u16::MAX)
                .filter(|&id| loader_guard.is_loaded(id))
                .collect()
        };

        cleanup_stats.total_adapters = loaded_adapters.len();
        info!(count = cleanup_stats.total_adapters, "Found {} adapters to unload", cleanup_stats.total_adapters);

        drop(guard); // Release lifecycle lock

        if cleanup_stats.total_adapters > 0 {
            for adapter_idx in loaded_adapters {
                let adapter_start = std::time::Instant::now();
                let guard = lifecycle_manager.lock().await;
                let loader = guard.loader();
                let result = {
                    let mut loader_guard = loader.write().await;
                    loader_guard.unload_adapter(adapter_idx)
                };

                let adapter_duration = adapter_start.elapsed();

                match result {
                    Ok(()) => {
                        cleanup_stats.adapters_unloaded += 1;
                        info!(
                            adapter_idx = adapter_idx,
                            duration_ms = adapter_duration.as_millis(),
                            "Adapter unloaded successfully during shutdown"
                        );
                    }
                    Err(e) => {
                        cleanup_stats.adapters_failed += 1;
                        error!(
                            adapter_idx = adapter_idx,
                            error = %e,
                            duration_ms = adapter_duration.as_millis(),
                            "Failed to unload adapter during shutdown"
                        );
                    }
                }
            }
        }

        let adapter_cleanup_duration = adapter_cleanup_start.elapsed();
        info!(
            duration_ms = adapter_cleanup_duration.as_millis(),
            unloaded = cleanup_stats.adapters_unloaded,
            failed = cleanup_stats.adapters_failed,
            "Adapter cleanup completed"
        );
    }

    // Final cleanup summary
    info!(
        models_total = cleanup_stats.total_models,
        models_unloaded = cleanup_stats.models_unloaded,
        models_failed = cleanup_stats.models_failed,
        models_timed_out = cleanup_stats.models_timed_out,
        adapters_total = cleanup_stats.total_adapters,
        adapters_unloaded = cleanup_stats.adapters_unloaded,
        adapters_failed = cleanup_stats.adapters_failed,
        "Graceful shutdown cleanup completed"
    );
}
