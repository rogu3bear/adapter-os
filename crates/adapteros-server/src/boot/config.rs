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

use adapteros_boot::{ensure_runtime_dir, EXIT_CONFIG_ERROR};
use adapteros_config::{
    path_resolver::PathSource, resolve_base_model_location, StorageBackend as CfgStorageBackend,
};
use adapteros_core::{resolve_var_dir, time};
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use anyhow::Result;
use serde_json::json;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{error, info, instrument, warn};

use crate::cli::{normalize_jwt_mode, Cli};
use crate::{logging, otel, telemetry_flush};

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
#[instrument(skip_all)]
pub async fn initialize_config(cli: &Cli) -> Result<ConfigContext> {
    let boot_state = BootStateManager::new();
    boot_state.start_phase("config_load");

    // Ensure runtime directory is writable before any other boot steps
    let preferred_var_dir = resolve_var_dir();
    let runtime_dir = match ensure_runtime_dir(&preferred_var_dir, None) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("FATAL: {}", e);
            std::process::exit(EXIT_CONFIG_ERROR);
        }
    };
    std::env::set_var("AOS_VAR_DIR", &runtime_dir.path);
    let effective_var_base = runtime_dir.path.clone();
    if runtime_dir.used_fallback {
        eprintln!(
            "WARNING: {} is not writable; using ephemeral runtime dir at {}",
            preferred_var_dir.display(),
            runtime_dir.path.display()
        );
    }

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
    boot_state.finish_phase_ok("config_load");

    // Harmonize critical config with canonical env vars so scripts/UI and the server agree.
    // Precedence: explicit env > config file > defaults.
    {
        let mut cfg = server_config
            .write()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        // Server port resolution with legacy alias support.
        // Precedence: AOS_SERVER_PORT > ADAPTEROS_SERVER_PORT > API_PORT > config file
        if let Some(port) = std::env::var("AOS_SERVER_PORT")
            .or_else(|_| std::env::var("ADAPTEROS_SERVER_PORT"))
            .or_else(|_| std::env::var("API_PORT"))
            .ok()
            .and_then(|raw| {
                raw.parse::<u16>()
                    .map_err(|_| {
                        eprintln!("WARNING: Invalid port value '{raw}'; using config file default");
                    })
                    .ok()
            })
        {
            cfg.server.port = port;
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

        // Storage backend: honor env override and canonicalize aliases so boot/database uses
        // the intended mode instead of stale config file defaults.
        if let Ok(raw_backend) =
            std::env::var("AOS_STORAGE_BACKEND").or_else(|_| std::env::var("AOS_STORAGE_MODE"))
        {
            match CfgStorageBackend::from_str(raw_backend.trim()) {
                Ok(parsed_backend) => {
                    let canonical = parsed_backend.as_str().to_string();
                    cfg.db.storage_mode = canonical.clone();
                    std::env::set_var("AOS_STORAGE_BACKEND", &canonical);
                    std::env::set_var("AOS_STORAGE_MODE", &canonical);
                }
                Err(_) => {
                    eprintln!(
                        "WARNING: Invalid storage backend '{raw_backend}'; using config file value '{}'",
                        cfg.db.storage_mode
                    );
                }
            }
        } else {
            // Publish the resolved config value for downstream crates that read env directly.
            std::env::set_var("AOS_STORAGE_BACKEND", cfg.db.storage_mode.trim());
            std::env::set_var("AOS_STORAGE_MODE", cfg.db.storage_mode.trim());
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
        let cfg = server_config.read().unwrap_or_else(|e| {
            eprintln!("FATAL: Config lock poisoned: {}", e);
            std::process::exit(1);
        });

        logging::init_logging(&cfg.logging, &cfg.otel)
            .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?
    };
    // Startup preamble: first structured log identifying this build
    info!(
        build_id = adapteros_core::version::BUILD_ID,
        git_commit = adapteros_core::version::GIT_COMMIT_HASH,
        version = adapteros_core::version::VERSION,
        run_id = %std::env::var("AOS_RUN_ID").unwrap_or_else(|_| "none".into()),
        profile = adapteros_core::version::BUILD_PROFILE,
        rustc = adapteros_core::version::RUSTC_VERSION,
        "adapterOS starting"
    );

    info!(
        aos_var_dir = %preferred_var_dir.display(),
        effective_var_base = %effective_var_base.display(),
        "Runtime var base resolved"
    );

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

    // Validate required secrets after logging is configured
    {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        if let Err(e) = cfg.validate_secrets() {
            error!(error = %e, "FATAL: secret validation failed");
            std::process::exit(1);
        }
    }

    // Validate production environment configuration
    {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        if let Err(e) = validate_production_config(&cfg) {
            error!(error = %e, "FATAL: production config validation failed");
            std::process::exit(1);
        }
    }

    // Set up panic hook to capture panics to log and write crash snapshots
    {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        if cfg.logging.capture_panics {
            let log_dir = cfg
                .logging
                .log_dir
                .clone()
                .unwrap_or_else(|| "var/logs".to_string());
            let log_prefix = cfg.logging.log_prefix.clone();
            let boot_trace_id = boot_state.boot_trace_id();

            // Minimal redacted config summary for crash reports
            let config_summary = json!({
                "server": {
                    "bind": cfg.server.bind,
                    "port": cfg.server.port,
                    "production_mode": cfg.server.production_mode,
                },
                "db": {
                    "path": cfg.db.path,
                    "storage_mode": cfg.db.storage_mode,
                },
                "logging": {
                    "log_dir": cfg.logging.log_dir,
                    "json_format": cfg.logging.json_format,
                }
            });

            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
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

                let backtrace = std::backtrace::Backtrace::force_capture().to_string();
                let ts = time::now_rfc3339();
                let crash_dir = std::path::Path::new(&log_dir);
                let _ = std::fs::create_dir_all(crash_dir);
                let crash_path = crash_dir.join(format!("crash-{}.json", ts.replace(':', "-")));

                // Try to capture the tail of the latest log file (best effort)
                let log_tail = latest_log_tail(&log_dir, &log_prefix, 200);

                let snapshot = serde_json::json!({
                    "ts": ts,
                    "trace_id": boot_trace_id,
                    "location": location,
                    "message": message,
                    "backtrace": backtrace,
                    "config": config_summary,
                    "log_tail": log_tail,
                });

                if let Ok(mut file) = std::fs::File::create(&crash_path) {
                    let _ = serde_json::to_writer_pretty(&mut file, &snapshot);
                }

                error!(
                    panic.location = %location,
                    panic.message = %message,
                    crash_path = %crash_path.display(),
                    "PANIC CAPTURED"
                );

                telemetry_flush::capture_panic(&location, &message, Duration::from_millis(750));

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

fn latest_log_tail(log_dir: &str, prefix: &str, max_lines: usize) -> Vec<String> {
    let path = std::path::Path::new(log_dir);
    if !path.exists() {
        return Vec::new();
    }

    let mut newest: Option<std::path::PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                if fname.starts_with(prefix) {
                    let modified = entry.metadata().ok().and_then(|m| m.modified().ok());
                    let is_newer = newest
                        .as_ref()
                        .and_then(|cur| cur.metadata().ok().and_then(|m| m.modified().ok()));
                    if is_newer.is_none_or(|cur_mod| modified.is_some_and(|m| m > cur_mod)) {
                        newest = Some(p);
                    }
                }
            }
        }
    }

    let Some(log_path) = newest else {
        return Vec::new();
    };
    if let Ok(content) = std::fs::read_to_string(&log_path) {
        let lines: Vec<_> = content
            .lines()
            .rev()
            .take(max_lines)
            .map(str::to_string)
            .collect();
        return lines.into_iter().rev().collect();
    }

    Vec::new()
}

/// Validate production-specific configuration requirements.
///
/// When `AOS_PRODUCTION_MODE` is true (or server.production_mode in config),
/// this function enforces that critical security settings are properly configured
/// and not set to placeholder values.
///
/// # Errors
///
/// Returns an error if:
/// - JWT secret contains "REPLACE" placeholder or is empty in production mode
/// - CORS origins contain "REPLACE" placeholder in production mode (warning only)
fn validate_production_config(config: &Config) -> Result<()> {
    // Check if production mode is enabled via env var or config
    let production_mode = std::env::var("AOS_PRODUCTION_MODE")
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(config.server.production_mode);

    if !production_mode {
        return Ok(());
    }

    info!("Validating production configuration requirements");

    // Validate JWT secret is not a placeholder
    let jwt_secret = std::env::var("AOS_SECURITY_JWT_SECRET")
        .unwrap_or_else(|_| config.security.jwt_secret.clone());

    if jwt_secret.is_empty() {
        return Err(anyhow::anyhow!(
            "AOS_SECURITY_JWT_SECRET must be set in production mode. \
             Generate a secure secret with: openssl rand -base64 32"
        ));
    }

    if jwt_secret.contains("REPLACE") || jwt_secret.contains("changeme") {
        return Err(anyhow::anyhow!(
            "AOS_SECURITY_JWT_SECRET contains a placeholder value ('REPLACE' or 'changeme'). \
             Set a secure secret for production. Generate with: openssl rand -base64 32"
        ));
    }

    // Warn about minimum secret length (but don't block)
    if jwt_secret.len() < 32 {
        warn!(
            secret_len = jwt_secret.len(),
            "JWT secret is shorter than recommended (32+ characters). Consider using a longer secret."
        );
    }

    // Check CORS origins configuration (warning only, not blocking)
    let allowed_origins = std::env::var("ALLOWED_ORIGINS").unwrap_or_default();
    if allowed_origins.contains("REPLACE") {
        warn!(
            "ALLOWED_ORIGINS contains placeholder value. Configure proper CORS origins for production."
        );
    } else if allowed_origins.is_empty() {
        warn!(
            "ALLOWED_ORIGINS not configured for production. \
             Consider setting explicit origins for better security."
        );
    }

    // Check for wildcard CORS in production (security warning)
    if allowed_origins == "*" {
        warn!(
            "ALLOWED_ORIGINS is set to wildcard '*' in production mode. \
             This allows requests from any origin and may pose a security risk."
        );
    }

    info!("Production configuration validation passed");
    Ok(())
}
