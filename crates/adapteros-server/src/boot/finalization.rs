//! Boot finalization phase (Phase 11-12).
//!
//! This module handles the final boot sequence phases:
//! - Phase 11: Adapter loading
//! - Phase 12: Router construction, server binding config resolution, and boot report generation
//!
//! # Overview
//!
//! The finalization phase assembles all boot artifacts needed to start the server:
//! - Axum router with API routes and UI assets
//! - Server binding configuration (production mode, UDS socket, bind address, port)
//! - In-flight request counter for graceful shutdown
//! - Drain timeout for connection draining
//!
//! It also generates and writes the boot report to `AOS_VAR_DIR/run/boot_report.json` for
//! external monitoring and verification of server readiness.

use adapteros_boot::BootReport;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::routes;
use adapteros_server_api::AppState;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tracing::{error, info, warn};

/// Server binding configuration extracted from config and environment variables.
#[derive(Debug, Clone)]
pub struct BindConfig {
    /// Whether the server is running in production mode.
    pub production_mode: bool,
    /// Unix domain socket path (if configured).
    pub uds_socket: Option<String>,
    /// IP address or hostname to bind to.
    pub bind: String,
    /// Port number to bind to.
    pub port: u16,
    /// Timeout duration for draining in-flight requests during shutdown.
    pub drain_timeout: Duration,
}

/// Boot artifacts returned by the finalization phase.
///
/// These artifacts are everything needed to bind and serve the server.
pub struct BootArtifacts {
    /// Axum router with API routes and UI assets.
    pub app: axum::Router,
    /// Server binding configuration.
    pub bind_config: BindConfig,
    /// In-flight request counter for graceful shutdown.
    pub in_flight_requests: Arc<AtomicUsize>,
}

/// Finalizes the boot sequence (Phases 11-12).
///
/// # Phase 11: Adapter Loading
/// - Transitions boot state to adapter loading
/// - Loads all configured adapters
///
/// # Phase 12: Finalization
/// - Constructs the Axum router with API routes and UI assets
/// - Resolves server binding configuration with environment variable precedence
/// - Extracts in-flight request counter for shutdown handler
///
/// # Arguments
///
/// * `state` - Application state containing all server dependencies
/// * `config` - Server configuration (wrapped in Arc<RwLock<>> for hot-reload)
/// * `ui_routes` - UI asset routes (from assets module)
/// * `boot_state` - Boot state manager for phase tracking
///
/// # Returns
///
/// Boot artifacts needed to bind and serve the server.
///
/// # Note on Port Resolution
///
/// Server port (including legacy aliases) is resolved during Phase 1 in
/// `config.rs`. This function reads the already-resolved `cfg.server.port`.
pub async fn finalize_boot(
    state: AppState,
    config: Arc<RwLock<Config>>,
    ui_routes: axum::Router,
    boot_state: &BootStateManager,
) -> Result<BootArtifacts> {
    // =========================================================================
    // Phase 11: Adapter Loading
    // =========================================================================
    info!(target: "boot", phase = 11, name = "adapters", "═══ BOOT PHASE 11/12: Adapter Loading ═══");

    // Transition through required boot states in order.
    // Note: load_policies() already called in migrations.rs after seeding.
    // The state machine requires strict ordering:
    // LoadingPolicies → StartingBackend → LoadingBaseModels → LoadingAdapters → WorkerDiscovery
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.worker_discovery().await;

    // Clone in_flight_requests counter for shutdown handler before moving state
    let in_flight_requests = Arc::clone(&state.in_flight_requests);

    // =========================================================================
    // Phase 12: Router Construction
    // =========================================================================
    info!(target: "boot", phase = 12, name = "finalization", "═══ BOOT PHASE 12/12: Finalization ═══");

    // Build router with UI
    let api_routes = routes::build(state.clone());

    // API routes are merged directly at root level (routes already have /v1/ prefix)
    // The api_routes already include /healthz and /readyz endpoints
    // Order matters: specific API routes first, then UI fallback for SPA
    let app = axum::Router::new()
        .merge(api_routes) // API routes at their defined paths (/v1/*, /healthz, etc.)
        .merge(ui_routes) // UI fallback for non-API paths
        .layer(CompressionLayer::new()); // Response compression (gzip, br) for all routes

    // =========================================================================
    // Server Binding Configuration Resolution
    // =========================================================================
    // Note: Port resolution (including legacy aliases AOS_SERVER_PORT,
    // ADAPTEROS_SERVER_PORT, API_PORT) is handled in config.rs during Phase 1.
    // Here we just read the already-resolved config values.
    let bind_config = {
        let cfg = config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        BindConfig {
            production_mode: cfg.server.production_mode,
            uds_socket: cfg.server.uds_socket.clone(),
            bind: cfg.server.bind.clone(),
            port: cfg.server.port,
            drain_timeout: Duration::from_secs(cfg.server.drain_timeout_secs),
        }
    };

    Ok(BootArtifacts {
        app,
        bind_config,
        in_flight_requests,
    })
}

/// Writes the boot report to `AOS_VAR_DIR/run/boot_report.json`.
///
/// The boot report contains:
/// - Config hash (SHA-256 of serialized config)
/// - Bind address and port
/// - Worker key ID (if worker keypair is configured)
///
/// # Arguments
///
/// * `config` - Server configuration for hashing
/// * `bind_config` - Server binding configuration
/// * `worker_keypair` - Optional worker signing keypair
/// * `strict_mode` - Whether to fail hard on errors
///
/// # Errors
///
/// In strict mode, returns an error if:
/// - Config serialization fails
/// - Boot report file writing fails
///
/// In non-strict mode, logs warnings but continues.
pub fn write_boot_report(
    config: Arc<RwLock<Config>>,
    bind_config: &BindConfig,
    worker_keypair: Option<&ed25519_dalek::SigningKey>,
    strict_mode: bool,
) -> Result<()> {
    let report_path = resolve_boot_report_path()?;

    // Serialize config for hashing. Handle errors explicitly instead of silently
    // producing an empty hash which would mask config issues.
    let config_bytes = {
        let config_guard = config.read().unwrap_or_else(|e| e.into_inner());
        match serde_json::to_vec(&*config_guard) {
            Ok(bytes) => bytes,
            Err(e) => {
                if strict_mode {
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
        .bind_addr(bind_config.bind.to_string())
        .port(bind_config.port);

    // Add worker key ID if available
    if let Some(keypair) = worker_keypair {
        let kid = adapteros_boot::derive_kid_from_verifying_key(&keypair.verifying_key());
        report_builder = report_builder.add_worker_key_kid(kid);
    }

    let report = report_builder.build();

    // Emit to log
    report.emit_log();

    // Write to file
    if let Some(parent) = report_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            if strict_mode {
                error!(
                    error = %e,
                    path = %report_path.display(),
                    "STRICT MODE: Failed to create boot report directory"
                );
                return Err(anyhow::anyhow!(
                    "Strict mode requires boot report directory at {}",
                    report_path.display()
                ));
            }
            warn!(
                error = %e,
                path = %report_path.display(),
                "Failed to create boot report directory"
            );
        }
    }

    let report_path_str = report_path.to_string_lossy();
    match report.write_to_file(&report_path_str) {
        Ok(()) => {
            info!(path = %report_path.display(), "Boot report written");
            Ok(())
        }
        Err(e) => {
            if strict_mode {
                error!(
                    error = %e,
                    path = %report_path.display(),
                    "STRICT MODE: Failed to write boot report"
                );
                Err(anyhow::anyhow!(
                    "Strict mode requires boot report at {}",
                    report_path.display()
                ))
            } else {
                warn!(
                    error = %e,
                    path = %report_path.display(),
                    "Failed to write boot report"
                );
                Ok(())
            }
        }
    }
}

fn resolve_boot_report_path() -> Result<PathBuf> {
    let var_base = adapteros_core::resolve_var_dir();
    let report_dir = var_base.join("run");
    let report_path = report_dir.join("boot_report.json");

    enforce_no_default_var_write(&report_path)?;

    Ok(report_path)
}

fn enforce_no_default_var_write(report_path: &Path) -> Result<()> {
    let Some(env_val) = std::env::var("AOS_VAR_DIR").ok() else {
        return Ok(());
    };
    let trimmed = env_val.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let default_var = cwd.join("var");

    // Resolve the configured var dir path
    let configured_var = if PathBuf::from(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        cwd.join(trimmed)
    };

    // If the configured var dir resolves to the same location as default var/,
    // treat it as if AOS_VAR_DIR wasn't set (backward compatibility)
    if let (Ok(canonical_default), Ok(canonical_configured)) = (
        default_var
            .canonicalize()
            .or_else(|_| Ok::<_, std::io::Error>(default_var.clone())),
        configured_var
            .canonicalize()
            .or_else(|_| Ok::<_, std::io::Error>(configured_var.clone())),
    ) {
        if canonical_default == canonical_configured {
            return Ok(());
        }
    }

    // Also check string-based defaults for cases where paths don't exist yet
    if trimmed == "var" || trimmed == "./var" {
        return Ok(());
    }

    let resolved = if report_path.is_absolute() {
        report_path.to_path_buf()
    } else {
        cwd.join(report_path)
    };

    if resolved.starts_with(&default_var) {
        return Err(anyhow::anyhow!(
            "boot report path resolves under default var/ while AOS_VAR_DIR is set: {}",
            resolved.display()
        ));
    }

    Ok(())
}
