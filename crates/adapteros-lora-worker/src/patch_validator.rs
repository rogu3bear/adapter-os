//! Policy validation and security checks for patch proposals
//!
//! Implements comprehensive validation against Code Policy requirements:
//! - Path restrictions and permissions
//! - Secret detection and scanning
//! - Forbidden operations detection
//! - Dependency policy enforcement
//! - Patch size limits
//! - Review requirements
//!
//! Aligns with Code Policy Rules #2-8 and security requirements.

use crate::patch_generator::FilePatch;
use adapteros_core::Result;
use adapteros_policy::PolicyEngine;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Policy validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub confidence: f32,
    pub violations: Vec<PolicyViolation>,
}

/// Policy violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolation {
    pub violation_type: ViolationType,
    pub severity: ViolationSeverity,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
    pub description: String,
    pub hint: Option<String>,
}

/// Types of policy violations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    PathDenied,
    SecretDetected,
    ForbiddenOperation,
    DependencyBlocked,
    SizeExceeded,
    ReviewRequired,
    InsufficientEvidence,
}

/// Violation severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Code policy configuration
#[derive(Debug, Clone)]
pub struct CodePolicy {
    pub min_evidence_spans: usize,
    pub allow_auto_apply: bool,
    pub test_coverage_min: f32,
    pub path_allowlist: Vec<String>,
    pub path_denylist: Vec<String>,
    pub secret_patterns: Vec<String>,
    pub max_patch_size_lines: usize,
    pub forbidden_operations: Vec<String>,
    pub allow_external_deps: bool,
    pub require_review: ReviewRequirements,
}

/// Review requirements for different change types
#[derive(Debug, Clone)]
pub struct ReviewRequirements {
    pub database_migrations: bool,
    pub security_changes: bool,
    pub config_changes: bool,
}

impl Default for CodePolicy {
    fn default() -> Self {
        Self {
            min_evidence_spans: 1,
            allow_auto_apply: false,
            test_coverage_min: 0.8,
            path_allowlist: vec![
                "src/**".to_string(),
                "lib/**".to_string(),
                "tests/**".to_string(),
            ],
            path_denylist: vec![
                "**/.env*".to_string(),
                "**/secrets/**".to_string(),
                "**/*.pem".to_string(),
            ],
            secret_patterns: vec![
                r"(?i)(api[_-]?key|password|secret|token)\\s*[:=]\\s*.{8,}".to_string(),
                r"(?i)(aws[_-]?access[_-]?key|aws[_-]?secret)".to_string(),
                r"-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----".to_string(),
            ],
            max_patch_size_lines: 500,
            forbidden_operations: vec![
                "shell_escape".to_string(),
                "eval".to_string(),
                "exec_raw".to_string(),
                "unsafe_deserialization".to_string(),
            ],
            allow_external_deps: false,
            require_review: ReviewRequirements {
                database_migrations: true,
                security_changes: true,
                config_changes: true,
            },
        }
    }
}

/// Comprehensive patch validator
pub struct PatchValidator {
    policy: CodePolicy,
    secret_scanner: SecretScanner,
    path_validator: PathValidator,
    dependency_checker: DependencyChecker,
    operation_detector: OperationDetector,
    policy_engine: PolicyEngine,
}

impl PatchValidator {
    pub fn new(policy: CodePolicy, policy_engine: PolicyEngine) -> Self {
        Self {
            secret_scanner: SecretScanner::new(policy.secret_patterns.clone()),
            path_validator: PathValidator::new(
                policy.path_allowlist.clone(),
                policy.path_denylist.clone(),
            ),
            dependency_checker: DependencyChecker::new(policy.allow_external_deps),
            operation_detector: OperationDetector::new(policy.forbidden_operations.clone()),
            policy_engine,
            policy,
        }
    }

    /// Validate patches against all policy requirements
    pub async fn validate(&self, patches: &[FilePatch]) -> Result<ValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut violations = Vec::new();

        info!("Validating {} patches against policy", patches.len());

        // 0. Global policy engine validation
        self.validate_with_policy_engine(patches, &mut errors, &mut warnings, &mut violations)?;

        for patch in patches {
            // 1. Path restrictions (Rule #2)
            if let Err(violation) = self.path_validator.validate_paths(patch) {
                errors.push(format!("Path validation failed: {}", violation.description));
                violations.push(violation);
            }

            // 2. Secret detection (Rule #3)
            if let Ok(secrets) = self.secret_scanner.scan_patch(patch) {
                for secret in secrets {
                    errors.push(format!("Secret detected: {}", secret.description));
                    violations.push(secret);
                }
            }

            // 3. Forbidden operations (Rule #4)
            if let Ok(ops) = self.operation_detector.detect_forbidden_operations(patch) {
                for op in ops {
                    errors.push(format!("Forbidden operation: {}", op.description));
                    violations.push(op);
                }
            }

            // 4. Patch size limits (Rule #6)
            if patch.total_lines > self.policy.max_patch_size_lines {
                let violation = PolicyViolation {
                    violation_type: ViolationType::SizeExceeded,
                    severity: ViolationSeverity::Medium,
                    file_path: Some(patch.file_path.clone()),
                    line_number: None,
                    description: format!(
                        "Patch size {} exceeds limit {}",
                        patch.total_lines, self.policy.max_patch_size_lines
                    ),
                    hint: Some("Consider breaking into smaller patches".to_string()),
                };
                warnings.push(violation.description.clone());
                violations.push(violation);
            }

            // 5. Dependency policy (Rule #7)
            if let Err(violation) = self.dependency_checker.check_dependencies(patch) {
                errors.push(format!(
                    "Dependency check failed: {}",
                    violation.description
                ));
                violations.push(violation);
            }

            // 6. Review requirements (Rule #8)
            if let Ok(requires_review) = self.requires_review(patch) {
                if requires_review {
                    let violation = PolicyViolation {
                        violation_type: ViolationType::ReviewRequired,
                        severity: ViolationSeverity::High,
                        file_path: Some(patch.file_path.clone()),
                        line_number: None,
                        description: "Manual review required for this change type".to_string(),
                        hint: Some("Contact admin for review approval".to_string()),
                    };
                    warnings.push(violation.description.clone());
                    violations.push(violation);
                }
            }
        }

        let is_valid = errors.is_empty();
        let confidence = if is_valid {
            if warnings.is_empty() {
                1.0
            } else {
                0.8
            }
        } else {
            0.0
        };

        info!(
            "Validation complete: valid={}, errors={}, warnings={}, confidence={:.3}",
            is_valid,
            errors.len(),
            warnings.len(),
            confidence
        );

        Ok(ValidationResult {
            is_valid,
            errors,
            warnings,
            confidence,
            violations,
        })
    }

    /// Validate patches using the global policy engine
    fn validate_with_policy_engine(
        &self,
        patches: &[FilePatch],
        errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
        violations: &mut Vec<PolicyViolation>,
    ) -> Result<()> {
        // Check total patch size against resource limits
        let total_lines: usize = patches.iter().map(|p| p.total_lines).sum();
        if let Err(e) = self.policy_engine.check_resource_limits(total_lines) {
            errors.push(format!("Resource limit exceeded: {}", e));
            violations.push(PolicyViolation {
                violation_type: ViolationType::SizeExceeded,
                severity: ViolationSeverity::Critical,
                file_path: None,
                line_number: None,
                description: format!("Total patch size {} exceeds resource limits", total_lines),
                hint: Some("Reduce patch size or split into smaller patches".to_string()),
            });
        }

        // Check for numeric claims without units (Numeric Ruleset #6)
        for patch in patches {
            for hunk in &patch.hunks {
                if let crate::patch_generator::HunkType::Modification = hunk.hunk_type {
                    for line in &hunk.modified_lines {
                        if self.contains_numeric_claim(line) && !self.has_units(line) {
                            warnings.push(format!(
                                "Numeric claim without units in {}: {}",
                                patch.file_path, line
                            ));
                            violations.push(PolicyViolation {
                                violation_type: ViolationType::ForbiddenOperation,
                                severity: ViolationSeverity::Medium,
                                file_path: Some(patch.file_path.clone()),
                                line_number: Some(hunk.start_line),
                                description: format!("Numeric claim without units: {}", line),
                                hint: Some(
                                    "Include units in numeric values per Numeric Ruleset #6"
                                        .to_string(),
                                ),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if line contains numeric claims
    fn contains_numeric_claim(&self, line: &str) -> bool {
        // Simple heuristic for numeric claims
        line.contains("=") && line.chars().any(|c| c.is_ascii_digit())
    }

    /// Check if line has units
    fn has_units(&self, line: &str) -> bool {
        // Check for common units
        let units = [
            "px",
            "em",
            "rem",
            "%",
            "ms",
            "s",
            "mb",
            "gb",
            "kb",
            "bytes",
            "bytes/sec",
            "req/sec",
        ];
        units.iter().any(|unit| line.to_lowercase().contains(unit))
    }

    /// Check if patch requires manual review
    fn requires_review(&self, patch: &FilePatch) -> Result<bool> {
        // Database migrations
        if self.policy.require_review.database_migrations {
            if self.is_migration_file(&patch.file_path) {
                return Ok(true);
            }
        }

        // Security changes
        if self.policy.require_review.security_changes {
            if self.is_security_file(&patch.file_path) {
                return Ok(true);
            }
        }

        // Config changes
        if self.policy.require_review.config_changes {
            if self.is_config_file(&patch.file_path) && self.is_production_config(&patch.file_path)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if file is a database migration
    fn is_migration_file(&self, file_path: &str) -> bool {
        file_path.contains("migrations/") || file_path.ends_with(".sql")
    }

    /// Check if file is security-related
    fn is_security_file(&self, file_path: &str) -> bool {
        file_path.contains("auth")
            || file_path.contains("security")
            || file_path.contains("crypto")
            || file_path.contains("permissions")
    }

    /// Check if file is a configuration file
    fn is_config_file(&self, file_path: &str) -> bool {
        file_path.ends_with(".toml")
            || file_path.ends_with(".yaml")
            || file_path.ends_with(".yml")
            || file_path.ends_with(".json")
            || file_path.ends_with(".env")
    }

    /// Check if config file is for production
    fn is_production_config(&self, file_path: &str) -> bool {
        file_path.contains("production")
            || file_path.contains("prod")
            || file_path.contains("deploy")
    }
}

/// Secret scanner for detecting sensitive information
pub struct SecretScanner {
    patterns: Vec<Regex>,
}

impl SecretScanner {
    pub fn new(patterns: Vec<String>) -> Self {
        let compiled_patterns: Vec<Regex> = patterns
            .into_iter()
            .filter_map(|p| Regex::new(&p).ok())
            .collect();

        Self {
            patterns: compiled_patterns,
        }
    }

    /// Scan patch for secrets
    pub fn scan_patch(&self, patch: &FilePatch) -> Result<Vec<PolicyViolation>> {
        let mut violations = Vec::new();

        for hunk in &patch.hunks {
            for (line_idx, line) in hunk.modified_lines.iter().enumerate() {
                for pattern in &self.patterns {
                    if pattern.is_match(line) {
                        let violation = PolicyViolation {
                            violation_type: ViolationType::SecretDetected,
                            severity: ViolationSeverity::Critical,
                            file_path: Some(patch.file_path.clone()),
                            line_number: Some(hunk.start_line + line_idx),
                            description: format!("Secret pattern detected: {}", pattern.as_str()),
                            hint: Some("Remove or mask sensitive information".to_string()),
                        };
                        violations.push(violation);
                    }
                }
            }
        }

        Ok(violations)
    }
}

/// Path validator for file permissions
pub struct PathValidator {
    allowlist: Vec<String>,
    denylist: Vec<String>,
}

impl PathValidator {
    pub fn new(allowlist: Vec<String>, denylist: Vec<String>) -> Self {
        Self {
            allowlist,
            denylist,
        }
    }

    /// Validate file paths against policy
    pub fn validate_paths(&self, patch: &FilePatch) -> std::result::Result<(), PolicyViolation> {
        // Check denylist first (higher priority)
        for pattern in &self.denylist {
            if self.matches_pattern(pattern, &patch.file_path) {
                return Err(PolicyViolation {
                    violation_type: ViolationType::PathDenied,
                    severity: ViolationSeverity::Critical,
                    file_path: Some(patch.file_path.clone()),
                    line_number: None,
                    description: format!(
                        "File {} matches denylist pattern {}",
                        patch.file_path, pattern
                    ),
                    hint: Some("File is explicitly denied for modification".to_string()),
                });
            }
        }

        // Check allowlist
        let mut allowed = false;
        for pattern in &self.allowlist {
            if self.matches_pattern(pattern, &patch.file_path) {
                allowed = true;
                break;
            }
        }

        if !allowed {
            return Err(PolicyViolation {
                violation_type: ViolationType::PathDenied,
                severity: ViolationSeverity::High,
                file_path: Some(patch.file_path.clone()),
                line_number: None,
                description: format!("File {} not in allowlist", patch.file_path),
                hint: Some("Add file to allowlist or modify allowed paths".to_string()),
            });
        }

        Ok(())
    }

    /// Check if path matches glob pattern (simplified implementation)
    fn matches_pattern(&self, pattern: &str, path: &str) -> bool {
        // Simplified glob matching - in real implementation would use proper glob library
        if pattern.contains("**") {
            let prefix = pattern.split("**").next().unwrap_or("");
            path.starts_with(prefix)
        } else if pattern.ends_with("*") {
            let prefix = pattern.trim_end_matches('*');
            path.starts_with(prefix)
        } else {
            path == pattern
        }
    }
}

/// Dependency checker for external dependencies
pub struct DependencyChecker {
    allow_external_deps: bool,
}

impl DependencyChecker {
    pub fn new(allow_external_deps: bool) -> Self {
        Self {
            allow_external_deps,
        }
    }

    /// Check for new external dependencies
    pub fn check_dependencies(
        &self,
        patch: &FilePatch,
    ) -> std::result::Result<(), PolicyViolation> {
        if self.allow_external_deps {
            return Ok(());
        }

        let new_deps = self.detect_new_dependencies(patch);
        if !new_deps.is_empty() {
            return Err(PolicyViolation {
                violation_type: ViolationType::DependencyBlocked,
                severity: ViolationSeverity::High,
                file_path: Some(patch.file_path.clone()),
                line_number: None,
                description: format!("External dependencies detected: {:?}", new_deps),
                hint: Some("Contact admin to request approval".to_string()),
            });
        }

        Ok(())
    }

    /// Detect new dependencies in patch
    fn detect_new_dependencies(&self, patch: &FilePatch) -> Vec<String> {
        let mut deps = Vec::new();

        for hunk in &patch.hunks {
            for line in &hunk.modified_lines {
                // Look for common dependency patterns
                if line.contains("use ") && line.contains("::") {
                    if let Some(dep) = self.extract_dependency(line) {
                        deps.push(dep);
                    }
                }
                if line.contains("import ") && line.contains("from ") {
                    if let Some(dep) = self.extract_python_dependency(line) {
                        deps.push(dep);
                    }
                }
            }
        }

        deps
    }

    /// Extract Rust dependency from use statement
    fn extract_dependency(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find("use ") {
            let after_use = &line[start + 4..];
            if let Some(end) = after_use.find("::") {
                Some(after_use[..end].trim().to_string())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Extract Python dependency from import statement
    fn extract_python_dependency(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find("from ") {
            let after_from = &line[start + 5..];
            if let Some(end) = after_from.find(" import") {
                Some(after_from[..end].trim().to_string())
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Operation detector for forbidden operations
pub struct OperationDetector {
    forbidden_ops: Vec<String>,
}

impl OperationDetector {
    pub fn new(forbidden_ops: Vec<String>) -> Self {
        Self { forbidden_ops }
    }

    /// Detect forbidden operations in patch
    pub fn detect_forbidden_operations(&self, patch: &FilePatch) -> Result<Vec<PolicyViolation>> {
        let mut violations = Vec::new();

        for hunk in &patch.hunks {
            for (line_idx, line) in hunk.modified_lines.iter().enumerate() {
                for op in &self.forbidden_ops {
                    if line.contains(op) {
                        let violation = PolicyViolation {
                            violation_type: ViolationType::ForbiddenOperation,
                            severity: ViolationSeverity::Critical,
                            file_path: Some(patch.file_path.clone()),
                            line_number: Some(hunk.start_line + line_idx),
                            description: format!("Forbidden operation detected: {}", op),
                            hint: Some("Use safer alternatives or request approval".to_string()),
                        };
                        violations.push(violation);
                    }
                }
            }
        }

        Ok(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch_generator::{HunkType, PatchHunk};
    use adapteros_core::B3Hash;
    use adapteros_manifest::{
        ArtifactsPolicy, DeterminismPolicy, EgressPolicy, EvidencePolicy, IsolationPolicy,
        MemoryPolicy, NumericPolicy, PerformancePolicy, Policies, RagPolicy, RefusalPolicy,
    };

    fn create_mock_policies() -> Policies {
        Policies {
            egress: EgressPolicy {
                mode: "deny_all".to_string(),
                serve_requires_pf: true,
                allow_tcp: false,
                allow_udp: false,
                uds_paths: vec!["/var/run/aos/*.sock".to_string()],
            },
            determinism: DeterminismPolicy {
                require_metallib_embed: true,
                require_kernel_hash_match: true,
                rng: "hkdf_seeded".to_string(),
                retrieval_tie_break: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
            },
            evidence: EvidencePolicy {
                require_open_book: true,
                min_spans: 1,
                prefer_latest_revision: true,
                warn_on_superseded: true,
            },
            refusal: RefusalPolicy {
                abstain_threshold: 0.55,
                missing_fields_templates: std::collections::HashMap::new(),
            },
            numeric: NumericPolicy {
                canonical_units: std::collections::HashMap::new(),
                max_rounding_error: 0.5,
                require_units_in_trace: true,
            },
            rag: RagPolicy {
                index_scope: "per_tenant".to_string(),
                doc_tags_required: vec!["doc_id".to_string(), "rev".to_string()],
                embedding_model_hash: B3Hash::hash(b"test"),
                topk: 5,
                order: vec!["score_desc".to_string(), "doc_id_asc".to_string()],
            },
            isolation: IsolationPolicy {
                process_model: "per_tenant".to_string(),
                uds_root: "/var/run/aos".to_string(),
                forbid_shm: true,
            },
            performance: PerformancePolicy {
                latency_p95_ms: 24,
                router_overhead_pct_max: 8,
                throughput_tokens_per_s_min: 40,
            },
            memory: MemoryPolicy {
                min_headroom_pct: 15u8,
                evict_order: vec!["ephemeral_ttl".to_string()],
                k_reduce_before_evict: true,
            },
            artifacts: ArtifactsPolicy {
                require_signature: true,
                require_sbom: true,
                cas_only: true,
            },
        }
    }

    fn create_test_patch() -> FilePatch {
        FilePatch {
            file_path: "src/test.rs".to_string(),
            hunks: vec![PatchHunk {
                start_line: 1,
                end_line: 5,
                context_lines: vec!["// Test file".to_string()],
                modified_lines: vec!["use std::collections::HashMap;".to_string()],
                hunk_type: HunkType::Addition,
            }],
            total_lines: 1,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_patch_validation_success() {
        use adapteros_policy::PolicyEngine;

        let policy = CodePolicy::default();
        let policies = create_mock_policies();
        let policy_engine = PolicyEngine::new(policies);
        let validator = PatchValidator::new(policy, policy_engine);
        let patch = create_test_patch();

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert!(result.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_path_validation_denied() {
        use adapteros_policy::PolicyEngine;

        let mut policy = CodePolicy::default();
        policy.path_allowlist = vec!["src/**".to_string()];
        policy.path_denylist = vec!["src/test.rs".to_string()];

        let policies = create_mock_policies();
        let policy_engine = PolicyEngine::new(policies);
        let validator = PatchValidator::new(policy, policy_engine);
        let patch = create_test_patch();

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, ViolationType::PathDenied)));
    }

    #[tokio::test]
    async fn test_secret_detection() {
        use adapteros_policy::PolicyEngine;

        let policy = CodePolicy::default();
        let policies = create_mock_policies();
        let policy_engine = PolicyEngine::new(policies);
        let validator = PatchValidator::new(policy, policy_engine);

        let mut patch = create_test_patch();
        patch.hunks[0].modified_lines = vec!["api_key = \"sk-1234567890abcdef\"".to_string()];

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(!result.is_valid);
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, ViolationType::SecretDetected)));
    }

    #[tokio::test]
    async fn test_forbidden_operation() {
        use adapteros_policy::PolicyEngine;

        let policy = CodePolicy::default();
        let policies = create_mock_policies();
        let policy_engine = PolicyEngine::new(policies);
        let validator = PatchValidator::new(policy, policy_engine);

        let mut patch = create_test_patch();
        patch.hunks[0].modified_lines = vec!["eval(user_input)".to_string()];

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(!result.is_valid);
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, ViolationType::ForbiddenOperation)));
    }

    #[tokio::test]
    async fn test_dependency_check() {
        use adapteros_policy::PolicyEngine;

        let mut policy = CodePolicy::default();
        policy.allow_external_deps = false;

        let policies = create_mock_policies();
        let policy_engine = PolicyEngine::new(policies);
        let validator = PatchValidator::new(policy, policy_engine);

        let mut patch = create_test_patch();
        patch.hunks[0].modified_lines = vec!["use external_crate::function;".to_string()];

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(!result.is_valid);
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, ViolationType::DependencyBlocked)));
    }

    #[tokio::test]
    async fn test_size_limit() {
        use adapteros_policy::PolicyEngine;

        let mut policy = CodePolicy::default();
        policy.max_patch_size_lines = 1;

        let policies = create_mock_policies();
        let policy_engine = PolicyEngine::new(policies);
        let validator = PatchValidator::new(policy, policy_engine);

        let mut patch = create_test_patch();
        patch.total_lines = 100;

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(result.is_valid); // Size limit is a warning, not an error
        assert!(!result.warnings.is_empty());
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, ViolationType::SizeExceeded)));
    }
}
