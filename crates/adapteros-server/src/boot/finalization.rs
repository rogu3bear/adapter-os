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
//! It also generates and writes the boot report to `var/run/boot_report.json` for
//! external monitoring and verification of server readiness.

use adapteros_boot::BootReport;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::routes;
use adapteros_server_api::AppState;
use anyhow::Result;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, RwLock};
use std::time::Duration;
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
/// # Environment Variable Precedence
///
/// Server port is resolved in this order (first wins):
/// 1. `AOS_SERVER_PORT` (canonical)
/// 2. `ADAPTEROS_SERVER_PORT` (legacy alias)
/// 3. `API_PORT` (legacy alias for demo safety)
/// 4. Config file value
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

    // Transition through required states before loading adapters
    boot_state.load_policies().await;
    boot_state.load_adapters().await;

    // Clone in_flight_requests counter for shutdown handler before moving state
    let in_flight_requests = Arc::clone(&state.in_flight_requests);

    // =========================================================================
    // Phase 12: Router Construction
    // =========================================================================
    info!(target: "boot", phase = 12, name = "finalization", "═══ BOOT PHASE 12/12: Finalization ═══");

    // Build router with UI
    let api_routes = routes::build(state);

    // NOTE: Legacy root-level API shims removed due to fallback handler conflict.
    // All API endpoints should use the `/api/*` prefix.
    let app = axum::Router::new()
        .nest("/api", api_routes) // API routes under /api prefix
        .merge(ui_routes); // UI fallback for non-API paths

    // =========================================================================
    // Server Binding Configuration Resolution
    // =========================================================================
    let bind_config = {
        let cfg = config
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

        BindConfig {
            production_mode: cfg.server.production_mode,
            uds_socket: cfg.server.uds_socket.clone(),
            bind: cfg.server.bind.clone(),
            port: server_port,
            drain_timeout: Duration::from_secs(cfg.server.drain_timeout_secs),
        }
    };

    Ok(BootArtifacts {
        app,
        bind_config,
        in_flight_requests,
    })
}

/// Writes the boot report to `var/run/boot_report.json`.
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
    let report_path = "var/run/boot_report.json";
    match report.write_to_file(report_path) {
        Ok(()) => {
            info!(path = %report_path, "Boot report written");
            Ok(())
        }
        Err(e) => {
            if strict_mode {
                error!(
                    error = %e,
                    path = %report_path,
                    "STRICT MODE: Failed to write boot report"
                );
                Err(anyhow::anyhow!(
                    "Strict mode requires boot report at {}",
                    report_path
                ))
            } else {
                warn!(error = %e, path = %report_path, "Failed to write boot report");
                Ok(())
            }
        }
    }
}
