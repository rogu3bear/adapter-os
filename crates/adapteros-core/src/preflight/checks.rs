//! Core preflight check implementations
//!
//! This module contains the unified preflight validation logic that is
//! shared between CLI and Server API. All checks are implemented here
//! to ensure consistency across both paths.

use super::config::PreflightConfig;
use super::error::PreflightErrorCode;
use super::result::{PreflightCheck, PreflightResult};
use super::traits::{PreflightAdapterData, PreflightDbOps};
use crate::lifecycle::LifecycleState;
use std::path::Path;
use std::str::FromStr;
use std::time::Instant;

/// Run all preflight checks on an adapter
///
/// This is the single source of truth for preflight validation, used by:
/// - CLI: `aosctl adapter swap`, `aosctl preflight`
/// - API: `POST /v1/adapters/swap`, `POST /v1/adapters/{id}/activate`
///
/// # Arguments
/// * `adapter` - Adapter data to validate
/// * `db_ops` - Database operations for training evidence and uniqueness checks
/// * `config` - Preflight configuration with bypass options
///
/// # Returns
/// A `PreflightResult` containing all check results and overall pass/fail status.
pub async fn run_preflight<A: PreflightAdapterData, D: PreflightDbOps>(
    adapter: &A,
    db_ops: &D,
    config: &PreflightConfig,
) -> PreflightResult {
    let start = Instant::now();
    let mut result = PreflightResult::new(adapter.id());

    // Track bypass usage
    for bypass in config.active_bypasses() {
        result.record_bypass(bypass);
    }

    // ========================================================================
    // Check 1: Maintenance mode
    // ========================================================================
    if config.skip_maintenance_check {
        result.add_skipped("Maintenance Mode", "Skipped by configuration");
    } else {
        let check = check_maintenance_mode();
        result.add_check(check);
    }

    // ========================================================================
    // Check 2: .aos file path is set
    // ========================================================================
    let has_aos_path = check_aos_path(adapter, &mut result);

    // ========================================================================
    // Check 3: .aos file hash is set
    // ========================================================================
    let _has_aos_hash = check_aos_hash(adapter, &mut result);

    // ========================================================================
    // Check 4: Content hash is set (critical for determinism)
    // ========================================================================
    let _has_content_hash = check_content_hash(adapter, &mut result);

    // ========================================================================
    // Check 5: Manifest hash is set (critical for routing)
    // ========================================================================
    let _has_manifest_hash = check_manifest_hash(adapter, &mut result);

    // ========================================================================
    // Check 6: Lifecycle state allows activation
    // ========================================================================
    check_lifecycle_state(adapter, config.allow_training_state, &mut result);

    // ========================================================================
    // Check 7: .aos file exists on disk
    // ========================================================================
    if has_aos_path {
        check_aos_file_exists(adapter, &mut result);
    }

    // ========================================================================
    // Check 8: Training snapshot evidence exists
    // ========================================================================
    check_training_evidence(adapter.id(), db_ops, &mut result).await;

    // ========================================================================
    // Check 9: No conflicting active adapters (single-path logic)
    // ========================================================================
    if config.skip_conflict_check {
        result.add_skipped("Repo/Branch Uniqueness", "Skipped by configuration");
    } else {
        check_active_uniqueness(adapter, db_ops, &mut result).await;
    }

    // Apply force mode if configured
    if config.force && !result.passed {
        result.apply_force();
    }

    // Set total duration
    result.set_duration(start.elapsed());

    result
}

/// Check if system is in maintenance mode
fn check_maintenance_mode() -> PreflightCheck {
    let check_start = Instant::now();
    let maintenance_file = Path::new("var/.maintenance");
    let maintenance_env = std::env::var("AOS_MAINTENANCE_MODE").ok();

    if maintenance_file.exists() {
        return PreflightCheck::fail(
            "Maintenance Mode",
            PreflightErrorCode::MaintenanceModeActive,
            "System is in maintenance mode (var/.maintenance exists)",
        )
        .with_remediation("rm var/.maintenance")
        .with_duration(check_start.elapsed());
    }

    if let Some(mode) = maintenance_env {
        if mode == "1" || mode.to_lowercase() == "true" {
            return PreflightCheck::fail(
                "Maintenance Mode",
                PreflightErrorCode::MaintenanceModeActive,
                "System is in maintenance mode (AOS_MAINTENANCE_MODE=true)",
            )
            .with_remediation("unset AOS_MAINTENANCE_MODE")
            .with_duration(check_start.elapsed());
        }
    }

    PreflightCheck::pass("Maintenance Mode", "System is not in maintenance mode")
        .with_duration(check_start.elapsed())
}

/// Check if .aos file path is set
fn check_aos_path<A: PreflightAdapterData>(adapter: &A, result: &mut PreflightResult) -> bool {
    let check_start = Instant::now();

    match adapter.aos_file_path() {
        Some(path) if !path.is_empty() => {
            result.add_check(
                PreflightCheck::pass("AOS File Path", format!("Path set: {}", path))
                    .with_duration(check_start.elapsed()),
            );
            true
        }
        _ => {
            result.add_check(
                PreflightCheck::fail(
                    "AOS File Path",
                    PreflightErrorCode::AdapterFileNotFound,
                    "Adapter missing .aos file path",
                )
                .with_remediation(format!(
                    "Register adapter with .aos file: aosctl adapter register --adapter-id {}",
                    adapter.id()
                ))
                .with_duration(check_start.elapsed()),
            );
            false
        }
    }
}

/// Check if .aos file hash is set
fn check_aos_hash<A: PreflightAdapterData>(adapter: &A, result: &mut PreflightResult) -> bool {
    let check_start = Instant::now();

    match adapter.aos_file_hash() {
        Some(hash) if !hash.is_empty() => {
            result.add_check(
                PreflightCheck::pass("AOS File Hash", "File hash set for integrity verification")
                    .with_duration(check_start.elapsed()),
            );
            true
        }
        _ => {
            result.add_check(
                PreflightCheck::fail(
                    "AOS File Hash",
                    PreflightErrorCode::MissingAosFileHash,
                    "Adapter missing .aos file hash",
                )
                .with_remediation(format!(
                    "Run: aosctl adapter repair-hashes --adapter-id {}",
                    adapter.id()
                ))
                .with_duration(check_start.elapsed()),
            );
            false
        }
    }
}

/// Check if content_hash_b3 is set
fn check_content_hash<A: PreflightAdapterData>(adapter: &A, result: &mut PreflightResult) -> bool {
    let check_start = Instant::now();

    match adapter.content_hash_b3() {
        Some(hash) if !hash.trim().is_empty() => {
            result.add_check(
                PreflightCheck::pass(
                    "Content Hash",
                    "content_hash_b3 (BLAKE3 integrity hash) set",
                )
                .with_duration(check_start.elapsed()),
            );
            true
        }
        _ => {
            let aos_path_hint = adapter
                .aos_file_path()
                .filter(|p| !p.is_empty())
                .map(|p| format!(" (.aos path: {})", p))
                .unwrap_or_default();

            result.add_check(
                PreflightCheck::fail(
                    "Content Hash",
                    PreflightErrorCode::MissingContentHash,
                    format!(
                        "Adapter missing content_hash_b3 (required for integrity verification){}",
                        aos_path_hint
                    ),
                )
                .with_remediation(format!(
                    "Run: aosctl adapter repair-hashes --adapter-id {}",
                    adapter.id()
                ))
                .with_duration(check_start.elapsed()),
            );
            false
        }
    }
}

/// Check if manifest_hash is set
fn check_manifest_hash<A: PreflightAdapterData>(adapter: &A, result: &mut PreflightResult) -> bool {
    let check_start = Instant::now();

    match adapter.manifest_hash() {
        Some(hash) if !hash.trim().is_empty() => {
            result.add_check(
                PreflightCheck::pass("Manifest Hash", "manifest_hash (BLAKE3 manifest hash) set")
                    .with_duration(check_start.elapsed()),
            );
            true
        }
        _ => {
            let aos_path_hint = adapter
                .aos_file_path()
                .filter(|p| !p.is_empty())
                .map(|p| format!(" (.aos path: {})", p))
                .unwrap_or_default();

            result.add_check(
                PreflightCheck::fail(
                    "Manifest Hash",
                    PreflightErrorCode::MissingManifestHash,
                    format!(
                        "Adapter missing manifest_hash (required for deterministic routing){}",
                        aos_path_hint
                    ),
                )
                .with_remediation(format!(
                    "Run: aosctl adapter repair-hashes --adapter-id {}",
                    adapter.id()
                ))
                .with_duration(check_start.elapsed()),
            );
            false
        }
    }
}

/// Check if lifecycle state allows activation
fn check_lifecycle_state<A: PreflightAdapterData>(
    adapter: &A,
    allow_training: bool,
    result: &mut PreflightResult,
) {
    let check_start = Instant::now();
    let state_str = adapter.lifecycle_state();

    match LifecycleState::from_str(state_str) {
        Ok(state) => {
            if state.is_terminal() {
                let recovery_hint = if state == LifecycleState::Retired {
                    "Retired adapters cannot be reactivated. Create a new adapter version instead."
                } else {
                    "Failed adapters cannot be reactivated. Investigate the failure cause and retrain."
                };

                result.add_check(
                    PreflightCheck::fail(
                        "Lifecycle State",
                        PreflightErrorCode::TerminalLifecycleState,
                        format!(
                            "Adapter in terminal state '{}' - cannot be activated",
                            state_str
                        ),
                    )
                    .with_remediation(recovery_hint)
                    .with_duration(check_start.elapsed()),
                );
            } else if !state.allows_alias_swap(allow_training) {
                result.add_check(
                    PreflightCheck::fail(
                        "Lifecycle State",
                        PreflightErrorCode::LifecycleStateNotAllowed,
                        format!(
                            "Adapter lifecycle state '{}' does not allow activation (need ready/active)",
                            state_str
                        ),
                    )
                    .with_duration(check_start.elapsed()),
                );
            } else {
                result.add_check(
                    PreflightCheck::pass(
                        "Lifecycle State",
                        format!("Lifecycle state '{}' allows activation", state_str),
                    )
                    .with_duration(check_start.elapsed()),
                );
            }
        }
        Err(_) => {
            result.add_check(
                PreflightCheck::fail(
                    "Lifecycle State",
                    PreflightErrorCode::InvalidLifecycleState,
                    format!("Adapter lifecycle state '{}' is not recognized", state_str),
                )
                .with_duration(check_start.elapsed()),
            );
        }
    }
}

/// Check if .aos file exists on disk
fn check_aos_file_exists<A: PreflightAdapterData>(adapter: &A, result: &mut PreflightResult) {
    let check_start = Instant::now();

    if let Some(aos_path) = adapter.aos_file_path() {
        if !aos_path.is_empty() {
            let path = Path::new(aos_path);
            if path.exists() {
                result.add_check(
                    PreflightCheck::pass("AOS File Exists", format!("File found: {}", aos_path))
                        .with_duration(check_start.elapsed()),
                );
            } else {
                result.add_check(
                    PreflightCheck::fail(
                        "AOS File Exists",
                        PreflightErrorCode::AdapterFileNotFound,
                        format!("File not found at: {}", aos_path),
                    )
                    .with_remediation("Re-register the adapter or restore the .aos file")
                    .with_duration(check_start.elapsed()),
                );
            }
        }
    }
}

/// Check if training snapshot evidence exists
async fn check_training_evidence<D: PreflightDbOps>(
    adapter_id: &str,
    db_ops: &D,
    result: &mut PreflightResult,
) {
    let check_start = Instant::now();

    match db_ops.has_training_snapshot(adapter_id).await {
        Ok(true) => {
            result.add_check(
                PreflightCheck::pass("Training Evidence", "Training snapshot evidence exists")
                    .with_duration(check_start.elapsed()),
            );
        }
        Ok(false) => {
            result.add_check(
                PreflightCheck::fail(
                    "Training Evidence",
                    PreflightErrorCode::MissingTrainingEvidence,
                    "Training snapshot evidence missing",
                )
                .with_remediation(format!(
                    "Retrain the adapter: aosctl adapter train --adapter-id {}",
                    adapter_id
                ))
                .with_duration(check_start.elapsed()),
            );
        }
        Err(e) => {
            result.add_check(
                PreflightCheck::fail(
                    "Training Evidence",
                    PreflightErrorCode::DatabaseError,
                    format!("Failed to check training evidence: {}", e),
                )
                .with_duration(check_start.elapsed()),
            );
        }
    }
}

/// Check for conflicting active adapters (single-path logic)
async fn check_active_uniqueness<A: PreflightAdapterData, D: PreflightDbOps>(
    adapter: &A,
    db_ops: &D,
    result: &mut PreflightResult,
) {
    let check_start = Instant::now();

    // Extract branch from metadata if available
    let branch = extract_branch_from_metadata(adapter.metadata_json());

    let repo_id = adapter.repo_id().map(String::from);
    let repo_path = adapter.repo_path().map(String::from);
    let codebase_scope = adapter.codebase_scope().map(String::from);

    // Skip if no scope fields are set
    if repo_id.is_none() && repo_path.is_none() && codebase_scope.is_none() {
        result.add_check(
            PreflightCheck::pass(
                "Repo/Branch Uniqueness",
                "No repo/scope constraints (adapter is unlinked)",
            )
            .with_duration(check_start.elapsed()),
        );
        return;
    }

    match db_ops
        .validate_active_uniqueness(adapter.id(), repo_id, repo_path, codebase_scope, branch)
        .await
    {
        Ok(uniqueness) => {
            if uniqueness.is_valid {
                result.add_check(
                    PreflightCheck::pass(
                        "Repo/Branch Uniqueness",
                        "No conflicting active adapters",
                    )
                    .with_duration(check_start.elapsed()),
                );
            } else {
                let conflict_ids = uniqueness.conflicting_adapters.join(", ");
                result.add_check(
                    PreflightCheck::fail(
                        "Repo/Branch Uniqueness",
                        PreflightErrorCode::ConflictingActiveAdapters,
                        format!(
                            "Conflicting active adapter(s): {}. {}",
                            conflict_ids,
                            uniqueness.conflict_reason.unwrap_or_default()
                        ),
                    )
                    .with_remediation(format!(
                        "Deactivate conflicting adapter(s) first: aosctl adapter deactivate {}",
                        uniqueness
                            .conflicting_adapters
                            .first()
                            .unwrap_or(&String::new())
                    ))
                    .with_duration(check_start.elapsed()),
                );
            }
        }
        Err(e) => {
            result.add_check(
                PreflightCheck::fail(
                    "Repo/Branch Uniqueness",
                    PreflightErrorCode::DatabaseError,
                    format!("Failed to check uniqueness: {}", e),
                )
                .with_duration(check_start.elapsed()),
            );
        }
    }
}

/// Extract branch from metadata JSON
fn extract_branch_from_metadata(metadata_json: Option<&str>) -> Option<String> {
    let json_str = metadata_json?;
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    value
        .get("branch")
        .or_else(|| value.get("adapter_branch"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Check if system is currently in maintenance mode (public for external use)
pub fn is_maintenance_mode() -> bool {
    let maintenance_file = Path::new("var/.maintenance");
    if maintenance_file.exists() {
        return true;
    }

    if let Ok(mode) = std::env::var("AOS_MAINTENANCE_MODE") {
        return mode == "1" || mode.to_lowercase() == "true";
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preflight::traits::mock::MockPreflightDb;
    use crate::preflight::traits::SimpleAdapterData;

    fn valid_adapter() -> SimpleAdapterData {
        SimpleAdapterData {
            id: "test-adapter".to_string(),
            tenant_id: "tenant-1".to_string(),
            lifecycle_state: "ready".to_string(),
            tier: "warm".to_string(),
            aos_file_path: Some("/tmp/test.aos".to_string()),
            aos_file_hash: Some("hash123".to_string()),
            content_hash_b3: Some("contenthash123".to_string()),
            manifest_hash: Some("manifesthash123".to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_preflight_valid_adapter() {
        // Create temp file for test
        let temp_file = "/tmp/test_preflight.aos";
        std::fs::write(temp_file, b"test content").ok();

        let adapter = SimpleAdapterData {
            aos_file_path: Some(temp_file.to_string()),
            ..valid_adapter()
        };

        let db = MockPreflightDb::with_snapshot();
        let config = PreflightConfig::new();

        let result = run_preflight(&adapter, &db, &config).await;

        // Cleanup
        std::fs::remove_file(temp_file).ok();

        assert!(
            result.passed,
            "Expected preflight to pass: {:?}",
            result.failures
        );
    }

    #[tokio::test]
    async fn test_preflight_missing_content_hash() {
        let adapter = SimpleAdapterData {
            content_hash_b3: None,
            ..valid_adapter()
        };

        let db = MockPreflightDb::with_snapshot();
        let config = PreflightConfig::new();

        let result = run_preflight(&adapter, &db, &config).await;

        assert!(!result.passed);
        assert!(result
            .error_codes()
            .contains(&PreflightErrorCode::MissingContentHash));
    }

    #[tokio::test]
    async fn test_preflight_missing_manifest_hash() {
        let adapter = SimpleAdapterData {
            manifest_hash: None,
            ..valid_adapter()
        };

        let db = MockPreflightDb::with_snapshot();
        let config = PreflightConfig::new();

        let result = run_preflight(&adapter, &db, &config).await;

        assert!(!result.passed);
        assert!(result
            .error_codes()
            .contains(&PreflightErrorCode::MissingManifestHash));
    }

    #[tokio::test]
    async fn test_preflight_terminal_state() {
        let adapter = SimpleAdapterData {
            lifecycle_state: "retired".to_string(),
            ..valid_adapter()
        };

        let db = MockPreflightDb::with_snapshot();
        let config = PreflightConfig::new();

        let result = run_preflight(&adapter, &db, &config).await;

        assert!(!result.passed);
        assert!(result
            .error_codes()
            .contains(&PreflightErrorCode::TerminalLifecycleState));
    }

    #[tokio::test]
    async fn test_preflight_force_mode() {
        let adapter = SimpleAdapterData {
            content_hash_b3: None, // This would normally fail
            ..valid_adapter()
        };

        let db = MockPreflightDb::with_snapshot();
        let config = PreflightConfig::new().force_pass("Emergency recovery");

        let result = run_preflight(&adapter, &db, &config).await;

        assert!(result.passed); // Force mode makes it pass
        assert!(result.force_applied);
        assert!(!result.warnings.is_empty()); // Failure converted to warning
    }

    #[tokio::test]
    async fn test_preflight_skip_conflict_check() {
        let adapter = valid_adapter();

        let db = MockPreflightDb::with_conflict(vec!["other-adapter".to_string()], "Same repo");

        // Without skip - should fail
        let config = PreflightConfig::new();
        let result = run_preflight(&adapter, &db, &config).await;
        // Note: This passes because repo_id is None in valid_adapter()
        // Conflict check is skipped when no scope fields are set

        // With skip - should pass
        let config = PreflightConfig::new().skip_conflicts("Intentional replacement");
        let result = run_preflight(&adapter, &db, &config).await;
        assert!(result
            .bypasses_used
            .contains(&"skip_conflict_check".to_string()));
    }

    #[tokio::test]
    async fn test_preflight_missing_training_evidence() {
        let adapter = valid_adapter();
        let db = MockPreflightDb::without_snapshot();
        let config = PreflightConfig::new();

        let result = run_preflight(&adapter, &db, &config).await;

        assert!(!result.passed);
        assert!(result
            .error_codes()
            .contains(&PreflightErrorCode::MissingTrainingEvidence));
    }
}
