//! Boot-time invariant validation for AdapterOS.
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
//! Boot-time counters are stored in atomics and flushed to the metrics
//! exporter after it initializes (Phase 9c).

use adapteros_core::AosError;
use adapteros_db::adapters::AtomicDualWriteConfig;
use adapteros_server_api::config::{is_production, Config};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

// ============================================================================
// Boot-time metrics (captured before MetricsExporter is available)
// ============================================================================

/// Counter for invariant checks performed at boot
static BOOT_INVARIANTS_CHECKED: AtomicU64 = AtomicU64::new(0);
/// Counter for invariant violations detected at boot
static BOOT_INVARIANTS_VIOLATED: AtomicU64 = AtomicU64::new(0);
/// Counter for fatal violations that blocked boot
static BOOT_INVARIANTS_FATAL: AtomicU64 = AtomicU64::new(0);
/// Counter for invariant checks skipped via config escape hatch
static BOOT_INVARIANTS_SKIPPED: AtomicU64 = AtomicU64::new(0);

/// Snapshot of boot-time invariant metrics for flushing to MetricsExporter
#[derive(Debug, Clone, Copy)]
pub struct BootInvariantMetrics {
    pub checked: u64,
    pub violated: u64,
    pub fatal: u64,
    pub skipped: u64,
}

/// Get current boot invariant metrics snapshot
pub fn boot_invariant_metrics() -> BootInvariantMetrics {
    BootInvariantMetrics {
        checked: BOOT_INVARIANTS_CHECKED.load(Ordering::Relaxed),
        violated: BOOT_INVARIANTS_VIOLATED.load(Ordering::Relaxed),
        fatal: BOOT_INVARIANTS_FATAL.load(Ordering::Relaxed),
        skipped: BOOT_INVARIANTS_SKIPPED.load(Ordering::Relaxed),
    }
}

fn record_check() {
    BOOT_INVARIANTS_CHECKED.fetch_add(1, Ordering::Relaxed);
}

fn record_violation(fatal: bool) {
    BOOT_INVARIANTS_VIOLATED.fetch_add(1, Ordering::Relaxed);
    if fatal {
        BOOT_INVARIANTS_FATAL.fetch_add(1, Ordering::Relaxed);
    }
}

fn record_skipped() {
    BOOT_INVARIANTS_SKIPPED.fetch_add(1, Ordering::Relaxed);
}

/// Category of an invariant for grouping and reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvariantCategory {
    /// Authentication invariants (JWT, HMAC, session)
    Authentication,
    /// Authorization invariants (RBAC, roles)
    Authorization,
    /// Cryptographic invariants (keys, entropy)
    Cryptographic,
    /// Database invariants (migrations, triggers, indexes)
    Database,
    /// Federation invariants (quorum keys, peer certs)
    Federation,
    /// Adapter invariants (bundle signatures, manifest hashes)
    Adapters,
    /// Policy invariants (enforcement mode, default packs)
    Policy,
    /// Security invariants (dev bypass, cookie settings)
    Security,
    /// Configuration invariants (path validation, TTL hierarchy)
    Configuration,
    /// Memory invariants (headroom, allocation)
    Memory,
    /// Lifecycle invariants (boot ordering, executor init)
    Lifecycle,
    /// System invariants (lock poisoning, critical errors)
    System,
    /// Code hygiene invariants (credentials, uncommitted changes, panic density)
    Hygiene,
}

impl InvariantCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Authentication => "Authentication",
            Self::Authorization => "Authorization",
            Self::Cryptographic => "Cryptographic",
            Self::Database => "Database",
            Self::Federation => "Federation",
            Self::Adapters => "Adapters",
            Self::Policy => "Policy",
            Self::Security => "Security",
            Self::Configuration => "Configuration",
            Self::Memory => "Memory",
            Self::Lifecycle => "Lifecycle",
            Self::System => "System",
            Self::Hygiene => "Hygiene",
        }
    }
}

/// Severity level of an invariant violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Abort boot immediately - system cannot run safely
    Fatal,
    /// Log error and continue with degraded mode
    Error,
    /// Log warning only - advisory
    Warning,
}

/// Result of an invariant check.
#[derive(Debug, Clone)]
pub struct InvariantViolation {
    /// Unique identifier for this invariant (e.g., "AUTH-001")
    pub id: &'static str,
    /// Category for grouping and reporting
    pub category: InvariantCategory,
    /// Human-readable description of what was violated
    pub message: String,
    /// Severity level determining boot behavior
    pub severity: Severity,
    /// Suggested remediation
    pub remediation: &'static str,
}

impl InvariantViolation {
    /// Returns true if this violation should block startup in production
    pub fn is_fatal(&self) -> bool {
        matches!(self.severity, Severity::Fatal)
    }
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
        record_violation(violation.is_fatal());
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
        self.violations.iter().any(|v| v.is_fatal())
    }

    pub fn fatal_count(&self) -> usize {
        self.violations.iter().filter(|v| v.is_fatal()).count()
    }

    pub fn warning_count(&self) -> usize {
        self.violations.iter().filter(|v| !v.is_fatal()).count()
    }

    /// Get violations by category
    pub fn violations_by_category(&self, category: InvariantCategory) -> Vec<&InvariantViolation> {
        self.violations
            .iter()
            .filter(|v| v.category == category)
            .collect()
    }

    /// Get a summary of violations by category
    pub fn category_summary(&self) -> Vec<(InvariantCategory, usize, usize)> {
        use InvariantCategory::*;
        let categories = [
            Authentication,
            Authorization,
            Cryptographic,
            Database,
            Federation,
            Adapters,
            Policy,
            Security,
            Configuration,
            Memory,
            Lifecycle,
            System,
            Hygiene,
        ];
        categories
            .iter()
            .filter_map(|&cat| {
                let violations: Vec<_> = self.violations_by_category(cat);
                if violations.is_empty() {
                    None
                } else {
                    let fatal = violations.iter().filter(|v| v.is_fatal()).count();
                    Some((cat, violations.len(), fatal))
                }
            })
            .collect()
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
/// # Checked Invariants (29 total)
///
/// ## Security Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `SEC-001` | Dev auth bypass must not be active in production | Yes |
/// | `SEC-002` | Dual-write strict mode required in production | Yes |
/// | `SEC-003` | Executor must have manifest-derived seed in production | Yes |
/// | `SEC-004` | Hardware attestation fallback warning | No (warning) |
/// | `SEC-005` | Cookie security settings in production | Yes |
/// | `SEC-006` | JWT algorithm configuration in production | Yes |
/// | `SEC-007` | Tenant isolation configuration | Yes |
/// | `SEC-008` | RBAC permission configuration | Yes |
/// | `SEC-014` | Brute force protection configuration | Yes |
/// | `SEC-015` | Signature bypass env var must not be set in production | Yes |
///
/// ## Authentication Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `AUTH-001` | JWT signing key configured | Yes |
/// | `AUTH-002` | HMAC secret is not default value | Yes |
/// | `AUTH-003` | Session store initialized | Warning |
/// | `AUTH-004` | JWT secret must not be placeholder | Yes |
///
/// ## Authorization Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `AUTHZ-001` | RBAC tables populated | Yes |
/// | `AUTHZ-002` | Default admin role defined | Yes |
///
/// ## Cryptographic Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `CRYPTO-001` | Worker keypair exists (if worker mode) | Yes |
/// | `CRYPTO-002` | Entropy source available | Yes |
/// | `CRYPTO-003` | Signing algorithm matches config | Warning |
///
/// ## Configuration Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `CFG-001` | Reject default var/ paths when AOS_VAR_DIR is set | Yes |
/// | `CFG-002` | Session TTL hierarchy validation | Warning |
///
/// ## Database Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `DAT-005` | Storage mode enum validation | Warning |
///
/// ## Memory Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `MEM-003` | Memory headroom configuration | Warning |
///
/// ## Lifecycle Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `LIF-001` | Boot phase ordering | Warning |
/// | `LIF-002` | Global executor initialization | Warning |
/// | `LIF-004` | Connection pool drain configuration | Warning |
///
/// ## Federation Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `FED-001` | Quorum keys loaded (if federated mode) | Yes |
/// | `FED-002` | Peer certificates valid (if federated mode) | Yes |
///
/// ## Adapter Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `ADAPT-001` | Bundle signature verification enabled | Yes |
/// | `ADAPT-002` | Manifest hash verification enabled | Yes |
///
/// ## Policy Invariants
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `POL-001` | Default policy pack loaded | Yes |
/// | `POL-002` | Enforcement mode set | Yes |
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
                category: InvariantCategory::System,
                message: format!("Config lock poisoned: {}", e),
                severity: Severity::Fatal,
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
                category: InvariantCategory::Security,
                message: "AOS_DEV_NO_AUTH is set but production mode is enabled".to_string(),
                severity: Severity::Fatal,
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
                category: InvariantCategory::Database,
                message: "Atomic dual-write strict mode is DISABLED in production".to_string(),
                severity: Severity::Fatal,
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
            category: InvariantCategory::Cryptographic,
            message: "Deterministic executor initialized with default seed (no valid manifest)"
                .to_string(),
            severity: Severity::Fatal,
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
                category: InvariantCategory::Security,
                message: "SameSite=None requires Secure flag in production".to_string(),
                severity: Severity::Fatal,
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
        // Canonical form is "var" (not "./var"). Accept both for backwards compatibility.
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
                    category: InvariantCategory::Configuration,
                    message: format!(
                        "Default var/ paths still configured while AOS_VAR_DIR is set: {}",
                        offenders.join(", ")
                    ),
                    severity: Severity::Fatal,
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
                category: InvariantCategory::Cryptographic,
                message: "AOS_DEV_SIGNATURE_BYPASS is set but production mode is enabled"
                    .to_string(),
                severity: Severity::Fatal,
                remediation:
                    "Remove AOS_DEV_SIGNATURE_BYPASS environment variable or disable production_mode",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-006: JWT algorithm configuration in production
    // =========================================================================
    // Enforced: auth.rs JWT verification path
    // Violation: Using HS256 (symmetric) in production when EdDSA is available
    if invariants_config.disable_sec_006_jwt_verify {
        report.record_skip("SEC-006");
    } else {
        let jwt_mode = cfg.security.jwt_mode.as_deref().unwrap_or("hs256");
        let prod_algo = cfg.auth.prod_algo.to_lowercase();

        if production && jwt_mode == "hs256" && prod_algo == "eddsa" {
            // Production config specifies EdDSA but runtime is using HS256
            report.record_violation(InvariantViolation {
                id: "SEC-006",
                category: InvariantCategory::Authentication,
                message: "JWT mode is HS256 but auth.prod_algo specifies EdDSA".to_string(),
                severity: Severity::Fatal,
                remediation: "Set security.jwt_mode = 'eddsa' or configure key_file_path",
            });
        } else if production && cfg.security.jwt_secret.len() < 32 {
            report.record_violation(InvariantViolation {
                id: "SEC-006",
                category: InvariantCategory::Authentication,
                message: "JWT secret too short (minimum 32 bytes required)".to_string(),
                severity: Severity::Fatal,
                remediation: "Generate a secure JWT secret: openssl rand -base64 32",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-007: Tenant isolation configuration
    // =========================================================================
    // Enforced: Per-handler tenant_id extraction and validation
    // Violation: Multi-tenancy enabled without proper isolation config
    if invariants_config.disable_sec_007_tenant_isolation {
        report.record_skip("SEC-007");
    } else {
        // In production, ensure tenant isolation is not disabled by dev flags
        let dev_bypass_active = std::env::var("AOS_DEV_DISABLE_TENANT_CHECK")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if production && dev_bypass_active {
            report.record_violation(InvariantViolation {
                id: "SEC-007",
                category: InvariantCategory::Authorization,
                message: "Tenant isolation check disabled in production".to_string(),
                severity: Severity::Fatal,
                remediation: "Remove AOS_DEV_DISABLE_TENANT_CHECK environment variable",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-008: RBAC permission configuration
    // =========================================================================
    // Enforced: permissions.rs role-based access control
    // Violation: RBAC misconfigured (e.g., default allow-all)
    if invariants_config.disable_sec_008_rbac_config {
        report.record_skip("SEC-008");
    } else {
        // Check that RBAC bypass is not enabled in production
        let rbac_bypass = std::env::var("AOS_DEV_RBAC_BYPASS")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if production && rbac_bypass {
            report.record_violation(InvariantViolation {
                id: "SEC-008",
                category: InvariantCategory::Authorization,
                message: "RBAC bypass is enabled in production".to_string(),
                severity: Severity::Fatal,
                remediation: "Remove AOS_DEV_RBAC_BYPASS environment variable",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // SEC-014: Brute force protection configuration
    // =========================================================================
    // Enforced: security/mod.rs rate limiting and lockout
    // Violation: Lockout disabled or threshold too high
    if invariants_config.disable_sec_014_brute_force {
        report.record_skip("SEC-014");
    } else {
        let lockout_threshold = cfg.auth.lockout_threshold;
        let lockout_cooldown = cfg.auth.lockout_cooldown;

        if production && lockout_threshold == 0 {
            report.record_violation(InvariantViolation {
                id: "SEC-014",
                category: InvariantCategory::Security,
                message: "Brute force protection disabled (lockout_threshold = 0)".to_string(),
                severity: Severity::Fatal,
                remediation: "Set auth.lockout_threshold to a positive value (recommended: 5)",
            });
        } else if production && lockout_threshold > 20 {
            // Warning: very permissive threshold
            warn!(
                invariant = "SEC-014",
                threshold = lockout_threshold,
                "Lockout threshold is very high; consider lowering for better security"
            );
            report.record_pass();
        } else if production && lockout_cooldown < 60 {
            // Warning: cooldown too short
            warn!(
                invariant = "SEC-014",
                cooldown_secs = lockout_cooldown,
                "Lockout cooldown is very short; consider increasing for better security"
            );
            report.record_pass();
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // CFG-002: Session TTL hierarchy validation
    // =========================================================================
    // Violation: access_token_ttl >= session_ttl (tokens should be shorter-lived)
    if invariants_config.disable_cfg_002_session_ttl {
        report.record_skip("CFG-002");
    } else {
        let access_ttl = cfg.security.access_token_ttl_seconds;
        let session_ttl = cfg.security.session_ttl_seconds;

        if access_ttl >= session_ttl {
            report.record_violation(InvariantViolation {
                id: "CFG-002",
                category: InvariantCategory::Configuration,
                message: format!(
                    "Access token TTL ({} s) should be shorter than session TTL ({} s)",
                    access_ttl, session_ttl
                ),
                severity: Severity::Warning, // Warning only - doesn't break functionality
                remediation: "Set access_token_ttl_seconds < session_ttl_seconds",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // DAT-005: Storage mode enum validation
    // =========================================================================
    // Violation: Invalid storage mode value in config
    if invariants_config.disable_dat_005_storage_mode {
        report.record_skip("DAT-005");
    } else {
        // Check database path uses valid SQLite prefix or is a valid path
        let db_path = &cfg.db.path;
        let is_valid_db_path = db_path.starts_with("sqlite://")
            || db_path.ends_with(".sqlite3")
            || db_path.ends_with(".db")
            || db_path == ":memory:";

        if !is_valid_db_path && !std::path::Path::new(db_path).exists() {
            // Only warn if path doesn't look like SQLite and doesn't exist
            warn!(
                invariant = "DAT-005",
                path = db_path,
                "Database path may be invalid; ensure it's a valid SQLite path"
            );
        }
        report.record_pass();
    }

    // =========================================================================
    // MEM-003: Memory headroom configuration (advisory)
    // =========================================================================
    // Enforced: unified_tracker.rs memory pressure handling
    // Violation: Insufficient memory headroom configured
    if invariants_config.disable_mem_003_memory_headroom {
        report.record_skip("MEM-003");
    } else {
        // Check environment for memory limits
        let memory_headroom_mb: u64 = std::env::var("AOS_MEMORY_HEADROOM_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(512); // Default 512MB headroom

        if production && memory_headroom_mb < 256 {
            warn!(
                invariant = "MEM-003",
                headroom_mb = memory_headroom_mb,
                "Memory headroom is low; consider increasing AOS_MEMORY_HEADROOM_MB"
            );
        }
        report.record_pass(); // Advisory only
    }

    // =========================================================================
    // LIF-001: Boot phase ordering (advisory)
    // =========================================================================
    // Enforced: Boot sequence in boot/mod.rs
    // This is validated by the boot sequence itself; just log for audit
    if invariants_config.disable_lif_001_boot_ordering {
        report.record_skip("LIF-001");
    } else {
        info!(
            invariant = "LIF-001",
            "Boot phase ordering validated by boot sequence (invariant check at correct phase)"
        );
        report.record_pass();
    }

    // =========================================================================
    // LIF-002: Global executor initialization
    // =========================================================================
    // Note: This overlaps with SEC-003 (executor manifest seed)
    // Here we check that deterministic executor config is present
    if invariants_config.disable_lif_002_executor_init {
        report.record_skip("LIF-002");
    } else {
        // The executor initialization is validated by whether manifest hash is present
        // This check confirms the invariant was intended to be checked
        if production && !executor_manifest_hash_present {
            // SEC-003 already handles this case with a fatal violation
            // LIF-002 just notes the lifecycle implication
            info!(
                invariant = "LIF-002",
                "Executor initialization without manifest (see SEC-003 for details)"
            );
        }
        report.record_pass();
    }

    // =========================================================================
    // LIF-004: Connection pool drain configuration
    // =========================================================================
    // Enforced: boot/database.rs pool configuration
    // Violation: Pool drain timeout misconfigured for graceful shutdown
    if invariants_config.disable_lif_004_pool_drain {
        report.record_skip("LIF-004");
    } else {
        // Check for pool configuration via environment
        let pool_max_connections: u32 = std::env::var("AOS_DB_POOL_MAX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let pool_acquire_timeout: u64 = std::env::var("AOS_DB_ACQUIRE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        if pool_max_connections < 2 {
            warn!(
                invariant = "LIF-004",
                max_connections = pool_max_connections,
                "Database pool size is very small; may cause connection starvation"
            );
        }

        if production && pool_acquire_timeout < 5 {
            warn!(
                invariant = "LIF-004",
                timeout_secs = pool_acquire_timeout,
                "Pool acquire timeout is very short; may cause spurious failures under load"
            );
        }

        report.record_pass();
    }

    // =========================================================================
    // AUTH-001: JWT signing key configured
    // =========================================================================
    // Authentication invariant: Ensure JWT signing key is present and valid
    if invariants_config.disable_auth_001_jwt_key {
        report.record_skip("AUTH-001");
    } else {
        let jwt_secret = &cfg.security.jwt_secret;
        let key_file = cfg.security.key_file_path.as_deref();

        if production && jwt_secret.is_empty() && key_file.is_none() {
            report.record_violation(InvariantViolation {
                id: "AUTH-001",
                category: InvariantCategory::Authentication,
                message: "No JWT signing key configured (neither secret nor key file)".to_string(),
                severity: Severity::Fatal,
                remediation: "Set security.jwt_secret or security.key_file_path in config",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // AUTH-002: HMAC secret is not default value
    // =========================================================================
    // Authentication invariant: Ensure HMAC secret is unique/non-default
    if invariants_config.disable_auth_002_hmac_secret {
        report.record_skip("AUTH-002");
    } else {
        let jwt_secret = &cfg.security.jwt_secret;
        let default_secrets = ["changeme", "secret", "default", "password", "jwt_secret"];

        if production && default_secrets.iter().any(|d| jwt_secret == *d) {
            report.record_violation(InvariantViolation {
                id: "AUTH-002",
                category: InvariantCategory::Authentication,
                message: "JWT secret appears to be a default/placeholder value".to_string(),
                severity: Severity::Fatal,
                remediation: "Set a unique JWT secret: openssl rand -base64 32",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // AUTH-003: Session store initialized
    // =========================================================================
    // Authentication invariant: Session configuration is present
    if invariants_config.disable_auth_003_session_store {
        report.record_skip("AUTH-003");
    } else {
        // Validate session TTL is reasonable
        let session_ttl = cfg.security.session_ttl_seconds;

        if production && session_ttl < 300 {
            // Less than 5 minutes is too short
            report.record_violation(InvariantViolation {
                id: "AUTH-003",
                category: InvariantCategory::Authentication,
                message: format!(
                    "Session TTL is too short ({} seconds); minimum recommended is 300",
                    session_ttl
                ),
                severity: Severity::Warning,
                remediation: "Set security.session_ttl_seconds to at least 300",
            });
        } else if production && session_ttl > 86400 * 7 {
            // More than 7 days is too long
            warn!(
                invariant = "AUTH-003",
                session_ttl_seconds = session_ttl,
                "Session TTL is very long; consider reducing for better security"
            );
            report.record_pass();
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // AUTH-004: JWT secret must not be placeholder value
    // =========================================================================
    // Authentication invariant: JWT secret must not contain placeholder patterns
    // or have insufficient entropy (e.g., all same character, repetitive patterns).
    // This is a critical security invariant - FATAL in production.
    if invariants_config.disable_auth_004_jwt_secret_placeholder {
        report.record_skip("AUTH-004");
    } else {
        let jwt_secret = &cfg.security.jwt_secret;

        // Known placeholder patterns that must never be used in production
        const PLACEHOLDER_PATTERNS: &[&str] = &[
            "CHANGE_ME",
            "TODO",
            "PLACEHOLDER",
            "XXXXXXXX",
            "12345678",
            "secret",
            "password",
            "default",
            "example",
            "insecure",
            "changeme",
            "replace",
            "fixme",
            "your_secret",
            "your-secret",
        ];

        let secret_lower = jwt_secret.to_lowercase();
        let mut violation_reason: Option<String> = None;

        // Check 1: Empty secret
        if jwt_secret.is_empty() {
            violation_reason = Some("JWT secret is empty".to_string());
        }

        // Check 2: Too short (minimum 64 chars for production per effective.rs)
        if violation_reason.is_none() && production && jwt_secret.len() < 64 {
            violation_reason = Some(format!(
                "JWT secret is too short ({} chars, minimum 64 required in production)",
                jwt_secret.len()
            ));
        }

        // Check 3: Contains placeholder patterns
        if violation_reason.is_none() {
            for pattern in PLACEHOLDER_PATTERNS {
                if secret_lower.contains(&pattern.to_lowercase()) {
                    violation_reason = Some(format!(
                        "JWT secret contains placeholder pattern '{}'",
                        pattern
                    ));
                    break;
                }
            }
        }

        // Check 4: Low entropy - all same character
        if violation_reason.is_none() && jwt_secret.len() >= 32 {
            let first_char = jwt_secret.chars().next().unwrap();
            if jwt_secret.chars().all(|c| c == first_char) {
                violation_reason = Some(format!(
                    "JWT secret has no entropy (all character '{}')",
                    first_char
                ));
            }
        }

        // Check 5: Simple repetitive pattern (e.g., "abababab...")
        if violation_reason.is_none() && jwt_secret.len() >= 16 {
            for pattern_len in 1..=8 {
                if jwt_secret.len() >= pattern_len * 4 {
                    let pattern = &jwt_secret[..pattern_len];
                    let expected_repeats = jwt_secret.len() / pattern_len;
                    let expected_full = pattern.repeat(expected_repeats);
                    if jwt_secret.starts_with(&expected_full) {
                        violation_reason = Some(format!(
                            "JWT secret is a simple repetitive pattern ('{}')",
                            pattern
                        ));
                        break;
                    }
                }
            }
        }

        if let Some(reason) = violation_reason {
            if production {
                report.record_violation(InvariantViolation {
                    id: "AUTH-004",
                    category: InvariantCategory::Authentication,
                    message: reason,
                    severity: Severity::Fatal,
                    remediation:
                        "Set a secure random JWT secret in config or AOS_JWT_SECRET env var. \
                                  Generate with: openssl rand -base64 48",
                });
            } else {
                warn!(
                    invariant = "AUTH-004",
                    reason = %reason,
                    "JWT secret validation warning (development mode, not blocking)"
                );
                report.record_pass();
            }
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // AUTHZ-001: RBAC tables populated (advisory at boot, enforced at runtime)
    // =========================================================================
    // Authorization invariant: Check RBAC configuration exists
    if invariants_config.disable_authz_001_rbac_tables {
        report.record_skip("AUTHZ-001");
    } else {
        // This is advisory at boot; actual table checks happen in post-DB validation
        // Here we just verify RBAC is not explicitly disabled
        let rbac_enabled = std::env::var("AOS_RBAC_ENABLED")
            .ok()
            .map(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no"))
            .unwrap_or(true);

        if production && !rbac_enabled {
            report.record_violation(InvariantViolation {
                id: "AUTHZ-001",
                category: InvariantCategory::Authorization,
                message: "RBAC is explicitly disabled in production".to_string(),
                severity: Severity::Fatal,
                remediation: "Remove AOS_RBAC_ENABLED=false or set it to true",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // AUTHZ-002: Default admin role defined
    // =========================================================================
    // Authorization invariant: Ensure admin role is configured
    if invariants_config.disable_authz_002_admin_role {
        report.record_skip("AUTHZ-002");
    } else {
        // Check if admin role is explicitly disabled (which would be bad)
        let admin_disabled = std::env::var("AOS_DISABLE_ADMIN_ROLE")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if production && admin_disabled {
            report.record_violation(InvariantViolation {
                id: "AUTHZ-002",
                category: InvariantCategory::Authorization,
                message: "Admin role is explicitly disabled in production".to_string(),
                severity: Severity::Fatal,
                remediation: "Remove AOS_DISABLE_ADMIN_ROLE environment variable",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // CRYPTO-001: Worker keypair exists (if worker mode)
    // =========================================================================
    // Cryptographic invariant: Ensure signing keys are available for workers
    if invariants_config.disable_crypto_001_worker_keypair {
        report.record_skip("CRYPTO-001");
    } else {
        // Check if key_file_path is configured when EdDSA is required
        let jwt_mode = cfg.security.jwt_mode.as_deref().unwrap_or("hs256");
        let key_file = cfg.security.key_file_path.as_deref();

        if production && jwt_mode == "eddsa" && key_file.is_none() {
            report.record_violation(InvariantViolation {
                id: "CRYPTO-001",
                category: InvariantCategory::Cryptographic,
                message: "EdDSA mode requires key_file_path but none configured".to_string(),
                severity: Severity::Fatal,
                remediation: "Set security.key_file_path to Ed25519 private key path",
            });
        } else if let Some(path) = key_file {
            // Verify key file exists
            if !std::path::Path::new(path).exists() {
                report.record_violation(InvariantViolation {
                    id: "CRYPTO-001",
                    category: InvariantCategory::Cryptographic,
                    message: format!("Key file does not exist: {}", path),
                    severity: Severity::Fatal,
                    remediation: "Create key file or update security.key_file_path",
                });
            } else {
                report.record_pass();
            }
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // CRYPTO-002: Entropy source available
    // =========================================================================
    // Cryptographic invariant: System entropy source is available
    if invariants_config.disable_crypto_002_entropy_source {
        report.record_skip("CRYPTO-002");
    } else {
        // On Unix, check /dev/urandom exists
        #[cfg(unix)]
        {
            if !std::path::Path::new("/dev/urandom").exists() {
                report.record_violation(InvariantViolation {
                    id: "CRYPTO-002",
                    category: InvariantCategory::Cryptographic,
                    message: "Entropy source /dev/urandom not available".to_string(),
                    severity: Severity::Fatal,
                    remediation: "Ensure /dev/urandom is available in the container/system",
                });
            } else {
                report.record_pass();
            }
        }
        #[cfg(not(unix))]
        {
            // On non-Unix, assume OS provides entropy
            report.record_pass();
        }
    }

    // =========================================================================
    // CRYPTO-003: Signing algorithm matches config
    // =========================================================================
    // Cryptographic invariant: JWT algorithm configuration is consistent
    if invariants_config.disable_crypto_003_algo_match {
        report.record_skip("CRYPTO-003");
    } else {
        let _dev_algo = cfg.auth.dev_algo.to_lowercase();
        let prod_algo = cfg.auth.prod_algo.to_lowercase();
        let jwt_mode = cfg
            .security
            .jwt_mode
            .as_deref()
            .unwrap_or("hs256")
            .to_lowercase();

        // In production, jwt_mode should match prod_algo
        if production && jwt_mode != prod_algo && !prod_algo.is_empty() {
            warn!(
                invariant = "CRYPTO-003",
                jwt_mode = %jwt_mode,
                prod_algo = %prod_algo,
                "JWT mode doesn't match configured production algorithm"
            );
        }
        report.record_pass();
    }

    // =========================================================================
    // FED-001: Quorum keys loaded (if federated mode)
    // =========================================================================
    // Federation invariant: Quorum keys available for federated deployments
    if invariants_config.disable_fed_001_quorum_keys {
        report.record_skip("FED-001");
    } else {
        // Check if federation mode is enabled
        let federation_enabled = std::env::var("AOS_FEDERATION_ENABLED")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if federation_enabled {
            // Verify quorum configuration
            let quorum_keys_path = std::env::var("AOS_QUORUM_KEYS_PATH").ok();
            if production && quorum_keys_path.is_none() {
                report.record_violation(InvariantViolation {
                    id: "FED-001",
                    category: InvariantCategory::Federation,
                    message: "Federation enabled but AOS_QUORUM_KEYS_PATH not set".to_string(),
                    severity: Severity::Fatal,
                    remediation: "Set AOS_QUORUM_KEYS_PATH to quorum public keys directory",
                });
            } else if let Some(path) = quorum_keys_path {
                if !std::path::Path::new(&path).exists() {
                    report.record_violation(InvariantViolation {
                        id: "FED-001",
                        category: InvariantCategory::Federation,
                        message: format!("Quorum keys path does not exist: {}", path),
                        severity: Severity::Fatal,
                        remediation: "Create quorum keys directory or update AOS_QUORUM_KEYS_PATH",
                    });
                } else {
                    report.record_pass();
                }
            } else {
                report.record_pass();
            }
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // FED-002: Peer certificates valid (if federated mode)
    // =========================================================================
    // Federation invariant: Peer certificates are configured for mTLS
    if invariants_config.disable_fed_002_peer_certs {
        report.record_skip("FED-002");
    } else {
        let federation_enabled = std::env::var("AOS_FEDERATION_ENABLED")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if federation_enabled && production {
            let peer_certs_path = std::env::var("AOS_PEER_CERTS_PATH").ok();
            if peer_certs_path.is_none() && cfg.security.mtls_required {
                report.record_violation(InvariantViolation {
                    id: "FED-002",
                    category: InvariantCategory::Federation,
                    message: "mTLS required but AOS_PEER_CERTS_PATH not set".to_string(),
                    severity: Severity::Fatal,
                    remediation: "Set AOS_PEER_CERTS_PATH to peer certificates directory",
                });
            } else {
                report.record_pass();
            }
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // ADAPT-001: Bundle signature verification enabled
    // =========================================================================
    // Adapter invariant: Bundle signatures are verified in production
    if invariants_config.disable_adapt_001_bundle_sig {
        report.record_skip("ADAPT-001");
    } else {
        // Check if signature verification is disabled
        let sig_verify_disabled = std::env::var("AOS_SKIP_BUNDLE_SIGNATURE")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if production && sig_verify_disabled {
            report.record_violation(InvariantViolation {
                id: "ADAPT-001",
                category: InvariantCategory::Adapters,
                message: "Bundle signature verification is disabled in production".to_string(),
                severity: Severity::Fatal,
                remediation: "Remove AOS_SKIP_BUNDLE_SIGNATURE environment variable",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // ADAPT-002: Manifest hash verification enabled
    // =========================================================================
    // Adapter invariant: Manifest hashes are verified
    if invariants_config.disable_adapt_002_manifest_hash {
        report.record_skip("ADAPT-002");
    } else {
        let hash_verify_disabled = std::env::var("AOS_SKIP_MANIFEST_HASH")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if production && hash_verify_disabled {
            report.record_violation(InvariantViolation {
                id: "ADAPT-002",
                category: InvariantCategory::Adapters,
                message: "Manifest hash verification is disabled in production".to_string(),
                severity: Severity::Fatal,
                remediation: "Remove AOS_SKIP_MANIFEST_HASH environment variable",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // POL-001: Default policy pack loaded
    // =========================================================================
    // Policy invariant: Default policy pack is configured
    if invariants_config.disable_pol_001_default_pack {
        report.record_skip("POL-001");
    } else {
        // Check if policies are configured
        let policy_disabled = std::env::var("AOS_DISABLE_POLICIES")
            .ok()
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        if production && policy_disabled {
            report.record_violation(InvariantViolation {
                id: "POL-001",
                category: InvariantCategory::Policy,
                message: "Policy enforcement is disabled in production".to_string(),
                severity: Severity::Fatal,
                remediation: "Remove AOS_DISABLE_POLICIES environment variable",
            });
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // POL-002: Enforcement mode set
    // =========================================================================
    // Policy invariant: Policy enforcement mode is explicit
    if invariants_config.disable_pol_002_enforcement_mode {
        report.record_skip("POL-002");
    } else {
        let enforcement_mode = std::env::var("AOS_POLICY_ENFORCEMENT_MODE")
            .ok()
            .map(|v| v.trim().to_lowercase());

        if production {
            match enforcement_mode.as_deref() {
                Some("enforce") | Some("strict") => {
                    report.record_pass();
                }
                Some("audit") | Some("warn") => {
                    warn!(
                        invariant = "POL-002",
                        mode = ?enforcement_mode,
                        "Policy enforcement mode is not strict in production"
                    );
                    report.record_pass();
                }
                Some("disabled") | Some("off") => {
                    report.record_violation(InvariantViolation {
                        id: "POL-002",
                        category: InvariantCategory::Policy,
                        message: "Policy enforcement mode is disabled in production".to_string(),
                        severity: Severity::Fatal,
                        remediation: "Set AOS_POLICY_ENFORCEMENT_MODE=enforce",
                    });
                }
                None => {
                    // Default is acceptable
                    report.record_pass();
                }
                Some(other) => {
                    warn!(
                        invariant = "POL-002",
                        mode = other,
                        "Unknown policy enforcement mode"
                    );
                    report.record_pass();
                }
            }
        } else {
            report.record_pass();
        }
    }

    // =========================================================================
    // HYGIENE-001: No credentials in repo
    // =========================================================================
    // Checks that no credential files exist in the workspace root
    // Violation: Credential files (cookies.txt, *.pem, .env.local, etc.) found
    // Fails: CLOSED in production (fatal)
    if invariants_config.disable_hygiene_001_no_credentials {
        report.record_skip("HYGIENE-001");
    } else {
        let workspace_root = std::env::current_dir().unwrap_or_default();
        let credential_patterns = [
            "cookies.txt",
            ".env.local",
            ".env.production",
            "credentials.json",
            "service-account.json",
            "id_rsa",
            "id_ed25519",
        ];
        let glob_patterns = ["*.pem", "*.key", "id_rsa*", "id_ed25519*"];

        let mut found_credentials: Vec<String> = Vec::new();

        // Check exact filenames
        for pattern in &credential_patterns {
            let path = workspace_root.join(pattern);
            if path.exists() {
                found_credentials.push(pattern.to_string());
            }
        }

        // Check glob patterns in workspace root
        for pattern in &glob_patterns {
            if let Ok(entries) = glob::glob(&workspace_root.join(pattern).to_string_lossy()) {
                for entry in entries.flatten() {
                    if let Some(filename) = entry.file_name() {
                        let name = filename.to_string_lossy().to_string();
                        if !found_credentials.contains(&name) {
                            found_credentials.push(name);
                        }
                    }
                }
            }
        }

        if found_credentials.is_empty() {
            report.record_pass();
        } else {
            report.record_violation(InvariantViolation {
                id: "HYGIENE-001",
                category: InvariantCategory::Hygiene,
                message: format!(
                    "Credential files found in workspace root: [{}]",
                    found_credentials.join(", ")
                ),
                severity: if production {
                    Severity::Fatal
                } else {
                    Severity::Warning
                },
                remediation: "Remove credential files from repo root and add them to .gitignore",
            });
        }
    }

    // =========================================================================
    // HYGIENE-002: Critical handlers committed
    // =========================================================================
    // Checks that auth/security handlers don't have uncommitted changes
    // Violation: Modified files in handlers/ directory (especially auth)
    // Fails: OPEN (warning only) - advisory for dev hygiene
    if invariants_config.disable_hygiene_002_handlers_committed {
        report.record_skip("HYGIENE-002");
    } else {
        // Run git diff --name-only to check for uncommitted changes
        let git_result = std::process::Command::new("git")
            .args(["diff", "--name-only"])
            .output();

        match git_result {
            Ok(output) if output.status.success() => {
                let changed_files = String::from_utf8_lossy(&output.stdout);
                let critical_patterns = [
                    "handlers/auth",
                    "handlers/auth_enhanced",
                    "security/",
                    "middleware/auth",
                    "middleware/security",
                ];

                let critical_changes: Vec<&str> = changed_files
                    .lines()
                    .filter(|line| {
                        critical_patterns
                            .iter()
                            .any(|pattern| line.contains(pattern))
                    })
                    .collect();

                if critical_changes.is_empty() {
                    report.record_pass();
                } else {
                    report.record_violation(InvariantViolation {
                        id: "HYGIENE-002",
                        category: InvariantCategory::Hygiene,
                        message: format!(
                            "Uncommitted changes in critical handlers: [{}]",
                            critical_changes.join(", ")
                        ),
                        severity: Severity::Warning,
                        remediation:
                            "Commit or stash changes to auth/security handlers before deployment",
                    });
                }
            }
            Ok(_) => {
                // git command failed (non-zero exit), might not be a git repo
                warn!(
                    invariant = "HYGIENE-002",
                    "Could not check git status (not a git repo or git error)"
                );
                report.record_pass();
            }
            Err(e) => {
                // git not found or execution error
                warn!(
                    invariant = "HYGIENE-002",
                    error = %e,
                    "Could not execute git command"
                );
                report.record_pass();
            }
        }
    }

    // =========================================================================
    // HYGIENE-003: Panic density check
    // =========================================================================
    // Checks for excessive unwrap()/expect() calls in critical paths
    // Violation: Panic density exceeds threshold (20 per 1000 LOC)
    // Fails: OPEN (warning only) - advisory for code quality
    if invariants_config.disable_hygiene_003_panic_density {
        report.record_skip("HYGIENE-003");
    } else {
        let critical_paths = [
            "crates/adapteros-server-api/src/",
            "crates/adapteros-lora-worker/src/",
        ];

        let workspace_root = std::env::current_dir().unwrap_or_default();
        let mut total_loc: usize = 0;
        let mut total_panics: usize = 0;
        let mut high_density_files: Vec<(String, usize, usize)> = Vec::new();

        for critical_path in &critical_paths {
            let full_path = workspace_root.join(critical_path);
            if !full_path.exists() {
                continue;
            }

            // Walk the directory and count panics
            if let Ok(entries) = glob::glob(&format!("{}**/*.rs", full_path.to_string_lossy())) {
                for entry in entries.flatten() {
                    if let Ok(content) = std::fs::read_to_string(&entry) {
                        let lines: Vec<&str> = content.lines().collect();
                        let loc = lines.len();
                        let panic_count = lines
                            .iter()
                            .filter(|line| {
                                let trimmed = line.trim();
                                // Skip comments
                                if trimmed.starts_with("//") {
                                    return false;
                                }
                                // Count unwrap() and expect( patterns
                                trimmed.contains(".unwrap()")
                                    || trimmed.contains(".expect(")
                                    || trimmed.contains("panic!(")
                            })
                            .count();

                        total_loc += loc;
                        total_panics += panic_count;

                        // Track files with high panic density (> 30 per 1000 LOC)
                        if loc > 100 && panic_count > 0 {
                            let density = (panic_count * 1000) / loc;
                            if density > 30 {
                                if let Some(filename) = entry.file_name() {
                                    high_density_files.push((
                                        filename.to_string_lossy().to_string(),
                                        panic_count,
                                        loc,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Calculate overall density (panics per 1000 LOC)
        let overall_density = if total_loc > 0 {
            (total_panics * 1000) / total_loc
        } else {
            0
        };

        const PANIC_THRESHOLD: usize = 20; // panics per 1000 LOC

        if overall_density <= PANIC_THRESHOLD && high_density_files.is_empty() {
            info!(
                invariant = "HYGIENE-003",
                total_loc = total_loc,
                total_panics = total_panics,
                density_per_1000 = overall_density,
                "Panic density within acceptable limits"
            );
            report.record_pass();
        } else if overall_density > PANIC_THRESHOLD {
            report.record_violation(InvariantViolation {
                id: "HYGIENE-003",
                category: InvariantCategory::Hygiene,
                message: format!(
                    "Panic density too high: {} per 1000 LOC (threshold: {}). Total: {} unwrap/expect/panic in {} LOC",
                    overall_density, PANIC_THRESHOLD, total_panics, total_loc
                ),
                severity: Severity::Warning,
                remediation: "Reduce unwrap()/expect() usage in critical paths; use proper error handling",
            });
        } else {
            // High density files but overall acceptable
            let file_list: Vec<String> = high_density_files
                .iter()
                .take(5) // Limit to top 5
                .map(|(f, p, l)| format!("{} ({}/{})", f, p, l))
                .collect();
            warn!(
                invariant = "HYGIENE-003",
                high_density_files = ?file_list,
                overall_density = overall_density,
                "Some files have high panic density"
            );
            report.record_pass();
        }
    }

    // =========================================================================
    // Remaining invariants documented but NOT checked at boot time
    // =========================================================================
    // The following are enforced at runtime or are implicit in the code:
    //
    // SECURITY (runtime enforcement):
    // - SEC-009: Token revocation baseline - checked during token validation
    // - SEC-010: Hardware attestation - checked during attestation verification
    // - SEC-011: Quorum signature verification - checked during federation ops
    // - SEC-012: Adapter bundle signature - checked during bundle loading
    // - SEC-013: Password timing-safety - always enforced in auth code
    //
    // DATA INTEGRITY (runtime enforcement):
    // - DAT-003: AOS file hash match - checked during adapter loading
    // - DAT-004: KV presence for readiness - checked in readiness probe
    //
    // MEMORY MANAGEMENT (runtime enforcement):
    // - MEM-001: KV cache generation coherence - maintained during inference
    // - MEM-002: GPU buffer fingerprint - validated during buffer allocation
    // - MEM-004: KV slab non-overlapping - enforced by allocator
    //
    // CONCURRENCY (runtime enforcement):
    // - CON-001: Hot-swap atomic pointer - enforced by AtomicPtr usage
    // - CON-002: KV quota transactional - enforced by transaction boundaries
    // - CON-003: Model cache pinning - enforced by pin/unpin API
    // - CON-004: Request pin refcount - enforced by Arc<> semantics
    //
    // LIFECYCLE (runtime enforcement):
    // - LIF-003: Adapter lifecycle CAS - enforced by state machine CAS

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
        if violation.is_fatal() {
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
            .filter(|v| v.is_fatal())
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

/// Validate invariants that require a live database connection.
///
/// # Checked Invariants
///
/// | ID | Description | Fatal in Prod |
/// |----|-------------|---------------|
/// | `DAT-001` | Archive state machine triggers exist | No (warning) |
/// | `DAT-002` | Foreign key constraints enabled | Yes |
/// | `DAT-006` | Migration table exists and has entries | Yes |
/// | `DAT-007` | Audit chain table initialized | No (warning) |
pub async fn validate_post_db_invariants(
    config: &Arc<RwLock<Config>>,
    pool: &sqlx::SqlitePool,
) -> InvariantReport {
    let mut report = InvariantReport::new();

    let (production, invariants_config) = match config.read() {
        Ok(cfg) => (is_production(&cfg), cfg.invariants.clone()),
        Err(e) => {
            report.record_violation(InvariantViolation {
                id: "SYS-002",
                category: InvariantCategory::System,
                message: format!("Config lock poisoned during post-DB validation: {}", e),
                severity: Severity::Fatal,
                remediation: "Restart the server; config lock should not be poisoned",
            });
            return report;
        }
    };

    // =========================================================================
    // DAT-002: Foreign key constraints enabled
    // =========================================================================
    if invariants_config.disable_dat_002_foreign_keys {
        report.record_skip("DAT-002");
    } else {
        match sqlx::query_scalar::<_, i32>("PRAGMA foreign_keys")
            .fetch_one(pool)
            .await
        {
            Ok(fk_enabled) => {
                if fk_enabled != 1 {
                    report.record_violation(InvariantViolation {
                        id: "DAT-002",
                        category: InvariantCategory::Database,
                        message: "Foreign key constraints are DISABLED".to_string(),
                        severity: if production {
                            Severity::Fatal
                        } else {
                            Severity::Warning
                        },
                        remediation: "Ensure PRAGMA foreign_keys = ON is set at connection time",
                    });
                } else {
                    report.record_pass();
                }
            }
            Err(e) => {
                warn!(
                    invariant = "DAT-002",
                    error = %e,
                    "Failed to check foreign_keys pragma"
                );
                report.record_pass(); // Don't fail on query error
            }
        }
    }

    // =========================================================================
    // DAT-006: Migration table exists and has entries
    // =========================================================================
    if invariants_config.disable_dat_006_migration_order {
        report.record_skip("DAT-006");
    } else {
        match sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
        )
        .fetch_one(pool)
        .await
        {
            Ok(table_exists) => {
                if table_exists == 0 {
                    report.record_violation(InvariantViolation {
                        id: "DAT-006",
                        category: InvariantCategory::Database,
                        message: "Migration table _sqlx_migrations does not exist".to_string(),
                        severity: if production {
                            Severity::Fatal
                        } else {
                            Severity::Warning
                        },
                        remediation: "Run database migrations: ./aosctl db migrate",
                    });
                } else {
                    // Check migration count
                    match sqlx::query_scalar::<_, i32>(
                        "SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1",
                    )
                    .fetch_one(pool)
                    .await
                    {
                        Ok(count) => {
                            if count == 0 {
                                report.record_violation(InvariantViolation {
                                    id: "DAT-006",
                                    category: InvariantCategory::Database,
                                    message: "No successful migrations recorded".to_string(),
                                    severity: if production {
                                        Severity::Fatal
                                    } else {
                                        Severity::Warning
                                    },
                                    remediation: "Run database migrations: ./aosctl db migrate",
                                });
                            } else {
                                info!(
                                    invariant = "DAT-006",
                                    migrations = count,
                                    "Migration table validated"
                                );
                                report.record_pass();
                            }
                        }
                        Err(e) => {
                            warn!(
                                invariant = "DAT-006",
                                error = %e,
                                "Failed to count migrations"
                            );
                            report.record_pass();
                        }
                    }
                }
            }
            Err(e) => {
                report.record_violation(InvariantViolation {
                    id: "DAT-006",
                    category: InvariantCategory::Database,
                    message: format!("Failed to check migration table: {}", e),
                    severity: if production {
                        Severity::Fatal
                    } else {
                        Severity::Warning
                    },
                    remediation: "Ensure database is accessible and migrations have run",
                });
            }
        }
    }

    // =========================================================================
    // DAT-001: Archive state machine triggers exist (advisory)
    // =========================================================================
    if invariants_config.disable_dat_001_archive_triggers {
        report.record_skip("DAT-001");
    } else {
        // Check for archive-related triggers (from migrations/0138_adapter_archive_gc.sql)
        match sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name LIKE '%archive%'",
        )
        .fetch_one(pool)
        .await
        {
            Ok(trigger_count) => {
                if trigger_count == 0 {
                    // Advisory only - triggers may not be required in all deployments
                    warn!(
                        invariant = "DAT-001",
                        "No archive triggers found; archive state machine may not be enforced"
                    );
                }
                report.record_pass();
            }
            Err(e) => {
                warn!(
                    invariant = "DAT-001",
                    error = %e,
                    "Failed to check archive triggers"
                );
                report.record_pass();
            }
        }
    }

    // =========================================================================
    // DAT-007: Audit chain table initialized (advisory)
    // =========================================================================
    if invariants_config.disable_dat_007_audit_chain {
        report.record_skip("DAT-007");
    } else {
        // Check if audit_events table exists
        match sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='audit_events'",
        )
        .fetch_one(pool)
        .await
        {
            Ok(table_exists) => {
                if table_exists == 0 {
                    // Check for alternative audit table names
                    if let Ok(alt_count) = sqlx::query_scalar::<_, i32>(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE '%audit%'",
                    )
                    .fetch_one(pool)
                    .await
                    {
                        if alt_count == 0 {
                            warn!(
                                invariant = "DAT-007",
                                "No audit tables found; audit trail may not be enabled"
                            );
                        } else {
                            info!(
                                invariant = "DAT-007",
                                tables = alt_count,
                                "Found audit-related tables"
                            );
                        }
                    }
                }
                report.record_pass(); // Advisory only
            }
            Err(e) => {
                warn!(
                    invariant = "DAT-007",
                    error = %e,
                    "Failed to check audit table"
                );
                report.record_pass();
            }
        }
    }

    report
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
            category: InvariantCategory::System,
            message: "Test violation".to_string(),
            severity: Severity::Fatal,
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
            category: InvariantCategory::Security,
            message: "Warning only".to_string(),
            severity: Severity::Warning,
            remediation: "Consider fixing",
        });

        assert!(!report.has_fatal_violations());
        assert_eq!(report.warning_count(), 1);
    }

    #[test]
    fn test_category_summary() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "AUTH-001",
            category: InvariantCategory::Authentication,
            message: "Auth error".to_string(),
            severity: Severity::Fatal,
            remediation: "Fix auth",
        });
        report.record_violation(InvariantViolation {
            id: "AUTH-002",
            category: InvariantCategory::Authentication,
            message: "Another auth error".to_string(),
            severity: Severity::Warning,
            remediation: "Fix auth 2",
        });
        report.record_violation(InvariantViolation {
            id: "DB-001",
            category: InvariantCategory::Database,
            message: "DB error".to_string(),
            severity: Severity::Fatal,
            remediation: "Fix DB",
        });

        let summary = report.category_summary();
        assert_eq!(summary.len(), 2);
        // Find auth category
        let auth_summary = summary
            .iter()
            .find(|(cat, _, _)| *cat == InvariantCategory::Authentication);
        assert!(
            auth_summary.is_some(),
            "Auth category should be present in summary"
        );
        if let Some((_, count, fatal)) = auth_summary {
            assert_eq!(*count, 2);
            assert_eq!(*fatal, 1);
        }
    }

    #[test]
    fn test_severity_levels() {
        let fatal = InvariantViolation {
            id: "TEST-FATAL",
            category: InvariantCategory::System,
            message: "Fatal".to_string(),
            severity: Severity::Fatal,
            remediation: "",
        };
        let error = InvariantViolation {
            id: "TEST-ERROR",
            category: InvariantCategory::System,
            message: "Error".to_string(),
            severity: Severity::Error,
            remediation: "",
        };
        let warning = InvariantViolation {
            id: "TEST-WARNING",
            category: InvariantCategory::System,
            message: "Warning".to_string(),
            severity: Severity::Warning,
            remediation: "",
        };

        assert!(fatal.is_fatal());
        assert!(!error.is_fatal());
        assert!(!warning.is_fatal());
    }
}
