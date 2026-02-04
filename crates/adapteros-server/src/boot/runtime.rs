//! Runtime configuration and initialization module.
//!
//! This module handles Phase 5b of the boot sequence:
//! - Hostname resolution
//! - Effective config initialization and drift detection
//! - Configuration guard freezing (security measure)
//! - Global tick ledger initialization
//! - Runtime mode resolution and validation
//! - Production security requirements validation
//! - Executor bootstrap audit event logging

use adapteros_config::{
    init_effective_config, try_effective_config, ConfigDriftSeverity, ConfigSnapshot,
};
use adapteros_db::{Db, RuntimeSession};
use adapteros_deterministic_exec::global_ledger::GlobalTickLedger;
use adapteros_server_api::config::Config;
use adapteros_server_api::runtime_mode::{RuntimeMode, RuntimeModeResolver};
use anyhow::Result;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

/// Runtime context containing initialized runtime components
pub struct RuntimeContext {
    /// Global tick ledger for inference tracking
    pub tick_ledger: Arc<GlobalTickLedger>,
    /// Resolved runtime mode (dev/staging/prod)
    pub runtime_mode: RuntimeMode,
    /// Hostname for session tracking
    pub hostname: String,
}

/// Initialize runtime configuration and components.
///
/// This function performs:
/// 1. Hostname resolution from environment
/// 2. Effective config initialization with drift detection
/// 3. Configuration guard freezing
/// 4. Global tick ledger initialization
/// 5. Runtime mode resolution and validation
/// 6. Production security requirements validation
/// 7. Executor bootstrap audit event logging
///
/// # Arguments
///
/// * `db` - Database connection
/// * `config` - Server configuration (Arc<RwLock<Config>>)
/// * `config_path` - Path to the config file (cp.toml) as a string
/// * `manifest_path` - Path to the manifest file
/// * `manifest_hash` - Optional manifest hash for audit logging
///
/// # Returns
///
/// Returns `RuntimeContext` containing initialized components
///
/// # Errors
///
/// Returns error if:
/// - Config lock is poisoned
/// - Effective config initialization fails in production mode
/// - Config guard freezing fails in production mode
/// - Runtime mode resolution or validation fails
/// - Production mode JWT secret is too short
pub async fn initialize_runtime(
    db: &Db,
    config: Arc<RwLock<Config>>,
    config_path: &str,
    manifest_path: &Path,
    manifest_hash: Option<&adapteros_core::B3Hash>,
) -> Result<RuntimeContext> {
    // Get hostname for session tracking
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown-host".to_string());

    // Get db_cfg early for session recording
    let db_cfg = config
        .read()
        .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
        .db
        .clone();

    // Initialize effective config and detect configuration drift
    {
        // Initialize EffectiveConfig with cp.toml path
        // In production mode, config errors are fatal; in dev mode, we warn and continue
        let is_production = config
            .read()
            .map(|c| c.server.production_mode)
            .unwrap_or(false);

        if let Err(e) = init_effective_config(Some(config_path), vec![]) {
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
                                ConfigDriftSeverity::Critical => {
                                    error!(key = %field.key, old = %field.old_value, new = %field.new_value, "CRITICAL config change");
                                }
                                ConfigDriftSeverity::Warning => {
                                    warn!(key = %field.key, old = %field.old_value, new = %field.new_value, "Config change");
                                }
                                ConfigDriftSeverity::Info => {
                                    info!(key = %field.key, old = %field.old_value, new = %field.new_value, "Config change");
                                }
                            }
                        }
                    }
                }
            }

            // Determine runtime mode string
            let runtime_mode_str = if config
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
    }

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
            let is_production = config
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
        let production_mode = config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
            .server
            .production_mode;

        // Create a minimal API config for runtime mode resolution
        let api_cfg = Config {
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
                skip_worker_check: false,
                worker_heartbeat_interval_secs: 30,
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
                allow_registration: None,
                clock_skew_seconds: 300,
                dev_bypass: false,
                ci_attestation_public_keys: None,
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
                synthesis_model_path: None,
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
            invariants: Default::default(),
            sse: Default::default(),
        };

        RuntimeModeResolver::resolve(&api_cfg, db)
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
        let api_cfg = {
            let cfg = config
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
            Config {
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
                    skip_worker_check: false,
                    worker_heartbeat_interval_secs: cfg.server.worker_heartbeat_interval_secs,
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
                    allow_registration: cfg.security.allow_registration,
                    clock_skew_seconds: cfg.security.clock_skew_seconds,
                    dev_bypass: cfg.security.dev_bypass,
                    ci_attestation_public_keys: cfg
                        .security
                        .ci_attestation_public_keys
                        .clone(),
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
                    synthesis_model_path: cfg.paths.synthesis_model_path.clone(),
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
                invariants: Default::default(),
                sse: Default::default(),
            }
        }; // Close the scope, dropping cfg

        RuntimeModeResolver::validate(runtime_mode, &api_cfg, db)
            .await
            .map_err(|e| anyhow::anyhow!("Runtime mode validation failed: {}", e))?;
    }

    // Audit log: Executor bootstrap event
    {
        let metadata = {
            let cfg = config
                .read()
                .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

            serde_json::json!({
                "manifest_path": manifest_path.display().to_string(),
                "manifest_based": manifest_hash.is_some(),
                "hkdf_label": "executor",
                "production_mode": cfg.security.require_pf_deny,
                "seed_source": if manifest_hash.is_some() { "manifest" } else { "default" },
            })
        };

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

    Ok(RuntimeContext {
        tick_ledger,
        runtime_mode,
        hostname,
    })
}
