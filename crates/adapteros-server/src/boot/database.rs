//! Database initialization and connection management for adapterOS boot sequence.
//!
//! This module handles Phase 5a of the boot sequence: Database Connection.
//!
//! # Responsibilities
//!
//! - Storage backend determination (SQL, Dual, KV-Primary, KV-Only)
//! - DbFactory creation and configuration
//! - Connection pool establishment
//! - Dual-write configuration validation with strict mode warnings
//! - BootStateManager database attachment
//! - Database path resolution and logging
//! - Effective configuration initialization and drift detection
//! - Configuration guards freezing (security measure)
//! - Global tick ledger initialization for inference tracking
//! - Runtime mode resolution
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_server::boot::database::{initialize_database, DatabaseContext};
//!
//! let db_context = initialize_database(config, &mut boot_state, &cli).await?;
//! // Use db_context.db, db_context.boot_state, db_context.tick_ledger, etc.
//! ```

use adapteros_config::{
    init_effective_config, try_effective_config, ConfigSnapshot,
    StorageBackend as CfgStorageBackend,
};
use adapteros_db::{Db, DbFactory, DbStorageBackend, RuntimeSession};
use adapteros_deterministic_exec::global_ledger::GlobalTickLedger;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use adapteros_server_api::runtime_mode::RuntimeModeResolver;
use anyhow::Result;

use crate::cli::Cli;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tracing::{error, info, instrument, warn};

/// Context returned from database initialization containing all database-related state.
pub struct DatabaseContext {
    /// The database connection
    pub db: Db,
    /// Updated boot state manager with database attached
    pub boot_state: BootStateManager,
    /// Global tick ledger for inference tracking
    pub tick_ledger: Arc<GlobalTickLedger>,
    /// Hostname for session tracking
    pub hostname: String,
    /// Runtime mode (production/development)
    pub runtime_mode: adapteros_server_api::runtime_mode::RuntimeMode,
}

/// Initialize database connection and related boot state.
///
/// This function performs the following operations:
/// 1. Determines storage backend from configuration
/// 2. Creates database connection with DbFactory
/// 3. Validates atomic dual-write configuration
/// 4. Attaches database to boot state manager
/// 5. Initializes effective config and detects drift
/// 6. Freezes configuration guards
/// 7. Initializes global tick ledger
/// 8. Resolves runtime mode
///
/// # Arguments
///
/// * `config` - Server configuration (Arc<RwLock<Config>>)
/// * `boot_state` - Mutable reference to boot state manager
/// * `cli` - Command-line interface arguments
///
/// # Returns
///
/// Returns `DatabaseContext` containing the database connection and related state.
///
/// # Errors
///
/// Returns error if:
/// - Configuration lock is poisoned
/// - Database connection fails
/// - Effective config initialization fails in production mode
/// - Configuration guard freeze fails in production mode
/// - Runtime mode resolution fails
#[instrument(skip_all)]
pub async fn initialize_database(
    config: Arc<RwLock<Config>>,
    boot_state: BootStateManager,
    cli: &Cli,
) -> Result<DatabaseContext> {
    info!(target: "boot", phase = 5, name = "database", "═══ BOOT PHASE 5/12: Database Connection ═══");
    boot_state.db_connecting().await;

    let db_cfg = config
        .read()
        .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?
        .db
        .clone();

    let cfg_backend =
        CfgStorageBackend::from_str(&db_cfg.storage_mode).unwrap_or(CfgStorageBackend::Sql);
    let db_backend = match cfg_backend {
        CfgStorageBackend::Sql => DbStorageBackend::Sql,
        CfgStorageBackend::Dual => DbStorageBackend::Dual,
        CfgStorageBackend::KvPrimary => DbStorageBackend::KvPrimary,
        CfgStorageBackend::KvOnly => DbStorageBackend::KvOnly,
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

    // Log effective storage mode (may differ from requested if KV backend unavailable)
    let effective_mode = db.storage_mode();
    if effective_mode.to_string() != cfg_backend.as_str() {
        warn!(
            requested_mode = %cfg_backend.as_str(),
            effective_mode = %effective_mode,
            "Storage mode adjusted from requested configuration (KV backend may be unavailable)"
        );
    } else {
        info!(
            storage_mode = %effective_mode,
            "Storage backend initialized successfully"
        );
    }

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
        let is_production = config
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

    Ok(DatabaseContext {
        db,
        boot_state,
        tick_ledger,
        hostname,
        runtime_mode,
    })
}
