mod assets;
mod db_index_monitor;
mod otel;

const DEFAULT_MANIFEST_HASH: &str =
    "756be0c4434c3fe5e1198fcf417c52a662e7a24d0716dbf12aae6246bea84f9e";

use adapteros_api_types::FailureCode;
use adapteros_boot::{load_or_generate_worker_keypair, BootReport};
use adapteros_config::{
    init_effective_config, resolve_base_model_location, resolve_manifest_path,
    try_effective_config, ConfigLoader, ConfigSnapshot,
};
use adapteros_core::{derive_seed, AosError, B3Hash, BackendKind, SeedMode};
use adapteros_db::{kv_metrics, Db, DbFactory, DbStorageBackend, RuntimeSession};
use adapteros_deterministic_exec::{
    global_ledger::GlobalTickLedger, init_global_executor, select::select_2, spawn_deterministic,
    EnforcementMode, ExecutorConfig,
};
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_manifest::ManifestV3;
use adapteros_model_hub::{ModelHubClient, ModelHubConfig};
use adapteros_server::boot::BackgroundTaskSpawner;
use adapteros_server::security::PfGuard;
use adapteros_server::shutdown::ShutdownCoordinator;
use adapteros_server::status_writer;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::kv_isolation;
use adapteros_server_api::runtime_mode::RuntimeModeResolver;
use adapteros_server_api::storage_reconciler::spawn_storage_reconciler;
use adapteros_server_api::worker_health::WorkerHealthMonitor;
use adapteros_server_api::{routes, AppState};
use adapteros_telemetry::AlertingEngine;
use anyhow::Result;
use clap::Parser;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::signal;
use tokio::time::MissedTickBehavior;
use tracing::{debug, error, info, trace, warn};
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
        info!(path = %path.display(), "PID lock acquired");

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

fn normalize_jwt_mode(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "hmac" | "hs256" => "hmac".to_string(),
        "eddsa" | "ed25519" => "eddsa".to_string(),
        other => other.to_string(),
    }
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

    /// Skip PF/firewall egress checks (DEBUG BUILDS ONLY)
    /// This flag is not available in release builds for security
    #[cfg_attr(debug_assertions, arg(long))]
    #[cfg_attr(not(debug_assertions), arg(skip))]
    skip_pf_check: bool,

    /// Skip environment drift detection (DEBUG BUILDS ONLY)
    /// This flag is not available in release builds for security
    #[cfg_attr(debug_assertions, arg(long))]
    #[cfg_attr(not(debug_assertions), arg(skip))]
    skip_drift_check: bool,

    /// Path to base model manifest for executor seeding
    /// Can also be set via AOS_MANIFEST_PATH environment variable
    #[arg(long, env = "AOS_MANIFEST_PATH")]
    manifest_path: Option<PathBuf>,

    /// Enable strict mode (fail-closed boot)
    /// When enabled:
    /// - Worker keypair must exist (var/keys/worker_signing.key)
    /// - Boot report emission is required
    /// - Legacy auth paths are disabled
    #[arg(long, env = "AOS_STRICT")]
    strict: bool,
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
            eprintln!(
                "FATAL: Failed to load configuration from {}: {}",
                cli.config, e
            );
            std::process::exit(1);
        }
    };

    // Harmonize critical config with canonical env vars so scripts/UI and the server agree.
    // Precedence: explicit env > config file > defaults.
    {
        use adapteros_config::path_resolver::PathSource;

        let mut cfg = server_config
            .write()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        if let Ok(raw) = std::env::var("AOS_SERVER_PORT") {
            match raw.parse::<u16>() {
                Ok(port) => cfg.server.port = port,
                Err(_) => eprintln!("WARNING: Invalid AOS_SERVER_PORT={raw}; ignoring"),
            }
        }

        if let Ok(bind) = std::env::var("AOS_SERVER_HOST") {
            cfg.server.bind = bind;
        }

        // Production mode should be consistent across the repo; other crates read AOS_SERVER_PRODUCTION_MODE.
        if let Ok(raw) = std::env::var("AOS_SERVER_PRODUCTION_MODE") {
            let enabled = matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            );
            cfg.server.production_mode = enabled;
        } else {
            std::env::set_var(
                "AOS_SERVER_PRODUCTION_MODE",
                if cfg.server.production_mode {
                    "true"
                } else {
                    "false"
                },
            );
        }

        // Optional: allow coarse log level / format env vars to influence startup logging defaults.
        if let Ok(level) = std::env::var("AOS_LOG_LEVEL") {
            let normalized = level.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "trace" | "debug" | "info" | "warn" | "error" => cfg.logging.level = normalized,
                _ => eprintln!("WARNING: Invalid AOS_LOG_LEVEL={level}; ignoring"),
            }
        }

        if let Ok(format) = std::env::var("AOS_LOG_FORMAT") {
            let normalized = format.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "json" => cfg.logging.json_format = true,
                "text" | "pretty" => cfg.logging.json_format = false,
                _ => eprintln!("WARNING: Invalid AOS_LOG_FORMAT={format}; ignoring"),
            }
        }

        // DB URL: prefer AOS_DATABASE_URL; accept DATABASE_URL as legacy alias.
        if let Ok(resolved) = adapteros_config::path_resolver::resolve_database_url() {
            if matches!(resolved.source, PathSource::Env(_)) {
                cfg.db.path = resolved.path.to_string_lossy().to_string();
            }
        }
    }

    // Validate base model path early to avoid drift across server/CLI
    if let Err(e) = resolve_base_model_location(None, None, true) {
        eprintln!(
            "FATAL: Base model path missing or invalid: {}. Set AOS_MODEL_CACHE_DIR/AOS_BASE_MODEL_ID or update config.",
            e
        );
        std::process::exit(1);
    }

    // Initialize tracing with config-based settings (including OpenTelemetry if enabled)
    let (_log_guard, _otel_guard): (
        Option<tracing_appender::non_blocking::WorkerGuard>,
        Option<otel::OtelGuard>,
    ) = {
        let cfg = server_config
            .read()
            .map_err(|e| {
                eprintln!("FATAL: Config lock poisoned: {}", e);
                std::process::exit(1);
            })
            .unwrap();

        initialize_logging(&cfg.logging, &cfg.otel)
            .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?
    };

    // Derive effective JWT mode and session lifetime from auth config
    {
        let mut cfg = server_config
            .write()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        let desired_mode = if cfg!(debug_assertions) {
            cfg.auth.dev_algo.clone()
        } else {
            cfg.auth.prod_algo.clone()
        };
        let normalized_mode = normalize_jwt_mode(&desired_mode);
        cfg.security.jwt_mode = Some(normalized_mode);
        cfg.security.session_ttl_seconds = cfg.auth.session_lifetime;
    }

    // Set up panic hook to capture panics to log
    {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

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

    info!(config_path = %cli.config, "Configuration loaded");

    // Initialize deterministic config system (validates all AOS_* env vars)
    if let Err(e) = adapteros_config::init_runtime_config() {
        warn!(error = %e, "Config validation failed");
        // Non-fatal: continue with defaults for missing vars
    }

    // Validate CORS configuration early (fail-fast in production mode)
    if let Err(e) = adapteros_server_api::middleware_security::validate_cors_config() {
        error!(error = %e, "FATAL: CORS config validation failed");
        std::process::exit(1);
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
    boot_state.start().await;

    // Get boot timeout from config (default: 300 seconds)
    let boot_timeout_secs = {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        cfg.server.boot_timeout_secs
    };

    info!(
        timeout_secs = boot_timeout_secs,
        "Starting boot sequence with timeout"
    );

    // Track boot phase timings
    let boot_start = std::time::Instant::now();

    info!(target: "boot", phase = 1, name = "config", "═══ BOOT PHASE 1/12: Configuration Complete ═══");

    // Wrap the entire boot sequence in a timeout
    let boot_timeout = Duration::from_secs(boot_timeout_secs);
    // Clone boot_state for use in timeout error handler (async block captures the original)
    let boot_state_for_timeout = boot_state.clone();
    let boot_result = tokio::time::timeout(boot_timeout, async {

    // Acquire PID file lock if single-writer mode enabled
    let _pid_lock = if cli.single_writer {
        Some(PidFileLock::acquire(cli.pid_file.clone())?)
    } else {
        None
    };

    // =========================================================================
    // Worker Authentication Keypair (Ed25519)
    // =========================================================================
    // Load or generate the worker signing keypair for CP->Worker authentication.
    // In strict mode, this is required; otherwise it's optional with a warning.
    info!(target: "boot", phase = 2, name = "security-init", "═══ BOOT PHASE 2/12: Security Initialization ═══");
    info!("Loading worker authentication keypair (CSPRNG + filesystem I/O may be slow on some systems)");
    let keypair_start = std::time::Instant::now();
    let worker_signing_keypair = {
        let keys_dir = std::path::Path::new("var/keys");
        std::fs::create_dir_all(keys_dir).ok();

        let key_path = keys_dir.join("worker_signing.key");
        match load_or_generate_worker_keypair(&key_path) {
            Ok(keypair) => {
                let kid = adapteros_boot::derive_kid_from_verifying_key(&keypair.verifying_key());
                info!(
                    kid = %kid,
                    path = %key_path.display(),
                    elapsed_ms = %keypair_start.elapsed().as_millis(),
                    "Worker signing keypair loaded for CP->Worker authentication"
                );
                Some(keypair)
            }
            Err(e) => {
                if cli.strict {
                    error!(
                        error = %e,
                        path = %key_path.display(),
                        elapsed_ms = %keypair_start.elapsed().as_millis(),
                        "STRICT MODE: Failed to load worker signing keypair"
                    );
                    return Err(anyhow::anyhow!(
                        "Strict mode requires worker signing keypair at {}",
                        key_path.display()
                    ));
                } else {
                    warn!(
                        error = %e,
                        path = %key_path.display(),
                        elapsed_ms = %keypair_start.elapsed().as_millis(),
                        "Worker signing keypair not available, CP->Worker auth disabled"
                    );
                    None
                }
            }
        }
    };

    // Log effective configuration at startup
    {
        let cfg = server_config.read().map_err(|e| {
            error!(error = %e, "Config lock poisoned at startup");
            anyhow::anyhow!("config lock poisoned at startup")
        })?;

        let parse_env_u16 = |key: &str| -> Option<u16> {
            std::env::var(key).ok().and_then(|v| v.parse::<u16>().ok())
        };
        let env_truthy = |key: &str| -> bool {
            std::env::var(key)
                .ok()
                .map(|v| {
                    matches!(
                        v.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    )
                })
                .unwrap_or(false)
        };

        let api_port = std::env::var("AOS_SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(cfg.server.port);
        let ui_port = parse_env_u16("AOS_UI_PORT");
        let panel_port = parse_env_u16("AOS_PANEL_PORT");

        let demo_mode = env_truthy("AOS_DEMO_MODE")
            || panel_port.is_some()
            || std::env::var("AOS_DATABASE_URL")
                .ok()
                .is_some_and(|v| v.contains("aos-demo"))
            || std::env::var("DATABASE_URL")
                .ok()
                .is_some_and(|v| v.contains("aos-demo"));

        let dev_no_auth_active = cfg!(debug_assertions) && env_truthy("AOS_DEV_NO_AUTH");
        let auth_mode = if dev_no_auth_active {
            "dev_no_auth"
        } else {
            "jwt"
        };

        info!(
            api_port,
            ui_port = ?ui_port,
            panel_port = ?panel_port,
            db_path = %cfg.db.path,
            auth_mode = %auth_mode,
            demo_mode,
            "Effective config summary"
        );

        info!(
            port = cfg.server.port,
            bind = %cfg.server.bind,
            production_mode = cfg.server.production_mode,
            uds_socket = ?cfg.server.uds_socket,
            drain_timeout_secs = cfg.server.drain_timeout_secs,
            boot_timeout_secs = cfg.server.boot_timeout_secs,
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

    info!(target: "boot", phase = 3, name = "executor", "═══ BOOT PHASE 3/12: Deterministic Executor ═══");

    // Resolve manifest path with precedence: env > CLI > config > dev fallback (debug-only)
    let config_manifest_path = {
        let loader = ConfigLoader::new();
        match loader.load(vec![], Some(cli.config.clone())) {
            Ok(cfg) => cfg.get("manifest.path").map(PathBuf::from),
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to load manifest.path from config; continuing without config override"
                );
                None
            }
        }
    };

    let manifest_resolution =
        resolve_manifest_path(cli.manifest_path.as_ref(), config_manifest_path.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to resolve manifest path: {}", e))?;
    let manifest_path = manifest_resolution.path.clone();
    info!(
        path = %manifest_path.display(),
        source = %manifest_resolution.source,
        dev_fallback = manifest_resolution.used_dev_fallback,
        "Resolved manifest path for executor seeding"
    );

    // Initialize shutdown coordinator for graceful lifecycle management
    let mut shutdown_coordinator = ShutdownCoordinator::new();

    // Initialize deterministic executor with manifest-derived seed
    info!("Initializing deterministic executor");

    // Load manifest for deterministic seeding
    let manifest_hash = if manifest_path.exists() {
        match std::fs::read_to_string(&manifest_path) {
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
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

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
    init_global_executor(executor_config.clone())
        .map_err(|e| anyhow::anyhow!("Deterministic executor init failed: {}", e))?;
    info!("Deterministic executor initialized with manifest-derived seed");

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

    // Security preflight: ensure egress is blocked
    info!(target: "boot", phase = 4, name = "security-preflight", "═══ BOOT PHASE 4/12: Security Preflight ═══");
    info!("Running security preflight checks");
    {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        let require_pf_deny = cfg.security.require_pf_deny;

        // Runtime guard: prevent security bypass flags in production mode (debug builds only)
        #[cfg(debug_assertions)]
        {
            let effective_production = cfg.server.production_mode || cfg.security.require_pf_deny;

            if effective_production && (cli.skip_pf_check || cli.skip_drift_check) {
                drop(cfg); // Release lock before error
                return Err(anyhow::anyhow!(
                    "Security bypass flags (--skip-pf-check, --skip-drift-check) \
                     cannot be used when production_mode=true or require_pf_deny=true. \
                     These flags are for development only."
                ));
            }
        }

        if require_pf_deny {
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
                jwt_additional_ed25519_public_keys: cfg
                    .security
                    .jwt_additional_ed25519_public_keys
                    .clone(),
                jwt_additional_hmac_secrets: cfg.security.jwt_additional_hmac_secrets.clone(),
                dev_login_enabled: cfg.security.dev_login_enabled,
                require_mfa: cfg.security.require_mfa,
                token_ttl_seconds: cfg.security.token_ttl_seconds,
                access_token_ttl_seconds: cfg.security.access_token_ttl_seconds,
                session_ttl_seconds: cfg.security.session_ttl_seconds,
                jwt_mode: cfg.security.jwt_mode.clone(),
                cookie_same_site: cfg.security.cookie_same_site.clone(),
                cookie_domain: cfg.security.cookie_domain.clone(),
                cookie_secure: cfg.security.cookie_secure,
            };

            if cli.skip_pf_check {
                warn!("PF security check skipped via --skip-pf-check flag (DEVELOPMENT ONLY)");
            } else {
                PfGuard::preflight(&api_security_config)?;
            }
        } else if cli.skip_pf_check {
            trace!("PF security check bypassed: require_pf_deny=false (development mode)");
        }
    }

    // Environment fingerprint drift detection
    info!("Verifying environment fingerprint");
    if !cli.skip_drift_check {
        use adapteros_verify::{
            get_or_create_fingerprint_keypair, DeviceFingerprint, DriftEvaluator,
        };

        let (production_mode, drift_policy) = {
            let cfg = server_config
                .read()
                .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
            (
                cfg.server.production_mode || cfg.security.require_pf_deny,
                cfg.policies.drift.clone(),
            )
        };

        let current_fp = DeviceFingerprint::capture_current()
            .map_err(|e| anyhow::anyhow!("Failed to capture fingerprint: {}", e))?;

        let baseline_path = std::path::PathBuf::from("var/baseline_fingerprint.json");

        if baseline_path.exists() {
            // Load baseline and compare
            let keypair = get_or_create_fingerprint_keypair()
                .map_err(|e| anyhow::anyhow!("Failed to get fingerprint keypair: {}", e))?;
            let baseline = DeviceFingerprint::load_verified(&baseline_path, &keypair.public_key())
                .map_err(|e| anyhow::anyhow!("Failed to load baseline fingerprint: {}", e))?;

            let evaluator = DriftEvaluator::from_policy(&drift_policy);
            let drift_report = evaluator
                .compare(&baseline, &current_fp)
                .map_err(|e| anyhow::anyhow!("Failed to compare fingerprints: {}", e))?;

            if drift_report.should_block() {
                if production_mode {
                    error!("Critical environment drift detected!");
                    error!(summary = %drift_report.summary(), "Critical drift details");
                    for field_drift in &drift_report.field_drifts {
                        error!(
                            field = %field_drift.field_name,
                            baseline = %field_drift.baseline_value,
                            current = %field_drift.current_value,
                            "Drift field"
                        );
                    }
                    return Err(AosError::PolicyViolation(
                        "Refusing to start due to critical environment drift. Run `aosctl drift-check` for details.".to_string()
                    ).into());
                } else {
                    warn!(
                        summary = %drift_report.summary(),
                        "Environment drift detected (development mode, not blocking)"
                    );
                    for field_drift in &drift_report.field_drifts {
                        warn!(
                            field = %field_drift.field_name,
                            baseline = %field_drift.baseline_value,
                            current = %field_drift.current_value,
                            "Drift field"
                        );
                    }
                }
            } else if drift_report.drift_detected {
                warn!(summary = %drift_report.summary(), "Environment drift detected");
                for field_drift in &drift_report.field_drifts {
                    warn!(
                        field = %field_drift.field_name,
                        baseline = %field_drift.baseline_value,
                        current = %field_drift.current_value,
                        "Drift field"
                    );
                }
            } else {
                info!("No environment drift detected");
            }
        } else {
            // First run: auto-create baseline
            warn!("No baseline fingerprint found, creating initial baseline");
            let keypair = get_or_create_fingerprint_keypair()
                .map_err(|e| anyhow::anyhow!("Failed to get fingerprint keypair: {}", e))?;

            // Ensure directory exists
            if let Some(parent) = baseline_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow::anyhow!("Failed to create baseline directory: {}", e))?;
            }

            current_fp
                .save_signed(&baseline_path, &keypair)
                .map_err(|e| anyhow::anyhow!("Failed to save baseline fingerprint: {}", e))?;
            info!(path = ?baseline_path, "Baseline fingerprint created");
        }
    } else {
        warn!("Environment drift check skipped via --skip-drift-check flag (DEVELOPMENT ONLY)");
    }

    // Connect to database / KV based on config
    info!(target: "boot", phase = 5, name = "database", "═══ BOOT PHASE 5/12: Database Connection ═══");
    boot_state.db_connecting().await;
    let db_cfg = server_config
        .read()
        .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
        .db
        .clone();

    let cfg_backend = adapteros_config::StorageBackend::from_str(&db_cfg.storage_mode)
        .unwrap_or(adapteros_config::StorageBackend::Sql);
    let db_backend = match cfg_backend {
        adapteros_config::StorageBackend::Sql => DbStorageBackend::Sql,
        adapteros_config::StorageBackend::Dual => DbStorageBackend::Dual,
        adapteros_config::StorageBackend::KvPrimary => DbStorageBackend::KvPrimary,
        adapteros_config::StorageBackend::KvOnly => DbStorageBackend::KvOnly,
    };

    let kv_path = PathBuf::from(db_cfg.kv_path.clone());
    let kv_tantivy_path = db_cfg.kv_tantivy_path.as_ref().map(PathBuf::from);

    info!(
        db_path = %db_cfg.path,
        storage_mode = %cfg_backend.as_str(),
        kv_path = %kv_path.display(),
        "Connecting to database with storage backend"
    );

    let db = DbFactory::create(
        &db_cfg.path,
        db_cfg.pool_size,
        db_backend,
        Some(kv_path.as_path()),
        kv_tantivy_path.as_deref(),
    )
    .await?;

    // Note: Storage mode adjustment logging removed due to type mismatch
    // TODO: Re-add proper logging when StorageMode implements Display

    // Check atomic dual-write configuration and warn if strict mode is disabled
    {
        use adapteros_db::adapters::AtomicDualWriteConfig;
        let dual_write_config = AtomicDualWriteConfig::from_env();
        if !dual_write_config.is_strict() {
            warn!(
                "PRODUCTION WARNING: Atomic dual-write strict mode is DISABLED. \
                 KV write failures will not rollback SQL writes. \
                 This is not recommended for production. \
                 Set AOS_ATOMIC_DUAL_WRITE_STRICT=1 to enable strict mode."
            );
        } else {
            info!("Atomic dual-write strict mode enabled (production safe)");
        }
    }

    // Upgrade boot state manager with database for audit logging (preserve state)
    let boot_state = boot_state.attach_db(Arc::new(db.clone()));

    // Get hostname for session tracking
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown-host".to_string());

    // Initialize effective config and detect configuration drift
    {
        // Initialize EffectiveConfig with cp.toml path
        // In production mode, config errors are fatal; in dev mode, we warn and continue
        let is_production = server_config
            .read()
            .map(|c| c.server.production_mode)
            .unwrap_or(false);

        if let Err(e) = init_effective_config(Some(&cli.config), vec![]) {
            if is_production {
                error!(error = %e, "FATAL: Failed to initialize effective config in production mode");
                anyhow::bail!(
                    "Failed to initialize config: {}. Production mode requires valid configuration.",
                    e
                );
            }
            warn!(error = %e, "Failed to initialize effective config, continuing with legacy config");
        }

        // Create config snapshot and session
        if let Some(cfg) = try_effective_config() {
            let snapshot = ConfigSnapshot::from_effective_config(cfg);
            let session_id = uuid::Uuid::new_v4().to_string();

            // Check for drift from previous session
            if let Ok(Some(prev_session)) = db.get_most_recent_session(&hostname).await {
                // Parse previous snapshot from JSON
                if let Ok(prev_snapshot) =
                    serde_json::from_str::<ConfigSnapshot>(&prev_session.config_snapshot)
                {
                    let drift = snapshot.diff(&prev_snapshot);
                    if drift.drift_detected {
                        warn!(
                            config_hash = %snapshot.hash,
                            previous_hash = %drift.previous_hash,
                            changed_fields = drift.field_count,
                            "Configuration drift detected from previous session"
                        );
                        for field in &drift.fields {
                            match field.severity {
                                adapteros_config::ConfigDriftSeverity::Critical => {
                                    error!(key = %field.key, old = %field.old_value, new = %field.new_value, "CRITICAL config change");
                                }
                                adapteros_config::ConfigDriftSeverity::Warning => {
                                    warn!(key = %field.key, old = %field.old_value, new = %field.new_value, "Config change");
                                }
                                adapteros_config::ConfigDriftSeverity::Info => {
                                    info!(key = %field.key, old = %field.old_value, new = %field.new_value, "Config change");
                                }
                            }
                        }
                    }
                }
            }

            // Determine runtime mode string
            let runtime_mode_str = if server_config
                .read()
                .map(|c| c.server.production_mode)
                .unwrap_or(false)
            {
                "production"
            } else {
                "development"
            };

            // Create new session record
            let model_path = cfg.model.path.as_ref().map(|p| p.display().to_string());
            let adapters_root = Some(cfg.paths.adapters_root.display().to_string());
            let var_dir = Some(cfg.paths.var_dir.display().to_string());

            let new_session = RuntimeSession {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: session_id.clone(),
                config_hash: snapshot.hash.clone(),
                binary_version: env!("CARGO_PKG_VERSION").to_string(),
                binary_commit: option_env!("GIT_COMMIT").map(|s| s.to_string()),
                started_at: chrono::Utc::now().to_rfc3339(),
                ended_at: None,
                end_reason: None,
                hostname: hostname.clone(),
                runtime_mode: runtime_mode_str.to_string(),
                config_snapshot: serde_json::to_string(&snapshot).unwrap_or_default(),
                drift_detected: false, // Updated after diff
                drift_summary: None,
                previous_session_id: None,
                model_path,
                adapters_root,
                database_path: Some(db_cfg.path.clone()),
                var_dir,
            };

            // Insert session record
            if let Err(e) = db.insert_runtime_session(&new_session).await {
                warn!(error = %e, "Failed to record runtime session");
            } else {
                info!(
                    session_id = %session_id,
                    config_hash = %snapshot.hash,
                    "Runtime session started"
                );
            }
        }
    };

    // Freeze configuration guards to prevent environment variable access after boot
    // This is a security measure to ensure deterministic behavior during request handling
    {
        use adapteros_config::guards::ConfigGuards;

        // Initialize guard system (idempotent if already initialized)
        if let Err(e) = ConfigGuards::initialize() {
            warn!(error = %e, "Failed to initialize config guards (may already be initialized)");
        }

        // Freeze configuration - any env var access after this point will be logged as a violation
        if let Err(e) = ConfigGuards::freeze() {
            // Non-fatal in dev mode, but log it prominently
            let is_production = server_config
                .read()
                .map(|c| c.server.production_mode)
                .unwrap_or(false);

            if is_production {
                error!(error = %e, "FATAL: Failed to freeze configuration guards in production mode");
                anyhow::bail!(
                    "Configuration guard freeze failed: {}. Production mode requires deterministic configuration.",
                    e
                );
            }
            warn!(error = %e, "Failed to freeze configuration guards");
        } else {
            info!("Configuration guards frozen - environment variable access now prohibited");
        }
    }

    // Initialize global tick ledger for inference tracking

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
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
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
                boot_timeout_secs: 300,
                health_check_db_timeout_ms: 2000,
                health_check_worker_timeout_ms: 5000,
                health_check_models_timeout_ms: 15000,
            },
            db: adapteros_server_api::config::DatabaseConfig {
                path: String::new(), // Unused
                pool_size: 5,
                storage_mode: "sql_only".to_string(),
                kv_path: String::new(),
                kv_tantivy_path: None,
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
                jwt_additional_ed25519_public_keys: None,
                jwt_additional_hmac_secrets: None,
                dev_login_enabled: false,
                require_mfa: None,
                token_ttl_seconds: None,
                access_token_ttl_seconds: 15 * 60,
                session_ttl_seconds: 12 * 3600,
                jwt_mode: None,
                cookie_same_site: "Lax".to_string(),
                cookie_domain: None,
                cookie_secure: None,
            },
            auth: adapteros_server_api::config::AuthConfig {
                dev_algo: "hs256".to_string(),
                prod_algo: "eddsa".to_string(),
                session_lifetime: 12 * 3600,
                lockout_threshold: 5,
                lockout_cooldown: 300,
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
            self_hosting: Default::default(),
            git: None,
            policies: Default::default(),
            logging: Default::default(),
            otel: Default::default(),
        };

        RuntimeModeResolver::resolve(&api_cfg, &db)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to resolve runtime mode: {}", e))?
    };

    info!(
        mode = %runtime_mode,
        allows_http = runtime_mode.allows_http(),
        requires_telemetry = runtime_mode.requires_telemetry(),
        requires_signing = runtime_mode.requires_event_signing(),
        "Runtime mode resolved"
    );

    // Validate runtime mode configuration (production security requirements)
    {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        let production_mode = cfg.server.production_mode;

        // Production mode validation: JWT secret must be at least 32 characters
        if production_mode {
            let jwt_secret_len = cfg.security.jwt_secret.len();
            if jwt_secret_len < 32 {
                error!(
                    jwt_secret_len,
                    required = 32,
                    "FATAL: JWT secret is too short for production mode"
                );
                anyhow::bail!(
                    "Production mode requires JWT secret of at least 32 characters (current: {}). \
                     Generate with: openssl rand -hex 32",
                    jwt_secret_len
                );
            }
            info!("JWT secret length validated for production mode");
        }

        // Create API config with actual values for runtime mode validation
        let api_cfg = adapteros_server_api::config::Config {
            server: adapteros_server_api::config::ServerConfig {
                port: cfg.server.port,
                bind: cfg.server.bind.clone(),
                production_mode,
                uds_socket: cfg.server.uds_socket.clone(),
                drain_timeout_secs: cfg.server.drain_timeout_secs,
                boot_timeout_secs: cfg.server.boot_timeout_secs,
                health_check_db_timeout_ms: 2000,
                health_check_worker_timeout_ms: 5000,
                health_check_models_timeout_ms: 15000,
            },
            db: adapteros_server_api::config::DatabaseConfig {
                path: cfg.db.path.clone(),
                pool_size: cfg.db.pool_size,
                storage_mode: cfg.db.storage_mode.clone(),
                kv_path: cfg.db.kv_path.clone(),
                kv_tantivy_path: cfg.db.kv_tantivy_path.clone(),
            },
            security: adapteros_server_api::config::SecurityConfig {
                require_pf_deny: cfg.security.require_pf_deny,
                mtls_required: cfg.security.mtls_required,
                jwt_secret: cfg.security.jwt_secret.clone(),
                jwt_ttl_hours: cfg.security.jwt_ttl_hours,
                key_provider_mode: cfg.security.key_provider_mode.clone(),
                key_file_path: cfg.security.key_file_path.clone(),
                jwt_issuer: cfg.security.jwt_issuer.clone(),
                jwt_audience: cfg.security.jwt_audience.clone(),
                jwt_additional_ed25519_public_keys: None,
                jwt_additional_hmac_secrets: None,
                dev_login_enabled: cfg.security.dev_login_enabled,
                require_mfa: cfg.security.require_mfa,
                token_ttl_seconds: cfg.security.token_ttl_seconds,
                access_token_ttl_seconds: 15 * 60,
                session_ttl_seconds: 12 * 3600,
                jwt_mode: cfg.security.jwt_mode.clone(),
                cookie_same_site: "Lax".to_string(),
                cookie_domain: None,
                cookie_secure: None,
            },
            auth: adapteros_server_api::config::AuthConfig {
                dev_algo: cfg.auth.dev_algo.clone(),
                prod_algo: cfg.auth.prod_algo.clone(),
                session_lifetime: cfg.auth.session_lifetime,
                lockout_threshold: cfg.auth.lockout_threshold,
                lockout_cooldown: cfg.auth.lockout_cooldown,
            },
            paths: adapteros_server_api::config::PathsConfig {
                artifacts_root: cfg.paths.artifacts_root.clone(),
                bundles_root: cfg.paths.bundles_root.clone(),
                adapters_root: cfg.paths.adapters_root.clone(),
                plan_dir: cfg.paths.plan_dir.clone(),
                datasets_root: cfg.paths.datasets_root.clone(),
                documents_root: cfg.paths.documents_root.clone(),
            },
            rate_limits: adapteros_server_api::config::RateLimitsConfig {
                requests_per_minute: cfg.rate_limits.requests_per_minute,
                burst_size: cfg.rate_limits.burst_size,
                inference_per_minute: cfg.rate_limits.inference_per_minute,
            },
            metrics: adapteros_server_api::config::MetricsConfig {
                enabled: cfg.metrics.enabled,
                bearer_token: cfg.metrics.bearer_token.clone(),
                histogram_buckets: cfg.metrics.histogram_buckets.clone(),
                include_histogram: cfg.metrics.include_histogram,
            },
            alerting: adapteros_server_api::config::AlertingConfig {
                enabled: cfg.alerting.enabled,
                alert_dir: cfg.alerting.alert_dir.clone(),
                max_alerts_per_file: cfg.alerting.max_alerts_per_file,
                rotate_size_mb: cfg.alerting.rotate_size_mb,
            },
            self_hosting: Default::default(),
            git: None,
            policies: Default::default(),
            logging: Default::default(),
            otel: Default::default(),
        };

        // Drop the read lock before async validation
        drop(cfg);

        RuntimeModeResolver::validate(runtime_mode, &api_cfg, &db)
            .await
            .map_err(|e| anyhow::anyhow!("Runtime mode validation failed: {}", e))?;
    }

    // Audit log: Executor bootstrap event
    {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        let metadata = serde_json::json!({
            "manifest_path": manifest_path.display().to_string(),
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
            warn!(error = %e, "Failed to log executor bootstrap audit event");
        } else {
            info!("Executor bootstrap event logged to audit trail");
        }
    }

    let sql_enabled = db.storage_mode().write_to_sql() || db.storage_mode().read_from_sql();

    info!(target: "boot", phase = 6, name = "migrations", "═══ BOOT PHASE 6/12: Database Migrations ═══");

    if sql_enabled {
        // Run migrations with Ed25519 signature verification
        info!("Running database migrations...");
        if let Err(e) = db.migrate().await {
            error!(
                target: "boot",
                code = %FailureCode::MigrationInvalid.as_str(),
                request_id = "-",
                tenant_id = "system",
                error = %e,
                "Database migrations failed"
            );
            return Err(e.into());
        }

        // Recover from any previous crash (orphaned adapters, stale state)
        info!("Running crash recovery checks...");
        db.recover_from_crash()
            .await
            .map_err(|e| anyhow::anyhow!("Crash recovery failed: {}", e))?;

        // Seed development data
        if let Err(e) = db.seed_dev_data().await {
            warn!(error = %e, "Failed to seed development data");
        }

        if let Err(e) = seed_models_from_cache_if_empty(&db).await {
            warn!(error = %e, "Failed to seed cached base models");
        }
    } else {
        info!("SQL backend disabled; skipping migrations, crash recovery, and SQL seed steps");
    }

    if cli.migrate_only {
        info!("Migrations complete, exiting");
        std::process::exit(0);
    }

    // Transition to loading policies state
    info!(target: "boot", phase = 7, name = "policies", "═══ BOOT PHASE 7/12: Policy Loading ═══");
    boot_state.load_policies().await;

    // Transition to starting backend state
    info!(target: "boot", phase = 8, name = "backend", "═══ BOOT PHASE 8/12: Backend Initialization ═══");
    boot_state.start_backend().await;

    // Transition to loading base models state
    info!(target: "boot", phase = 9, name = "models", "═══ BOOT PHASE 9/12: Base Model Loading ═══");
    boot_state.load_base_models().await;

    // Download priority models from HuggingFace Hub if enabled
    download_priority_models().await;

    // Create API config (subset needed by handlers)
    let api_config = {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        Arc::new(RwLock::new(adapteros_server_api::state::ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: cfg.metrics.enabled,
                bearer_token: cfg.metrics.bearer_token.clone(),
            },
            directory_analysis_timeout_secs: 120,
            use_session_stack_for_routing: false, // Default until routing config is added
            capacity_limits: Default::default(),
            general: None,
            server: adapteros_server_api::state::ServerConfigApi {
                http_port: Some(cfg.server.port),
                https_port: None,
                uds_socket: cfg.server.uds_socket.clone(),
                production_mode: cfg.server.production_mode,
                health_check_db_timeout_ms: cfg.server.health_check_db_timeout_ms,
                health_check_worker_timeout_ms: 5000,
                health_check_models_timeout_ms: 15000,
            },
            security: adapteros_server_api::state::SecurityConfigApi {
                jwt_mode: cfg.security.jwt_mode.clone(),
                token_ttl_seconds: cfg.security.token_ttl_seconds,
                access_token_ttl_seconds: Some(cfg.security.access_token_ttl_seconds),
                session_ttl_seconds: Some(cfg.security.session_ttl_seconds),
                jwt_additional_ed25519_public_keys: cfg
                    .security
                    .jwt_additional_ed25519_public_keys
                    .clone(),
                jwt_additional_hmac_secrets: cfg.security.jwt_additional_hmac_secrets.clone(),
                require_mfa: cfg.security.require_mfa,
                require_pf_deny: cfg.security.require_pf_deny,
                dev_login_enabled: cfg.security.dev_login_enabled,
                cookie_same_site: Some(cfg.security.cookie_same_site.clone()),
                cookie_domain: cfg.security.cookie_domain.clone(),
                cookie_secure: cfg.security.cookie_secure,
            },
            auth: adapteros_server_api::state::AuthConfigApi {
                dev_algo: cfg.auth.dev_algo.clone(),
                prod_algo: cfg.auth.prod_algo.clone(),
                session_lifetime: cfg.auth.session_lifetime,
                lockout_threshold: cfg.auth.lockout_threshold,
                lockout_cooldown: cfg.auth.lockout_cooldown,
            },
            self_hosting: adapteros_server_api::state::SelfHostingConfigApi {
                mode: cfg.self_hosting.mode.clone(),
                repo_allowlist: cfg.self_hosting.repo_allowlist.clone(),
                promotion_threshold: cfg.self_hosting.promotion_threshold,
                require_human_approval: cfg.self_hosting.mode.eq_ignore_ascii_case("safe"),
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
            chat_context: Default::default(),
            seed_mode: SeedMode::default(),
            backend_profile: BackendKind::default_inference_backend(),
            worker_id: 0,
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
                                cfg.self_hosting = new_config.self_hosting.clone();
                            }
                            Err(e) => {
                                error!(error = %e, "Config lock poisoned during reload");
                                continue;
                            }
                        }
                        // Update API config subset
                        match api_config_clone.write() {
                            Ok(mut api_cfg) => {
                                api_cfg.metrics.enabled = new_config.metrics.enabled;
                                api_cfg.metrics.bearer_token =
                                    new_config.metrics.bearer_token.clone();
                                api_cfg.self_hosting.mode = new_config.self_hosting.mode.clone();
                                api_cfg.self_hosting.repo_allowlist =
                                    new_config.self_hosting.repo_allowlist.clone();
                                api_cfg.self_hosting.promotion_threshold =
                                    new_config.self_hosting.promotion_threshold;
                                api_cfg.self_hosting.require_human_approval =
                                    new_config.self_hosting.mode.eq_ignore_ascii_case("safe");
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
                                error!(error = %e, "API config lock poisoned during reload");
                                continue;
                            }
                        }
                        info!("Config reloaded successfully");
                    }
                    Err(e) => error!(error = %e, "Failed to reload config"),
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
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        if cfg.alerting.enabled {
            info!("Starting alert watcher");
            // Convert server AlertingConfig to API AlertingConfig
            let api_alerting_config = adapteros_server_api::config::AlertingConfig {
                enabled: cfg.alerting.enabled,
                alert_dir: cfg.alerting.alert_dir.clone(),
                max_alerts_per_file: cfg.alerting.max_alerts_per_file,
                rotate_size_mb: cfg.alerting.rotate_size_mb,
            };
            let alert_handle = alerting::spawn_alert_watcher(db.clone(), api_alerting_config)
                .map_err(|e| anyhow::anyhow!("Failed to spawn alert watcher: {}", e))?;
            shutdown_coordinator.set_alert_handle(alert_handle);
        }
    }

    // Initialize policy hash watcher (continuous monitoring)
    // Create telemetry writer and policy watcher first (needed by federation daemon)
    let (policy_watcher, telemetry) = {
        info!("Initializing policy hash watcher");

        // Create telemetry writer
        let bundles_path = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
            .paths
            .bundles_root
            .clone();

        std::fs::create_dir_all(&bundles_path)
            .map_err(|e| anyhow::anyhow!("Failed to create bundles directory: {}", e))?;

        let telemetry = Arc::new(
            adapteros_telemetry::TelemetryWriter::new(
                &bundles_path,
                10000,            // max_events_per_bundle
                50 * 1024 * 1024, // max_bundle_size (50MB)
            )
            .map_err(|e| anyhow::anyhow!("Failed to create telemetry writer: {}", e))?,
        );

        // Create policy hash watcher
        let policy_watcher = Arc::new(adapteros_policy::PolicyHashWatcher::new(
            Arc::new(db.clone()),
            telemetry.clone(),
            None, // cpid - will be set per-tenant
        ));

        // Load baseline hashes from database
        if let Err(e) = policy_watcher.load_cache().await {
            warn!(error = %e, "Failed to load policy hash cache");
        }

        // Start background watcher (60 second interval)
        let policy_hashes = Arc::new(RwLock::new(std::collections::HashMap::new()));
        let watcher_handle = policy_watcher
            .clone()
            .start_background_watcher(Duration::from_secs(60), policy_hashes.clone());
        shutdown_coordinator.set_policy_watcher_handle(watcher_handle);

        info!("Policy hash watcher started (60s interval)");

        (policy_watcher, telemetry)
    };

    // Initialize Federation Daemon (needs policy_watcher and telemetry from above)
    info!("Initializing federation daemon");

    let federation_keypair = adapteros_crypto::Keypair::generate();
    let federation_manager = Arc::new(
        adapteros_federation::FederationManager::new(
            db.clone(),
            federation_keypair,
            "default".to_string(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to create federation manager: {}", e))?,
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

    let federation_shutdown_rx = shutdown_coordinator.subscribe_shutdown();
    let federation_daemon_for_state = federation_daemon.clone();
    let federation_handle = federation_daemon.start(federation_shutdown_rx);
    shutdown_coordinator.set_federation_handle(federation_handle);
    info!("Federation daemon started (300s interval)");

    // Initialize UDS metrics exporter (zero-network metrics per Egress Ruleset #1)
    {
        info!("Initializing UDS metrics exporter");

        let socket_path = PathBuf::from("var/run/metrics.sock");

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
                                }
                                _ = kv_shutdown_rx.recv() => {
                                    info!("KV metrics exporter loop shutting down");
                                    break;
                                }
                            }
                        }
                    });
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
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        Arc::new(adapteros_metrics_exporter::MetricsExporter::new(
            cfg.metrics.histogram_buckets.clone(),
        )?)
    };

    // Build application state
    let jwt_secret = {
        let cfg = server_config
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
            let jwt_key_path = PathBuf::from(&keys_dir).join("jwt_signing.key");
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

    // Initialize worker health monitor for Worker Health, Hung Detection & Log Centralization
    info!(target: "boot", phase = 10, name = "services", "═══ BOOT PHASE 10/12: Service Initialization ═══");

    info!("Initializing worker health monitor");
    let health_monitor = Arc::new(WorkerHealthMonitor::with_defaults(db.clone()));
    {
        let monitor_clone = Arc::clone(&health_monitor);
        match spawn_deterministic("Worker health monitor".to_string(), async move {
            monitor_clone.run_polling_loop().await;
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!(
                    polling_interval_secs = 30,
                    latency_threshold_ms = 5000,
                    consecutive_slow_count = 5,
                    "Worker health monitor started"
                );
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to spawn worker health monitor, health checks will be unavailable"
                );
            }
        }
    }

    // Create metrics collector and registry for AppState
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new(
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
    let training_service = Arc::new(adapteros_orchestrator::TrainingService::with_db(
        db.clone(),
        training_storage_root.clone(),
    ));
    info!(
        path = %training_storage_root.display(),
        "Training service initialized with DB-backed storage root"
    );

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
    .with_strict_mode(cli.strict)
    .with_tick_ledger(tick_ledger.clone())
    .with_health_monitor(health_monitor.clone())
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

    // Load embedding model for RAG if embeddings feature enabled
    #[cfg(feature = "embeddings")]
    {
        use adapteros_ingest_docs::EmbeddingModel;
        use adapteros_server_api::state::RagStatus;
        use std::path::Path;

        let embedding_model = match adapteros_config::resolve_embedding_model_path() {
            Ok(resolved) => Some(resolved),
            Err(e) => {
                let reason = format!("Failed to resolve embedding model path: {}", e);

                // Use ERROR level in production, WARN in dev
                if production_mode {
                    error!(error = %e, "Embedding model path invalid, RAG disabled");
                } else {
                    warn!(error = %e, "Embedding model path invalid, RAG disabled");
                }

                state = state.with_rag_status(RagStatus::Disabled { reason });
                None
            }
        };

        if let Some(embedding_model) = embedding_model {
            let embedding_model_path = embedding_model.path;
            let tokenizer_path = embedding_model_path.join("tokenizer.json");
            info!(
                path = %embedding_model_path.display(),
                tokenizer_path = %tokenizer_path.display(),
                source = %embedding_model.source,
                dev_fallback = embedding_model.used_dev_fallback,
                "Resolved embedding model paths for RAG"
            );

            if tokenizer_path.exists() {
                match adapteros_ingest_docs::load_tokenizer(&tokenizer_path) {
                    Ok(tokenizer) => {
                        let embedding_model_path_str = embedding_model_path.to_string_lossy();
                        let embedding_model =
                            Arc::new(adapteros_ingest_docs::ProductionEmbeddingModel::load(
                                Some(&embedding_model_path_str),
                                tokenizer,
                            ));

                        let model_hash = embedding_model.model_hash().to_hex()[..16].to_string();
                        let dimension = embedding_model.dimension();

                        info!(
                            path = %embedding_model_path.display(),
                            source = %embedding_model.source,
                            dimension = dimension,
                            hash = %model_hash,
                            "Loaded embedding model for RAG"
                        );

                        state = state.with_embedding_model(embedding_model).with_rag_status(
                            RagStatus::Enabled {
                                model_hash,
                                dimension,
                            },
                        );
                    }
                    Err(e) => {
                        let reason = format!("Failed to load tokenizer: {}", e);

                        // Use ERROR level in production, WARN in dev
                        if production_mode {
                            error!(error = %e, path = %tokenizer_path.display(), "Failed to load tokenizer for embedding model, RAG disabled");
                        } else {
                            warn!(error = %e, path = %tokenizer_path.display(), "Failed to load tokenizer for embedding model, RAG disabled");
                        }

                        state = state.with_rag_status(RagStatus::Disabled { reason });
                    }
                }
            } else {
                let reason = format!("Tokenizer not found at: {}", tokenizer_path.display());

                // Use ERROR level in production, WARN in dev
                if production_mode {
                    error!(
                        path = %tokenizer_path.display(),
                        "Embedding model tokenizer not found, RAG disabled. \
                         Set AOS_EMBEDDING_MODEL_PATH to point to a sentence-transformer model."
                    );
                } else {
                    warn!(path = %tokenizer_path.display(), "Embedding model tokenizer not found, RAG disabled.                      Set AOS_EMBEDDING_MODEL_PATH to point to a sentence-transformer model.");
                }

                state = state.with_rag_status(RagStatus::Disabled { reason });
            }
        }
    }

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

    // Spawn status writer background task (using BackgroundTaskSpawner)
    {
        let state_clone = state.clone();
        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator);
        let _ = spawner.spawn_with_details(
            "Status writer",
            async move {
                let mut interval = tokio::time::interval(Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    if let Err(e) = status_writer::write_status(&state_clone).await {
                        warn!(error = %e, "Failed to write status");
                    }
                }
            },
            "5s interval",
        );
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

        let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator);
        let _ = spawner.spawn_with_details(
            "KV isolation scan",
            async move {
                let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
                interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

                loop {
                    interval.tick().await;
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
            },
            &format!("{}s interval, read-only, deterministic ordering", interval_secs),
        );
        shutdown_coordinator = spawner.into_coordinator();
    }

    // Spawn KV metrics alert monitor (drift/fallback/error/degraded)
    {
        let metrics_registry = Arc::clone(&metrics_registry);
        match spawn_deterministic("KV alert monitor".to_string(), async move {
            let mut alerting = AlertingEngine::new(100);
            for rule in kv_metrics::kv_alert_rules() {
                alerting.register_rule(rule);
            }

            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

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
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!("KV alert monitor started (5s interval)");
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to spawn KV alert monitor; KV alerting will be disabled"
                );
            }
        }
    }

    // Spawn log cleanup background task
    {
        let cfg = server_config.read().map_err(|e| {
            error!(error = %e, "Config lock poisoned during log cleanup setup");
            anyhow::anyhow!("config lock poisoned")
        })?;

        if let Some(ref log_dir) = cfg.logging.log_dir {
            if cfg.logging.retention_days > 0 {
                let log_dir = log_dir.clone();
                let log_dir_for_info = log_dir.clone();
                let retention_days = cfg.logging.retention_days;

                // Run cleanup on startup
                if let Err(e) = cleanup_old_logs(&log_dir, retention_days).await {
                    error!(error = %e, "Failed to cleanup old logs on startup");
                }

                // Spawn daily cleanup task
                match spawn_deterministic("Log cleanup".to_string(), async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(86400)); // 24 hours
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                    loop {
                        interval.tick().await;

                        match cleanup_old_logs(&log_dir, retention_days).await {
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
                }) {
                    Ok(handle) => {
                        shutdown_coordinator.register_task(handle);
                        info!(
                            retention_days,
                            log_dir = %log_dir_for_info,
                            "Log cleanup task started (daily interval)"
                        );
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            "Failed to spawn log cleanup task; old logs will not be automatically deleted"
                        );
                    }
                }
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

    // Spawn WAL checkpoint background task
    {
        let db_clone = db.clone();
        match spawn_deterministic("WAL checkpoint".to_string(), async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

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
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!("WAL checkpoint task started (5 minute interval)");
            }
            Err(e) => {
                // Non-fatal: SQLite will still auto-checkpoint, just less frequently
                warn!(
                    error = %e,
                    "Failed to spawn WAL checkpoint task; relying on auto-checkpoint only"
                );
            }
        }
    }

    // Spawn DB index health monitor + maintenance automation
    {
        let state_clone = state.clone();
        match spawn_deterministic("DB index monitor".to_string(), async move {
            db_index_monitor::run_db_index_monitor(state_clone).await;
        }) {
            Ok(handle) => {
                shutdown_coordinator.register_task(handle);
                info!("DB index monitor started");
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to spawn DB index monitor; index health monitoring will be disabled"
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
    info!(target: "boot", phase = 11, name = "adapters", "═══ BOOT PHASE 11/12: Adapter Loading ═══");
    boot_state.load_adapters().await;

    // Clone in_flight_requests counter for shutdown handler before moving state
    let in_flight_requests = Arc::clone(&state.in_flight_requests);

    // Build router with UI
    let api_routes = routes::build(state);
    let ui_routes = assets::routes();

    // NOTE: Legacy root-level API shims removed due to fallback handler conflict.
    // All API endpoints should use the `/api/*` prefix.
    let app = axum::Router::new()
        .nest("/api", api_routes) // API routes under /api prefix
        .merge(ui_routes); // UI fallback for non-API paths

    // Bind and serve
    let (production_mode, uds_socket, bind, port, drain_timeout) = {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        // Environment variables take precedence over config file.
        //
        // Canonical: AOS_SERVER_PORT
        // Legacy aliases (demo safety): ADAPTEROS_SERVER_PORT, API_PORT
        let server_port = std::env::var("AOS_SERVER_PORT")
            .or_else(|_| std::env::var("ADAPTEROS_SERVER_PORT"))
            .or_else(|_| std::env::var("API_PORT"))
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(cfg.server.port);

        (
            cfg.server.production_mode,
            cfg.server.uds_socket.clone(),
            cfg.server.bind.clone(),
            server_port,
            Duration::from_secs(cfg.server.drain_timeout_secs),
        )
    };

    // =========================================================================
    // Boot Report Generation
    // =========================================================================
    // Emit boot report to file and log. In strict mode, this is required.
    info!(target: "boot", phase = 12, name = "finalization", "═══ BOOT PHASE 12/12: Finalization ═══");
    {
        // Serialize config for hashing. Handle errors explicitly instead of silently
        // producing an empty hash which would mask config issues.
        let config_bytes = {
            let config_guard = server_config.read().unwrap_or_else(|e| e.into_inner());
            match serde_json::to_vec(&*config_guard) {
                Ok(bytes) => bytes,
                Err(e) => {
                    if cli.strict {
                        error!(error = %e, "STRICT MODE: Failed to serialize config for boot report");
                        return Err(anyhow::anyhow!("Config serialization failed: {}", e));
                    }
                    // In non-strict mode, use error message as input to produce an identifiable hash
                    // instead of an empty hash that would be indistinguishable from a real empty config
                    warn!(error = %e, "Failed to serialize config for boot report, using placeholder hash");
                    format!("CONFIG_SERIALIZE_ERROR:{}", e).into_bytes()
                }
            }
        };

        let mut report_builder = BootReport::builder()
            .config_hash_from_bytes(&config_bytes)
            .bind_addr(bind.to_string())
            .port(port);

        // Add worker key ID if available
        if let Some(ref keypair) = worker_signing_keypair {
            let kid = adapteros_boot::derive_kid_from_verifying_key(&keypair.verifying_key());
            report_builder = report_builder.add_worker_key_kid(kid);
        }

        let report = report_builder.build();

        // Emit to log
        report.emit_log();

        // Write to file
        let report_path = "var/run/boot_report.json";
        match report.write_to_file(report_path) {
            Ok(()) => {
                info!(path = %report_path, "Boot report written");
            }
            Err(e) => {
                if cli.strict {
                    error!(
                        error = %e,
                        path = %report_path,
                        "STRICT MODE: Failed to write boot report"
                    );
                    return Err(anyhow::anyhow!(
                        "Strict mode requires boot report at {}",
                        report_path
                    ));
                } else {
                    warn!(error = %e, path = %report_path, "Failed to write boot report");
                }
            }
        }
    }

    // Return all boot artifacts needed for server startup
    Ok::<_, anyhow::Error>((
        boot_state.clone(),
        in_flight_requests,
        app,
        production_mode,
        uds_socket,
        bind,
        port,
        drain_timeout,
        shutdown_coordinator,
    ))
    }).await;

    // Handle boot timeout
    let (
        boot_state,
        in_flight_requests,
        app,
        production_mode,
        uds_socket,
        bind,
        port,
        drain_timeout,
        shutdown_coordinator,
    ) = match boot_result {
        Ok(Ok(artifacts)) => {
            let boot_duration = boot_start.elapsed();
            info!(
                target: "boot",
                duration_ms = boot_duration.as_millis() as u64,
                duration_secs = format!("{:.1}", boot_duration.as_secs_f64()),
                "╔═══════════════════════════════════════════════════════════════╗"
            );
            info!(
                target: "boot",
                "║             BOOT COMPLETE - AdapterOS Ready                   ║"
            );
            info!(
                target: "boot",
                duration_secs = format!("{:.1}s", boot_duration.as_secs_f64()),
                "╚═══════════════════════════════════════════════════════════════╝"
            );
            artifacts
        }
        Ok(Err(e)) => {
            error!(error = %e, "Boot sequence failed with error");
            return Err(e);
        }
        Err(_) => {
            let current_state = boot_state_for_timeout.current_state();
            error!(
                timeout_secs = %boot_timeout.as_secs(),
                boot_state = ?current_state,
                "Boot sequence exceeded timeout - server failed to initialize in time"
            );
            eprintln!(
                "FATAL: Boot timeout after {} seconds. Boot was stuck in state: {:?}",
                boot_timeout.as_secs(),
                current_state
            );
            // Exit with Config error code (10)
            std::process::exit(10);
        }
    };

    // Egress policy: production_mode requires UDS-only
    if production_mode {
        let socket_path: String = uds_socket.ok_or_else(|| {
            anyhow::anyhow!(
                "Egress policy violation: production_mode requires uds_socket configuration"
            )
        })?;

        info!(socket_path = %socket_path, "Starting control plane on UDS");
        info!("Production mode enabled - TCP binding disabled per Egress policy");

        // Remove existing socket file if present
        let _ = std::fs::remove_file(&socket_path);

        // Bind first, fail fast if socket cannot be created
        let listener = match tokio::net::UnixListener::bind(&socket_path) {
            Ok(l) => l,
            Err(e) => {
                error!(
                    socket = %socket_path,
                    error = %e,
                    "Failed to bind UDS socket: {}. Check permissions or remove stale socket: rm {}",
                    e,
                    socket_path
                );
                std::process::exit(10);
            }
        };

        // Now safe to mark ready - socket is secured
        boot_state.ready().await;
        // Mark fully ready once boot tasks have completed
        boot_state.fully_ready().await;
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
                        warn!(failed_count = failed_count, "Partial shutdown failure - components failed but system integrity maintained");
                        // Don't exit - partial failures are acceptable
                    }
                    _ => {
                        error!(error = %e, "Shutdown error");
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
        let bind_ip = bind.parse::<IpAddr>().unwrap_or_else(|_| {
            warn!(bind = %bind, "Invalid server.bind; falling back to 127.0.0.1");
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        });

        let addr = SocketAddr::from((bind_ip, port));
        let display_host = if bind_ip == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            bind_ip
        };
        info!(addr = %addr, "Starting control plane");
        info!(url = %format!("http://{}:{}/", display_host, port), "UI available");
        info!(url = %format!("http://{}:{}/api/", display_host, port), "API available");
        warn!("Development mode: TCP binding enabled. Set production_mode=true for UDS-only");

        // Bind first, fail fast if port in use
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                error!(
                    port = port,
                    addr = %addr,
                    "Port {} already in use. Kill existing process: lsof -ti:{} | xargs kill",
                    port,
                    port
                );
                std::process::exit(10);
            }
            Err(e) => return Err(e.into()),
        };

        // Now safe to mark ready - port is secured
        boot_state.ready().await;
        // Mark fully ready once boot tasks have completed
        boot_state.fully_ready().await;
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
                        warn!(failed_count = failed_count, "Partial shutdown failure - components failed but system integrity maintained");
                        // Don't exit - partial failures are acceptable
                    }
                    _ => {
                        error!(error = %e, "Shutdown error");
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

/// Cleanup old log files based on retention policy
///
/// Deletes log files older than the specified retention period.
/// Returns the number of files deleted.
async fn cleanup_old_logs(log_dir: &str, retention_days: u32) -> Result<usize> {
    use std::time::SystemTime;

    let retention_duration = std::time::Duration::from_secs(retention_days as u64 * 86400);
    let now = SystemTime::now();
    let mut deleted_count = 0;

    let log_path = std::path::Path::new(log_dir);
    if !log_path.exists() {
        return Ok(0);
    }

    let entries = tokio::fs::read_dir(log_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read log directory: {}", e))?;

    let mut entries = entries;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read directory entry: {}", e))?
    {
        let path = entry.path();

        // Only process files (not directories)
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to read file metadata, skipping"
                );
                continue;
            }
        };

        if !metadata.is_file() {
            continue;
        }

        // Check if file is old enough to delete
        let modified = match metadata.modified() {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to get file modification time, skipping"
                );
                continue;
            }
        };

        let age = match now.duration_since(modified) {
            Ok(d) => d,
            Err(_) => continue, // File modified in the future? Skip it
        };

        if age > retention_duration {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {
                    deleted_count += 1;
                    info!(
                        path = %path.display(),
                        age_days = age.as_secs() / 86400,
                        "Deleted old log file"
                    );
                }
                Err(e) => {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to delete old log file"
                    );
                }
            }
        }
    }

    Ok(deleted_count)
}

/// Initialize logging with configuration-based settings
///
/// Sets up tracing with:
/// - Console output (always)
/// - File output with rotation (if log_dir configured)
/// - Configurable log levels
/// - JSON or human-readable format
///
/// Returns guards that must be kept alive for the duration of the program
/// to ensure log files are properly flushed and OpenTelemetry spans are exported.
fn initialize_logging(
    config: &adapteros_server_api::config::LoggingConfig,
    otel_config: &adapteros_server_api::config::OtelConfig,
) -> Result<(
    Option<tracing_appender::non_blocking::WorkerGuard>,
    Option<otel::OtelGuard>,
)> {
    use tracing_subscriber::EnvFilter;

    // Parse log level from config or environment
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    // Determine rotation strategy
    let rotation = match config.rotation.as_str() {
        "hourly" => Rotation::HOURLY,
        "daily" => Rotation::DAILY,
        "never" => Rotation::NEVER,
        _ => {
            eprintln!(
                "WARNING: Unknown rotation '{}', defaulting to daily",
                config.rotation
            );
            Rotation::DAILY
        }
    };

    // Set up file logging if log_dir is configured
    let (file_layer, guard) = if let Some(ref log_dir) = config.log_dir {
        // Ensure log directory exists
        std::fs::create_dir_all(log_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create log directory {}: {}", log_dir, e))?;

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

    // Try to initialize OpenTelemetry (graceful degradation on failure)
    let (otel_tracer, otel_guard) = match otel::init_otel(otel_config) {
        Ok(Some((tracer, guard))) => (Some(tracer), Some(guard)),
        Ok(None) => (None, None),
        Err(e) => {
            eprintln!(
                "WARNING: OpenTelemetry initialization failed: {}. Continuing without OTLP export.",
                e
            );
            (None, None)
        }
    };

    // Create the OTel layer inline to avoid type composition issues with boxed layers.
    // The layer is created from the tracer here rather than in otel.rs so that the
    // type system can properly compose it with the other layers.
    let otel_layer = otel_tracer.map(|tracer| tracing_opentelemetry::layer().with_tracer(tracer));

    // Build the subscriber with all layers.
    // Option<L> implements Layer<S> where None is a no-op, allowing conditional composition.
    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .with(otel_layer)
        .init();

    // Log effective logging configuration
    if let Some(ref log_dir) = config.log_dir {
        // Can't use tracing yet since it's being initialized, use eprintln
        eprintln!(
            "Logging initialized: level={}, dir={}, rotation={}, json={}",
            config.level, log_dir, config.rotation, config.json_format
        );
    } else {
        eprintln!("Logging initialized: level={}, stdout only", config.level);
    }

    if otel_config.enabled {
        eprintln!(
            "OpenTelemetry enabled: endpoint={}, protocol={}, sampling={}",
            otel_config.endpoint, otel_config.protocol, otel_config.sampling_ratio
        );
    }

    Ok((guard, otel_guard))
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
        max_concurrent_downloads: {
            let raw = std::env::var("AOS_MAX_CONCURRENT_DOWNLOADS").ok();
            let parsed = raw
                .as_deref()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(4);
            let clamped = parsed.clamp(1, 10);
            if clamped != parsed {
                warn!(
                    env = "AOS_MAX_CONCURRENT_DOWNLOADS",
                    raw = ?raw,
                    parsed,
                    clamped,
                    "Value out of bounds; clamping to safe range"
                );
            }
            clamped
        },
        timeout_secs: {
            let raw = std::env::var("AOS_DOWNLOAD_TIMEOUT_SECS").ok();
            let parsed = raw
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300);
            let clamped = parsed.clamp(30, 3600);
            if clamped != parsed {
                warn!(
                    env = "AOS_DOWNLOAD_TIMEOUT_SECS",
                    raw = ?raw,
                    parsed,
                    clamped,
                    "Value out of bounds; clamping to safe range"
                );
            }
            clamped
        },
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

/// Dev helper: register cached base models from var/model-cache into DB when empty.
///
/// This runs only when explicitly enabled or in debug builds, and only if the
/// `models` table is currently empty. It scans `AOS_MODEL_CACHE_DIR/models`
/// (defaults to `var/model-cache/models`) and registers each directory as a
/// base model so the UI can surface them without manual import.
async fn seed_models_from_cache_if_empty(db: &Db) -> Result<()> {
    let seed_enabled = std::env::var("AOS_SEED_MODEL_CACHE")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(cfg!(debug_assertions));

    if !seed_enabled {
        return Ok(());
    }

    if db.pool_opt().is_none() {
        info!("Skipping model cache seed: SQL pool not available");
        return Ok(());
    }

    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool())
        .await?;
    if existing > 0 {
        return Ok(());
    }

    let cache_root = std::env::var("AOS_MODEL_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/model-cache"));
    let primary_models_dir = cache_root.join("models");
    let fallback_dir = std::env::var("AOS_MODEL_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/models"));

    // Collect candidate model directories to seed.
    let mut model_dirs = Vec::new();
    if primary_models_dir.exists() {
        model_dirs.push(primary_models_dir.clone());
    } else if fallback_dir.exists() {
        model_dirs.push(fallback_dir.clone());
    } else {
        info!(
            path = %primary_models_dir.display(),
            fallback = %fallback_dir.display(),
            "Model cache directory not found, skipping seed"
        );
        return Ok(());
    }

    let mut seeded = 0usize;
    let mut errors = 0usize;

    for root in model_dirs {
        // If this root is a single model directory (like var/models/Qwen2.5...), seed it directly.
        let entries: Vec<PathBuf> = if root.join("config.json").exists() {
            vec![root.clone()]
        } else {
            std::fs::read_dir(&root)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect()
        };

        for path in entries {
            if !path.is_dir() {
                continue;
            }

            let Some(path_str) = path.to_str() else {
                errors += 1;
                warn!(path = ?path, "Skipping model dir with non-UTF8 path");
                continue;
            };

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "model".to_string());
            let (format, backend) = detect_model_format_backend(&path);

            match db
                .import_model_from_path(&name, path_str, &format, &backend, "system", "system")
                .await
            {
                Ok(model_id) => {
                    if let Err(e) = db
                        .update_model_import_status(&model_id, "available", None)
                        .await
                    {
                        warn!(model_id = %model_id, error = %e, "Failed to mark model available");
                        errors += 1;
                    } else {
                        seeded += 1;
                    }
                }
                Err(e) => {
                    warn!(model = %name, error = %e, "Failed to seed cached model");
                    errors += 1;
                }
            }
        }
    }

    info!(
        seeded,
        errors,
        path = %primary_models_dir.display(),
        "Seeded cached base models into database"
    );

    Ok(())
}

fn detect_model_format_backend(path: &std::path::Path) -> (String, String) {
    // Default to safetensors + mlx backend, override if we detect a CoreML package.
    let mut format = "safetensors".to_string();
    let mut backend = "mlx-ffi".to_string();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("mlpackage") {
                    format = "mlpackage".to_string();
                    backend = "coreml".to_string();
                    break;
                }
                if ext.eq_ignore_ascii_case("gguf") {
                    format = "gguf".to_string();
                    backend = "metal".to_string();
                }
            }
        }
    }

    (format, backend)
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
                // Return immediately so ctrl_c handler can still work
                // In this case, SIGTERM won't trigger shutdown, but Ctrl+C will
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
