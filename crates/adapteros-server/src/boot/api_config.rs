//! API configuration builder for the AdapterOS server.
//!
//! This module handles the construction of [`ApiConfig`] from the main server configuration
//! and provides hot-reload capabilities via SIGHUP signal handling on Unix systems.
//!
//! # Hot Reload
//!
//! On Unix systems, sending SIGHUP to the server process will trigger a configuration reload.
//! Only specific fields are reloadable (metrics, self-hosting, paths) to prevent disruption
//! of active connections and security settings.

use adapteros_core::{BackendKind, SeedMode};
use adapteros_server_api::config::Config;
use adapteros_server_api::state::{ApiConfig, BackgroundTaskTracker};
use anyhow::Result;
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

use crate::boot::BackgroundTaskSpawner;
use crate::shutdown::ShutdownCoordinator;

/// Builds the API configuration from the server configuration.
///
/// This creates a subset of the full server config that is needed by API handlers,
/// transforming the server config types into the API-specific config structure.
///
/// # Arguments
///
/// * `server_config` - The main server configuration (shared, thread-safe)
///
/// # Returns
///
/// An `Arc<RwLock<ApiConfig>>` that can be shared across handlers and hot-reloaded.
///
/// # Errors
///
/// Returns an error if the config lock is poisoned.
pub fn build_api_config(server_config: Arc<RwLock<Config>>) -> Result<Arc<RwLock<ApiConfig>>> {
    let cfg = server_config
        .read()
        .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

    Ok(Arc::new(RwLock::new(ApiConfig {
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
            clock_skew_seconds: cfg.security.clock_skew_seconds,
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
    })))
}

/// Spawns a SIGHUP handler for hot-reloading configuration on Unix systems.
///
/// This handler listens for SIGHUP signals and reloads specific configuration fields
/// without restarting the server. Only non-security-critical and non-connection-critical
/// fields are reloaded to prevent disruption.
///
/// # Reloadable Fields
///
/// - Metrics configuration (enabled, bearer_token)
/// - Self-hosting settings (mode, repo_allowlist, promotion_threshold)
/// - Path configurations (artifacts, bundles, adapters, plan, datasets, documents)
/// - Rate limits and alerting (in server_config)
///
/// # Arguments
///
/// * `server_config` - The main server configuration to reload into
/// * `api_config` - The API config to update with reloaded values
/// * `shutdown_coordinator` - Coordinator to register the background task
/// * `background_tasks` - Tracker for monitoring background task health
///
/// # Returns
///
/// Returns the updated shutdown coordinator with the SIGHUP handler registered.
///
/// # Errors
///
/// Returns an error if the shutdown coordinator is in an invalid state.
/// Signal handler registration failures are logged but not fatal.
#[cfg(unix)]
pub fn spawn_sighup_handler(
    server_config: Arc<RwLock<Config>>,
    api_config: Arc<RwLock<ApiConfig>>,
    config_path: String,
    shutdown_coordinator: ShutdownCoordinator,
    background_tasks: Arc<BackgroundTaskTracker>,
) -> Result<ShutdownCoordinator> {
    let config_clone = Arc::clone(&server_config);
    let api_config_clone = Arc::clone(&api_config);
    let tracker = Arc::clone(&background_tasks);

    let mut spawner = BackgroundTaskSpawner::new(shutdown_coordinator)
        .with_task_tracker(Arc::clone(&background_tasks));

    if spawner
        .spawn_optional(
            "SIGHUP handler",
            async move {
                use tokio::signal::unix::{signal, SignalKind};

                // Attempt to register signal handler, gracefully degrade if unavailable
                let mut sig = match signal(SignalKind::hangup()) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(
                            error = %e,
                            "Failed to register SIGHUP handler, config reload will be unavailable"
                        );
                        tracker.record_failed("SIGHUP handler", &e.to_string(), false);
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
                                    api_cfg.self_hosting.mode =
                                        new_config.self_hosting.mode.clone();
                                    api_cfg.self_hosting.repo_allowlist =
                                        new_config.self_hosting.repo_allowlist.clone();
                                    api_cfg.self_hosting.promotion_threshold =
                                        new_config.self_hosting.promotion_threshold;
                                    api_cfg.self_hosting.require_human_approval =
                                        new_config.self_hosting.mode.eq_ignore_ascii_case("safe");
                                    // Reload paths config
                                    api_cfg.paths.artifacts_root =
                                        new_config.paths.artifacts_root.clone();
                                    api_cfg.paths.bundles_root =
                                        new_config.paths.bundles_root.clone();
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
            },
            "Config reload unavailable",
        )
        .is_ok()
    {
        info!("SIGHUP handler registered for config reload");
    }

    Ok(spawner.into_coordinator())
}
