//! Configuration and early boot initialization.
//!
//! This module handles:
//! - CLI parsing
//! - Configuration file loading
//! - Environment variable harmonization
//! - Base model path validation
//! - Logging/tracing initialization
//! - Panic hook setup
//! - Boot state manager creation
//! - Boot timeout configuration

use adapteros_config::{path_resolver::PathSource, resolve_base_model_location};
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use anyhow::Result;
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

use crate::cli::{normalize_jwt_mode, Cli};
use crate::{logging, otel};

/// Context containing all configuration and boot state needed for subsequent phases.
pub struct ConfigContext {
    /// Shared server configuration
    pub server_config: Arc<RwLock<Config>>,
    /// Boot state manager for tracking boot progress
    pub boot_state: BootStateManager,
    /// Boot timeout in seconds
    pub boot_timeout_secs: u64,
    /// Log guard to keep logging active
    pub _log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    /// OpenTelemetry guard to keep OTEL active
    pub _otel_guard: Option<otel::OtelGuard>,
}

/// Initialize configuration, logging, and boot state.
///
/// This function performs the first phase of the boot sequence:
/// 1. Loads configuration from file
/// 2. Harmonizes environment variables with config
/// 3. Validates base model path
/// 4. Initializes logging/tracing
/// 5. Sets up panic hooks
/// 6. Creates boot state manager
/// 7. Validates CORS configuration
///
/// # Arguments
///
/// * `cli` - Parsed command-line arguments
///
/// # Returns
///
/// Returns `ConfigContext` containing the initialized configuration and boot state,
/// or an error if initialization fails.
///
/// # Errors
///
/// This function will return an error if:
/// - Configuration file cannot be loaded
/// - Configuration lock is poisoned
/// - Logging initialization fails
/// - CORS configuration validation fails
pub async fn initialize_config(cli: &Cli) -> Result<ConfigContext> {
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

        logging::initialize_logging(&cfg.logging, &cfg.otel)
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

    info!(target: "boot", phase = 1, name = "config", "═══ BOOT PHASE 1/12: Configuration Complete ═══");

    Ok(ConfigContext {
        server_config,
        boot_state,
        boot_timeout_secs,
        _log_guard,
        _otel_guard,
    })
}
