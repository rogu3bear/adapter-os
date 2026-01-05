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
/// | `SEC-015` | Signature bypass env var must not be set in production | Yes |
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
    // TODO: Remaining invariants to implement (28 more from analysis)
    // =========================================================================
    // See: ~/.claude/plans/vivid-imagining-rocket.md for full details
    //
    // SECURITY (remaining):
    // - SEC-006: JWT signature verification (auth.rs:926-979) - implicit
    // - SEC-007: Tenant isolation (security/mod.rs:68-179) - implicit per-handler
    // - SEC-008: RBAC permission checks (permissions.rs:206+) - implicit
    // - SEC-009: Token revocation baseline (security/mod.rs:288-320) - implicit
    // - SEC-010: Hardware attestation (federation/attestation.rs:130-144) - implicit
    // - SEC-011: Quorum signature verification (federation/signature.rs:318-350)
    // - SEC-012: Adapter bundle signature (cli/verify_adapter.rs:48-64)
    // - SEC-013: Password timing-safety (auth.rs:298-349) - always enforced
    // - SEC-014: Brute force protection (security/mod.rs:341-457) - always enforced
    //
    // DATA INTEGRITY:
    // - DAT-001: Archive state machine (migrations/0138_adapter_archive_gc.sql)
    // - DAT-002: Foreign key constraints (migrations/0001_init.sql)
    // - DAT-003: AOS file hash match (adapter_aos_invariant_tests.rs)
    // - DAT-004: KV presence for readiness (adapter_consistency.rs)
    // - DAT-005: Enum constraints (migrations/0012_enhanced_adapter_schema.sql)
    // - DAT-006: Migration ordering (sqlx migrations)
    // - DAT-007: Audit log chain (audit.rs:80-100) - FAILS OPEN
    //
    // MEMORY MANAGEMENT:
    // - MEM-001: KV cache generation coherence (kvcache.rs:262-298)
    // - MEM-002: GPU buffer fingerprint (unified_tracker.rs:399-425)
    // - MEM-003: Memory pressure headroom (unified_tracker.rs:497-536) - FAILS OPEN
    // - MEM-004: KV slab non-overlapping (kvcache.rs:424-464) - FAILS OPEN (race)
    //
    // CONCURRENCY:
    // - CON-001: Hot-swap atomic pointer (adapter_hotswap.rs:194-196) - FAILS OPEN
    // - CON-002: KV quota transactional (kv_quota.rs:140-208)
    // - CON-003: Model cache pinning (model_handle_cache.rs) - FAILS OPEN (OOM)
    // - CON-004: Request pin refcount (request_pinner.rs:17-42)
    //
    // LIFECYCLE:
    // - LIF-001: Boot phase ordering (boot/) - FAILS OPEN
    // - LIF-002: Global executor init (backend_factory.rs:126-143)
    // - LIF-003: Adapter lifecycle CAS (lora-lifecycle/state.rs:107-148)
    // - LIF-004: Connection pool drain (boot/database.rs) - FAILS OPEN

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
}
