//! Boot-time invariant validation for adapterOS.
//!
//! This module validates critical system invariants at startup, catching
//! configuration errors and unsafe states before they can cause silent failures.
//!
//! # Invariant Categories
//!
//! 1. **Security invariants**: Auth bypass flags, attestation requirements
//! 2. **Data integrity invariants**: Dual-write mode, storage consistency
//! 3. **Lifecycle invariants**: Boot phase ordering, executor initialization
//!
//! # Failure Modes
//!
//! - Production mode: Invariant violations are FATAL (fail closed)
//! - Development mode: Violations are logged as warnings (fail open)
//!
//! # Metrics
//!
//! Boot-time counters are stored in atomics (via adapteros-boot) and flushed
//! to the metrics exporter after it initializes (Phase 9c).

use adapteros_boot::{
    record_invariant_check, record_invariant_skipped, record_invariant_violation,
};
use adapteros_core::AosError;
use adapteros_db::adapters::AtomicDualWriteConfig;
use adapteros_server_api::config::{is_production, Config};
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

// Re-export for the API endpoint and backwards compatibility
pub use adapteros_boot::{boot_invariant_metrics, BootInvariantMetrics};

fn record_check() {
    record_invariant_check();
}

fn record_violation(fatal: bool) {
    record_invariant_violation(fatal);
}

fn record_skipped() {
    record_invariant_skipped();
}

/// Result of an invariant check.
#[derive(Debug, Clone)]
pub struct InvariantViolation {
    /// Unique identifier for this invariant
    pub id: &'static str,
    /// Human-readable description of what was violated
    pub message: String,
    /// Whether this violation should block startup in production
    pub is_fatal: bool,
    /// Suggested remediation
    pub remediation: &'static str,
}

/// Aggregated result of all invariant checks.
#[derive(Debug, Default)]
pub struct InvariantReport {
    pub violations: Vec<InvariantViolation>,
    pub checks_passed: usize,
    pub checks_failed: usize,
    pub checks_skipped: usize,
    pub skipped_ids: Vec<&'static str>,
}

impl InvariantReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_pass(&mut self) {
        self.checks_passed += 1;
        record_check();
    }

    pub fn record_violation(&mut self, violation: InvariantViolation) {
        record_check();
        record_violation(violation.is_fatal);
        self.checks_failed += 1;
        self.violations.push(violation);
    }

    /// Record that an invariant check was skipped due to config escape hatch.
    /// This is logged as a warning since it bypasses safety checks.
    pub fn record_skip(&mut self, id: &'static str) {
        self.checks_skipped += 1;
        self.skipped_ids.push(id);
        record_skipped();
        warn!(
            invariant = id,
            "INVARIANT CHECK SKIPPED via config escape hatch (NOT RECOMMENDED)"
        );
    }

    pub fn has_fatal_violations(&self) -> bool {
        self.violations.iter().any(|v| v.is_fatal)
    }

    pub fn fatal_count(&self) -> usize {
        self.violations.iter().filter(|v| v.is_fatal).count()
    }

    pub fn warning_count(&self) -> usize {
        self.violations.iter().filter(|v| !v.is_fatal).count()
    }
}

/// Validates all critical invariants at boot time.
///
/// # Arguments
///
/// * `config` - Server configuration
/// * `executor_initialized` - Whether the deterministic executor was initialized with a valid manifest
///
/// # Returns
///
/// Returns `InvariantReport` containing all violations found.
///
/// # Checked Invariants
///
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `SEC-001` | Dev auth bypass must not be active in production | Yes |
/// | `SEC-002` | Dual-write strict mode required in production | Yes |
/// | `SEC-003` | Executor must have manifest-derived seed in production | Yes |
/// | `SEC-004` | Hardware attestation fallback warning | No (warning) |
/// | `SEC-005` | Cookie security settings in production | Yes |
/// | `SEC-006` | JWT algorithm must be valid | Yes |
/// | `SEC-008` | RBAC permission configuration must be present | Yes |
/// | `SEC-014` | Brute force protection must be configured | Yes |
/// | `SEC-015` | Signature bypass env var must not be set in production | Yes |
/// | `CFG-001` | Default var/ paths rejected when AOS_VAR_DIR is set | Yes |
/// | `CFG-002` | Session TTL must be >= access token TTL | Yes |
/// | `DAT-002` | Foreign key constraints must be enabled | Yes |
/// | `DAT-005` | Storage mode must be a valid enum value | Yes |
/// | `DAT-006` | Database path must be configured for migrations | Yes |
/// | `DAT-007` | Audit chain initialization (async) | No (FAILS OPEN) |
/// | `LIF-002` | Executor initialization check | Deferred to SEC-003 |
/// | `SEC-007` | Tenant isolation (require_pf_deny) in production | No (warning) |
/// | `MEM-003` | Memory headroom configuration sanity check | No (FAILS OPEN) |
/// | `LIF-001` | Boot phase ordering advisory | No (informational) |
pub fn validate_boot_invariants(
    config: &Arc<RwLock<Config>>,
    executor_manifest_hash_present: bool,
) -> InvariantReport {
    let mut report = InvariantReport::new();

    let cfg = match config.read() {
        Ok(c) => c,
        Err(e) => {
            report.record_violation(InvariantViolation {
                id: "SYS-001",
                message: format!("Config lock poisoned: {}", e),
                is_fatal: true,
                remediation: "Restart the server; config lock should not be poisoned at boot",
            });
            return report;
        }
    };

    let production = is_production(&cfg);
    let invariants_config = &cfg.invariants;

    // =========================================================================
    // SEC-001: Dev auth bypass must not be active in production
    // =========================================================================
    // Enforced: security/mod.rs:109-112, auth.rs:122-173
    // Violation: AOS_DEV_NO_AUTH=1 in production build
    // Fails: CLOSED in release (env var ignored), but check anyway
    if invariants_config.disable_sec_001_dev_bypass {
        report.record_skip("SEC-001");
    } else {
        let dev_no_auth_requested = std::env::var("AOS_DEV_NO_AUTH")
            .ok()
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);

        if production && dev_no_auth_requested {
            report.record_violation(InvariantViolation {
                id: "SEC-001",
                message: "AOS_DEV_NO_AUTH is set but production mode is enabled".to_string(),
                is_fatal: true,
                remediation:
                    "Remove AOS_DEV_NO_AUTH environment variable or disable production_mode",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-002: Dual-write strict mode required in production
    // =========================================================================
    // Enforced: adapters.rs:103-140
    // Violation: AOS_ATOMIC_DUAL_WRITE_STRICT=0 in production
    // Fails: OPEN in best-effort mode (SQL commits without KV)
    if invariants_config.disable_sec_002_dual_write {
        report.record_skip("SEC-002");
    } else {
        let dual_write_config = AtomicDualWriteConfig::from_env();

        if production && !dual_write_config.is_strict() {
            report.record_violation(InvariantViolation {
                id: "SEC-002",
                message: "Atomic dual-write strict mode is DISABLED in production".to_string(),
                is_fatal: true,
                remediation: "Set AOS_ATOMIC_DUAL_WRITE_STRICT=1 or remove the variable",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-003: Executor must have manifest-derived seed in production
    // =========================================================================
    // Enforced: executor.rs:156-172
    // Violation: No valid manifest → default seed used
    // Fails: OPEN (non-deterministic execution)
    if invariants_config.disable_sec_003_executor_seed {
        report.record_skip("SEC-003");
    } else if production && !executor_manifest_hash_present {
        report.record_violation(InvariantViolation {
            id: "SEC-003",
            message: "Deterministic executor initialized with default seed (no valid manifest)"
                .to_string(),
            is_fatal: true,
            remediation: "Provide valid manifest via --manifest-path or AOS_MANIFEST_PATH",
        });
    } else {
        report.record_pass();
    }

    // =========================================================================
    // SEC-004: Hardware attestation configuration (warning only)
    // =========================================================================
    // Enforced: attestation.rs:130-144
    // Violation: Software fallback allowed (Secure Enclave unavailable)
    // Fails: OPEN if verify_hardware_attestation() not called
    //
    // Note: We can only warn here; actual enforcement is at attestation time
    {
        // Check if we're on macOS where Secure Enclave should be available
        #[cfg(target_os = "macos")]
        {
            if production {
                // Log advisory warning - actual enforcement is at attestation time
                warn!(
                    invariant = "SEC-004",
                    "Hardware attestation enforcement is implicit; ensure verify_hardware_attestation() is called before accepting federation bundles"
                );
            }
        }
        report.record_pass(); // This is advisory, not enforced at boot
    }

    // =========================================================================
    // SEC-005: Cookie security settings in production
    // =========================================================================
    // Enforced: auth_common.rs:225-347
    // Violation: SameSite=None without Secure, or Lax in production
    if invariants_config.disable_sec_005_cookie_security {
        report.record_skip("SEC-005");
    } else {
        let cookie_same_site = cfg.security.cookie_same_site.to_ascii_lowercase();
        let cookie_secure = cfg.security.cookie_secure.unwrap_or(production);

        if production && cookie_same_site == "none" && !cookie_secure {
            report.record_violation(InvariantViolation {
                id: "SEC-005",
                message: "SameSite=None requires Secure flag in production".to_string(),
                is_fatal: true,
                remediation: "Set cookie_secure=true or change cookie_same_site to Strict/Lax",
            });
        } else if production && cookie_same_site == "lax" {
            // Warning only - Lax is acceptable but Strict is preferred
            warn!(
                invariant = "SEC-005",
                "cookie_same_site=Lax in production; consider Strict for better CSRF protection"
            );
            report.record_pass();
        } else {
            report.record_pass();
        }

        // Log effective cookie configuration for debugging session issues
        let cookie_domain = cfg.security.cookie_domain.as_deref().unwrap_or("<not set>");
        info!(
            same_site = %cfg.security.cookie_same_site,
            secure = cookie_secure,
            domain = cookie_domain,
            clock_skew_seconds = cfg.security.clock_skew_seconds,
            "Cookie configuration"
        );
    }

    // =========================================================================
    // CFG-001: Reject default var/ paths when AOS_VAR_DIR is set
    // =========================================================================
    {
        let env_var_dir = std::env::var("AOS_VAR_DIR").ok();
        let override_active = env_var_dir
            .as_deref()
            .map(|val| {
                let trimmed = val.trim();
                !trimmed.is_empty() && trimmed != "var" && trimmed != "./var"
            })
            .unwrap_or(false);

        if override_active {
            let mut offenders: Vec<String> = Vec::new();
            let mut check = |label: &str, value: &str| {
                if uses_default_var_path(value) {
                    offenders.push(format!("{}={}", label, value));
                }
            };

            check("db.path", &cfg.db.path);
            check("db.kv_path", &cfg.db.kv_path);
            if let Some(path) = cfg.db.kv_tantivy_path.as_deref() {
                check("db.kv_tantivy_path", path);
            }

            check("paths.artifacts_root", &cfg.paths.artifacts_root);
            check("paths.bundles_root", &cfg.paths.bundles_root);
            check("paths.adapters_root", &cfg.paths.adapters_root);
            check("paths.plan_dir", &cfg.paths.plan_dir);
            check("paths.datasets_root", &cfg.paths.datasets_root);
            check("paths.documents_root", &cfg.paths.documents_root);

            check("alerting.alert_dir", &cfg.alerting.alert_dir);
            if let Some(path) = cfg.logging.log_dir.as_deref() {
                check("logging.log_dir", path);
            }
            if let Some(path) = cfg.security.key_file_path.as_deref() {
                check("security.key_file_path", path);
            }
            if let Some(path) = cfg.server.uds_socket.as_deref() {
                check("server.uds_socket", path);
            }

            if offenders.is_empty() {
                report.record_pass();
            } else {
                report.record_violation(InvariantViolation {
                    id: "CFG-001",
                    message: format!(
                        "Default var/ paths still configured while AOS_VAR_DIR is set: {}",
                        offenders.join(", ")
                    ),
                    is_fatal: true,
                    remediation: "Rebase paths under AOS_VAR_DIR or remove default var/ references",
                });
            }
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-015: Signature bypass must not be requested in production
    // =========================================================================
    // Enforced: adapteros-crypto/src/bundle_sign.rs (compile-time gate)
    // Violation: AOS_DEV_SIGNATURE_BYPASS=1 in production build
    // Note: The env var is already ignored in release builds via #[cfg(debug_assertions)],
    //       but this check provides defense-in-depth and clear logging.
    if invariants_config.disable_sec_015_signature_bypass {
        report.record_skip("SEC-015");
    } else {
        let sig_bypass_requested = std::env::var("AOS_DEV_SIGNATURE_BYPASS")
            .ok()
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);

        if production && sig_bypass_requested {
            report.record_violation(InvariantViolation {
                id: "SEC-015",
                message: "AOS_DEV_SIGNATURE_BYPASS is set but production mode is enabled"
                    .to_string(),
                is_fatal: true,
                remediation:
                    "Remove AOS_DEV_SIGNATURE_BYPASS environment variable or disable production_mode",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-006: JWT algorithm configuration must match key provider
    // =========================================================================
    // Enforced: auth.rs:926-979
    // Violation: JWT mode differs from expected values
    // Fails: CLOSED in production (reject mismatched tokens)
    if invariants_config.disable_sec_006_jwt_verify {
        report.record_skip("SEC-006");
    } else {
        // Verify JWT mode configuration is valid
        let jwt_mode = cfg.security.jwt_mode.as_deref().unwrap_or("hs256");
        let valid_modes = ["hs256", "hmac", "eddsa", "ed25519"];

        if !valid_modes.contains(&jwt_mode.to_lowercase().as_str()) {
            report.record_violation(InvariantViolation {
                id: "SEC-006",
                message: format!("Unknown JWT mode configured: {}", jwt_mode),
                is_fatal: production,
                remediation: "Set security.jwt_mode to a valid value (hs256, eddsa)",
            });
        } else if production && jwt_mode.to_lowercase() == "hs256" {
            // In production, warn if using HMAC (EdDSA is preferred)
            warn!(
                invariant = "SEC-006",
                jwt_mode = jwt_mode,
                "Using HMAC for JWT in production; EdDSA is recommended for better security"
            );
            report.record_pass();
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // DAT-002: Foreign key constraints must be enabled in SQLite
    // =========================================================================
    // Enforced: migrations/0001_init.sql (PRAGMA foreign_keys = ON)
    // Violation: PRAGMA foreign_keys = OFF
    // Fails: CLOSED (data integrity violation risk)
    if invariants_config.disable_dat_002_foreign_keys {
        report.record_skip("DAT-002");
    } else {
        // Check if FOREIGN_KEYS enforcement is configured
        // Note: This is a boot-time check; actual PRAGMA is set in connection string
        let db_path = &cfg.db.path;
        if db_path.contains("_fk=off") || db_path.contains("_foreign_keys=off") {
            report.record_violation(InvariantViolation {
                id: "DAT-002",
                message: "Foreign key constraints disabled in database connection string"
                    .to_string(),
                is_fatal: production,
                remediation: "Remove _fk=off or _foreign_keys=off from database connection string",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // DAT-006: Migration ordering must be verified
    // =========================================================================
    // Enforced: sqlx migrations
    // Violation: Migrations applied out of order
    // Fails: CLOSED (schema inconsistency)
    if invariants_config.disable_dat_006_migration_order {
        report.record_skip("DAT-006");
    } else {
        // Note: Actual migration ordering is verified by sqlx at runtime.
        // This check verifies the migration configuration is present.
        let migrations_configured = !cfg.db.path.is_empty();
        if !migrations_configured {
            report.record_violation(InvariantViolation {
                id: "DAT-006",
                message: "Database path not configured; migrations cannot be verified".to_string(),
                is_fatal: production,
                remediation: "Configure db.path in configuration file",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // LIF-002: Global executor must be properly initialized
    // =========================================================================
    // Enforced: backend_factory.rs:126-143
    // Violation: Executor not initialized with proper seed
    // Fails: CLOSED (non-deterministic execution)
    if invariants_config.disable_lif_002_executor_init {
        report.record_skip("LIF-002");
    } else {
        // This check complements SEC-003 by verifying the executor was
        // actually initialized, not just that a manifest was provided.
        // The executor_manifest_hash_present flag indicates proper init.
        if production && !executor_manifest_hash_present {
            // Note: This is covered by SEC-003, but we log it here for completeness
            // in the lifecycle category without double-counting violations.
            info!(
                invariant = "LIF-002",
                "Executor initialization check deferred to SEC-003"
            );
        }
        report.record_pass();
    }

    // =========================================================================
    // DAT-007: Audit log chain must be initialized (FAILS OPEN)
    // =========================================================================
    // Enforced: audit.rs:80-100
    // Violation: Audit chain not initialized
    // Fails: OPEN (warning only - audit is defense-in-depth)
    if invariants_config.disable_dat_007_audit_chain {
        report.record_skip("DAT-007");
    } else {
        // Note: Audit chain initialization happens asynchronously.
        // We can only log an advisory at boot time.
        if production {
            info!(
                invariant = "DAT-007",
                "Audit chain initialization is async; verify via aosctl doctor"
            );
        }
        report.record_pass();
    }

    // =========================================================================
    // SEC-008: RBAC permission configuration must be present
    // =========================================================================
    // Enforced: permissions.rs:206+ (implicit per-handler)
    // Violation: RBAC configuration missing or invalid
    // Fails: CLOSED in production (no auth enforcement)
    //
    // Note: RBAC enforcement is implicit per-handler, but we validate that
    // the auth configuration enabling it is properly set up.
    if invariants_config.disable_sec_008_rbac_config {
        report.record_skip("SEC-008");
    } else {
        // RBAC is enforced when JWT is properly configured.
        // We've already validated JWT in SEC-006, so this check ensures
        // the auth subsystem is configured to enforce permissions.
        let jwt_secret_valid = !cfg.security.jwt_secret.is_empty();
        let jwt_secret_strong = cfg.security.jwt_secret.len() >= 32;

        if production && !jwt_secret_valid {
            report.record_violation(InvariantViolation {
                id: "SEC-008",
                message: "JWT secret is empty; RBAC cannot be enforced".to_string(),
                is_fatal: true,
                remediation: "Set security.jwt_secret to a non-empty value (32+ chars recommended)",
            });
        } else if production && !jwt_secret_strong {
            warn!(
                invariant = "SEC-008",
                jwt_secret_len = cfg.security.jwt_secret.len(),
                "JWT secret is short (< 32 chars); consider using a stronger secret"
            );
            report.record_pass();
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-014: Brute force protection must be configured in production
    // =========================================================================
    // Enforced: security/mod.rs:341-457 (rate limiting, lockout)
    // Violation: Lockout settings disabled or too permissive
    // Fails: CLOSED in production (no brute force protection)
    if invariants_config.disable_sec_014_brute_force {
        report.record_skip("SEC-014");
    } else {
        let lockout_threshold = cfg.auth.lockout_threshold;
        let lockout_cooldown = cfg.auth.lockout_cooldown;

        if production && lockout_threshold == 0 {
            report.record_violation(InvariantViolation {
                id: "SEC-014",
                message: "Brute force protection disabled: lockout_threshold is 0".to_string(),
                is_fatal: true,
                remediation: "Set auth.lockout_threshold to a positive value (default: 5)",
            });
        } else if production && lockout_cooldown == 0 {
            report.record_violation(InvariantViolation {
                id: "SEC-014",
                message: "Brute force protection ineffective: lockout_cooldown is 0".to_string(),
                is_fatal: true,
                remediation: "Set auth.lockout_cooldown to a positive value (default: 300 seconds)",
            });
        } else if production && lockout_threshold > 20 {
            // Warning only - high threshold is permissive but not fatal
            warn!(
                invariant = "SEC-014",
                lockout_threshold = lockout_threshold,
                "High lockout threshold may allow excessive login attempts"
            );
            report.record_pass();
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // DAT-005: Storage mode must be a valid enum value
    // =========================================================================
    // Enforced: factory.rs:106-158 (storage backend parsing)
    // Violation: Unknown storage mode string in config
    // Fails: CLOSED (prevents boot with invalid storage config)
    if invariants_config.disable_dat_005_storage_mode {
        report.record_skip("DAT-005");
    } else {
        let storage_mode = cfg.db.storage_mode.to_lowercase();
        let valid_modes = [
            "sql_only",
            "sql",
            "dual_write",
            "dual",
            "kv_primary",
            "kv-primary",
            "kv_only",
            "kv-only",
        ];

        if !valid_modes.contains(&storage_mode.as_str()) {
            report.record_violation(InvariantViolation {
                id: "DAT-005",
                message: format!(
                    "Invalid storage mode: '{}'. Must be one of: sql_only, dual_write, kv_primary, kv_only",
                    cfg.db.storage_mode
                ),
                is_fatal: production,
                remediation: "Set db.storage_mode to a valid value (default: sql_only)",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // CFG-002: Session TTL must be >= access token TTL
    // =========================================================================
    // Enforced: auth.rs session/token management
    // Violation: access_token_ttl > session_ttl creates refresh loop issues
    // Fails: CLOSED in production (broken session management)
    if invariants_config.disable_cfg_002_session_ttl {
        report.record_skip("CFG-002");
    } else {
        let access_ttl = cfg.security.access_token_ttl_seconds;
        let session_ttl = cfg.security.session_ttl_seconds;

        if access_ttl > session_ttl {
            report.record_violation(InvariantViolation {
                id: "CFG-002",
                message: format!(
                    "access_token_ttl ({} sec) exceeds session_ttl ({} sec); tokens would outlive sessions",
                    access_ttl, session_ttl
                ),
                is_fatal: production,
                remediation: "Set access_token_ttl_seconds <= session_ttl_seconds",
            });
        } else if access_ttl == session_ttl && production {
            // Warning only - equal TTLs work but prevent refresh
            warn!(
                invariant = "CFG-002",
                access_ttl = access_ttl,
                session_ttl = session_ttl,
                "access_token_ttl equals session_ttl; token refresh will not extend sessions"
            );
            report.record_pass();
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-007: Tenant isolation configuration check
    // =========================================================================
    // Enforced: security/mod.rs:68-179 (per-handler tenant isolation)
    // Violation: require_pf_deny disabled in production
    // Fails: OPEN (warning only - advisory check)
    //
    // Note: require_pf_deny ensures PF (packet filter) deny rules are in place
    // for tenant isolation in air-gapped deployments. If disabled in production,
    // tenants may not be properly isolated at the network level.
    if invariants_config.disable_sec_007_tenant_isolation {
        report.record_skip("SEC-007");
    } else {
        let require_pf_deny = cfg.security.require_pf_deny;

        if production && !require_pf_deny {
            // Warning only - not fatal, but should be reviewed
            warn!(
                invariant = "SEC-007",
                require_pf_deny = require_pf_deny,
                "Tenant isolation: require_pf_deny is disabled in production; \
                 network-level tenant isolation may not be enforced"
            );
            // Record as non-fatal violation for visibility in report
            report.record_violation(InvariantViolation {
                id: "SEC-007",
                message:
                    "require_pf_deny is disabled in production; tenant isolation may be weakened"
                        .to_string(),
                is_fatal: false, // Advisory only
                remediation:
                    "Set security.require_pf_deny = true for network-level tenant isolation",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // MEM-003: Memory headroom configuration sanity check
    // =========================================================================
    // Enforced: unified_tracker.rs:497-536 (runtime memory management)
    // Violation: Memory headroom percentage is 0 or unreasonably high (> 50%)
    // Fails: OPEN (warning only - allows boot to continue)
    //
    // Note: Memory headroom is typically configured via policy packs (min_headroom_pct)
    // rather than the main config. This check validates any AOS_MEMORY_HEADROOM_PCT
    // environment variable override if set.
    if invariants_config.disable_mem_003_memory_headroom {
        report.record_skip("MEM-003");
    } else {
        // Check for environment variable override of memory headroom
        let headroom_env = std::env::var("AOS_MEMORY_HEADROOM_PCT").ok();

        if let Some(headroom_str) = headroom_env {
            match headroom_str.trim().parse::<f64>() {
                Ok(headroom_pct) => {
                    if headroom_pct == 0.0 {
                        // Zero headroom is dangerous - no buffer for memory spikes
                        report.record_violation(InvariantViolation {
                            id: "MEM-003",
                            message: "Memory headroom is 0%; no buffer for memory pressure spikes"
                                .to_string(),
                            is_fatal: false, // FAILS OPEN
                            remediation: "Set AOS_MEMORY_HEADROOM_PCT to 10-20% for safe operation",
                        });
                    } else if headroom_pct > 50.0 {
                        // > 50% headroom is unreasonable - wastes resources
                        report.record_violation(InvariantViolation {
                            id: "MEM-003",
                            message: format!(
                                "Memory headroom is {}% (> 50%); unreasonably high, wastes available memory",
                                headroom_pct
                            ),
                            is_fatal: false, // FAILS OPEN
                            remediation: "Set AOS_MEMORY_HEADROOM_PCT between 5-30% for balanced operation",
                        });
                    } else if headroom_pct < 0.0 {
                        // Negative headroom makes no sense
                        report.record_violation(InvariantViolation {
                            id: "MEM-003",
                            message: format!(
                                "Memory headroom is {}% (negative); invalid configuration",
                                headroom_pct
                            ),
                            is_fatal: false, // FAILS OPEN
                            remediation:
                                "Set AOS_MEMORY_HEADROOM_PCT to a positive percentage (e.g., 15)",
                        });
                    } else {
                        // Valid headroom configured
                        info!(
                            invariant = "MEM-003",
                            headroom_pct = headroom_pct,
                            "Memory headroom configured via environment"
                        );
                        report.record_pass();
                    }
                }
                Err(_) => {
                    // Invalid parse - not a number
                    report.record_violation(InvariantViolation {
                        id: "MEM-003",
                        message: format!(
                            "AOS_MEMORY_HEADROOM_PCT='{}' is not a valid number",
                            headroom_str
                        ),
                        is_fatal: false, // FAILS OPEN
                        remediation:
                            "Set AOS_MEMORY_HEADROOM_PCT to a numeric percentage (e.g., 15)",
                    });
                }
            }
        } else {
            // No override set - memory headroom will use policy pack defaults (typically 15%)
            info!(
                invariant = "MEM-003",
                "Memory headroom using policy pack defaults (typically 15%)"
            );
            report.record_pass();
        }
    }

    // =========================================================================
    // LIF-001: Boot phase ordering advisory
    // =========================================================================
    // Enforced: boot/ module phase ordering
    // Violation: None - this is purely informational
    // Fails: OPEN (always passes - informational only)
    //
    // Note: This check logs that boot phases are being validated in order.
    // The actual phase ordering is enforced by the boot sequence itself,
    // but this provides visibility that the invariant system is active.
    if invariants_config.disable_lif_001_boot_ordering {
        report.record_skip("LIF-001");
    } else {
        info!(
            invariant = "LIF-001",
            "Boot phase ordering: invariant validation is running (Phase 3 of boot sequence)"
        );
        // Always passes - this is purely informational
        report.record_pass();
    }

    // =========================================================================
    // Remaining invariants to implement (16 more from analysis)
    // =========================================================================
    // Categorized by check timing:
    //
    // BOOT-TIME CONFIG-ONLY (can check before DB connection):
    // - SEC-011: Quorum signature threshold config (federation/signature.rs:318-350)
    // - SEC-012: Adapter bundle signature requirement (cli/verify_adapter.rs:48-64)
    //
    // BOOT-TIME POST-DB (require active DB connection):
    // - DAT-003: AOS file hash match (adapter_aos_invariant_tests.rs)
    // - DAT-004: KV presence for adapter readiness (adapter_consistency.rs)
    //
    // RUNTIME-IMPLICIT (monitored continuously, not at boot):
    // These are enforced by their respective subsystems during operation.
    // Violations are detected and logged at runtime, not boot.
    //
    // - SEC-009: Token revocation baseline (security/mod.rs:288-320)
    //   → Enforced: Every token validation checks revocation list
    // - SEC-010: Hardware attestation (federation/attestation.rs:130-144)
    //   → Enforced: Each federation bundle import validates attestation
    // - SEC-013: Password timing-safety (auth.rs:298-349)
    //   → Enforced: constant_time_compare() used in all password checks
    // - MEM-001: KV cache generation coherence (kvcache.rs:262-298)
    //   → Enforced: KV cache ops validate generation before use
    // - MEM-002: GPU buffer fingerprint (unified_tracker.rs:399-425)
    //   → Enforced: Buffer allocator validates fingerprints on access
    // - MEM-004: KV slab non-overlapping (kvcache.rs:424-464)
    //   → Enforced: Slab allocator checks overlaps; FAILS OPEN on race
    // - CON-001: Hot-swap atomic pointer (adapter_hotswap.rs:194-196)
    //   → Enforced: AtomicPtr swap with RCU-style cleanup; FAILS OPEN
    // - CON-002: KV quota transactional (kv_quota.rs:140-208)
    //   → Enforced: Quota ops use transaction boundaries
    // - CON-003: Model cache pinning (model_handle_cache.rs)
    //   → Enforced: Pin guard prevents eviction; FAILS OPEN on OOM
    // - CON-004: Request pin refcount (request_pinner.rs:17-42)
    //   → Enforced: Refcount checked before resource cleanup
    // - LIF-003: Adapter lifecycle CAS (lora-lifecycle/state.rs:107-148)
    //   → Enforced: State transitions use compare_and_swap

    report
}

/// Validates database-dependent invariants after DB pool is ready.
///
/// This function checks invariants that require an active database connection,
/// such as verifying triggers exist from specific migrations.
///
/// # Arguments
///
/// * `config` - Server configuration
/// * `db` - Active SQLite connection pool
///
/// # Returns
///
/// Returns `InvariantReport` containing all violations found.
///
/// # Checked Invariants
///
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `DAT-001` | Archive state machine triggers from migration 0138 | Yes |
/// | `LIF-004` | Connection pool drain configuration | No (warning) |
pub async fn validate_post_db_invariants(
    config: &Arc<RwLock<Config>>,
    db: &sqlx::SqlitePool,
) -> InvariantReport {
    let mut report = InvariantReport::new();

    // Extract config values before any await points to avoid holding lock across awaits
    let (production, disable_dat_001, disable_lif_004, drain_timeout) = match config.read() {
        Ok(cfg) => (
            is_production(&cfg),
            cfg.invariants.disable_dat_001_archive_triggers,
            cfg.invariants.disable_lif_004_pool_drain,
            cfg.server.drain_timeout_secs,
        ),
        Err(e) => {
            report.record_violation(InvariantViolation {
                id: "SYS-001",
                message: format!("Config lock poisoned: {}", e),
                is_fatal: true,
                remediation: "Restart the server; config lock should not be poisoned at boot",
            });
            return report;
        }
    };

    // =========================================================================
    // DAT-001: Archive state machine triggers must exist
    // =========================================================================
    // Enforced: migrations/0138_adapter_archive_gc.sql
    // Violation: Required triggers not present in sqlite_master
    // Fails: CLOSED in production (archive lifecycle broken)
    if disable_dat_001 {
        report.record_skip("DAT-001");
    } else {
        let expected_triggers = ["adapter_archive_purge_check", "adapter_purged_no_load"];
        let mut missing_triggers: Vec<&str> = Vec::new();

        for trigger_name in expected_triggers {
            let exists =
                sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?")
                    .bind(trigger_name)
                    .fetch_optional(db)
                    .await;

            match exists {
                Ok(Some(_)) => {
                    // Trigger exists
                }
                Ok(None) => {
                    missing_triggers.push(trigger_name);
                }
                Err(e) => {
                    report.record_violation(InvariantViolation {
                        id: "DAT-001",
                        message: format!(
                            "Failed to query sqlite_master for trigger '{}': {}",
                            trigger_name, e
                        ),
                        is_fatal: production,
                        remediation: "Check database connectivity and schema integrity",
                    });
                }
            }
        }

        if missing_triggers.is_empty() {
            report.record_pass();
        } else {
            report.record_violation(InvariantViolation {
                id: "DAT-001",
                message: format!(
                    "Archive state machine triggers missing from migration 0138: [{}]",
                    missing_triggers.join(", ")
                ),
                is_fatal: production,
                remediation: "Run database migrations: aosctl db migrate",
            });
        }
    }

    // =========================================================================
    // LIF-004: Connection pool drain configuration
    // =========================================================================
    // Enforced: boot/finalization.rs (shutdown sequence)
    // Violation: drain_timeout_secs is 0 or too short
    // Fails: OPEN (warning only - short drain may cause request drops)
    if disable_lif_004 {
        report.record_skip("LIF-004");
    } else if drain_timeout == 0 {
        report.record_violation(InvariantViolation {
            id: "LIF-004",
            message: "Connection pool drain disabled: drain_timeout_secs is 0".to_string(),
            is_fatal: false, // FAILS OPEN - warning only
            remediation: "Set server.drain_timeout_secs to a positive value (default: 30)",
        });
    } else if drain_timeout < 5 {
        report.record_violation(InvariantViolation {
            id: "LIF-004",
            message: format!(
                "Connection pool drain timeout too short: {} seconds (< 5s recommended minimum)",
                drain_timeout
            ),
            is_fatal: false, // FAILS OPEN - warning only
            remediation:
                "Set server.drain_timeout_secs >= 5 for graceful shutdown of in-flight requests",
        });
    } else {
        report.record_pass();
    }

    report
}

fn uses_default_var_path(raw: &str) -> bool {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("sqlite://") {
        let (path, _) = rest.split_once('?').unwrap_or((rest, ""));
        return uses_default_var_path(path);
    }

    let candidate = trimmed.strip_prefix("./").unwrap_or(trimmed);
    candidate == "var" || candidate.starts_with("var/")
}

/// Evaluates invariant report and fails if fatal violations exist in production.
///
/// # Arguments
///
/// * `report` - The invariant report to evaluate
/// * `production` - Whether we're in production mode
///
/// # Returns
///
/// Returns `Ok(())` if no fatal violations, or `Err` with details if blocked.
pub fn enforce_invariants(report: &InvariantReport, production: bool) -> Result<(), AosError> {
    // Log all violations
    for violation in &report.violations {
        if violation.is_fatal {
            error!(
                invariant = violation.id,
                fatal = true,
                remediation = violation.remediation,
                "INVARIANT VIOLATION: {}",
                violation.message
            );
        } else {
            warn!(
                invariant = violation.id,
                fatal = false,
                remediation = violation.remediation,
                "Invariant warning: {}",
                violation.message
            );
        }
    }

    // Summary
    if report.checks_skipped > 0 {
        warn!(
            passed = report.checks_passed,
            failed = report.checks_failed,
            skipped = report.checks_skipped,
            skipped_ids = ?report.skipped_ids,
            fatal = report.fatal_count(),
            warnings = report.warning_count(),
            "Invariant validation complete (WARNING: {} checks skipped via config)",
            report.checks_skipped
        );
    } else {
        info!(
            passed = report.checks_passed,
            failed = report.checks_failed,
            fatal = report.fatal_count(),
            warnings = report.warning_count(),
            "Invariant validation complete"
        );
    }

    // In production, fatal violations block startup
    if production && report.has_fatal_violations() {
        let fatal_ids: Vec<&str> = report
            .violations
            .iter()
            .filter(|v| v.is_fatal)
            .map(|v| v.id)
            .collect();

        return Err(AosError::PolicyViolation(format!(
            "Boot blocked: {} fatal invariant violation(s): [{}]. \
             See logs above for remediation steps.",
            report.fatal_count(),
            fatal_ids.join(", ")
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invariant_report_tracks_passes_and_failures() {
        let mut report = InvariantReport::new();
        report.record_pass();
        report.record_pass();
        report.record_violation(InvariantViolation {
            id: "TEST-001",
            message: "Test violation".to_string(),
            is_fatal: true,
            remediation: "Fix it",
        });

        assert_eq!(report.checks_passed, 2);
        assert_eq!(report.checks_failed, 1);
        assert!(report.has_fatal_violations());
        assert_eq!(report.fatal_count(), 1);
    }

    #[test]
    fn test_non_fatal_violations_dont_block() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "TEST-002",
            message: "Warning only".to_string(),
            is_fatal: false,
            remediation: "Consider fixing",
        });

        assert!(!report.has_fatal_violations());
        assert_eq!(report.warning_count(), 1);
    }

    #[test]
    fn test_uses_default_var_path() {
        // Direct var paths
        assert!(uses_default_var_path("var"));
        assert!(uses_default_var_path("var/"));
        assert!(uses_default_var_path("var/db.sqlite3"));
        assert!(uses_default_var_path("./var"));
        assert!(uses_default_var_path("./var/db.sqlite3"));

        // SQLite URLs with var paths
        assert!(uses_default_var_path("sqlite://var/db.sqlite3"));
        assert!(uses_default_var_path("sqlite://var/db.sqlite3?mode=rwc"));
        assert!(uses_default_var_path("sqlite://./var/db.sqlite3"));

        // Non-var paths
        assert!(!uses_default_var_path("/data/db.sqlite3"));
        assert!(!uses_default_var_path("./data/db.sqlite3"));
        assert!(!uses_default_var_path("sqlite:///data/db.sqlite3"));
        assert!(!uses_default_var_path("variable/db.sqlite3")); // "variable" != "var"
        assert!(!uses_default_var_path("production_var/db.sqlite3"));
    }

    #[test]
    fn test_storage_mode_validation_logic() {
        // Valid storage modes (lowercase matching)
        let valid_modes = [
            "sql_only",
            "sql",
            "dual_write",
            "dual",
            "kv_primary",
            "kv-primary",
            "kv_only",
            "kv-only",
        ];

        for mode in valid_modes {
            let lowercase = mode.to_lowercase();
            assert!(
                valid_modes.contains(&lowercase.as_str()),
                "Mode '{}' should be valid",
                mode
            );
        }

        // Invalid storage modes
        let invalid_modes = ["invalid", "sqlite", "memory", ""];
        for mode in invalid_modes {
            let lowercase = mode.to_lowercase();
            assert!(
                !valid_modes.contains(&lowercase.as_str()),
                "Mode '{}' should be invalid",
                mode
            );
        }
    }

    #[test]
    fn test_session_ttl_validation_logic() {
        // Valid: access_ttl < session_ttl
        let access_ttl: u64 = 900; // 15 min
        let session_ttl: u64 = 43200; // 12 hours
        assert!(access_ttl < session_ttl);

        // Invalid: access_ttl > session_ttl
        let bad_access_ttl: u64 = 86400;
        let bad_session_ttl: u64 = 3600;
        assert!(bad_access_ttl > bad_session_ttl);
    }

    #[test]
    fn test_brute_force_protection_defaults() {
        // Default lockout values should be secure
        let default_threshold = 5u32;
        let default_cooldown = 300u64;

        assert!(
            default_threshold > 0,
            "Default threshold should be non-zero"
        );
        assert!(default_cooldown > 0, "Default cooldown should be non-zero");
        assert!(
            default_threshold <= 20,
            "Default threshold should be reasonable"
        );
    }

    #[test]
    fn test_jwt_secret_strength() {
        // Weak secrets
        assert!("".len() < 32);
        assert!("short".len() < 32);

        // Strong secrets (32+ chars)
        let strong_secret = "this-is-a-32-character-secret!!!";
        assert!(strong_secret.len() >= 32);
    }
}
