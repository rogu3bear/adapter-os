//! Federation daemon initialization for AdapterOS control plane.
//!
//! This module handles the initialization of:
//! - Policy hash watcher for continuous monitoring
//! - Telemetry writer for bundle creation
//! - Federation manager for federated policy synchronization
//! - Federation daemon for background sweeps
//!
//! # Architecture
//!
//! The federation system consists of three main components:
//! 1. **PolicyHashWatcher**: Monitors policy hashes across tenants, detects drift,
//!    and records violations via telemetry bundles (60s interval).
//! 2. **TelemetryWriter**: Creates compressed bundles of telemetry events for
//!    archival and analysis (50MB max bundle size, 10k events per bundle).
//! 3. **FederationDaemon**: Periodically sweeps federated hosts, validates policies,
//!    and enforces quarantine for non-compliant hosts (5min interval).
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_server::boot::federation::initialize_federation;
//!
//! let ctx = initialize_federation(
//!     &db,
//!     config.clone(),
//!     &mut shutdown_coordinator,
//!     background_tasks.clone(),
//! ).await?;
//!
//! // Use the federation context components
//! let policy_watcher = ctx.policy_watcher;
//! let telemetry = ctx.telemetry;
//! let federation_daemon = ctx.federation_daemon;
//! ```

use crate::shutdown::ShutdownCoordinator;
use adapteros_db::Db;
use adapteros_server_api::config::Config;
use adapteros_server_api::state::BackgroundTaskTracker;
use anyhow::Result;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{info, warn};

/// Federation context containing initialized federation components.
///
/// This struct bundles together the policy watcher, telemetry writer,
/// and federation daemon for use in application state.
#[derive(Clone)]
pub struct FederationContext {
    /// Policy hash watcher for continuous monitoring
    pub policy_watcher: Arc<adapteros_policy::PolicyHashWatcher>,
    /// Telemetry writer for event bundling
    pub telemetry: Arc<adapteros_telemetry::TelemetryWriter>,
    /// Federation daemon for background sweeps
    pub federation_daemon: Arc<adapteros_orchestrator::FederationDaemon>,
}

/// Initialize the federation system with all required components.
///
/// This function performs the following initialization steps:
/// 1. Creates the telemetry writer from bundles directory
/// 2. Creates the policy hash watcher with database and telemetry
/// 3. Loads baseline policy hashes from database cache
/// 4. Starts background policy watcher (60s interval)
/// 5. Creates federation manager with generated keypair
/// 6. Configures federation daemon (5min interval, quarantine enabled)
/// 7. Starts federation daemon with shutdown signal
///
/// # Arguments
/// * `db` - Database connection from DbFactory
/// * `config` - Server configuration (contains bundles path)
/// * `shutdown_coordinator` - Shutdown coordinator for registering task handles
/// * `background_tasks` - Background task tracker for health monitoring
///
/// # Returns
/// `FederationContext` containing the policy watcher, telemetry writer, and federation daemon.
///
/// # Errors
/// Returns an error if:
/// - Config lock is poisoned
/// - Bundles directory cannot be created
/// - Telemetry writer creation fails
/// - Policy hash cache loading fails critically
/// - Federation manager creation fails
/// - Federation daemon startup fails
pub async fn initialize_federation(
    db: &Db,
    config: Arc<RwLock<Config>>,
    shutdown_coordinator: &mut ShutdownCoordinator,
    background_tasks: Arc<BackgroundTaskTracker>,
) -> Result<FederationContext> {
    // Initialize policy hash watcher (continuous monitoring)
    // Create telemetry writer and policy watcher first (needed by federation daemon)
    let (policy_watcher, telemetry) = {
        info!("Initializing policy hash watcher");

        // Create telemetry writer
        let bundles_path = config
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
        background_tasks.record_spawned("Policy hash watcher", false);

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
        quorum_min_peers: 2,
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
    let federation_handle = federation_daemon.clone().start(federation_shutdown_rx);
    shutdown_coordinator.set_federation_handle(federation_handle);
    background_tasks.record_spawned("Federation daemon", false);
    info!("Federation daemon started (300s interval)");

    Ok(FederationContext {
        policy_watcher,
        telemetry,
        federation_daemon,
    })
}
