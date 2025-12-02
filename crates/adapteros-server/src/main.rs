mod assets;

use adapteros_core::{derive_seed, AosError, B3Hash};
use adapteros_db::Db;
use adapteros_deterministic_exec::{
    global_ledger::GlobalTickLedger, init_global_executor, select::select_2, spawn_deterministic,
    EnforcementMode, ExecutorConfig,
};
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_manifest::ManifestV3;
use adapteros_model_hub::{ModelHubClient, ModelHubConfig};
use adapteros_server::security::PfGuard;
use adapteros_server::shutdown::ShutdownCoordinator;
use adapteros_server::status_writer;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::runtime_mode::RuntimeModeResolver;
use adapteros_server_api::{routes, AppState};
use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    Layer,
};

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

    /// Skip environment drift detection (for development only)
    #[arg(long)]
    skip_drift_check: bool,

    /// Path to base model manifest for executor seeding
    /// Can also be set via AOS_MANIFEST_PATH environment variable
    #[arg(
        long,
        env = "AOS_MANIFEST_PATH",
        default_value = "./var/model-cache/models/qwen2.5-7b-instruct-bf16/config.json"
    )]
    manifest_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI first (before logging, so we know config path)
    let cli = Cli::parse();

    // Load configuration early - needed for logging setup
    // Use eprintln for errors here since logging isn't initialized yet
    let server_config = match Config::load(&cli.config) {
        Ok(cfg) => Arc::new(RwLock::new(cfg)),
        Err(e) => {
            eprintln!("FATAL: Failed to load configuration from {}: {}", cli.config, e);
            std::process::exit(1);
        }
    };

    // Initialize tracing with config-based settings
    let _guard = {
        let cfg = server_config.read().map_err(|e| {
            eprintln!("FATAL: Config lock poisoned: {}", e);
            std::process::exit(1);
        }).unwrap();

        initialize_logging(&cfg.logging)?
    };

    // Set up panic hook to capture panics to log
    {
        let cfg = server_config.read().map_err(|e| {
            AosError::Config(format!("Config lock poisoned: {}", e))
        })?;

        if cfg.logging.capture_panics {
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                // Log the panic using tracing
                let location = panic_info
                    .location()
                    .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                    .unwrap_or_else(|| "unknown".to_string());

                let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic payload".to_string()
                };

                // Log to tracing (will go to file if configured)
                error!(
                    panic.location = %location,
                    panic.message = %message,
                    "PANIC CAPTURED"
                );

                // Also call default hook for stderr output
                default_hook(panic_info);
            }));
            info!("Panic capture hook installed");
        }
    }

    info!("Configuration loaded from {}", cli.config);

    // Initialize deterministic config system (validates all AOS_* env vars)
    if let Err(e) = adapteros_config::init_runtime_config() {
        warn!("Config validation: {}", e);
        // Non-fatal: continue with defaults for missing vars
    }

    // Handle OpenAPI generation
    if cli.generate_openapi {
        info!("Generating OpenAPI specification");
        openapi::generate_openapi()?;
        info!("OpenAPI spec written to openapi.json");
        return Ok(());
    }

    // Initialize boot state manager (without DB until connected)
    let boot_state = BootStateManager::new();
    boot_state.boot().await;

    // Acquire PID file lock if single-writer mode enabled
    let _pid_lock = if cli.single_writer {
        Some(PidFileLock::acquire(cli.pid_file.clone())?)
    } else {
        None
    };

    // Log effective configuration at startup
    {
        let cfg = server_config.read().map_err(|e| {
            error!("Config lock poisoned at startup: {}", e);
            AosError::Config("config lock poisoned at startup".into())
        })?;
        info!(
            port = cfg.server.port,
            bind = %cfg.server.bind,
            production_mode = cfg.server.production_mode,
            uds_socket = ?cfg.server.uds_socket,
            drain_timeout_secs = cfg.server.drain_timeout_secs,
            db_path = %cfg.db.path,
            artifacts_root = %cfg.paths.artifacts_root,
            bundles_root = %cfg.paths.bundles_root,
            adapters_root = %cfg.paths.adapters_root,
            datasets_root = %cfg.paths.datasets_root,
            documents_root = %cfg.paths.documents_root,
            "Effective server configuration"
        );
        info!(
            require_pf_deny = cfg.security.require_pf_deny,
            mtls_required = cfg.security.mtls_required,
            jwt_ttl_hours = cfg.security.jwt_ttl_hours,
            jwt_issuer = %cfg.security.jwt_issuer,
            key_provider_mode = %cfg.security.key_provider_mode,
            "Effective security configuration"
        );
        info!(
            rate_limit_rpm = cfg.rate_limits.requests_per_minute,
            burst_size = cfg.rate_limits.burst_size,
            inference_rpm = cfg.rate_limits.inference_per_minute,
            metrics_enabled = cfg.metrics.enabled,
            alerting_enabled = cfg.alerting.enabled,
            "Effective operational configuration"
        );
    }

    // Initialize shutdown coordinator for graceful lifecycle management
    let mut shutdown_coordinator = ShutdownCoordinator::new();

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
        let cfg = server_config
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

    let executor_config = ExecutorConfig {
        global_seed,
        enable_event_logging: true,
        max_ticks_per_task: 10000,
        enforcement_mode: EnforcementMode::AuditOnly,
        ..Default::default()
    };

    // Note: Tick ledger will be initialized after DB connection and attached via init_global_executor_with_ledger
    // For now, initialize executor without ledger
    init_global_executor(executor_config.clone())?;
    info!("Deterministic executor initialized with manifest-derived seed");

    // Transition to starting backend state
    boot_state.start_backend().await;

    // Initialize MLX runtime (idempotent, safe to call multiple times)
    #[cfg(feature = "multi-backend")]
    {
        if let Err(e) = adapteros_lora_mlx_ffi::mlx_runtime_init() {
            tracing::warn!(
                "MLX runtime initialization failed: {}. Continuing with Metal/CoreML fallback.",
                e
            );
        } else {
            tracing::info!("MLX runtime initialized successfully");
        }
    }

    // Transition to loading base models state
    boot_state.load_base_models().await;

    // Download priority models from HuggingFace Hub if enabled
    download_priority_models().await;

    // Security preflight: ensure egress is blocked
    info!("Running security preflight checks");
    {
        let cfg = server_config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        if cfg.security.require_pf_deny && !cli.skip_pf_check {
            // Convert server SecurityConfig to API SecurityConfig
            let api_security_config = adapteros_server_api::config::SecurityConfig {
                require_pf_deny: cfg.security.require_pf_deny,
                mtls_required: cfg.security.mtls_required,
                jwt_secret: cfg.security.jwt_secret.clone(),
                jwt_ttl_hours: cfg.security.jwt_ttl_hours,
                key_provider_mode: cfg.security.key_provider_mode.clone(),
                key_file_path: cfg.security.key_file_path.clone(),
                jwt_issuer: cfg.security.jwt_issuer.clone(),
                jwt_audience: cfg.security.jwt_audience.clone(),
                dev_login_enabled: cfg.security.dev_login_enabled,
                require_mfa: cfg.security.require_mfa,
                token_ttl_seconds: cfg.security.token_ttl_seconds,
                jwt_mode: cfg.security.jwt_mode.clone(),
            };
            PfGuard::preflight(&api_security_config)?;
        } else if cli.skip_pf_check {
            warn!("PF security check skipped via --skip-pf-check flag (DEVELOPMENT ONLY)");
        }
    }

    // Environment fingerprint drift detection
    info!("Verifying environment fingerprint");
    if !cli.skip_drift_check {
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

            let cfg = server_config
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
                info!("No environment drift detected");
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
            info!("Baseline fingerprint created at {:?}", baseline_path);
        }
    } else {
        warn!("Environment drift check skipped via --skip-drift-check flag (DEVELOPMENT ONLY)");
    }

    // Connect to database
    boot_state.init_db().await;
    let db_path = server_config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
        .db
        .path
        .clone();
    info!("Connecting to database: {}", db_path);
    let db = Db::connect(&db_path).await?;

    // Upgrade boot state manager with database for audit logging
    let boot_state = BootStateManager::with_db(Arc::new(db.clone()));

    // Initialize global tick ledger for inference tracking
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown-host".to_string());

    let tick_ledger = Arc::new(GlobalTickLedger::new(
        Arc::new(db.clone()),
        "default".to_string(), // Tenant ID - will be replaced by actual tenant in multi-tenant setups
        hostname.clone(),
    ));

    info!(
        host_id = %hostname,
        "Initialized global tick ledger for inference tracking"
    );

    // Resolve runtime mode with precedence: env > db > config > default
    let runtime_mode = {
        let production_mode = server_config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
            .server
            .production_mode;

        // Create a minimal API config for runtime mode resolution
        let api_cfg = adapteros_server_api::config::Config {
            server: adapteros_server_api::config::ServerConfig {
                port: 0,             // Unused
                bind: String::new(), // Unused
                production_mode,
                uds_socket: None,
                drain_timeout_secs: 30,
            },
            db: adapteros_server_api::config::DatabaseConfig {
                path: String::new(), // Unused
            },
            security: adapteros_server_api::config::SecurityConfig {
                require_pf_deny: false,
                mtls_required: false,
                jwt_secret: String::new(),
                jwt_ttl_hours: 8,
                key_provider_mode: String::new(),
                key_file_path: None,
                jwt_issuer: String::new(),
                jwt_audience: None,
                dev_login_enabled: false,
                require_mfa: None,
                token_ttl_seconds: None,
                jwt_mode: None,
            },
            paths: adapteros_server_api::config::PathsConfig {
                artifacts_root: String::new(),
                bundles_root: String::new(),
                adapters_root: String::new(),
                plan_dir: String::new(),
                datasets_root: String::new(),
                documents_root: String::new(),
            },
            rate_limits: adapteros_server_api::config::RateLimitsConfig {
                requests_per_minute: 0,
                burst_size: 0,
                inference_per_minute: 0,
            },
            metrics: adapteros_server_api::config::MetricsConfig {
                enabled: false,
                bearer_token: String::new(),
                include_histogram: false,
                histogram_buckets: vec![],
            },
            alerting: adapteros_server_api::config::AlertingConfig {
                enabled: false,
                alert_dir: String::new(),
                max_alerts_per_file: 0,
                rotate_size_mb: 0,
            },
            git: None,
            policies: Default::default(),
            logging: Default::default(),
        };

        RuntimeModeResolver::resolve(&api_cfg, &db)
            .await
            .map_err(|e| AosError::Config(format!("Failed to resolve runtime mode: {}", e)))?
    };

    info!(
        mode = %runtime_mode,
        allows_http = runtime_mode.allows_http(),
        requires_telemetry = runtime_mode.requires_telemetry(),
        requires_signing = runtime_mode.requires_event_signing(),
        "Runtime mode resolved"
    );

    // Validate runtime mode configuration
    {
        let production_mode = server_config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
            .server
            .production_mode;

        // Create a minimal API config for validation
        let api_cfg = adapteros_server_api::config::Config {
            server: adapteros_server_api::config::ServerConfig {
                port: 0,
                bind: String::new(),
                production_mode,
                uds_socket: None,
                drain_timeout_secs: 30,
            },
            db: adapteros_server_api::config::DatabaseConfig {
                path: String::new(),
            },
            security: adapteros_server_api::config::SecurityConfig {
                require_pf_deny: false,
                mtls_required: false,
                jwt_secret: String::new(),
                jwt_ttl_hours: 8,
                key_provider_mode: String::new(),
                key_file_path: None,
                jwt_issuer: String::new(),
                jwt_audience: None,
                dev_login_enabled: false,
                require_mfa: None,
                token_ttl_seconds: None,
                jwt_mode: None,
            },
            paths: adapteros_server_api::config::PathsConfig {
                artifacts_root: String::new(),
                bundles_root: String::new(),
                adapters_root: String::new(),
                plan_dir: String::new(),
                datasets_root: String::new(),
                documents_root: String::new(),
            },
            rate_limits: adapteros_server_api::config::RateLimitsConfig {
                requests_per_minute: 0,
                burst_size: 0,
                inference_per_minute: 0,
            },
            metrics: adapteros_server_api::config::MetricsConfig {
                enabled: false,
                bearer_token: String::new(),
                include_histogram: false,
                histogram_buckets: vec![],
            },
            alerting: adapteros_server_api::config::AlertingConfig {
                enabled: false,
                alert_dir: String::new(),
                max_alerts_per_file: 0,
                rotate_size_mb: 0,
            },
            git: None,
            policies: Default::default(),
            logging: Default::default(),
        };

        RuntimeModeResolver::validate(runtime_mode, &api_cfg, &db)
            .await
            .map_err(|e| AosError::Config(format!("Runtime mode validation failed: {}", e)))?;
    }

    // Audit log: Executor bootstrap event
    {
        let cfg = server_config
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

    // Transition to loading policies state
    boot_state.load_policies().await;

    // Create API config (subset needed by handlers)
    let api_config = {
        let cfg = server_config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        Arc::new(RwLock::new(adapteros_server_api::state::ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: cfg.metrics.enabled,
                bearer_token: cfg.metrics.bearer_token.clone(),
            },
            directory_analysis_timeout_secs: 120,
            capacity_limits: Default::default(),
            general: None,
            server: adapteros_server_api::state::ServerConfigApi {
                http_port: Some(cfg.server.port),
                https_port: None,
                uds_socket: cfg.server.uds_socket.clone(),
                production_mode: cfg.server.production_mode,
            },
            security: adapteros_server_api::state::SecurityConfigApi {
                jwt_mode: cfg.security.jwt_mode.clone(),
                token_ttl_seconds: cfg.security.token_ttl_seconds,
                require_mfa: cfg.security.require_mfa,
                require_pf_deny: cfg.security.require_pf_deny,
                dev_login_enabled: cfg.security.dev_login_enabled,
            },
            performance: Default::default(),
            paths: adapteros_server_api::PathsConfig {
                artifacts_root: cfg.paths.artifacts_root.clone(),
                bundles_root: cfg.paths.bundles_root.clone(),
                adapters_root: cfg.paths.adapters_root.clone(),
                plan_dir: cfg.paths.plan_dir.clone(),
                datasets_root: cfg.paths.datasets_root.clone(),
                documents_root: cfg.paths.documents_root.clone(),
            },
        }))
    };

    // Setup SIGHUP handler for config reload
    #[cfg(unix)]
    {
        let config_clone = Arc::clone(&server_config);
        let api_config_clone = Arc::clone(&api_config);
        let config_path = cli.config.clone();

        match spawn_deterministic("SIGHUP handler".to_string(), async move {
            use tokio::signal::unix::{signal, SignalKind};

            // Attempt to register signal handler, gracefully degrade if unavailable
            let mut sig = match signal(SignalKind::hangup()) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        error = %e,
                        "Failed to register SIGHUP handler, config reload will be unavailable"
                    );
                    return;
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
                                // Reload paths config
                                api_cfg.paths.artifacts_root =
                                    new_config.paths.artifacts_root.clone();
                                api_cfg.paths.bundles_root = new_config.paths.bundles_root.clone();
                                api_cfg.paths.adapters_root =
                                    new_config.paths.adapters_root.clone();
                                api_cfg.paths.plan_dir = new_config.paths.plan_dir.clone();
                                api_cfg.paths.datasets_root =
                                    new_config.paths.datasets_root.clone();
                                api_cfg.paths.documents_root =
                                    new_config.paths.documents_root.clone();
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
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!("SIGHUP handler registered for config reload");
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to spawn SIGHUP handler task, config reload will be unavailable"
                );
            }
        }
    }

    // Initialize status writer uptime tracking early
    status_writer::init_uptime_tracking();

    // Spawn alert watcher if enabled
    {
        let cfg = server_config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        if cfg.alerting.enabled {
            info!("Starting alert watcher");
            // Convert server AlertingConfig to API AlertingConfig
            let api_alerting_config = adapteros_server_api::config::AlertingConfig {
                enabled: cfg.alerting.enabled,
                alert_dir: cfg.alerting.alert_dir.clone(),
                max_alerts_per_file: cfg.alerting.max_alerts_per_file,
                rotate_size_mb: cfg.alerting.rotate_size_mb,
            };
            let alert_handle = alerting::spawn_alert_watcher(db.clone(), api_alerting_config)?;
            shutdown_coordinator.set_alert_handle(alert_handle);
        }
    }

    // Initialize policy hash watcher (continuous monitoring)
    {
        info!("Initializing policy hash watcher");

        // Create telemetry writer
        let bundles_path = server_config
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
            telemetry.clone(),
            None, // cpid - will be set per-tenant
        ));

        // Load baseline hashes from database
        if let Err(e) = policy_watcher.load_cache().await {
            warn!("Failed to load policy hash cache: {}", e);
        }

        // Start background watcher (60 second interval)
        let policy_hashes = Arc::new(RwLock::new(std::collections::HashMap::new()));
        let watcher_handle = policy_watcher
            .clone()
            .start_background_watcher(Duration::from_secs(60), policy_hashes.clone());
        shutdown_coordinator.set_policy_watcher_handle(watcher_handle);

        info!("Policy hash watcher started (60s interval)");

        // Initialize Federation Daemon
        {
            info!("Initializing federation daemon");

            let federation_keypair = adapteros_crypto::Keypair::generate();
            let federation_manager = Arc::new(adapteros_federation::FederationManager::new(
                db.clone(),
                federation_keypair,
                "default".to_string(),
            )?);

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

            let federation_shutdown_rx = shutdown_coordinator.subscribe_shutdown();
            let federation_handle = federation_daemon.start(federation_shutdown_rx);
            shutdown_coordinator.set_federation_handle(federation_handle);
            info!("Federation daemon started (300s interval)");
        }
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

        // Bind and start serving in background
        match uds_exporter.bind().await {
            Ok(()) => {
                let exporter_socket_path = socket_path.clone();
                let shutdown_rx = shutdown_coordinator.subscribe_shutdown();
                let uds_handle = tokio::spawn(async move {
                    if let Err(e) = uds_exporter.serve(shutdown_rx).await {
                        error!("UDS metrics exporter error: {}", e);
                    }
                });

                shutdown_coordinator.set_uds_metrics_handle(uds_handle);

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
                warn!(
                    error = %e,
                    "UDS metrics exporter disabled (socket unavailable)"
                );
            }
        }
    }

    // Create metrics exporter
    let metrics_exporter = {
        let cfg = server_config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;
        Arc::new(adapteros_metrics_exporter::MetricsExporter::new(
            cfg.metrics.histogram_buckets.clone(),
        )?)
    };

    // Build application state
    let jwt_secret = server_config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
        .security
        .jwt_secret
        .clone();

    // UMA monitor for memory pressure detection
    // Start polling before wrapping in Arc since start_polling requires &mut self
    let mut uma_monitor = UmaPressureMonitor::new(15, None);
    uma_monitor.start_polling().await;
    let uma_monitor = Arc::new(uma_monitor);

    // Create metrics collector and registry for AppState
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new(
        adapteros_telemetry::MetricsConfig::default(),
    ));
    let metrics_registry = Arc::new(adapteros_server_api::telemetry::MetricsRegistry::new());

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
    .with_dataset_progress(dataset_progress_tx)
    .with_boot_state(boot_state.clone())
    .with_runtime_mode(runtime_mode)
    .with_tick_ledger(tick_ledger.clone());

    state = state.with_plugin_registry(Arc::new(adapteros_server_api::PluginRegistry::new(
        db.clone(),
    )));

    // Load embedding model for RAG if embeddings feature enabled
    #[cfg(feature = "embeddings")]
    {
        use adapteros_ingest_docs::EmbeddingModel;
        use std::path::Path;

        let embedding_model_path = std::env::var("AOS_EMBEDDING_MODEL_PATH")
            .unwrap_or_else(|_| "./var/model-cache/models/bge-small-en-v1.5".to_string());

        let tokenizer_path = format!("{}/tokenizer.json", embedding_model_path);

        if Path::new(&tokenizer_path).exists() {
            match adapteros_ingest_docs::load_tokenizer(Path::new(&tokenizer_path)) {
                Ok(tokenizer) => {
                    let embedding_model = Arc::new(
                        adapteros_ingest_docs::ProductionEmbeddingModel::load(
                            Some(&embedding_model_path),
                            tokenizer,
                        )
                    );

                    info!(
                        path = %embedding_model_path,
                        dimension = embedding_model.dimension(),
                        hash = %embedding_model.model_hash().to_hex()[..16],
                        "Loaded embedding model for RAG"
                    );

                    state = state.with_embedding_model(embedding_model);
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        path = %tokenizer_path,
                        "Failed to load tokenizer for embedding model, RAG disabled"
                    );
                }
            }
        } else {
            warn!(
                path = %tokenizer_path,
                "Embedding model tokenizer not found, RAG disabled. \
                 Set AOS_EMBEDDING_MODEL_PATH to point to a sentence-transformer model."
            );
        }
    }

    // Git subsystem initialization
    let git_enabled = server_config
        .read()
        .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?
        .git
        .as_ref()
        .map(|c| c.enabled)
        .unwrap_or(false);

    if git_enabled {
        info!("Initializing Git subsystem");
        let git_config = server_config
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
        match spawn_deterministic("Status writer".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Err(e) = status_writer::write_status(&state_clone).await {
                    warn!("Failed to write status: {}", e);
                }
            }
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!("Status writer started (5s interval)");
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to spawn status writer task, status updates will be unavailable"
                );
            }
        }
    }

    // Spawn TTL cleanup background task
    {
        let db_clone = db.clone();
        match spawn_deterministic("TTL cleanup".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            let mut consecutive_errors = 0u32;
            const MAX_CONSECUTIVE_ERRORS: u32 = 5;
            const CIRCUIT_BREAKER_PAUSE_SECS: u64 = 1800; // 30 minutes

            loop {
                interval.tick().await;

                // Circuit breaker: pause if too many consecutive errors
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    error!(
                        consecutive_errors,
                        pause_duration_secs = CIRCUIT_BREAKER_PAUSE_SECS,
                        "TTL cleanup circuit breaker triggered, pausing task"
                    );
                    tokio::time::sleep(Duration::from_secs(CIRCUIT_BREAKER_PAUSE_SECS)).await;
                    consecutive_errors = 0;
                    continue;
                }

                let mut had_error = false;

                // Find and clean up expired adapters
                match db_clone.find_expired_adapters().await {
                    Ok(expired) => {
                        if !expired.is_empty() {
                            info!(count = expired.len(), "Found expired adapters, cleaning up");

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
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                } else {
                    consecutive_errors = 0; // Reset on success
                }
            }
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!("TTL cleanup task started (5 minute interval, circuit breaker enabled)");
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to spawn TTL cleanup task, expired adapters may not be cleaned up automatically"
                );
            }
        }
    }

    // Spawn heartbeat recovery background task
    {
        let db_clone = db.clone();
        match spawn_deterministic("Heartbeat recovery".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            let mut consecutive_errors = 0u32;
            const MAX_CONSECUTIVE_ERRORS: u32 = 5;
            const CIRCUIT_BREAKER_PAUSE_SECS: u64 = 1800; // 30 minutes

            loop {
                interval.tick().await;

                // Circuit breaker: pause if too many consecutive errors
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    error!(
                        consecutive_errors,
                        pause_duration_secs = CIRCUIT_BREAKER_PAUSE_SECS,
                        "Heartbeat recovery circuit breaker triggered, pausing task"
                    );
                    tokio::time::sleep(Duration::from_secs(CIRCUIT_BREAKER_PAUSE_SECS)).await;
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
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    }
                }
            }
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!("Heartbeat recovery task started (5 minute interval, 300s timeout, circuit breaker enabled)");
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to spawn heartbeat recovery task, stale adapters may not be recovered automatically"
                );
            }
        }
    }

    // Transition to loading adapters state
    boot_state.load_adapters().await;

    // Clone in_flight_requests counter for shutdown handler before moving state
    let in_flight_requests = Arc::clone(&state.in_flight_requests);

    // Build router with UI
    let api_routes = routes::build(state);
    let ui_routes = assets::routes();

    let app = axum::Router::new()
        .nest("/api", api_routes) // API routes first (higher priority)
        .merge(ui_routes); // UI fallback for non-API paths

    // Bind and serve
    let (production_mode, uds_socket, port, drain_timeout) = {
        let cfg = server_config
            .read()
            .map_err(|e| AosError::Config(format!("Config lock poisoned: {}", e)))?;

        // Environment variable takes precedence over config file
        let server_port = std::env::var("AOS_SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(cfg.server.port);

        (
            cfg.server.production_mode,
            cfg.server.uds_socket.clone(),
            server_port,
            Duration::from_secs(cfg.server.drain_timeout_secs),
        )
    };

    // Egress policy: production_mode requires UDS-only
    if production_mode {
        let socket_path: String = uds_socket.ok_or_else(|| {
            AosError::PolicyViolation(
                "Egress policy violation: production_mode requires uds_socket configuration".into(),
            )
        })?;

        info!("Starting control plane on UDS: {}", socket_path);
        info!("Production mode enabled - TCP binding disabled per Egress policy");

        // Remove existing socket file if present
        let _ = std::fs::remove_file(&socket_path);

        // Transition to ready state - server is now accepting requests
        boot_state.ready().await;

        let listener = tokio::net::UnixListener::bind(&socket_path)?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal_with_drain(
                boot_state.clone(),
                Arc::clone(&in_flight_requests),
                drain_timeout,
            ))
            .await?;

        // Server has shut down, now perform coordinated shutdown
        info!("Server shutdown complete, performing coordinated component shutdown");
        match shutdown_coordinator.shutdown().await {
            Ok(()) => {
                info!("All components shut down successfully");
            }
            Err(e) => {
                match e {
                    adapteros_server::shutdown::ShutdownError::CriticalFailure { component } => {
                        error!(
                            "Critical shutdown failure in {} - system integrity compromised",
                            component
                        );
                        std::process::exit(1);
                    }
                    adapteros_server::shutdown::ShutdownError::PartialFailure { failed_count } => {
                        warn!("Partial shutdown failure - {} components failed but system integrity maintained", failed_count);
                        // Don't exit - partial failures are acceptable
                    }
                    _ => {
                        error!("Shutdown error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }

        // Final MLX cleanup after all other components
        #[cfg(feature = "multi-backend")]
        {
            adapteros_lora_mlx_ffi::mlx_runtime_shutdown();
            tracing::info!("MLX runtime shut down");
        }
    } else {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        info!("Starting control plane on {}", addr);
        info!("UI available at http://127.0.0.1:{}/", port);
        info!("API available at http://127.0.0.1:{}/api/", port);
        warn!("Development mode: TCP binding enabled. Set production_mode=true for UDS-only");

        // Transition to ready state - server is now accepting requests
        boot_state.ready().await;

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal_with_drain(
                boot_state.clone(),
                Arc::clone(&in_flight_requests),
                drain_timeout,
            ))
            .await?;

        // Server has shut down, now perform coordinated shutdown
        info!("Server shutdown complete, performing coordinated component shutdown");
        match shutdown_coordinator.shutdown().await {
            Ok(()) => {
                info!("All components shut down successfully");
            }
            Err(e) => {
                match e {
                    adapteros_server::shutdown::ShutdownError::CriticalFailure { component } => {
                        error!(
                            "Critical shutdown failure in {} - system integrity compromised",
                            component
                        );
                        std::process::exit(1);
                    }
                    adapteros_server::shutdown::ShutdownError::PartialFailure { failed_count } => {
                        warn!("Partial shutdown failure - {} components failed but system integrity maintained", failed_count);
                        // Don't exit - partial failures are acceptable
                    }
                    _ => {
                        error!("Shutdown error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }

        // Final MLX cleanup after all other components
        #[cfg(feature = "multi-backend")]
        {
            adapteros_lora_mlx_ffi::mlx_runtime_shutdown();
            tracing::info!("MLX runtime shut down");
        }
    }

    Ok(())
}

/// Initialize logging with configuration-based settings
///
/// Sets up tracing with:
/// - Console output (always)
/// - File output with rotation (if log_dir configured)
/// - Configurable log levels
/// - JSON or human-readable format
///
/// Returns a guard that must be kept alive for the duration of the program
/// to ensure log files are properly flushed.
fn initialize_logging(
    config: &adapteros_server_api::config::LoggingConfig,
) -> Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    use tracing_subscriber::EnvFilter;

    // Parse log level from config or environment
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    // Determine rotation strategy
    let rotation = match config.rotation.as_str() {
        "hourly" => Rotation::HOURLY,
        "daily" => Rotation::DAILY,
        "never" => Rotation::NEVER,
        _ => {
            eprintln!("WARNING: Unknown rotation '{}', defaulting to daily", config.rotation);
            Rotation::DAILY
        }
    };

    // Set up file logging if log_dir is configured
    let (file_layer, guard) = if let Some(ref log_dir) = config.log_dir {
        // Ensure log directory exists
        std::fs::create_dir_all(log_dir).map_err(|e| {
            anyhow::anyhow!("Failed to create log directory {}: {}", log_dir, e)
        })?;

        let file_appender = RollingFileAppender::new(rotation, log_dir, &config.log_prefix);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = if config.json_format {
            fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_span_events(FmtSpan::CLOSE)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .boxed()
        } else {
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false) // No ANSI colors in log files
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .boxed()
        };

        (Some(file_layer), Some(guard))
    } else {
        (None, None)
    };

    // Console layer (always enabled)
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false) // Cleaner console output
        .with_file(false)
        .with_line_number(false);

    // Build the subscriber
    let subscriber = tracing_subscriber::registry().with(env_filter);

    if let Some(file_layer) = file_layer {
        subscriber
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        subscriber.with(console_layer).init();
    }

    // Log effective logging configuration
    if let Some(ref log_dir) = config.log_dir {
        // Can't use tracing yet since it's being initialized, use eprintln
        eprintln!(
            "Logging initialized: level={}, dir={}, rotation={}, json={}",
            config.level, log_dir, config.rotation, config.json_format
        );
    } else {
        eprintln!(
            "Logging initialized: level={}, stdout only",
            config.level
        );
    }

    Ok(guard)
}

/// Download priority models from HuggingFace Hub if enabled
///
/// This function checks if the HF Hub integration is enabled via environment variables
/// and downloads a configured list of priority models during server startup.
/// Download failures are logged but do not block server startup.
async fn download_priority_models() {
    // Check if HF Hub is enabled
    let hf_enabled = std::env::var("AOS_HF_HUB_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if !hf_enabled {
        info!("HF Hub integration disabled, skipping priority model downloads");
        return;
    }

    // Get priority models from environment variable
    let priority_models_str = match std::env::var("AOS_PRIORITY_MODELS") {
        Ok(models) => models,
        Err(_) => {
            info!("No priority models configured (AOS_PRIORITY_MODELS not set)");
            return;
        }
    };

    let priority_models: Vec<String> = priority_models_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if priority_models.is_empty() {
        info!("No priority models configured");
        return;
    }

    info!(
        count = priority_models.len(),
        models = ?priority_models,
        "Starting priority model downloads"
    );

    // Create ModelHub client configuration
    let cache_dir = std::env::var("AOS_MODEL_CACHE_DIR").unwrap_or_else(|_| {
        let default = std::path::PathBuf::from("var/model-cache");
        default.to_string_lossy().to_string()
    });

    let hf_token = std::env::var("HF_TOKEN").ok();

    let config = ModelHubConfig {
        registry_url: std::env::var("AOS_HF_REGISTRY_URL")
            .unwrap_or_else(|_| "https://huggingface.co".to_string()),
        cache_dir: PathBuf::from(cache_dir),
        max_concurrent_downloads: std::env::var("AOS_MAX_CONCURRENT_DOWNLOADS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4),
        timeout_secs: std::env::var("AOS_DOWNLOAD_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300),
        hf_token,
    };

    // Create ModelHub client
    let client = match ModelHubClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                error = %e,
                "Failed to create ModelHub client, skipping model downloads"
            );
            return;
        }
    };

    // Download each priority model
    for model_id in priority_models {
        info!(model_id = %model_id, "Attempting to download priority model");

        match client.download_model(&model_id).await {
            Ok(path) => {
                info!(
                    model_id = %model_id,
                    path = %path.display(),
                    "Priority model downloaded successfully"
                );
            }
            Err(e) => {
                warn!(
                    model_id = %model_id,
                    error = %e,
                    "Failed to download priority model (continuing with boot)"
                );
                // Don't fail boot - continue with other models
            }
        }
    }

    info!("Priority model downloads complete");
}

async fn shutdown_signal_with_drain(
    boot_state: BootStateManager,
    in_flight_requests: Arc<std::sync::atomic::AtomicUsize>,
    drain_timeout: Duration,
) {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => {}
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to install Ctrl+C handler, shutdown may not work as expected"
                );
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to install SIGTERM handler, will only respond to Ctrl+C"
                );
                // Block forever since we can't handle this signal
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // Use deterministic select instead of tokio::select!
    // Left (ctrl_c) has priority over Right (terminate)
    let _ = select_2(ctrl_c, terminate).await;

    info!("Shutdown signal received");

    // Transition to draining state
    boot_state.drain().await;

    // Wait for in-flight requests to complete (with timeout)
    let start = tokio::time::Instant::now();
    let mut logged_waiting = false;
    let mut sample_count = 0u64;
    let mut total_in_flight = 0u64;
    let mut peak_in_flight = 0usize;

    loop {
        let count = in_flight_requests.load(std::sync::atomic::Ordering::SeqCst);

        // Track statistics for drain analysis
        sample_count += 1;
        total_in_flight += count as u64;
        peak_in_flight = peak_in_flight.max(count);

        if count == 0 {
            info!("All in-flight requests completed");
            break;
        }

        if !logged_waiting {
            info!(
                in_flight = count,
                timeout_secs = drain_timeout.as_secs(),
                "Waiting for in-flight requests to complete"
            );
            logged_waiting = true;
        }

        let elapsed = start.elapsed();
        if elapsed >= drain_timeout {
            // Calculate average in-flight requests during drain
            let avg_in_flight = if sample_count > 0 {
                total_in_flight as f64 / sample_count as f64
            } else {
                0.0
            };

            error!(
                in_flight_current = count,
                in_flight_peak = peak_in_flight,
                in_flight_avg = format!("{:.2}", avg_in_flight),
                elapsed_secs = elapsed.as_secs(),
                timeout_secs = drain_timeout.as_secs(),
                sample_count,
                "Drain timeout exceeded - incomplete operations detected"
            );

            // Log detailed recovery instructions
            error!(
                "MANUAL RECOVERY REQUIRED: {} requests did not complete within {}s drain timeout. \
                 Check application logs for long-running operations. \
                 Peak in-flight: {}, Average: {:.2}. \
                 Consider investigating: database locks, slow network I/O, or stuck async tasks.",
                count,
                drain_timeout.as_secs(),
                peak_in_flight,
                avg_in_flight
            );

            break;
        }

        // Check every 100ms
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Transition to stopping state
    boot_state.stop().await;
}
