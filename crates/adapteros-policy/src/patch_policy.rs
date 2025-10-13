//! Complete policy engine for patch validation
//!
//! Integrates security validation, test execution, and linter checks
//! into a comprehensive patch validation pipeline.

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Comprehensive patch policy engine
#[derive(Debug)]
pub struct PatchPolicyEngine {
    code_policy: CodePolicy,
}

/// Code policy configuration (simplified version for policy engine)
#[derive(Debug, Clone)]
pub struct CodePolicy {
    pub detect_secrets: bool,
    pub allowed_paths: Vec<String>,
    pub forbidden_operations: Vec<String>,
    pub max_patch_size: usize,
}

impl Default for CodePolicy {
    fn default() -> Self {
        Self {
            detect_secrets: true,
            allowed_paths: vec![],
            forbidden_operations: vec![
                "exec".to_string(),
                "eval".to_string(),
                "shell_escape".to_string(),
            ],
            max_patch_size: 100000,
        }
    }
}

/// Comprehensive validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComprehensiveValidation {
    /// Security validation results
    pub security: SecurityValidation,
    /// Test execution results (if enabled)
    pub tests: Option<TestValidation>,
    /// Linter results (if enabled)
    pub lints: Option<LintValidation>,
    /// Overall pass/fail status
    pub overall_passed: bool,
    /// Aggregate score (0.0 - 1.0)
    pub score: f32,
}

/// Security validation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityValidation {
    pub passed: bool,
    pub violations: Vec<SecurityViolation>,
    pub warnings: Vec<String>,
}

/// Security violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityViolation {
    pub violation_type: String,
    pub severity: String,
    pub file_path: String,
    pub line_number: Option<usize>,
    pub description: String,
}

/// Test validation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestValidation {
    pub passed: bool,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub failures: Vec<String>,
}

/// Lint validation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintValidation {
    pub passed: bool,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub critical_issues: Vec<String>,
}

impl PatchPolicyEngine {
    /// Create a new policy engine with default settings
    pub fn new(code_policy: CodePolicy) -> Self {
        Self { code_policy }
    }

    /// Validate patches comprehensively (security only)
    ///
    /// For test execution and linter integration, use the separate
    /// TestExecutor and LinterRunner from mplora-worker and aggregate
    /// results manually using `aggregate_with_external_results`.
    pub async fn validate_comprehensive(
        &self,
        patches: &[FilePatch],
        _repo_path: &Path,
    ) -> Result<ComprehensiveValidation> {
        info!("Starting comprehensive patch validation");

        // 1. Security validation (always runs)
        let security = self.validate_security(patches).await?;
        info!(
            "Security validation: {}",
            if security.passed { "PASS" } else { "FAIL" }
        );

        // 2. Test execution and linter checks are handled externally
        // Users should integrate TestExecutor and LinterRunner from mplora-worker
        let tests = None;
        let lints = None;

        // 3. Aggregate results
        let overall_passed = self.aggregate_results(&security, &tests, &lints);
        let score = self.calculate_score(&security, &tests, &lints);

        info!(
            "Overall validation: {} (score: {:.2})",
            if overall_passed { "PASS" } else { "FAIL" },
            score
        );

        Ok(ComprehensiveValidation {
            security,
            tests,
            lints,
            overall_passed,
            score,
        })
    }

    /// Aggregate results with external test and lint results
    ///
    /// Use this method when you've run TestExecutor and LinterRunner separately
    /// and want to combine them with security validation.
    pub fn aggregate_with_external_results(
        &self,
        security: SecurityValidation,
        tests: Option<TestValidation>,
        lints: Option<LintValidation>,
    ) -> ComprehensiveValidation {
        let overall_passed = self.aggregate_results(&security, &tests, &lints);
        let score = self.calculate_score(&security, &tests, &lints);

        ComprehensiveValidation {
            security,
            tests,
            lints,
            overall_passed,
            score,
        }
    }

    /// Validate security aspects of patches
    async fn validate_security(&self, patches: &[FilePatch]) -> Result<SecurityValidation> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        for patch in patches {
            // Secret detection
            if self.code_policy.detect_secrets && self.contains_secrets(&patch.new_content) {
                violations.push(SecurityViolation {
                    violation_type: "secret_detected".to_string(),
                    severity: "critical".to_string(),
                    file_path: patch.file_path.clone(),
                    line_number: None,
                    description: "Potential secret or credential detected in patch".to_string(),
                });
            }

            // Path restrictions
            if !self.is_path_allowed(&patch.file_path) {
                violations.push(SecurityViolation {
                    violation_type: "forbidden_path".to_string(),
                    severity: "high".to_string(),
                    file_path: patch.file_path.clone(),
                    line_number: None,
                    description: "Patch modifies restricted file path".to_string(),
                });
            }

            // Forbidden operations
            if self.contains_forbidden_operations(&patch.new_content) {
                violations.push(SecurityViolation {
                    violation_type: "forbidden_operation".to_string(),
                    severity: "high".to_string(),
                    file_path: patch.file_path.clone(),
                    line_number: None,
                    description: "Patch contains forbidden operations (e.g., exec, eval)"
                        .to_string(),
                });
            }

            // Size limits
            if patch.new_content.len() > self.code_policy.max_patch_size {
                warnings.push(format!("Patch {} exceeds size limit", patch.file_path));
            }
        }

        Ok(SecurityValidation {
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }

    /// Aggregate validation results
    fn aggregate_results(
        &self,
        security: &SecurityValidation,
        tests: &Option<TestValidation>,
        lints: &Option<LintValidation>,
    ) -> bool {
        // Security must always pass
        if !security.passed {
            return false;
        }

        // Tests must pass if enabled
        if let Some(test_result) = tests {
            if !test_result.passed {
                return false;
            }
        }

        // Lints must pass (no errors) if enabled
        if let Some(lint_result) = lints {
            if !lint_result.passed {
                return false;
            }
        }

        true
    }

    /// Calculate aggregate score
    fn calculate_score(
        &self,
        security: &SecurityValidation,
        tests: &Option<TestValidation>,
        lints: &Option<LintValidation>,
    ) -> f32 {
        let mut score = 0.0;
        let mut weight_sum = 0.0;

        // Security score (weight: 0.5)
        let security_score = if security.passed { 1.0 } else { 0.0 };
        score += security_score * 0.5;
        weight_sum += 0.5;

        // Test score (weight: 0.3)
        if let Some(test_result) = tests {
            let test_score = if test_result.total_tests > 0 {
                test_result.passed_tests as f32 / test_result.total_tests as f32
            } else {
                1.0
            };
            score += test_score * 0.3;
            weight_sum += 0.3;
        }

        // Lint score (weight: 0.2)
        if let Some(lint_result) = lints {
            let lint_score = if lint_result.passed { 1.0 } else { 0.0 };
            score += lint_score * 0.2;
            weight_sum += 0.2;
        }

        if weight_sum > 0.0 {
            score / weight_sum
        } else {
            0.0
        }
    }

    /// Check if content contains secrets
    fn contains_secrets(&self, content: &str) -> bool {
        // Simple pattern matching for common secret patterns
        let secret_patterns = [
            r"(?i)api[_-]?key",
            r"(?i)secret[_-]?key",
            r"(?i)password\s*=",
            r"(?i)token\s*=",
            r"-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----",
            r"[A-Za-z0-9+/]{40,}==?", // Base64-like strings
        ];

        for pattern in &secret_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(content) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if path is allowed by policy
    fn is_path_allowed(&self, path: &str) -> bool {
        // Forbidden paths
        let forbidden_prefixes = [
            ".git/",
            ".env",
            "secrets/",
            "credentials/",
            "/etc/",
            "/sys/",
            "/proc/",
        ];

        for prefix in &forbidden_prefixes {
            if path.starts_with(prefix) {
                return false;
            }
        }

        // Check against policy allowed paths
        if self.code_policy.allowed_paths.is_empty() {
            return true;
        }

        self.code_policy
            .allowed_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }

    /// Check for forbidden operations
    fn contains_forbidden_operations(&self, content: &str) -> bool {
        let forbidden = &self.code_policy.forbidden_operations;

        for op in forbidden {
            if content.contains(op) {
                return true;
            }
        }

        false
    }
}

/// File patch for validation
#[derive(Debug, Clone)]
pub struct FilePatch {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_secrets() {
        let engine = PatchPolicyEngine::new(CodePolicy::default());

        assert!(engine.contains_secrets("api_key = \"secret123\""));
        assert!(engine.contains_secrets("password = \"pass123\""));
        assert!(!engine.contains_secrets("fn test() {}"));
    }

    #[test]
    fn test_is_path_allowed() {
        let mut policy = CodePolicy::default();
        policy.allowed_paths = vec!["src/".to_string(), "tests/".to_string()];

        let engine = PatchPolicyEngine::new(policy);

        assert!(engine.is_path_allowed("src/main.rs"));
        assert!(engine.is_path_allowed("tests/test.rs"));
        assert!(!engine.is_path_allowed(".git/config"));
        assert!(!engine.is_path_allowed(".env"));
    }

    #[test]
    fn test_contains_forbidden_operations() {
        let mut policy = CodePolicy::default();
        policy.forbidden_operations = vec!["exec".to_string(), "eval".to_string()];

        let engine = PatchPolicyEngine::new(policy);

        assert!(engine.contains_forbidden_operations("exec('ls')"));
        assert!(engine.contains_forbidden_operations("eval(code)"));
        assert!(!engine.contains_forbidden_operations("fn safe() {}"));
    }

    #[tokio::test]
    async fn test_validate_security() {
        let policy = CodePolicy {
            detect_secrets: true,
            allowed_paths: vec!["src/".to_string()],
            forbidden_operations: vec!["exec".to_string()],
            max_patch_size: 10000,
        };

        let engine = PatchPolicyEngine::new(policy);

        let patches = vec![FilePatch {
            file_path: "src/main.rs".to_string(),
            old_content: "".to_string(),
            new_content: "fn main() {}".to_string(),
        }];

        let result = engine.validate_security(&patches).await.unwrap();
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_validate_security_with_secret() {
        let policy = CodePolicy::default();
        let engine = PatchPolicyEngine::new(policy);

        let patches = vec![FilePatch {
            file_path: "src/config.rs".to_string(),
            old_content: "".to_string(),
            new_content: "const API_KEY = \"secret123\";".to_string(),
        }];

        let result = engine.validate_security(&patches).await.unwrap();
        assert!(!result.passed);
        assert!(!result.violations.is_empty());
    }
}
