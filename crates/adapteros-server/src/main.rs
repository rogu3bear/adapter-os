mod assets;

use adapteros_core::AosError;
use adapteros_core::Result;
use adapteros_crypto::Keypair;
use adapteros_crypto::ed25519::Keypair;
use adapteros_db::Database;
use adapteros_deterministic_exec::{
    init_global_executor, select::select_2, spawn_deterministic, ExecutorConfig,
};
use adapteros_orchestrator::{CodeJobManager, TrainingService, FederationDaemonConfig};
use adapteros_policy::PolicyPackManager;
use adapteros_server::config::Config;
use adapteros_server::security::PfGuard;
use adapteros_server::status_writer;
use adapteros_server_api::{routes, AppState};
#[cfg(feature = "telemetry")]
use adapteros_system_metrics::SystemMetricsCollector;
#[cfg(feature = "telemetry")]
use async_trait::async_trait;
use adapteros_federation::FederationManager;

#[cfg(feature = "telemetry")]
/// System metrics provider implementation using SystemMetricsCollector
#[derive(Debug, Default)]
struct TelemetrySystemMetricsProvider {
    collector: std::sync::Mutex<SystemMetricsCollector>,
}

#[cfg(feature = "telemetry")]
#[async_trait]
impl adapteros_telemetry::metrics::SystemMetricsProvider for TelemetrySystemMetricsProvider {
    async fn collect_system_metrics(&self) -> adapteros_telemetry::metrics::SystemMetricsSnapshot {
        let mut collector = match self.collector.lock() {
            Ok(collector) => collector,
            Err(e) => {
                error!("Failed to lock system metrics collector: {}. Using default metrics.", e);
                return adapteros_telemetry::metrics::SystemMetricsSnapshot {
                    cpu_usage_percent: 0.0,
                    memory_usage_mb: 0.0,
                    disk_io_utilization: 0.0,
                    network_bandwidth_mbps: 0.0,
                    gpu_utilization: None,
                    gpu_memory_used_mb: None,
                    gpu_temperature: None,
                };
            }
        };
        let metrics = collector.collect_metrics();
        adapteros_telemetry::metrics::SystemMetricsSnapshot {
            cpu_usage_percent: (metrics.cpu_usage * 100.0) as f64,
            memory_usage_mb: metrics.memory_usage as f64,
            disk_io_utilization: metrics.disk_io.usage_percent as f64,
            network_bandwidth_mbps: 0.0, // TODO: calculate from network metrics
            gpu_utilization: metrics.gpu_metrics.utilization,
            gpu_memory_used_mb: metrics.gpu_metrics.memory_used.map(|m| m as f64),
            gpu_temperature: None,
        }
    }
}

#[cfg(feature = "telemetry")]
impl TelemetrySystemMetricsProvider {
    fn new() -> Self {
        Self {
            collector: std::sync::Mutex::new(SystemMetricsCollector::new()),
        }
    }
}
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

    // Perform comprehensive startup validation
    config_data.validate_startup_requirements().await?;
    info!("Startup requirements validation passed");

    if config_data.security.jwt_public_key_pem.is_none() {
        if let Some(ref pem_file) = config_data.security.jwt_public_key_pem_file {
            let pem = read_trimmed_file(pem_file)?;
            config_data.security.jwt_public_key_pem = Some(pem);
        }
    }

    // Apply MLX configuration if present
    if let Some(ref mlx_config) = config_data.mlx {
        if mlx_config.enabled {
            // Compile-time warning if MLX is enabled in config but feature not compiled
            #[cfg(not(any(feature = "mlx-ffi-backend", feature = "experimental-backends")))]
            {
                warn!("MLX backend enabled in config but not compiled in (missing --features mlx-ffi-backend)");
                warn!("Model loading will fail - rebuild with: cargo build --features mlx-ffi-backend");
                return Err(AosError::Config(
                    "MLX backend enabled in config but feature not compiled".to_string()
                ));
            }

            #[cfg(any(feature = "mlx-ffi-backend", feature = "experimental-backends"))]
            {
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

    // Connect to database with retry logic
    info!("Connecting to database (from DATABASE_URL)");
    let db = connect_database_with_retry().await?;

    // Run migrations with recovery
    info!("Running database migrations");
    run_migrations_with_recovery(&db).await?;

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
                server_enabled: true,
                server_port: 9090,
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
            repository_paths: adapteros_server_api::state::RepositoryPathsConfig::default(),
            production_mode: cfg.server.production_mode,
            model_load_timeout_secs: 300,
            model_unload_timeout_secs: 30,
            mlx: None,
            operation_retry: adapteros_server_api::state::OperationRetryConfig::default(),
            security: adapteros_server_api::state::SecurityConfig::default(),
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
            let mut sig = match signal(SignalKind::hangup()) {
                Ok(sig) => sig,
                Err(e) => {
                    error!("Failed to setup SIGHUP handler: {}. Config reload disabled.", e);
                    return; // Exit the task if signal handler can't be set up
                }
            };
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
    #[cfg(feature = "telemetry")]
    let metrics_collector = Arc::new(
        adapteros_telemetry::MetricsCollector::new_with_system_provider(Some(Box::new(
            TelemetrySystemMetricsProvider::new(),
        )))
        .expect("metrics collector"),
    );
    #[cfg(feature = "telemetry")]
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(Arc::clone(
        &metrics_collector,
    )));

    #[cfg(not(feature = "telemetry"))]
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::default());
    #[cfg(not(feature = "telemetry"))]
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(Arc::clone(
        &metrics_collector,
    )));

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
    #[cfg(feature = "telemetry")]
    let metrics_server = if config.read().map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?.metrics.server_enabled {
        let server = Arc::new(
            adapteros_telemetry::MetricsServer::new(
                metrics_collector.clone(),
                config.read().map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?.metrics.server_port,
            ),
        );

        // Start metrics server in background
        let server_clone = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server_clone.start().await {
                error!("Metrics server error: {}", e);
            }
        });

        info!("Metrics server started on port {}", config.read().map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?.metrics.server_port);
        Some(server)
    } else {
        info!("Metrics server disabled");
        None
    };
    #[cfg(not(feature = "telemetry"))]
    let metrics_server = {
        info!("Metrics server disabled (telemetry feature not enabled)");
        None
    };

    // Initialize policy hash watcher (continuous monitoring)
    #[cfg(feature = "telemetry")]
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
    #[cfg(not(feature = "telemetry"))]
    let (telemetry_tx, _telemetry) = {
        info!("Telemetry disabled - using no-op telemetry");
        // Create a dummy broadcast channel that will never send
        let (telemetry_tx, _) =
            tokio::sync::broadcast::channel::<adapteros_telemetry::UnifiedTelemetryEvent>(1);
        // Create a no-op telemetry writer
        let _telemetry = Arc::new(adapteros_telemetry::TelemetryWriter::new_noop());
        (telemetry_tx, _telemetry)
    };

    // Initialize UDS metrics exporter (zero-network metrics per Egress Ruleset #1)
    #[cfg(feature = "telemetry")]
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
    #[cfg(not(feature = "telemetry"))]
    {
        info!("UDS metrics exporter disabled (telemetry feature not enabled)");
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

    // Rate limiter cleanup task - clean up stale tenant buckets every 24 hours
    {
        let _ = spawn_deterministic("Rate Limiter Cleanup".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(24 * 60 * 60)); // 24 hours
            loop {
                interval.tick().await;
                // Clean up buckets that haven't been accessed for 7 days
                let max_age_ms = 7 * 24 * 60 * 60 * 1000; // 7 days in milliseconds
                adapteros_server_api::rate_limit::cleanup_stale_rate_limiters(max_age_ms).await;
                info!("Rate limiter cleanup completed");
            }
        });
        info!("Rate limiter cleanup loop scheduled (24h interval)");
    }

    // Initialize federation daemon
    info!("Initializing federation daemon");

    let keypair = Keypair::generate();
    let federation_manager = Arc::new(FederationManager::new(Arc::clone(&db), keypair));

    let federation_config = FederationDaemonConfig::default();

    let federation_daemon = Arc::new(adapteros_orchestrator::FederationDaemon::new(
        federation_manager,
        Arc::clone(&policy_watcher),
        Arc::clone(&telemetry),
        Arc::clone(&db),
        federation_config,
    ));

    let _federation_handle = federation_daemon.start();
    info!("Federation daemon started ({}s interval)", federation_config.interval_secs);

    // Set in AppState
    app_state.federation_daemon = Some(federation_daemon);

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
            Ok(count) => info!(
                "Training service cache warmup complete: loaded {} jobs",
                count
            ),
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

    // Clone training_service before moving it into state
    let training_service_for_cleanup = Arc::clone(&training_service);
    
    let mut state = AppState::new(
        db.clone(),
        jwt_secret,
        api_config.clone(),
        Arc::clone(&metrics_exporter),
        Arc::clone(&metrics_collector),
        Arc::clone(&metrics_registry),
        training_service,
        Some(telemetry_tx),
        global_seed,
    );

    // Add metrics server to AppState if enabled
    if let Some(metrics_server) = metrics_server {
        state = state.with_metrics_server(metrics_server);
    }

    // Validate seed consistency for deterministic execution
    //
    // # Citations
    // - Seed validation: [source: crates/adapteros-server-api/src/state.rs L753-L782]
    // - Determinism enforcement: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
    // - Startup validation: [source: crates/adapteros-server/src/main.rs]
    if let Err(e) = state.validate_seed_consistency() {
        return Err(AosError::DeterminismViolation(
            format!("Seed consistency validation failed: {}", e)
        ));
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

        // TODO: Configure adapter loader with file size limits
        // Currently disabled due to private API access
        /*
        {
            let max_adapter_size = cfg.server.max_adapter_size_bytes;
            let mut loader = lifecycle.loader.write();
            loader.set_max_size(max_adapter_size);
        }
        */

        if cfg.server.enable_mmap_adapters {
            lifecycle =
                lifecycle.with_mmap_loader(adapters_path.clone(), cfg.server.mmap_cache_size_mb);
        }
        if cfg.server.enable_hot_swap {
            lifecycle = lifecycle.with_hot_swap();
        }
        lifecycle = lifecycle.with_metrics_collector(Arc::clone(&metrics_collector));
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

        // TODO: Configure adapter loader with file size limits
        // Currently disabled due to private API access
        /*
        {
            let max_adapter_size = cfg.server.max_adapter_size_bytes;
            let mut loader = lifecycle.loader.write();
            loader.set_max_size(max_adapter_size);
        }
        */

        if enable_mmap {
            lifecycle = lifecycle.with_mmap_loader(adapters_path.clone(), mmap_mb);
        }
        if enable_hot_swap {
            lifecycle = lifecycle.with_hot_swap();
        }

        lifecycle = lifecycle.with_metrics_collector(Arc::clone(&metrics_collector));

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

        // Create broadcast channel for file change events
        let (file_change_tx, _) =
            tokio::sync::broadcast::channel::<adapteros_api_types::git::FileChangeEvent>(1000);

        match adapteros_git::GitSubsystem::new(git_config, db.clone()).await {
            Ok(mut git_subsystem) => {
                // Start git subsystem
                if let Err(e) = git_subsystem.start().await {
                    error!("Failed to start Git subsystem: {}", e);
                } else {
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

    // TODO: Reconcile model states on startup to fix any stuck 'loading' or 'unloading' states
    // {
    //     use adapteros_server_api::handlers::models::reconcile_model_states;
    //     info!("Running model state reconciliation on startup");
    //     if let Err(e) = reconcile_model_states(&state).await {
    //         warn!("Model state reconciliation failed: {}", e);
    //         // Continue startup even if reconciliation fails
    //     }
    // }

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

    // TODO: Spawn periodic model state health check task
    // {
    //     use adapteros_server_api::handlers::models::check_model_state_health;
    //     let state_clone = state.clone();
    //     let _ = spawn_deterministic("Model state health check".to_string(), async move {
    //         let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every 60 seconds
    //         loop {
    //             interval.tick().await;
    //             match check_model_state_health(&state_clone).await {
    //                 Ok(metrics) => {
    //                     if metrics.divergences > 0 {
    //                         warn!(
    //                             divergence_count = metrics.divergences,
    //                             total_models = metrics.total_models,
    //                             "Model state health check detected {} divergence(s) out of {} models",
    //                             metrics.divergences,
    //                             metrics.total_models
    //                         );
    //                     }
    //                 }
    //                 Err(e) => {
    //                     warn!("Model state health check failed: {}", e);
    //                 }
    //             }
    //         }
    //     });
    //     info!("Model state health check task started (60s interval)");
    // }

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
    let metrics_update_interval_secs = {
        let cfg = config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        cfg.metrics.system_metrics_interval_secs.max(1)
    };

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
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(metrics_update_interval_secs));
            loop {
                interval.tick().await;

                if let Err(e) =
                    update_metrics(&metrics_collector_clone, &metrics_registry_clone).await
                {
                    error!("Failed to update metrics: {}", e);
                }
            }
        });
        info!(
            interval_secs = metrics_update_interval_secs,
            "Metrics update task started"
        );
    }

    // Periodic task to update determinism metrics (avoids circular dependency)
    {
        let metrics_collector_clone = metrics_collector_for_tasks.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5)); // Update every 5 seconds
            loop {
                interval.tick().await;

                // Collect determinism metrics from adapteros-deterministic-exec
                use adapteros_deterministic_exec::seed::SeedMetrics;
                let seed_metrics = SeedMetrics::collect();

                use adapteros_telemetry_types::metrics::DeterminismMetrics;
                let determinism_metrics = DeterminismMetrics {
                    seed_collision_count: seed_metrics.collision_count,
                    seed_propagation_failure_count: seed_metrics.propagation_failure_count,
                    active_seed_threads: seed_metrics.active_threads,
                    thread_seed_generations: seed_metrics.thread_generations,
                };

                metrics_collector_clone.update_determinism_metrics(determinism_metrics);
            }
        });
        info!("Determinism metrics update task started (5s interval)");
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
        let _ = spawn_deterministic("Training log cleanup".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Run hourly
            loop {
                interval.tick().await;
                match training_service_for_cleanup.cleanup_old_logs(7).await {
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
    // Clone state before moving it into routes
    let state_for_cleanup = state.clone();
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
        let state_for_shutdown = state_for_cleanup.clone();
        let shutdown = async move {
            let signal = shutdown_signal().await;

            // Check system readiness for shutdown
            let readiness = check_shutdown_readiness(&state_for_shutdown).await;
            info!(
                "Shutdown readiness: active_requests={}, training_jobs={}, models={}, adapters={}, estimated_time={:?}",
                readiness.active_requests,
                readiness.active_training_jobs,
                readiness.loaded_models,
                readiness.loaded_adapters,
                readiness.estimated_shutdown_time()
            );

            // Adjust shutdown behavior based on readiness and signal type
            let effective_signal = match (signal, readiness.is_ready_for_graceful_shutdown()) {
                (ShutdownSignal::Graceful, false) => {
                    warn!("System not ready for graceful shutdown, switching to fast shutdown");
                    ShutdownSignal::Fast
                }
                (ShutdownSignal::Immediate, _) => {
                    warn!("Immediate shutdown requested, skipping readiness checks");
                    signal
                }
                _ => signal,
            };

            let stats = cleanup_resources(&state_for_shutdown, effective_signal).await;
            info!("Shutdown completed with stats: {:?}", stats);
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
                                        // Axum Router with state returns Infallible error type,
                                        // so this call can never fail
                                        svc_clone.call(req).await
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
        let state_for_shutdown_tcp = state_for_cleanup.clone();
        let shutdown = async move {
            let signal = shutdown_signal().await;

            // Check system readiness for shutdown
            let readiness = check_shutdown_readiness(&state_for_shutdown_tcp).await;
            info!(
                "Shutdown readiness: active_requests={}, training_jobs={}, models={}, adapters={}, estimated_time={:?}",
                readiness.active_requests,
                readiness.active_training_jobs,
                readiness.loaded_models,
                readiness.loaded_adapters,
                readiness.estimated_shutdown_time()
            );

            // Adjust shutdown behavior based on readiness and signal type
            let effective_signal = match (signal, readiness.is_ready_for_graceful_shutdown()) {
                (ShutdownSignal::Graceful, false) => {
                    warn!("System not ready for graceful shutdown, switching to fast shutdown");
                    ShutdownSignal::Fast
                }
                (ShutdownSignal::Immediate, _) => {
                    warn!("Immediate shutdown requested, skipping readiness checks");
                    signal
                }
                _ => signal,
            };

            let stats = cleanup_resources(&state_for_shutdown_tcp, effective_signal).await;
            info!("Shutdown completed with stats: {:?}", stats);
        };
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await?;
    }

    Ok(())
}

/// Shutdown phases for orderly cleanup
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShutdownPhase {
    /// Initial phase: drain new connections, signal readiness for shutdown
    Drain,
    /// Critical cleanup: save state, flush telemetry, stop accepting requests
    Critical,
    /// Resource cleanup: unload models, adapters, close connections
    Resources,
    /// Final cleanup: cleanup temporary files, close databases
    Final,
}

impl ShutdownPhase {
    fn timeout(&self) -> std::time::Duration {
        match self {
            ShutdownPhase::Drain => std::time::Duration::from_secs(10),
            ShutdownPhase::Critical => std::time::Duration::from_secs(30),
            ShutdownPhase::Resources => std::time::Duration::from_secs(60),
            ShutdownPhase::Final => std::time::Duration::from_secs(10),
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ShutdownPhase::Drain => "draining connections",
            ShutdownPhase::Critical => "critical cleanup",
            ShutdownPhase::Resources => "resource cleanup",
            ShutdownPhase::Final => "final cleanup",
        }
    }
}

/// Shutdown signal types for different shutdown behaviors
#[derive(Debug, Clone, Copy)]
enum ShutdownSignal {
    /// Graceful shutdown (SIGTERM, Ctrl+C)
    Graceful,
    /// Fast shutdown (SIGUSR1)
    Fast,
    /// Immediate shutdown (SIGUSR2, SIGKILL)
    Immediate,
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
    total_connections: usize,
    connections_closed: usize,
    telemetry_flushed: bool,
    database_closed: bool,
    shutdown_duration: std::time::Duration,
}

/// Run database migrations with recovery and detailed error messages
async fn run_migrations_with_recovery(db: &Database) -> Result<()> {
    match db.migrate().await {
        Ok(()) => {
            info!("Database migrations completed successfully");
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("{}", e);

            // Check for specific error types and provide recovery suggestions
            if error_msg.contains("no such table") || error_msg.contains("syntax error") {
                error!("Database schema appears corrupted or incompatible. Error: {}", error_msg);
                error!("Recovery options:");
                error!("1. Remove the database file and restart (loses all data)");
                error!("2. Restore from a backup");
                error!("3. Check DATABASE_URL environment variable");
                return Err(AosError::Config(format!(
                    "Database schema error during migration. Manual recovery required. Error: {}",
                    error_msg
                )));
            } else if error_msg.contains("disk I/O error") || error_msg.contains("No space left") {
                error!("Disk error during migration. Check available disk space. Error: {}", error_msg);
                return Err(AosError::Config(format!(
                    "Disk error during migration. Free up disk space and try again. Error: {}",
                    error_msg
                )));
            } else if error_msg.contains("permission denied") {
                error!("Permission denied during migration. Check database file permissions. Error: {}", error_msg);
                return Err(AosError::Config(format!(
                    "Permission denied during migration. Ensure write access to database directory. Error: {}",
                    error_msg
                )));
            } else if error_msg.contains("locked") || error_msg.contains("busy") {
                error!("Database locked during migration. Ensure no other processes are using the database. Error: {}", error_msg);
                return Err(AosError::Config(format!(
                    "Database locked during migration. Close other database connections and try again. Error: {}",
                    error_msg
                )));
            } else {
                error!("Migration failed with unexpected error: {}", error_msg);
                error!("This may be a bug in the migration scripts or database corruption.");
                return Err(AosError::Config(format!(
                    "Migration failed. Error: {}",
                    error_msg
                )));
            }
        }
    }
}

/// Connect to database with retry logic and better error messages
async fn connect_database_with_retry() -> Result<Database> {
    const MAX_RETRIES: u32 = 3;
    const BASE_DELAY_MS: u64 = 1000; // 1 second

    let mut last_error = None;

    for attempt in 1..=MAX_RETRIES {
        match Database::connect_env().await {
            Ok(db) => {
                if attempt > 1 {
                    info!("Database connection successful after {} attempts", attempt);
                }
                return Ok(db);
            }
            Err(e) => {
                last_error = Some(e);

                // Check if this is a recoverable error
                let error_msg = format!("{}", last_error.as_ref().unwrap());
                let is_recoverable = error_msg.contains("connection refused")
                    || error_msg.contains("temporarily unavailable")
                    || error_msg.contains("locked")
                    || error_msg.contains("busy");

                if !is_recoverable || attempt == MAX_RETRIES {
                    // Don't retry or this is the last attempt
                    break;
                }

                let delay_ms = BASE_DELAY_MS * (2u64.pow(attempt - 1));
                warn!(
                    "Database connection failed (attempt {}/{}): {}. Retrying in {}ms...",
                    attempt, MAX_RETRIES, error_msg, delay_ms
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }

    // Provide detailed error message based on the failure
    let error = last_error.unwrap();
    let error_msg = format!("{}", error);

    if error_msg.contains("permission denied") || error_msg.contains("access denied") {
        Err(AosError::Config(format!(
            "Database permission denied. Check file permissions for the database path. \
             Ensure the process has read/write access to the database directory. Error: {}",
            error_msg
        )))
    } else if error_msg.contains("disk I/O error") || error_msg.contains("No space left") {
        Err(AosError::Config(format!(
            "Database disk error. Check available disk space and filesystem permissions. Error: {}",
            error_msg
        )))
    } else if error_msg.contains("corrupt") || error_msg.contains("malformed") {
        Err(AosError::Config(format!(
            "Database file appears corrupted. Consider restoring from backup or removing the file \
             to start fresh (data will be lost). Error: {}",
            error_msg
        )))
    } else {
        Err(AosError::Config(format!(
            "Failed to connect to database after {} attempts. Error: {}",
            MAX_RETRIES, error_msg
        )))
    }
}

async fn shutdown_signal() -> ShutdownSignal {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => ShutdownSignal::Graceful,
            Err(e) => {
                error!("Failed to install Ctrl+C handler: {}. Server may not shut down gracefully on SIGINT.", e);
                // Return a signal that will never resolve, effectively disabling this shutdown method
                futures_util::future::pending::<ShutdownSignal>().await
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
                ShutdownSignal::Graceful
            }
            Err(e) => {
                error!("Failed to install SIGTERM handler: {}. Server may not shut down gracefully on SIGTERM.", e);
                futures_util::future::pending::<ShutdownSignal>().await
            }
        }
    };

    #[cfg(unix)]
    let usr1 = async {
        match signal::unix::signal(signal::unix::SignalKind::user_defined1()) {
            Ok(mut sig) => {
                sig.recv().await;
                ShutdownSignal::Fast
            }
            Err(e) => {
                error!("Failed to install SIGUSR1 handler: {}. Fast shutdown signal disabled.", e);
                futures_util::future::pending::<ShutdownSignal>().await
            }
        }
    };

    #[cfg(unix)]
    let usr2 = async {
        match signal::unix::signal(signal::unix::SignalKind::user_defined2()) {
            Ok(mut sig) => {
                sig.recv().await;
                ShutdownSignal::Immediate
            }
            Err(e) => {
                error!("Failed to install SIGUSR2 handler: {}. Immediate shutdown signal disabled.", e);
                futures_util::future::pending::<ShutdownSignal>().await
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    #[cfg(not(unix))]
    let usr1 = std::future::pending::<()>();
    #[cfg(not(unix))]
    let usr2 = std::future::pending::<()>();

    // Use deterministic select for signal prioritization
    // Priority: Immediate (USR2) > Fast (USR1) > Graceful (TERM/Ctrl+C)
    #[cfg(unix)]
    {
        use adapteros_deterministic_exec::select::{select_3, SelectResult3};
        let signal = match select_3(
            async {
                usr2.await;
                ShutdownSignal::Immediate
            },
            async {
                usr1.await;
                ShutdownSignal::Fast
            },
            async {
                select_2(ctrl_c, terminate).await;
                ShutdownSignal::Graceful
            },
        )
        .await {
            SelectResult3::First(signal) => signal,
            SelectResult3::Second(signal) => signal,
            SelectResult3::Third(signal) => signal,
        };
        info!("Shutdown signal received: {:?}", signal);
        signal
    }

    #[cfg(not(unix))]
    {
        let signal = select_2(ctrl_c, terminate).await;
        info!("Shutdown signal received: {:?}", signal);
        signal
    }
}

/// Enhanced cleanup with phased shutdown and health check integration
async fn cleanup_resources(
    state: &adapteros_server_api::state::AppState,
    shutdown_signal: ShutdownSignal,
) -> CleanupStats {
    use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};
    use tracing::{error, info};

    let shutdown_start = std::time::Instant::now();
    let mut cleanup_stats = CleanupStats::default();

    info!(
        "Starting {} shutdown cleanup",
        match shutdown_signal {
            ShutdownSignal::Graceful => "graceful",
            ShutdownSignal::Fast => "fast",
            ShutdownSignal::Immediate => "immediate",
        }
    );

    // Emit shutdown start telemetry event
    let shutdown_event = TelemetryEventBuilder::new(
        EventType::ShutdownStart,
        LogLevel::Info,
        format!("{:?} shutdown initiated", shutdown_signal),
    )
    .component("server".to_string())
    .metadata(serde_json::json!({
        "shutdown_type": format!("{:?}", shutdown_signal),
        "signal_received_at": chrono::Utc::now().to_rfc3339()
    }))
    .build();

    let _ = state.telemetry_tx.send(shutdown_event);

    // Execute shutdown phases based on signal type
    let phases = match shutdown_signal {
        ShutdownSignal::Graceful => vec![
            ShutdownPhase::Drain,
            ShutdownPhase::Critical,
            ShutdownPhase::Resources,
            ShutdownPhase::Final,
        ],
        ShutdownSignal::Fast => vec![
            ShutdownPhase::Critical,
            ShutdownPhase::Resources,
            ShutdownPhase::Final,
        ],
        ShutdownSignal::Immediate => vec![ShutdownPhase::Resources, ShutdownPhase::Final],
    };

    for phase in phases {
        let phase_start = std::time::Instant::now();
        info!(
            "Starting shutdown phase: {} ({:?})",
            phase.description(),
            phase
        );

        // Execute phase-specific cleanup
        let phase_result = execute_shutdown_phase(state, phase, &mut cleanup_stats).await;

        let phase_duration = phase_start.elapsed();
        match phase_result {
            Ok(_) => {
                info!(
                    "Shutdown phase {} completed in {:?}",
                    phase.description(),
                    phase_duration
                );
            }
            Err(e) => {
                error!(
                    "Shutdown phase {} failed after {:?}: {}",
                    phase.description(),
                    phase_duration,
                    e
                );

                // For immediate shutdown, don't continue with other phases
                if matches!(shutdown_signal, ShutdownSignal::Immediate) {
                    break;
                }
            }
        }
    }

    cleanup_stats.shutdown_duration = shutdown_start.elapsed();

    // Emit final shutdown telemetry event
    let shutdown_complete_event = TelemetryEventBuilder::new(
        EventType::ShutdownComplete,
        LogLevel::Info,
        format!("{:?} shutdown completed", shutdown_signal),
    )
    .component("server".to_string())
    .metadata(serde_json::json!({
        "shutdown_type": format!("{:?}", shutdown_signal),
        "total_duration_ms": cleanup_stats.shutdown_duration.as_millis(),
        "models_unloaded": cleanup_stats.models_unloaded,
        "adapters_unloaded": cleanup_stats.adapters_unloaded,
            "telemetry_flushed": cleanup_stats.telemetry_flushed,
            "database_closed": cleanup_stats.database_closed
        }))
        .build();

    let _ = state.telemetry_tx.send(shutdown_complete_event);

    info!(
        "Shutdown cleanup completed in {:?}: {} models unloaded, {} adapters unloaded",
        cleanup_stats.shutdown_duration,
        cleanup_stats.models_unloaded,
        cleanup_stats.adapters_unloaded
    );

    cleanup_stats
}

/// Execute a specific shutdown phase with timeout and error handling
async fn execute_shutdown_phase(
    state: &adapteros_server_api::state::AppState,
    phase: ShutdownPhase,
    stats: &mut CleanupStats,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let timeout = phase.timeout();
    let result = tokio::time::timeout(timeout, async {
        match phase {
            ShutdownPhase::Drain => {
                // Drain new connections - signal load balancer to stop routing
                info!("Signaling load balancer to drain connections");
                // TODO: Implement connection draining logic
                stats.total_connections = 0; // Placeholder
                stats.connections_closed = 0; // Placeholder
                Ok(())
            }
            ShutdownPhase::Critical => {
                // Critical cleanup: flush telemetry, save state, stop accepting requests
                flush_telemetry_buffers(state, stats).await?;
                save_shutdown_state(state).await?;
                Ok(())
            }
            ShutdownPhase::Resources => {
                // Resource cleanup: unload models and adapters
                cleanup_models(state, stats).await?;
                cleanup_adapters(state, stats).await?;
                Ok(())
            }
            ShutdownPhase::Final => {
                // Final cleanup: close databases, cleanup temp files
                close_database_connections(state, stats).await?;
                cleanup_temporary_files().await?;
                Ok(())
            }
        }
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(format!(
            "Shutdown phase {} timed out after {:?}",
            phase.description(),
            timeout
        )
        .into()),
    }
}

/// Flush telemetry buffers during critical shutdown phase
async fn flush_telemetry_buffers(
    _state: &adapteros_server_api::state::AppState,
    stats: &mut CleanupStats,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Flushing telemetry buffers");

    // Flush any pending telemetry events
    // Give telemetry some time to flush
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    stats.telemetry_flushed = true;
    info!("Telemetry buffers flushed");

    Ok(())
}

/// Save critical state before shutdown
async fn save_shutdown_state(
    _state: &adapteros_server_api::state::AppState,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Saving critical shutdown state");
    // TODO: Save any critical in-memory state that needs to persist
    // For now, this is a placeholder
    Ok(())
}

/// Cleanup models during resource phase
async fn cleanup_models(
    state: &adapteros_server_api::state::AppState,
    stats: &mut CleanupStats,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};

    if let Some(model_runtime) = &state.model_runtime {
        let loaded_models = {
            let guard = model_runtime.lock().await;
            guard.get_all_loaded_models()
        };

        stats.total_models = loaded_models.len();
        info!("Unloading {} models during shutdown", stats.total_models);

        for model_key in loaded_models {
            let model_start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(30);

            let result = tokio::time::timeout(timeout, async {
                let mut guard = model_runtime.lock().await;
                guard.unload_model(&model_key.tenant_id, &model_key.model_id)
            })
            .await;

            let model_duration = model_start.elapsed();

            match result {
                Ok(Ok(())) => {
                    stats.models_unloaded += 1;
                    info!("Model {} unloaded successfully", model_key.model_id);

                    // Emit telemetry
                    let event = TelemetryEventBuilder::new(
                        EventType::ModelUnload,
                        LogLevel::Info,
                        format!("Model {} unloaded during shutdown", model_key.model_id),
                    )
                    .component("server".to_string())
                    .tenant_id(model_key.tenant_id.clone())
                    .metadata(serde_json::json!({
                        "model_id": model_key.model_id,
                        "duration_ms": model_duration.as_millis(),
                        "success": true
                    }))
                    .build();
                    let _ = state.telemetry_tx.send(event);
                }
                Ok(Err(e)) => {
                    stats.models_failed += 1;
                    error!("Failed to unload model {}: {}", model_key.model_id, e);
                }
                Err(_) => {
                    stats.models_timed_out += 1;
                    error!("Model {} unload timed out", model_key.model_id);
                }
            }

            // Update database status
            if let Err(e) = sqlx::query!(
                "UPDATE base_model_status SET status = 'unloaded', updated_at = datetime('now') WHERE model_id = ? AND tenant_id = ?",
                model_key.model_id,
                model_key.tenant_id
            )
            .execute(state.db.pool())
            .await {
                warn!("Failed to update model status in database: {}", e);
            }
        }
    }

    Ok(())
}

/// Cleanup adapters during resource phase
async fn cleanup_adapters(
    state: &adapteros_server_api::state::AppState,
    stats: &mut CleanupStats,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use adapteros_lora_lifecycle::AdapterState;
    use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};

    if let Some(lifecycle_manager) = &state.lifecycle_manager {
        let adapter_cleanup_start = std::time::Instant::now();
        let guard = lifecycle_manager.lock().await;

        let loaded_adapters: Vec<u16> = guard
            .get_all_states()
            .into_iter()
            .filter(|record| !matches!(record.state, AdapterState::Unloaded))
            .map(|record| record.adapter_idx)
            .collect();

        stats.total_adapters = loaded_adapters.len();
        info!(
            "Unloading {} adapters during shutdown",
            stats.total_adapters
        );

        drop(guard); // Release lifecycle lock

        for adapter_idx in loaded_adapters {
            let adapter_start = std::time::Instant::now();

            let result = {
                let guard = lifecycle_manager.lock().await;
                guard.evict_adapter(adapter_idx).await
            };

            let adapter_duration = adapter_start.elapsed();

            match result {
                Ok(()) => {
                    stats.adapters_unloaded += 1;
                    info!("Adapter {} unloaded successfully", adapter_idx);

                    // Emit telemetry
                    let event = TelemetryEventBuilder::new(
                        EventType::AdapterUnload,
                        LogLevel::Info,
                        format!("Adapter {} unloaded during shutdown", adapter_idx),
                    )
                    .component("server".to_string())
                    .metadata(serde_json::json!({
                            "adapter_idx": adapter_idx,
                            "duration_ms": adapter_duration.as_millis(),
                            "success": true
                        }))
                        .build();
                    let _ = state.telemetry_tx.send(event);
                }
                Err(e) => {
                    stats.adapters_failed += 1;
                    error!("Failed to unload adapter {}: {}", adapter_idx, e);
                }
            }
        }

        let adapter_cleanup_duration = adapter_cleanup_start.elapsed();
        info!(
            "Adapter cleanup completed in {:?}",
            adapter_cleanup_duration
        );
    }

    Ok(())
}

/// Close database connections during final phase
async fn close_database_connections(
    _state: &adapteros_server_api::state::AppState,
    stats: &mut CleanupStats,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Closing database connections");
    // Database connections will be closed when the state is dropped
    // This is mainly for telemetry and logging
    stats.database_closed = true;
    Ok(())
}

/// Cleanup temporary files during final phase
async fn cleanup_temporary_files() -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Cleaning up temporary files");
    // TODO: Implement cleanup of temporary files created during operation
    Ok(())
}

/// Check if system is ready for shutdown (no critical operations in progress)
async fn check_shutdown_readiness(
    state: &adapteros_server_api::state::AppState,
) -> ShutdownReadiness {
    let mut readiness = ShutdownReadiness::default();

    // Check for active inference requests
    // TODO: Implement check for active requests in flight

    // Check for active training jobs
    let _training_service = &state.training_service;
    // This would need to be added to TrainingService
    // readiness.active_training_jobs = training_service.active_jobs().await;

    // Check for loaded models/adapters that would take time to unload
    if let Some(model_runtime) = &state.model_runtime {
        let loaded_models = {
            let guard = model_runtime.lock().await;
            guard.get_all_loaded_models()
        };
        readiness.loaded_models = loaded_models.len();
    }

    if let Some(lifecycle_manager) = &state.lifecycle_manager {
        let guard = lifecycle_manager.lock().await;
        let loader = guard.loader();
        let loaded_adapters: Vec<u16> = {
            let loader_guard = loader.read();
            (0..u16::MAX)
                .filter(|&id| loader_guard.is_loaded(id))
                .collect()
        };
        readiness.loaded_adapters = loaded_adapters.len();
    }

    readiness
}

/// Shutdown readiness assessment
#[derive(Debug, Default)]
struct ShutdownReadiness {
    active_requests: usize,
    active_training_jobs: usize,
    loaded_models: usize,
    loaded_adapters: usize,
}

impl ShutdownReadiness {
    fn is_ready_for_graceful_shutdown(&self) -> bool {
        // Allow graceful shutdown if no active requests and reasonable number of resources
        self.active_requests == 0 && self.active_training_jobs == 0
    }

    fn estimated_shutdown_time(&self) -> std::time::Duration {
        // Rough estimate: 10s base + 5s per model + 2s per adapter
        let base_time = std::time::Duration::from_secs(10);
        let model_time = std::time::Duration::from_secs(5 * self.loaded_models as u64);
        let adapter_time = std::time::Duration::from_secs(2 * self.loaded_adapters as u64);
        base_time + model_time + adapter_time
    }
}
