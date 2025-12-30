//! Deployment verification for codebase adapters
//!
//! Provides verification checks to ensure codebase adapters can be safely activated:
//! - Repository is clean (no uncommitted changes)
//! - Manifest hash matches stored value
//! - CoreML package hash matches (if applicable)
//! - No session binding conflicts
//!
//! # Usage
//!
//! Before activating a codebase adapter, run deployment verification:
//!
//! ```ignore
//! use adapteros_core::deployment_verification::{
//!     verify_codebase_deployment, DeploymentCheckResult,
//! };
//!
//! let check = verify_codebase_deployment(
//!     &adapter,
//!     Some(&repo_path),
//!     Some(&session_id),
//!     &db,
//! ).await?;
//!
//! if !check.all_passed() {
//!     return Err(AosError::Lifecycle(format!(
//!         "Deployment verification failed: {:?}", check.failures()
//!     )));
//! }
//! ```
//!
//! 【2025-01-29†prd-adapters†deployment_verification】

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Result of a single deployment check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentCheck {
    /// Name of the check
    pub name: String,
    /// Whether the check passed
    pub passed: bool,
    /// Details about the check result
    pub details: Option<String>,
}

impl DeploymentCheck {
    /// Create a passed check
    pub fn passed(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            details: None,
        }
    }

    /// Create a passed check with details
    pub fn passed_with_details(name: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            details: Some(details.into()),
        }
    }

    /// Create a failed check
    pub fn failed(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            details: Some(reason.into()),
        }
    }

    /// Create a skipped check (treated as passed)
    pub fn skipped(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            details: Some(format!("Skipped: {}", reason.into())),
        }
    }
}

/// Complete result of deployment verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentCheckResult {
    /// Individual check results
    pub checks: Vec<DeploymentCheck>,
    /// Timestamp of verification
    pub verified_at: String,
}

impl DeploymentCheckResult {
    /// Create a new result with the current timestamp
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
            verified_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    /// Add a check to the result
    pub fn add_check(&mut self, check: DeploymentCheck) {
        self.checks.push(check);
    }

    /// Check if all verifications passed
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    /// Get list of failed checks
    pub fn failures(&self) -> Vec<&DeploymentCheck> {
        self.checks.iter().filter(|c| !c.passed).collect()
    }

    /// Get list of passed checks
    pub fn passed_checks(&self) -> Vec<&DeploymentCheck> {
        self.checks.iter().filter(|c| c.passed).collect()
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        let passed = self.checks.iter().filter(|c| c.passed).count();
        let total = self.checks.len();
        format!(
            "{}/{} checks passed{}",
            passed,
            total,
            if self.all_passed() { "" } else { " (FAILED)" }
        )
    }
}

impl Default for DeploymentCheckResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a git repository is clean (no uncommitted changes)
///
/// This is a synchronous check that runs `git status --porcelain`.
///
/// # Arguments
///
/// * `repo_path` - Path to the git repository
///
/// # Returns
///
/// Returns `true` if the repository has no uncommitted changes.
pub fn check_repo_clean(repo_path: &Path) -> DeploymentCheck {
    if !repo_path.exists() {
        return DeploymentCheck::failed(
            "repo_clean",
            format!("Repository path does not exist: {:?}", repo_path),
        );
    }

    match std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["status", "--porcelain"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    DeploymentCheck::passed_with_details(
                        "repo_clean",
                        "No uncommitted changes detected",
                    )
                } else {
                    let lines: Vec<&str> = stdout.lines().take(5).collect();
                    DeploymentCheck::failed(
                        "repo_clean",
                        format!(
                            "Repository has uncommitted changes: {}{}",
                            lines.join(", "),
                            if stdout.lines().count() > 5 {
                                "..."
                            } else {
                                ""
                            }
                        ),
                    )
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                DeploymentCheck::failed(
                    "repo_clean",
                    format!("git status failed: {}", stderr.trim()),
                )
            }
        }
        Err(e) => DeploymentCheck::failed("repo_clean", format!("Failed to run git: {}", e)),
    }
}

/// Check if a manifest hash matches the expected value
///
/// # Arguments
///
/// * `actual_hash` - The actual manifest hash
/// * `expected_hash` - The expected manifest hash
///
/// # Returns
///
/// Returns a check result indicating if hashes match.
pub fn check_manifest_hash(actual_hash: Option<&str>, expected_hash: &str) -> DeploymentCheck {
    match actual_hash {
        Some(actual) => {
            if actual == expected_hash {
                DeploymentCheck::passed_with_details(
                    "manifest_hash",
                    format!(
                        "Hash matches: {}",
                        &expected_hash[..16.min(expected_hash.len())]
                    ),
                )
            } else {
                DeploymentCheck::failed(
                    "manifest_hash",
                    format!(
                        "Hash mismatch: expected {}, got {}",
                        &expected_hash[..16.min(expected_hash.len())],
                        &actual[..16.min(actual.len())]
                    ),
                )
            }
        }
        None => {
            DeploymentCheck::failed("manifest_hash", "No manifest hash available for comparison")
        }
    }
}

/// Check if CoreML package hash matches (for static deployment verification)
///
/// # Arguments
///
/// * `actual_hash` - The actual CoreML package hash
/// * `expected_hash` - The expected CoreML package hash
///
/// # Returns
///
/// Returns a check result indicating if hashes match.
pub fn check_coreml_hash(
    actual_hash: Option<&str>,
    expected_hash: Option<&str>,
) -> DeploymentCheck {
    match (actual_hash, expected_hash) {
        (Some(actual), Some(expected)) => {
            if actual == expected {
                DeploymentCheck::passed_with_details(
                    "coreml_hash",
                    format!(
                        "CoreML hash matches: {}",
                        &expected[..16.min(expected.len())]
                    ),
                )
            } else {
                DeploymentCheck::failed(
                    "coreml_hash",
                    format!(
                        "CoreML hash mismatch: expected {}, got {}",
                        &expected[..16.min(expected.len())],
                        &actual[..16.min(actual.len())]
                    ),
                )
            }
        }
        (None, Some(expected)) => DeploymentCheck::failed(
            "coreml_hash",
            format!(
                "Expected CoreML hash {} but adapter has none",
                &expected[..16.min(expected.len())]
            ),
        ),
        (Some(_), None) => {
            DeploymentCheck::skipped("coreml_hash", "No expected CoreML hash to verify against")
        }
        (None, None) => DeploymentCheck::skipped("coreml_hash", "CoreML package not configured"),
    }
}

/// Check that no session conflict exists
///
/// # Arguments
///
/// * `adapter_session_id` - The session ID bound to the adapter
/// * `current_session_id` - The current session ID
///
/// # Returns
///
/// Returns a check result indicating if there's a session conflict.
pub fn check_no_session_conflict(
    adapter_session_id: Option<&str>,
    current_session_id: Option<&str>,
) -> DeploymentCheck {
    match (adapter_session_id, current_session_id) {
        (Some(adapter_sess), Some(current_sess)) => {
            if adapter_sess == current_sess {
                DeploymentCheck::passed_with_details(
                    "session_conflict",
                    format!("Adapter bound to current session: {}", current_sess),
                )
            } else {
                DeploymentCheck::failed(
                    "session_conflict",
                    format!(
                        "Adapter bound to different session: {} (current: {})",
                        adapter_sess, current_sess
                    ),
                )
            }
        }
        (Some(adapter_sess), None) => DeploymentCheck::failed(
            "session_conflict",
            format!(
                "Adapter bound to session {} but no current session provided",
                adapter_sess
            ),
        ),
        (None, Some(current_sess)) => DeploymentCheck::passed_with_details(
            "session_conflict",
            format!("Adapter not bound, current session: {}", current_sess),
        ),
        (None, None) => DeploymentCheck::skipped("session_conflict", "No session context"),
    }
}

/// Adapter state snapshot for verification
///
/// Contains the minimal fields needed for deployment verification.
pub struct AdapterVerificationState {
    pub adapter_type: Option<String>,
    pub manifest_hash: Option<String>,
    pub coreml_package_hash: Option<String>,
    pub stream_session_id: Option<String>,
    pub repo_path: Option<String>,
}

/// Run full deployment verification for a codebase adapter
///
/// # Arguments
///
/// * `adapter` - Adapter state to verify
/// * `repo_path` - Optional override for repo path check
/// * `expected_manifest_hash` - Expected manifest hash (uses adapter's if None)
/// * `expected_coreml_hash` - Expected CoreML hash for verification
/// * `current_session_id` - Current session ID for conflict check
///
/// # Returns
///
/// Returns a complete verification result.
pub fn verify_codebase_deployment(
    adapter: &AdapterVerificationState,
    repo_path: Option<&Path>,
    expected_manifest_hash: Option<&str>,
    expected_coreml_hash: Option<&str>,
    current_session_id: Option<&str>,
) -> DeploymentCheckResult {
    let mut result = DeploymentCheckResult::new();

    // Only verify codebase adapters
    if adapter.adapter_type.as_deref() != Some("codebase") {
        result.add_check(DeploymentCheck::skipped(
            "codebase_check",
            "Not a codebase adapter",
        ));
        return result;
    }

    // Check 1: Repository clean state
    if let Some(path) = repo_path.or(adapter.repo_path.as_ref().map(Path::new)) {
        result.add_check(check_repo_clean(path));
    } else {
        result.add_check(DeploymentCheck::skipped(
            "repo_clean",
            "No repo path available",
        ));
    }

    // Check 2: Manifest hash
    if let Some(expected) = expected_manifest_hash {
        result.add_check(check_manifest_hash(
            adapter.manifest_hash.as_deref(),
            expected,
        ));
    } else if adapter.manifest_hash.is_some() {
        result.add_check(DeploymentCheck::skipped(
            "manifest_hash",
            "No expected hash to verify against",
        ));
    } else {
        result.add_check(DeploymentCheck::skipped(
            "manifest_hash",
            "No manifest hash configured",
        ));
    }

    // Check 3: CoreML package hash
    result.add_check(check_coreml_hash(
        adapter.coreml_package_hash.as_deref(),
        expected_coreml_hash,
    ));

    // Check 4: Session conflict
    result.add_check(check_no_session_conflict(
        adapter.stream_session_id.as_deref(),
        current_session_id,
    ));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_check_passed() {
        let check = DeploymentCheck::passed("test");
        assert!(check.passed);
        assert!(check.details.is_none());
    }

    #[test]
    fn test_deployment_check_failed() {
        let check = DeploymentCheck::failed("test", "reason");
        assert!(!check.passed);
        assert_eq!(check.details, Some("reason".to_string()));
    }

    #[test]
    fn test_deployment_check_skipped() {
        let check = DeploymentCheck::skipped("test", "why");
        assert!(check.passed); // Skipped counts as passed
        assert!(check.details.unwrap().contains("Skipped"));
    }

    #[test]
    fn test_deployment_result_all_passed() {
        let mut result = DeploymentCheckResult::new();
        result.add_check(DeploymentCheck::passed("check1"));
        result.add_check(DeploymentCheck::passed("check2"));
        assert!(result.all_passed());
        assert_eq!(result.failures().len(), 0);
    }

    #[test]
    fn test_deployment_result_with_failure() {
        let mut result = DeploymentCheckResult::new();
        result.add_check(DeploymentCheck::passed("check1"));
        result.add_check(DeploymentCheck::failed("check2", "failed"));
        assert!(!result.all_passed());
        assert_eq!(result.failures().len(), 1);
    }

    #[test]
    fn test_manifest_hash_check_match() {
        let check = check_manifest_hash(Some("abc123"), "abc123");
        assert!(check.passed);
    }

    #[test]
    fn test_manifest_hash_check_mismatch() {
        let check = check_manifest_hash(Some("abc123"), "xyz789");
        assert!(!check.passed);
    }

    #[test]
    fn test_session_conflict_check_match() {
        let check = check_no_session_conflict(Some("sess-1"), Some("sess-1"));
        assert!(check.passed);
    }

    #[test]
    fn test_session_conflict_check_mismatch() {
        let check = check_no_session_conflict(Some("sess-1"), Some("sess-2"));
        assert!(!check.passed);
    }

    #[test]
    fn test_verify_non_codebase_skips() {
        let adapter = AdapterVerificationState {
            adapter_type: Some("standard".to_string()),
            manifest_hash: None,
            coreml_package_hash: None,
            stream_session_id: None,
            repo_path: None,
        };

        let result = verify_codebase_deployment(&adapter, None, None, None, None);
        assert!(result.all_passed());
        assert_eq!(result.checks.len(), 1);
        assert!(result.checks[0]
            .details
            .as_ref()
            .unwrap()
            .contains("Not a codebase"));
    }
}
