//! Security initialization and preflight checks for AdapterOS control plane.
//!
//! This module handles:
//! - PID file lock acquisition for single-writer mode
//! - Worker Ed25519 keypair loading/generation for CP->Worker authentication
//! - Effective configuration logging at startup
//! - PF (packet filter) rule validation on macOS/Linux
//! - Environment fingerprint verification and drift detection

use adapteros_boot::{load_or_generate_worker_keypair, derive_kid_from_verifying_key};
use adapteros_core::AosError;
use adapteros_server_api::config::Config;
use anyhow::Result;
use crate::cli::Cli;
use crate::pid_lock::PidFileLock;
use crate::security::PfGuard;
use ed25519_dalek::SigningKey;
use std::sync::{Arc, RwLock};
use tracing::{error, info, trace, warn};

/// Security context established during boot phase 2.
pub struct SecurityContext {
    /// PID file lock (if single-writer mode enabled)
    pub pid_lock: Option<PidFileLock>,
    /// Worker signing keypair for CP->Worker authentication
    pub worker_keypair: Option<SigningKey>,
}

/// Initialize security components (boot phase 2).
///
/// This function:
/// - Acquires PID file lock if single-writer mode is enabled
/// - Loads or generates the worker Ed25519 signing keypair
/// - Logs the effective configuration
///
/// # Arguments
///
/// * `config` - Server configuration
/// * `cli` - CLI arguments
///
/// # Returns
///
/// Returns a `SecurityContext` containing the PID lock and worker keypair.
pub async fn initialize_security(
    config: Arc<RwLock<Config>>,
    cli: &Cli,
) -> Result<SecurityContext> {
    info!(target: "boot", phase = 2, name = "security-init", "═══ BOOT PHASE 2/12: Security Initialization ═══");

    // =========================================================================
    // PID File Lock
    // =========================================================================
    // Acquire PID file lock if single-writer mode enabled
    let pid_lock = if cli.single_writer {
        Some(PidFileLock::acquire(cli.pid_file.clone())?)
    } else {
        None
    };

    // =========================================================================
    // Worker Authentication Keypair (Ed25519)
    // =========================================================================
    // Load or generate the worker signing keypair for CP->Worker authentication.
    // In strict mode, this is required; otherwise it's optional with a warning.
    info!("Loading worker authentication keypair (CSPRNG + filesystem I/O may be slow on some systems)");
    let keypair_start = std::time::Instant::now();
    let worker_keypair = {
        let keys_dir = std::path::Path::new("var/keys");
        std::fs::create_dir_all(keys_dir).ok();

        let key_path = keys_dir.join("worker_signing.key");
        match load_or_generate_worker_keypair(&key_path) {
            Ok(keypair) => {
                let kid = derive_kid_from_verifying_key(&keypair.verifying_key());
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
    log_effective_config(&config)?;

    Ok(SecurityContext {
        pid_lock,
        worker_keypair,
    })
}

/// Log the effective configuration at startup.
///
/// This logs:
/// - Effective config summary (ports, DB path, auth mode, demo mode)
/// - Server configuration (bind address, paths, timeouts)
/// - Security configuration (PF requirements, mTLS, JWT settings)
/// - Operational configuration (rate limits, metrics, alerting)
pub fn log_effective_config(config: &Arc<RwLock<Config>>) -> Result<()> {
    let cfg = config.read().map_err(|e| {
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

    Ok(())
}

/// Run security preflight checks (boot phase 4).
///
/// This function:
/// - Validates PF (packet filter) rules on macOS/Linux
/// - Verifies environment fingerprint and detects drift
/// - Blocks startup on critical drift in production mode
///
/// # Arguments
///
/// * `config` - Server configuration
/// * `cli` - CLI arguments
///
/// # Returns
///
/// Returns an error if security checks fail in production mode.
pub async fn run_preflight_checks(config: Arc<RwLock<Config>>, cli: &Cli) -> Result<()> {
    info!(target: "boot", phase = 4, name = "security-preflight", "═══ BOOT PHASE 4/12: Security Preflight ═══");
    info!("Running security preflight checks");

    // =========================================================================
    // PF (Packet Filter) Rule Validation
    // =========================================================================
    {
        let cfg = config
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

    // =========================================================================
    // Environment Fingerprint Drift Detection
    // =========================================================================
    info!("Verifying environment fingerprint");
    if !cli.skip_drift_check {
        use adapteros_verify::{
            get_or_create_fingerprint_keypair, DeviceFingerprint, DriftEvaluator,
        };

        let (production_mode, drift_policy) = {
            let cfg = config
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

    Ok(())
}
