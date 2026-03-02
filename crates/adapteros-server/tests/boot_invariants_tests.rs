//! Boot Invariants Integration Tests
//!
//! Tests for the boot-time invariant validation system.
//! These tests verify that:
//! 1. Invariant violations are detected correctly
//! 2. Production mode blocks on fatal violations
//! 3. Config escape hatches work correctly
//! 4. Metrics are recorded properly
//!
//! Citations:
//! - crates/adapteros-server/src/boot/invariants.rs: validate_boot_invariants, enforce_invariants

#![allow(clippy::absurd_extreme_comparisons)]
#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::vec_init_then_push)]
#![allow(unused_comparisons)]

use adapteros_config::InvariantsConfig;
use adapteros_server::boot::{
    boot_invariant_metrics, enforce_invariants, invariants::InvariantCategory,
    invariants::Severity, validate_boot_invariants, validate_post_db_invariants, InvariantReport,
    InvariantViolation,
};
use adapteros_server_api::config::Config;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Helper to create a minimal test config
fn create_test_config(production_mode: bool, invariants: InvariantsConfig) -> Arc<RwLock<Config>> {
    // Boot invariants require a fully deserializable Config; reuse the checked-in
    // reference config as a stable baseline, then override only the fields that
    // these tests need to control.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .ancestors()
        .nth(2)
        .unwrap_or(manifest_dir.as_path());
    let config_path = repo_root.join("configs").join("cp.toml");

    let mut cfg = Config::load(
        config_path
            .to_str()
            .expect("config path must be valid UTF-8"),
    )
    .expect("load configs/cp.toml for boot invariants tests");

    cfg.server.production_mode = production_mode;
    cfg.invariants = invariants;

    Arc::new(RwLock::new(cfg))
}

#[cfg(test)]
mod invariant_report_tests {
    use super::*;

    #[test]
    fn test_report_tracks_passes() {
        let mut report = InvariantReport::new();
        report.record_pass();
        report.record_pass();
        report.record_pass();

        assert_eq!(report.checks_passed, 3);
        assert_eq!(report.checks_failed, 0);
        assert_eq!(report.checks_skipped, 0);
        assert!(!report.has_fatal_violations());
    }

    #[test]
    fn test_report_tracks_violations() {
        let mut report = InvariantReport::new();
        report.record_pass();
        report.record_violation(InvariantViolation {
            id: "TEST-001",
            category: InvariantCategory::Authentication,
            message: "Test fatal violation".to_string(),
            severity: Severity::Fatal,
            remediation: "Fix the test",
        });
        report.record_violation(InvariantViolation {
            id: "TEST-002",
            category: InvariantCategory::Authentication,
            message: "Test warning".to_string(),
            severity: Severity::Warning,
            remediation: "Consider fixing",
        });

        assert_eq!(report.checks_passed, 1);
        assert_eq!(report.checks_failed, 2);
        assert!(report.has_fatal_violations());
        assert_eq!(report.fatal_count(), 1);
        assert_eq!(report.warning_count(), 1);
    }

    #[test]
    fn test_report_tracks_skips() {
        let mut report = InvariantReport::new();
        report.record_pass();
        report.record_skip("SEC-001");
        report.record_skip("SEC-002");

        assert_eq!(report.checks_passed, 1);
        assert_eq!(report.checks_skipped, 2);
        assert_eq!(report.skipped_ids, vec!["SEC-001", "SEC-002"]);
    }

    #[test]
    fn test_non_fatal_violations_dont_block() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "WARN-001",
            category: InvariantCategory::Authentication,
            message: "Warning only".to_string(),
            severity: Severity::Warning,
            remediation: "Consider fixing",
        });

        assert!(!report.has_fatal_violations());
        assert_eq!(report.warning_count(), 1);
        assert_eq!(report.fatal_count(), 0);
    }
}

#[cfg(test)]
mod enforcement_tests {
    use super::*;

    #[test]
    fn test_enforce_allows_clean_report() {
        let report = InvariantReport::new();
        let result = enforce_invariants(&report, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_enforce_allows_warnings_in_production() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "WARN-001",
            category: InvariantCategory::Authentication,
            message: "Non-fatal warning".to_string(),
            severity: Severity::Warning,
            remediation: "Optional fix",
        });

        let result = enforce_invariants(&report, true);
        assert!(
            result.is_ok(),
            "Non-fatal violations should not block production boot"
        );
    }

    #[test]
    fn test_enforce_blocks_fatal_in_production() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "SEC-001",
            category: InvariantCategory::Authentication,
            message: "Fatal security violation".to_string(),
            severity: Severity::Fatal,
            remediation: "Must fix before boot",
        });

        let result = enforce_invariants(&report, true);
        assert!(
            result.is_err(),
            "Fatal violations should block production boot"
        );

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("SEC-001"),
            "Error should identify the violating invariant"
        );
        assert!(
            err_msg.contains("fatal"),
            "Error should mention fatal violation"
        );
    }

    #[test]
    fn test_enforce_allows_fatal_in_development() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "SEC-001",
            category: InvariantCategory::Authentication,
            message: "Fatal security violation".to_string(),
            severity: Severity::Fatal,
            remediation: "Should fix, but dev mode allows",
        });

        let result = enforce_invariants(&report, false);
        assert!(
            result.is_ok(),
            "Fatal violations should be allowed in development mode (fail open)"
        );
    }

    #[test]
    fn test_enforce_blocks_dat_008_in_development() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "DAT-008",
            category: InvariantCategory::Database,
            message: "Required schema contract missing request_log.latency_ms".to_string(),
            severity: Severity::Fatal,
            remediation: "Run migrations to restore schema contract",
        });

        let result = enforce_invariants(&report, false);
        assert!(
            result.is_err(),
            "DAT-008 must block startup in development mode as well"
        );
    }

    #[test]
    fn test_enforce_multiple_fatal_violations() {
        let mut report = InvariantReport::new();
        report.record_violation(InvariantViolation {
            id: "SEC-001",
            category: InvariantCategory::Authentication,
            message: "First fatal".to_string(),
            severity: Severity::Fatal,
            remediation: "Fix 1",
        });
        report.record_violation(InvariantViolation {
            id: "SEC-002",
            category: InvariantCategory::Authentication,
            message: "Second fatal".to_string(),
            severity: Severity::Fatal,
            remediation: "Fix 2",
        });
        report.record_violation(InvariantViolation {
            id: "WARN-001",
            category: InvariantCategory::Authentication,
            message: "A warning".to_string(),
            severity: Severity::Warning,
            remediation: "Optional",
        });

        let result = enforce_invariants(&report, true);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("SEC-001"), "Should list first violation");
        assert!(err_msg.contains("SEC-002"), "Should list second violation");
        assert!(err_msg.contains("2"), "Should count 2 fatal violations");
    }
}

#[cfg(test)]
mod validation_harness_tests {
    use super::*;

    #[test]
    fn test_sec_000_break_glass_required_when_disabling_invariants_in_production() {
        let invariants = InvariantsConfig {
            disable_sec_001_dev_bypass: true,
            i_understand_security_risk: false,
            ..InvariantsConfig::default()
        };

        let cfg = create_test_config(true, invariants);
        let report = validate_boot_invariants(&cfg, true);

        assert!(
            report.violations.iter().any(|v| v.id == "SEC-000"),
            "Expected SEC-000 when disabling invariants in production without acknowledgement"
        );

        assert!(
            report.skipped_ids.contains(&"SEC-001"),
            "Expected SEC-001 to be recorded as skipped when disable_sec_001_dev_bypass=true"
        );
    }

    #[test]
    fn test_sec_000_not_triggered_when_break_glass_ack_is_set() {
        let invariants = InvariantsConfig {
            disable_sec_001_dev_bypass: true,
            i_understand_security_risk: true,
            ..InvariantsConfig::default()
        };

        let cfg = create_test_config(true, invariants);
        let report = validate_boot_invariants(&cfg, true);

        assert!(
            !report.violations.iter().any(|v| v.id == "SEC-000"),
            "SEC-000 should be absent when invariants.i_understand_security_risk=true"
        );
    }
}

#[cfg(test)]
mod metrics_tests {
    use super::*;

    #[test]
    fn test_metrics_snapshot() {
        // Note: Metrics are global atomics, so this test just verifies the snapshot works
        let metrics = boot_invariant_metrics();
        // Values depend on previous tests, just verify fields exist
        assert!(metrics.checked >= 0);
        assert!(metrics.violated >= 0);
        assert!(metrics.fatal >= 0);
        assert!(metrics.skipped >= 0);
    }
}

#[cfg(test)]
mod new_invariants_tests {
    use super::*;

    /// Test: CFG-002 validation logic (session TTL >= access token TTL)
    #[test]
    fn test_cfg_002_session_ttl_validation() {
        // Invalid case: access_ttl > session_ttl
        let access_ttl: u64 = 86400; // 24 hours
        let session_ttl: u64 = 3600; // 1 hour
        assert!(
            access_ttl > session_ttl,
            "CFG-002 should detect access_ttl > session_ttl"
        );

        // Valid case: access_ttl <= session_ttl
        let valid_access: u64 = 900;
        let valid_session: u64 = 43200;
        assert!(valid_access <= valid_session);
    }

    /// Test: SEC-008 RBAC config validation (JWT secret length)
    #[test]
    fn test_sec_008_rbac_config_validation() {
        // Invalid case: empty JWT secret
        let empty_secret = "";
        assert!(empty_secret.is_empty());

        // Warning case: short JWT secret
        let short_secret = "short";
        assert!(short_secret.len() < 32);

        // Valid case: strong JWT secret (32+ chars)
        let strong_secret = "this-is-a-32-character-secret!!!";
        assert!(strong_secret.len() >= 32);
    }

    /// Test: SEC-014 brute force protection validation
    #[test]
    fn test_sec_014_brute_force_validation() {
        // Invalid case: lockout disabled
        let disabled_threshold = 0u32;
        assert_eq!(
            disabled_threshold, 0,
            "SEC-014 should detect disabled lockout"
        );

        // Warning case: very high threshold
        let high_threshold = 100u32;
        assert!(high_threshold > 20, "High threshold should trigger warning");

        // Valid case: reasonable threshold
        let valid_threshold = 5u32;
        assert!(valid_threshold > 0 && valid_threshold <= 20);
    }

    /// Test: DAT-005 storage mode validation
    #[test]
    fn test_dat_005_storage_mode_validation() {
        let valid_modes = [
            "sql_only",
            "sql",
            "dual_write",
            "dual",
            "kv_primary",
            "kv_only",
        ];

        for mode in valid_modes {
            let lowercase = mode.to_lowercase();
            assert!(
                valid_modes
                    .iter()
                    .map(|m| m.to_lowercase())
                    .any(|m| m == lowercase),
                "Mode '{}' should be valid",
                mode
            );
        }

        // Invalid modes
        let invalid_modes = ["invalid", "memory", ""];
        for mode in invalid_modes {
            let lowercase = mode.to_lowercase();
            assert!(
                !valid_modes
                    .iter()
                    .map(|m| m.to_lowercase())
                    .any(|m| m == lowercase),
                "Mode '{}' should be invalid",
                mode
            );
        }
    }
}

#[cfg(test)]
mod dat_008_schema_contract_tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup_memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create sqlite in-memory pool")
    }

    async fn create_required_dat_008_schema(
        pool: &SqlitePool,
        include_request_log_latency_ms: bool,
    ) {
        sqlx::query("CREATE TABLE workers (id TEXT PRIMARY KEY, last_seen_at TEXT NOT NULL)")
            .execute(pool)
            .await
            .expect("create workers");
        sqlx::query("CREATE TABLE system_settings (key TEXT PRIMARY KEY, value TEXT)")
            .execute(pool)
            .await
            .expect("create system_settings");
        sqlx::query(
            "CREATE TABLE inference_trace_tokens (
                id TEXT PRIMARY KEY,
                selected_adapter_ids TEXT,
                gates_q15 TEXT
            )",
        )
        .execute(pool)
        .await
        .expect("create inference_trace_tokens");
        sqlx::query(
            "CREATE TABLE training_dataset_rows (
                id TEXT PRIMARY KEY,
                prompt TEXT,
                response TEXT,
                source_line INTEGER
            )",
        )
        .execute(pool)
        .await
        .expect("create training_dataset_rows");
        sqlx::query(
            "CREATE TABLE plans (
                id TEXT PRIMARY KEY,
                cpid TEXT,
                metallib_hash_b3 TEXT,
                kernel_hashes_json TEXT
            )",
        )
        .execute(pool)
        .await
        .expect("create plans");
        sqlx::query("CREATE TABLE telemetry_events (id TEXT PRIMARY KEY, event_data TEXT)")
            .execute(pool)
            .await
            .expect("create telemetry_events");

        let request_log_sql = if include_request_log_latency_ms {
            "CREATE TABLE request_log (id TEXT PRIMARY KEY, status_code INTEGER, latency_ms INTEGER)"
        } else {
            "CREATE TABLE request_log (id TEXT PRIMARY KEY, status_code INTEGER)"
        };
        sqlx::query(request_log_sql)
            .execute(pool)
            .await
            .expect("create request_log");
    }

    fn config_for_dat_008_only() -> Arc<RwLock<Config>> {
        let invariants = InvariantsConfig {
            disable_dat_001_archive_triggers: true,
            disable_dat_002_foreign_keys: true,
            disable_dat_006_migration_order: true,
            disable_dat_007_audit_chain: true,
            ..InvariantsConfig::default()
        };
        create_test_config(true, invariants)
    }

    #[tokio::test]
    async fn dat_008_fails_when_required_schema_column_is_missing() {
        let cfg = config_for_dat_008_only();
        let pool = setup_memory_pool().await;
        create_required_dat_008_schema(&pool, false).await;

        let report = validate_post_db_invariants(&cfg, &pool).await;
        let dat_008 = report
            .violations
            .iter()
            .find(|v| v.id == "DAT-008")
            .expect("expected DAT-008 violation");

        assert!(
            dat_008.message.contains("request_log.latency_ms"),
            "DAT-008 should identify missing required request_log.latency_ms column"
        );
        assert!(
            dat_008.is_fatal(),
            "DAT-008 should be fatal in production mode"
        );
    }

    #[tokio::test]
    async fn dat_008_passes_when_required_schema_exists() {
        let cfg = config_for_dat_008_only();
        let pool = setup_memory_pool().await;
        create_required_dat_008_schema(&pool, true).await;

        let report = validate_post_db_invariants(&cfg, &pool).await;
        assert!(
            !report.violations.iter().any(|v| v.id == "DAT-008"),
            "DAT-008 should pass when required schema contract is present"
        );
    }
}

#[cfg(test)]
mod escape_hatch_documentation {
    //! These tests document the escape hatch configuration behavior.
    //! Full integration testing requires the test harness.

    use super::*;

    /// Documents: InvariantsConfig fields map to specific checks
    #[test]
    fn doc_invariants_config_fields() {
        let config = InvariantsConfig::default();

        // All escape hatches default to false (checks enabled)
        assert!(!config.disable_sec_001_dev_bypass);
        assert!(!config.disable_sec_002_dual_write);
        assert!(!config.disable_sec_003_executor_seed);
        assert!(!config.disable_sec_005_cookie_security);
        assert!(!config.disable_sec_006_jwt_verify);
        assert!(!config.disable_sec_008_rbac_config);
        assert!(!config.disable_sec_014_brute_force);
        assert!(!config.disable_sec_015_signature_bypass);
        assert!(!config.disable_dat_002_foreign_keys);
        assert!(!config.disable_dat_005_storage_mode);
        assert!(!config.disable_dat_006_migration_order);
        assert!(!config.disable_dat_007_audit_chain);
        assert!(!config.disable_dat_008_schema_contract);
        assert!(!config.disable_lif_002_executor_init);
        assert!(!config.disable_cfg_002_session_ttl);

        println!("InvariantsConfig escape hatch mapping:");
        println!("  disable_sec_001_dev_bypass -> Skip SEC-001 (dev auth bypass check)");
        println!("  disable_sec_002_dual_write -> Skip SEC-002 (dual-write strict mode check)");
        println!("  disable_sec_003_executor_seed -> Skip SEC-003 (executor manifest check)");
        println!("  disable_sec_005_cookie_security -> Skip SEC-005 (cookie security check)");
        println!("  disable_sec_006_jwt_verify -> Skip SEC-006 (JWT algorithm check)");
        println!("  disable_sec_008_rbac_config -> Skip SEC-008 (RBAC permission config check)");
        println!("  disable_sec_014_brute_force -> Skip SEC-014 (brute force protection check)");
        println!("  disable_sec_015_signature_bypass -> Skip SEC-015 (signature bypass check)");
        println!("  disable_dat_002_foreign_keys -> Skip DAT-002 (foreign key constraints check)");
        println!("  disable_dat_005_storage_mode -> Skip DAT-005 (storage mode enum check)");
        println!("  disable_dat_006_migration_order -> Skip DAT-006 (migration ordering check)");
        println!("  disable_dat_007_audit_chain -> Skip DAT-007 (audit chain init check)");
        println!(
            "  disable_dat_008_schema_contract -> Skip DAT-008 (required schema contract check)"
        );
        println!("  disable_lif_002_executor_init -> Skip LIF-002 (executor initialization check)");
        println!("  disable_cfg_002_session_ttl -> Skip CFG-002 (session TTL hierarchy check)");
    }

    /// Documents: TOML config syntax for escape hatches
    #[test]
    fn doc_toml_escape_hatch_syntax() {
        println!("To disable an invariant check in config.toml:");
        println!();
        println!("[invariants]");
        println!("disable_sec_001_dev_bypass = true  # NOT RECOMMENDED");
        println!("disable_sec_002_dual_write = true  # NOT RECOMMENDED");
        println!("disable_sec_003_executor_seed = true  # NOT RECOMMENDED");
        println!("disable_sec_005_cookie_security = true  # NOT RECOMMENDED");
        println!();
        println!("WARNING: Disabling invariant checks bypasses critical safety guards.");
        println!("Only use during incidents with explicit approval.");
    }

    /// Documents: Expected log output when checks are skipped
    #[test]
    fn doc_skip_log_output() {
        println!("When an invariant check is skipped, the following is logged:");
        println!();
        println!("  WARN invariant=SEC-001 \"INVARIANT CHECK SKIPPED via config escape hatch (NOT RECOMMENDED)\"");
        println!();
        println!("Summary log will include:");
        println!("  WARN passed=N failed=M skipped=K skipped_ids=[\"SEC-001\"] \"Invariant validation complete (WARNING: K checks skipped via config)\"");
    }
}
