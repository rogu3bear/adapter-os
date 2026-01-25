//! Security initialization and preflight checks for adapterOS control plane.
//!
//! This module handles:
//! - PID file lock acquisition for single-writer mode
//! - Worker Ed25519 keypair loading/generation for CP->Worker authentication
//! - Effective configuration logging at startup
//! - PF (packet filter) rule validation on macOS/Linux
//! - Environment fingerprint verification and drift detection

use crate::cli::Cli;
use crate::pid_lock::PidFileLock;
use crate::security::PfGuard;
use adapteros_boot::{derive_kid_from_verifying_key, load_or_generate_worker_keypair};
use adapteros_core::{resolve_var_dir, AosError};
use adapteros_server_api::config::Config;
use anyhow::Result;
use ed25519_dalek::SigningKey;
use serde::Serialize;
use std::sync::{Arc, RwLock};
use tracing::{error, info, instrument, trace, warn};

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
#[instrument(skip_all)]
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
        let keys_dir = resolve_var_dir().join("keys");
        std::fs::create_dir_all(&keys_dir).map_err(|e| {
            anyhow::anyhow!(
                "Failed to create worker keys directory at {}: {} (check permissions or disk space)",
                keys_dir.display(),
                e
            )
        })?;

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

    // =========================================================================
    // JWT Secret Validation (#163: Hardcoded insecure JWT secret)
    // =========================================================================
    // Validate that the JWT secret is not a placeholder value.
    // This prevents production deployments with insecure secrets like "CHANGE_ME".
    {
        let cfg = config.read().map_err(|e| {
            error!(error = %e, "Config lock poisoned during JWT validation");
            anyhow::anyhow!("config lock poisoned during JWT validation")
        })?;
        let production_mode = cfg.server.production_mode || cfg.security.require_pf_deny;
        validate_jwt_secret(&cfg.security.jwt_secret, production_mode)?;
    }

    Ok(SecurityContext {
        pid_lock,
        worker_keypair,
    })
}

#[derive(Debug, Serialize)]
struct RedactedConfig {
    server: adapteros_server_api::config::ServerConfig,
    db: adapteros_server_api::config::DatabaseConfig,
    security: RedactedSecurityConfig,
    paths: adapteros_server_api::config::PathsConfig,
    rate_limits: adapteros_server_api::config::RateLimitsConfig,
    metrics: RedactedMetricsConfig,
    alerting: adapteros_server_api::config::AlertingConfig,
}

#[derive(Debug, Serialize)]
struct RedactedSecurityConfig {
    require_pf_deny: bool,
    mtls_required: bool,
    jwt_secret: String,
    jwt_ttl_hours: u32,
    key_provider_mode: String,
    jwt_issuer: String,
    jwt_additional_ed25519_public_keys: Option<Vec<String>>,
    jwt_additional_hmac_secrets: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct RedactedMetricsConfig {
    enabled: bool,
    bearer_token: String,
    include_histogram: bool,
}

impl From<&Config> for RedactedConfig {
    fn from(cfg: &Config) -> Self {
        let redact = |s: &str| -> String {
            if s.is_empty() {
                "[missing]".to_string()
            } else {
                "[REDACTED]".to_string()
            }
        };

        Self {
            server: cfg.server.clone(),
            db: cfg.db.clone(),
            security: RedactedSecurityConfig {
                require_pf_deny: cfg.security.require_pf_deny,
                mtls_required: cfg.security.mtls_required,
                jwt_secret: redact(&cfg.security.jwt_secret),
                jwt_ttl_hours: cfg.security.jwt_ttl_hours,
                key_provider_mode: cfg.security.key_provider_mode.clone(),
                jwt_issuer: cfg.security.jwt_issuer.clone(),
                jwt_additional_ed25519_public_keys: cfg
                    .security
                    .jwt_additional_ed25519_public_keys
                    .as_ref()
                    .map(|_| vec!["[REDACTED]".to_string()]),
                jwt_additional_hmac_secrets: cfg
                    .security
                    .jwt_additional_hmac_secrets
                    .as_ref()
                    .map(|_| vec!["[REDACTED]".to_string()]),
            },
            paths: cfg.paths.clone(),
            rate_limits: cfg.rate_limits.clone(),
            metrics: RedactedMetricsConfig {
                enabled: cfg.metrics.enabled,
                bearer_token: if cfg.metrics.enabled {
                    redact(&cfg.metrics.bearer_token)
                } else {
                    "[disabled]".to_string()
                },
                include_histogram: cfg.metrics.include_histogram,
            },
            alerting: cfg.alerting.clone(),
        }
    }
}

/// Log the effective configuration at startup.
///
/// This logs:
/// - Effective config summary (ports, DB path, auth mode, reference mode)
/// - Server configuration (bind address, paths, timeouts)
/// - Security configuration (PF requirements, mTLS, JWT settings)
/// - Operational configuration (rate limits, metrics, alerting)
#[instrument(skip_all)]
pub fn log_effective_config(config: &Arc<RwLock<Config>>) -> Result<()> {
    let cfg = config.read().map_err(|e| {
        error!(error = %e, "Config lock poisoned at startup");
        anyhow::anyhow!("config lock poisoned at startup")
    })?;

    let redacted_config = RedactedConfig::from(&*cfg);

    info!(config = ?redacted_config, "Effective configuration loaded");

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
#[instrument(skip_all)]
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
                allow_registration: cfg.security.allow_registration,
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
                clock_skew_seconds: cfg.security.clock_skew_seconds,
                dev_bypass: cfg.security.dev_bypass,
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

// Production Security Environment Validation (P0 Security Gate)
const DEV_BYPASS_ENV_VARS: &[&str] = &[
    "AOS_DEV_NO_AUTH",
    "AOS_DEV_SIGNATURE_BYPASS",
    "AOS_SKIP_MIGRATION_SIGNATURES",
];
const ALLOW_INSECURE_FLAG: &str = "AOS_ALLOW_INSECURE_DEV_FLAGS";

/// Insecure placeholder patterns that must never be used in production.
/// These are common placeholder values that developers might forget to change.
const INSECURE_SECRET_PATTERNS: &[&str] = &[
    "CHANGE_ME",
    "changeme",
    "change-me",
    "REPLACE_ME",
    "replace_me",
    "placeholder",
    "PLACEHOLDER",
    "secret",
    "SECRET",
    "your-secret-here",
    "your_secret_here",
    "YOUR_SECRET_HERE",
    "xxx",
    "XXX",
    "test",
    "TEST",
    "development",
    "DEVELOPMENT",
];

#[derive(Debug)]
pub struct SecurityEnvValidation {
    pub passed: bool,
    pub offending_vars: Vec<String>,
    pub override_used: bool,
    pub is_release_build: bool,
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Validate that the JWT secret is not a placeholder value.
///
/// This function checks if the JWT secret matches any known placeholder patterns
/// that developers might forget to change before deploying to production.
///
/// # Arguments
///
/// * `jwt_secret` - The JWT secret to validate
/// * `production_mode` - Whether the server is running in production mode
///
/// # Returns
///
/// Returns an error if the JWT secret is a placeholder and we're in production mode.
/// In development mode, logs a warning but allows the server to start.
pub fn validate_jwt_secret(jwt_secret: &str, production_mode: bool) -> Result<()> {
    // Empty secret is always invalid
    if jwt_secret.is_empty() {
        if production_mode {
            return Err(anyhow::anyhow!(
                "SECURITY VIOLATION: JWT secret is empty. \
                 Set AOS_JWT_SECRET or security.jwt_secret in config."
            ));
        } else {
            warn!("JWT secret is empty (development mode, not blocking)");
            return Ok(());
        }
    }

    // Check for minimum length (HMAC secrets should be at least 32 bytes for HS256)
    const MIN_SECRET_LENGTH: usize = 32;
    if jwt_secret.len() < MIN_SECRET_LENGTH {
        if production_mode {
            return Err(anyhow::anyhow!(
                "SECURITY VIOLATION: JWT secret is too short ({} chars, minimum {}). \
                 Use a cryptographically secure random string.",
                jwt_secret.len(),
                MIN_SECRET_LENGTH
            ));
        } else {
            warn!(
                length = jwt_secret.len(),
                min = MIN_SECRET_LENGTH,
                "JWT secret is too short (development mode, not blocking)"
            );
        }
    }

    // Check for placeholder patterns
    let secret_lower = jwt_secret.to_lowercase();
    for pattern in INSECURE_SECRET_PATTERNS {
        if secret_lower.contains(&pattern.to_lowercase()) {
            if production_mode {
                return Err(anyhow::anyhow!(
                    "SECURITY VIOLATION: JWT secret contains placeholder pattern '{}'. \
                     Replace with a cryptographically secure random string. \
                     Generate one with: openssl rand -base64 32",
                    pattern
                ));
            } else {
                warn!(
                    pattern = %pattern,
                    "JWT secret contains placeholder pattern (development mode, not blocking)"
                );
                return Ok(());
            }
        }
    }

    Ok(())
}

/// Block dev bypass flags in release builds. Fails fast before any other init.
pub fn validate_production_security_env() -> Result<SecurityEnvValidation> {
    let is_release = !cfg!(debug_assertions);
    let offending: Vec<String> = DEV_BYPASS_ENV_VARS
        .iter()
        .filter(|&&var| env_truthy(var))
        .map(|&s| s.to_string())
        .collect();
    let override_requested = env_truthy(ALLOW_INSECURE_FLAG);

    if offending.is_empty() {
        return Ok(SecurityEnvValidation {
            passed: true,
            offending_vars: vec![],
            override_used: false,
            is_release_build: is_release,
        });
    }

    if is_release {
        if override_requested {
            error!(offending_vars = ?offending, "DANGER: Dev bypass flags in RELEASE build");
            Ok(SecurityEnvValidation {
                passed: true,
                offending_vars: offending,
                override_used: true,
                is_release_build: true,
            })
        } else {
            for var in &offending {
                error!(env_var = %var, "Offending environment variable");
            }
            Err(anyhow::anyhow!(
                "SECURITY VIOLATION: Dev bypass flags [{}] in release build. Set {}=1 to override.",
                offending.join(", "),
                ALLOW_INSECURE_FLAG
            ))
        }
    } else {
        warn!(offending_vars = ?offending, "Dev bypass flags detected (debug build)");
        Ok(SecurityEnvValidation {
            passed: true,
            offending_vars: offending,
            override_used: false,
            is_release_build: false,
        })
    }
}
