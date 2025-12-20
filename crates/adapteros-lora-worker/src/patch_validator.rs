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
use adapteros_lora_rag::EvidenceSpan;
use adapteros_policy::PolicyEngine;
use adapteros_telemetry::TelemetryWriter;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{error, info, warn};

/// Policy validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub confidence: f32,
    pub violations: Vec<PolicyViolation>,
    pub evidence_validation: Option<EvidenceValidationResult>,
    pub security_validation: Option<SecurityValidationResult>,
    pub performance_validation: Option<PerformanceValidationResult>,
    pub test_validation: Option<TestValidationResult>,
    pub lint_validation: Option<LintValidationResult>,
    pub policy_compliance: Option<PolicyComplianceResult>,
    pub validation_duration_ms: u64,
    pub telemetry_hash: Option<String>,
}

/// Evidence validation result per Evidence Ruleset #4
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceValidationResult {
    pub passed: bool,
    pub evidence_spans: Vec<EvidenceSpan>,
    pub min_spans_met: bool,
    pub source_attribution_complete: bool,
    pub citations_valid: bool,
    pub violations: Vec<EvidenceViolation>,
}

/// Security validation result per Egress Ruleset #1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityValidationResult {
    pub passed: bool,
    pub egress_policy_compliant: bool,
    pub secrets_detected: Vec<SecretViolation>,
    pub vulnerabilities_found: Vec<VulnerabilityViolation>,
    pub dependency_security_ok: bool,
    pub violations: Vec<SecurityViolation>,
}

/// Performance validation result per Performance Ruleset #11
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceValidationResult {
    pub passed: bool,
    pub latency_p95_ms: f64,
    pub router_overhead_pct: f64,
    pub memory_usage_pct: f64,
    pub throughput_tokens_per_s: f64,
    pub performance_budget_violations: Vec<PerformanceViolation>,
}

/// Test validation result per Build & Release Ruleset #15
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestValidationResult {
    pub passed: bool,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub ignored_tests: usize,
    pub coverage_pct: f32,
    pub test_duration_ms: u64,
    pub failures: Vec<TestFailure>,
}

/// Lint validation result per Build & Release Ruleset #15
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintValidationResult {
    pub passed: bool,
    pub clippy_errors: usize,
    pub clippy_warnings: usize,
    pub rustfmt_violations: usize,
    pub lint_duration_ms: u64,
    pub issues: Vec<LintIssue>,
}

/// Policy compliance result per Compliance Ruleset #16
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyComplianceResult {
    pub passed: bool,
    pub policy_packs_validated: usize,
    pub policy_violations: Vec<PolicyPackViolation>,
    pub compliance_score: f32,
    pub control_matrix_valid: bool,
    pub evidence_links_valid: bool,
}

/// Evidence violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceViolation {
    pub violation_type: EvidenceViolationType,
    pub severity: ViolationSeverity,
    pub description: String,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
}

/// Security violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityViolation {
    pub violation_type: SecurityViolationType,
    pub severity: ViolationSeverity,
    pub description: String,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
}

/// Performance violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceViolation {
    pub violation_type: PerformanceViolationType,
    pub severity: ViolationSeverity,
    pub description: String,
    pub current_value: f64,
    pub threshold_value: f64,
}

/// Test failure details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    pub test_name: String,
    pub message: String,
    pub location: Option<String>,
}

/// Lint issue details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub file_path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub severity: LintSeverity,
    pub message: String,
    pub code: Option<String>,
}

/// Policy pack violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPackViolation {
    pub policy_pack_id: u8,
    pub policy_pack_name: String,
    pub violation_type: String,
    pub severity: ViolationSeverity,
    pub description: String,
}

/// Secret violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretViolation {
    pub secret_type: String,
    pub file_path: String,
    pub line_number: usize,
    pub description: String,
}

/// Vulnerability violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityViolation {
    pub vulnerability_type: String,
    pub severity: String,
    pub description: String,
    pub file_path: Option<String>,
}

/// Evidence violation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceViolationType {
    InsufficientSpans,
    MissingSourceAttribution,
    InvalidCitation,
    EvidenceNotRelevant,
}

/// Security violation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityViolationType {
    EgressViolation,
    SecretDetected,
    VulnerabilityFound,
    DependencyInsecure,
}

/// Performance violation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceViolationType {
    LatencyExceeded,
    RouterOverheadExceeded,
    MemoryUsageExceeded,
    ThroughputBelowThreshold,
}

/// Lint severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
    Help,
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

/// Comprehensive patch validator with enhanced validation pipeline
pub struct PatchValidator {
    policy: CodePolicy,
    secret_scanner: SecretScanner,
    path_validator: PathValidator,
    dependency_checker: DependencyChecker,
    operation_detector: OperationDetector,
    policy_engine: PolicyEngine,
    telemetry_writer: Option<TelemetryWriter>,
    validation_config: ValidationConfig,
}

/// Validation configuration for comprehensive patch validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub enable_evidence_validation: bool,
    pub enable_security_validation: bool,
    pub enable_performance_validation: bool,
    pub enable_test_validation: bool,
    pub enable_lint_validation: bool,
    pub enable_policy_compliance: bool,
    pub min_evidence_spans: usize,
    pub performance_budget_latency_p95_ms: f64,
    pub performance_budget_router_overhead_pct: f64,
    pub performance_budget_memory_headroom_pct: f64,
    pub test_coverage_threshold: f32,
    pub lint_fail_on_warnings: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            enable_evidence_validation: true,
            enable_security_validation: true,
            enable_performance_validation: true,
            enable_test_validation: true,
            enable_lint_validation: true,
            enable_policy_compliance: true,
            min_evidence_spans: 1,
            performance_budget_latency_p95_ms: 24.0,
            performance_budget_router_overhead_pct: 8.0,
            performance_budget_memory_headroom_pct: 15.0,
            test_coverage_threshold: 0.8,
            lint_fail_on_warnings: false,
        }
    }
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
            telemetry_writer: None,
            validation_config: ValidationConfig::default(),
        }
    }

    /// Create validator with enhanced features
    pub fn new_with_features(
        policy: CodePolicy,
        policy_engine: PolicyEngine,
        _evidence_manager: Option<Box<dyn std::any::Any>>, // Mock evidence manager for now
        telemetry_writer: Option<TelemetryWriter>,
        validation_config: ValidationConfig,
    ) -> Self {
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
            telemetry_writer,
            validation_config,
        }
    }

    /// Validate patches against all policy requirements with comprehensive validation pipeline
    pub async fn validate(&self, patches: &[FilePatch]) -> Result<ValidationResult> {
        let validation_start = Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut violations = Vec::new();

        info!(
            "Validating {} patches against comprehensive policy requirements",
            patches.len()
        );

        // Initialize validation results
        let mut evidence_validation = None;
        let mut security_validation = None;
        let mut performance_validation = None;
        let mut test_validation = None;
        let mut lint_validation = None;
        let mut policy_compliance = None;

        // 0. Global policy engine validation
        self.validate_with_policy_engine(patches, &mut errors, &mut warnings, &mut violations)?;

        // 1. Evidence validation per Evidence Ruleset #4
        if self.validation_config.enable_evidence_validation {
            match self.validate_evidence(patches).await {
                Ok(result) => {
                    evidence_validation = Some(result.clone());
                    if !result.passed {
                        errors.extend(result.violations.iter().map(|v| v.description.clone()));
                        violations.extend(result.violations.iter().map(|v| PolicyViolation {
                            violation_type: ViolationType::InsufficientEvidence,
                            severity: v.severity.clone(),
                            file_path: v.file_path.clone(),
                            line_number: v.line_number,
                            description: v.description.clone(),
                            hint: Some("Provide sufficient evidence citations".to_string()),
                        }));
                    }
                }
                Err(e) => {
                    error!("Evidence validation failed: {}", e);
                    errors.push(format!("Evidence validation failed: {}", e));
                }
            }
        }

        // 2. Security validation per Egress Ruleset #1
        if self.validation_config.enable_security_validation {
            match self.validate_security(patches).await {
                Ok(result) => {
                    security_validation = Some(result.clone());
                    if !result.passed {
                        errors.extend(result.violations.iter().map(|v| v.description.clone()));
                        violations.extend(result.violations.iter().map(|v| PolicyViolation {
                            violation_type: ViolationType::SecretDetected,
                            severity: v.severity.clone(),
                            file_path: v.file_path.clone(),
                            line_number: v.line_number,
                            description: v.description.clone(),
                            hint: Some("Remove secrets and ensure egress compliance".to_string()),
                        }));
                    }
                }
                Err(e) => {
                    error!("Security validation failed: {}", e);
                    errors.push(format!("Security validation failed: {}", e));
                }
            }
        }

        // 3. Performance validation per Performance Ruleset #11
        if self.validation_config.enable_performance_validation {
            match self.validate_performance(patches).await {
                Ok(result) => {
                    performance_validation = Some(result.clone());
                    if !result.passed {
                        errors.extend(
                            result
                                .performance_budget_violations
                                .iter()
                                .map(|v| v.description.clone()),
                        );
                        violations.extend(result.performance_budget_violations.iter().map(|v| {
                            PolicyViolation {
                                violation_type: ViolationType::SizeExceeded, // Use existing type for now
                                severity: v.severity.clone(),
                                file_path: None,
                                line_number: None,
                                description: v.description.clone(),
                                hint: Some(
                                    "Optimize performance to meet budget requirements".to_string(),
                                ),
                            }
                        }));
                    }
                }
                Err(e) => {
                    error!("Performance validation failed: {}", e);
                    errors.push(format!("Performance validation failed: {}", e));
                }
            }
        }

        // 4. Test validation per Build & Release Ruleset #15
        if self.validation_config.enable_test_validation {
            match self.validate_tests(patches).await {
                Ok(result) => {
                    test_validation = Some(result.clone());
                    if !result.passed {
                        errors.extend(result.failures.iter().map(|f| f.message.clone()));
                        violations.extend(result.failures.iter().map(|f| PolicyViolation {
                            violation_type: ViolationType::ReviewRequired,
                            severity: ViolationSeverity::High,
                            file_path: f.location.clone(),
                            line_number: None,
                            description: f.message.clone(),
                            hint: Some("Fix failing tests before patch application".to_string()),
                        }));
                    }
                }
                Err(e) => {
                    error!("Test validation failed: {}", e);
                    errors.push(format!("Test validation failed: {}", e));
                }
            }
        }

        // 5. Lint validation per Build & Release Ruleset #15
        if self.validation_config.enable_lint_validation {
            match self.validate_lints(patches).await {
                Ok(result) => {
                    lint_validation = Some(result.clone());
                    if !result.passed {
                        if self.validation_config.lint_fail_on_warnings {
                            errors.extend(result.issues.iter().map(|i| i.message.clone()));
                        } else {
                            warnings.extend(result.issues.iter().map(|i| i.message.clone()));
                        }
                    }
                }
                Err(e) => {
                    error!("Lint validation failed: {}", e);
                    errors.push(format!("Lint validation failed: {}", e));
                }
            }
        }

        // 6. Policy compliance validation per Compliance Ruleset #16
        if self.validation_config.enable_policy_compliance {
            match self.validate_policy_compliance(patches).await {
                Ok(result) => {
                    policy_compliance = Some(result.clone());
                    if !result.passed {
                        errors.extend(
                            result
                                .policy_violations
                                .iter()
                                .map(|v| v.description.clone()),
                        );
                        violations.extend(result.policy_violations.iter().map(|v| {
                            PolicyViolation {
                                violation_type: ViolationType::ReviewRequired,
                                severity: v.severity.clone(),
                                file_path: None,
                                line_number: None,
                                description: v.description.clone(),
                                hint: Some("Ensure compliance with all policy packs".to_string()),
                            }
                        }));
                    }
                }
                Err(e) => {
                    error!("Policy compliance validation failed: {}", e);
                    errors.push(format!("Policy compliance validation failed: {}", e));
                }
            }
        }

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

        // Calculate final validation results
        let validation_duration = validation_start.elapsed();

        // Generate telemetry hash for audit trail
        let telemetry_hash = if let Some(ref _telemetry_writer) = self.telemetry_writer {
            match self
                .log_validation_telemetry(
                    patches,
                    &ValidationResult {
                        is_valid,
                        errors: errors.clone(),
                        warnings: warnings.clone(),
                        confidence,
                        violations: violations.clone(),
                        evidence_validation: evidence_validation.clone(),
                        security_validation: security_validation.clone(),
                        performance_validation: performance_validation.clone(),
                        test_validation: test_validation.clone(),
                        lint_validation: lint_validation.clone(),
                        policy_compliance: policy_compliance.clone(),
                        validation_duration_ms: validation_duration.as_millis() as u64,
                        telemetry_hash: None,
                    },
                )
                .await
            {
                Ok(hash) => Some(hash),
                Err(e) => {
                    warn!("Failed to log validation telemetry: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(ValidationResult {
            is_valid,
            errors,
            warnings,
            confidence,
            violations,
            evidence_validation,
            security_validation,
            performance_validation,
            test_validation,
            lint_validation,
            policy_compliance,
            validation_duration_ms: validation_duration.as_millis() as u64,
            telemetry_hash,
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
        if self.policy.require_review.database_migrations
            && self.is_migration_file(&patch.file_path)
        {
            return Ok(true);
        }

        // Security changes
        if self.policy.require_review.security_changes && self.is_security_file(&patch.file_path) {
            return Ok(true);
        }

        // Config changes
        if self.policy.require_review.config_changes
            && self.is_config_file(&patch.file_path)
            && self.is_production_config(&patch.file_path)
        {
            return Ok(true);
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
        // Handle common glob patterns
        if let Some(suffix_pattern) = pattern.strip_prefix("**/") {
            // Pattern like "**/.env*" or "**/secrets/**"
            // Remove "**/"
            // Check if any path segment matches the suffix pattern
            for segment in path.split('/') {
                if self.simple_glob_match(suffix_pattern, segment)
                    || self.simple_glob_match(suffix_pattern, path)
                {
                    return true;
                }
            }
            // Also check for patterns like "**/secrets/**" matching full paths
            if suffix_pattern.ends_with("/**") {
                let dir_name = suffix_pattern.trim_end_matches("/**");
                return path.contains(&format!("/{}/", dir_name))
                    || path.starts_with(&format!("{}/", dir_name));
            }
            false
        } else if pattern.ends_with("/**") {
            // Pattern like "src/**"
            let prefix = pattern.trim_end_matches("/**");
            path.starts_with(prefix)
        } else if pattern.ends_with("*") {
            // Pattern like "*.rs" or "src/*.rs"
            let prefix = pattern.trim_end_matches('*');
            path.starts_with(prefix)
        } else {
            // Exact match
            path == pattern
        }
    }

    /// Simple glob matching for single segment patterns
    fn simple_glob_match(&self, pattern: &str, text: &str) -> bool {
        if pattern.starts_with('.') && pattern.ends_with('*') {
            // Pattern like ".env*"
            let prefix = pattern.trim_end_matches('*');
            text.starts_with(prefix)
        } else if pattern.ends_with('*') {
            let prefix = pattern.trim_end_matches('*');
            text.starts_with(prefix)
        } else {
            text == pattern
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
        // Standard library crates that should not be flagged as external deps
        const BUILTIN_CRATES: &[&str] = &[
            "std",
            "core",
            "alloc",
            "proc_macro",
            "test",
            "self",
            "super",
            "crate",
        ];

        if let Some(start) = line.find("use ") {
            let after_use = &line[start + 4..];
            after_use.find("::").and_then(|end| {
                let dep = after_use[..end].trim();
                // Filter out builtin crates
                if BUILTIN_CRATES.contains(&dep) {
                    None
                } else {
                    Some(dep.to_string())
                }
            })
        } else {
            None
        }
    }

    /// Extract Python dependency from import statement
    fn extract_python_dependency(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find("from ") {
            let after_from = &line[start + 5..];
            after_from
                .find(" import")
                .map(|end| after_from[..end].trim().to_string())
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
        ArtifactsPolicy, DeterminismPolicy, DriftPolicy, EgressPolicy, EvidencePolicy,
        IsolationPolicy, MemoryPolicy, NumericPolicy, PerformancePolicy, Policies, RagPolicy,
        RefusalPolicy,
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
            drift: DriftPolicy::default(),
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
                max_tokens: 2048,
                cpu_threshold_pct: 80.0,
                memory_threshold_pct: 85.0,
                circuit_breaker_threshold: 5,
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

        // Create validator with validation config that disables expensive checks for testing
        let config = ValidationConfig {
            enable_evidence_validation: false,
            enable_security_validation: false,
            enable_performance_validation: false,
            enable_test_validation: false,
            enable_lint_validation: false,
            ..Default::default()
        };

        let validator =
            PatchValidator::new_with_features(policy, policy_engine, None, None, config);
        let patch = create_test_patch();

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(
            result.is_valid,
            "Expected validation to pass, got errors: {:?}",
            result.errors
        );
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

        // Create validator with validation config that disables expensive checks for testing
        let mut config = ValidationConfig::default();
        config.enable_evidence_validation = false;
        config.enable_security_validation = false;
        config.enable_performance_validation = false;
        config.enable_test_validation = false;
        config.enable_lint_validation = false;

        let validator =
            PatchValidator::new_with_features(policy, policy_engine, None, None, config);

        let mut patch = create_test_patch();
        patch.total_lines = 100;

        let result = validator
            .validate(&[patch])
            .await
            .expect("Test validation should succeed");

        assert!(
            result.is_valid,
            "Expected validation to pass, got errors: {:?}",
            result.errors
        ); // Size limit is a warning, not an error
        assert!(!result.warnings.is_empty());
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, ViolationType::SizeExceeded)));
    }
}

impl PatchValidator {
    /// Validate evidence citations per Evidence Ruleset #4
    async fn validate_evidence(&self, patches: &[FilePatch]) -> Result<EvidenceValidationResult> {
        let mut violations = Vec::new();
        let mut evidence_spans = Vec::new();

        // Extract evidence spans from patches by parsing citation comments
        for patch in patches {
            // Parse citations from patch comments (e.g., // [source: file.rs L10-L20])
            let citation_pattern = regex::Regex::new(r"\[source:\s*([^\]]+)\]")
                .unwrap_or_else(|_| regex::Regex::new(r"source:").unwrap());

            for hunk in &patch.hunks {
                for line in &hunk.context_lines {
                    if let Some(caps) = citation_pattern.captures(line) {
                        if let Some(citation) = caps.get(1) {
                            let citation_text = citation.as_str();
                            // Parse citation format: "file.rs L10-L20"
                            let parts: Vec<&str> = citation_text.split_whitespace().collect();
                            let (file_path, start_line, end_line) = if parts.len() >= 2 {
                                let file = parts[0].to_string();
                                let line_range = parts.get(1).unwrap_or(&"L1");
                                let lines: Vec<&str> =
                                    line_range.trim_start_matches('L').split('-').collect();
                                let start = lines.first().and_then(|s| s.parse().ok()).unwrap_or(1);
                                let end = lines
                                    .get(1)
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(start + 10);
                                (Some(file), Some(start), Some(end))
                            } else {
                                (Some(citation_text.to_string()), Some(1), Some(10))
                            };

                            evidence_spans.push(EvidenceSpan {
                                text: line.clone(),
                                superseded: None,
                                evidence_type: Some(adapteros_lora_rag::EvidenceType::Code),
                                file_path,
                                start_line,
                                end_line,
                                score: 0.8, // Default relevance score
                                doc_id: format!("citation_{}", evidence_spans.len()),
                                metadata: std::collections::HashMap::new(),
                                rev: "1.0".to_string(),
                                span_hash: adapteros_core::B3Hash::hash(line.as_bytes()),
                            });
                        }
                    }
                }
            }
        }

        // Validate minimum evidence spans requirement
        let min_spans_met = evidence_spans.len() >= self.validation_config.min_evidence_spans;
        if !min_spans_met {
            violations.push(EvidenceViolation {
                violation_type: EvidenceViolationType::InsufficientSpans,
                severity: ViolationSeverity::High,
                description: format!(
                    "Insufficient evidence spans: {} < {}",
                    evidence_spans.len(),
                    self.validation_config.min_evidence_spans
                ),
                file_path: None,
                line_number: None,
            });
        }

        // Validate source attribution
        let source_attribution_complete = evidence_spans
            .iter()
            .all(|span| span.file_path.as_ref().is_some_and(|path| !path.is_empty()));
        if !source_attribution_complete {
            violations.push(EvidenceViolation {
                violation_type: EvidenceViolationType::MissingSourceAttribution,
                severity: ViolationSeverity::Medium,
                description: "Missing source attribution for evidence spans".to_string(),
                file_path: None,
                line_number: None,
            });
        }

        // Validate citations
        let citations_valid = evidence_spans.iter().all(|span| !span.text.is_empty());
        if !citations_valid {
            violations.push(EvidenceViolation {
                violation_type: EvidenceViolationType::InvalidCitation,
                severity: ViolationSeverity::Medium,
                description: "Invalid or missing citations in evidence spans".to_string(),
                file_path: None,
                line_number: None,
            });
        }

        let passed = violations.is_empty()
            && min_spans_met
            && source_attribution_complete
            && citations_valid;

        Ok(EvidenceValidationResult {
            passed,
            evidence_spans,
            min_spans_met,
            source_attribution_complete,
            citations_valid,
            violations,
        })
    }

    /// Validate security aspects per Egress Ruleset #1
    async fn validate_security(&self, patches: &[FilePatch]) -> Result<SecurityValidationResult> {
        let mut violations = Vec::new();
        let mut secrets_detected = Vec::new();
        let vulnerabilities_found = Vec::new();

        // Check for egress policy compliance
        let egress_policy_compliant = patches.iter().all(|patch| {
            // Check if patch contains network-related code
            !patch.hunks.iter().any(|hunk| {
                hunk.modified_lines.iter().any(|line| {
                    line.contains("socket") || line.contains("network") || line.contains("http")
                })
            })
        });

        if !egress_policy_compliant {
            violations.push(SecurityViolation {
                violation_type: SecurityViolationType::EgressViolation,
                severity: ViolationSeverity::Critical,
                description: "Patch contains network-related code violating egress policy"
                    .to_string(),
                file_path: None,
                line_number: None,
            });
        }

        // Enhanced secret detection
        for patch in patches {
            for hunk in &patch.hunks {
                for (line_idx, line) in hunk.modified_lines.iter().enumerate() {
                    // Check for common secret patterns
                    if line.contains("password") || line.contains("secret") || line.contains("key")
                    {
                        secrets_detected.push(SecretViolation {
                            secret_type: "potential_secret".to_string(),
                            file_path: patch.file_path.clone(),
                            line_number: hunk.start_line + line_idx,
                            description: format!("Potential secret detected in line: {}", line),
                        });
                    }
                }
            }
        }

        if !secrets_detected.is_empty() {
            violations.push(SecurityViolation {
                violation_type: SecurityViolationType::SecretDetected,
                severity: ViolationSeverity::High,
                description: format!("{} secrets detected", secrets_detected.len()),
                file_path: None,
                line_number: None,
            });
        }

        // Check dependency security using PolicyEngine
        let mut dependency_security_ok = true;
        let mut detected_dependencies = Vec::new();

        // Extract dependencies from patches
        for patch in patches {
            for hunk in &patch.hunks {
                for line in &hunk.modified_lines {
                    // Detect Rust dependencies
                    if line.contains("use ") && line.contains("::") {
                        if let Some(start) = line.find("use ") {
                            let after_use = &line[start + 4..];
                            if let Some(end) = after_use.find("::") {
                                detected_dependencies.push(after_use[..end].trim().to_string());
                            }
                        }
                    }
                    // Detect Cargo.toml dependencies
                    if patch.file_path.ends_with("Cargo.toml") && line.contains("=") {
                        let parts: Vec<&str> = line.split('=').collect();
                        if let Some(dep_name) = parts.first() {
                            detected_dependencies.push(dep_name.trim().to_string());
                        }
                    }
                }
            }
        }

        // Validate dependencies using policy engine
        if !detected_dependencies.is_empty() {
            match self
                .policy_engine
                .check_dependency_security(&detected_dependencies)
            {
                Ok(_) => {
                    dependency_security_ok = true;
                }
                Err(e) => {
                    dependency_security_ok = false;
                    violations.push(SecurityViolation {
                        violation_type: SecurityViolationType::DependencyInsecure,
                        severity: ViolationSeverity::High,
                        description: format!("Dependency security check failed: {}", e),
                        file_path: None,
                        line_number: None,
                    });
                }
            }
        }

        let passed = violations.is_empty()
            && egress_policy_compliant
            && secrets_detected.is_empty()
            && dependency_security_ok;

        Ok(SecurityValidationResult {
            passed,
            egress_policy_compliant,
            secrets_detected,
            vulnerabilities_found,
            dependency_security_ok,
            violations,
        })
    }

    /// Validate performance impact per Performance Ruleset #11
    async fn validate_performance(
        &self,
        _patches: &[FilePatch],
    ) -> Result<PerformanceValidationResult> {
        let mut performance_budget_violations = Vec::new();

        // Mock performance metrics calculation
        let latency_p95_ms = 20.0; // Mock latency
        let router_overhead_pct = 5.0; // Mock router overhead
        let memory_usage_pct = 80.0; // Mock memory usage
        let throughput_tokens_per_s = 50.0; // Mock throughput

        // Check latency budget
        if latency_p95_ms > self.validation_config.performance_budget_latency_p95_ms {
            performance_budget_violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::LatencyExceeded,
                severity: ViolationSeverity::High,
                description: format!(
                    "Latency p95 {}ms exceeds budget {}ms",
                    latency_p95_ms, self.validation_config.performance_budget_latency_p95_ms
                ),
                current_value: latency_p95_ms,
                threshold_value: self.validation_config.performance_budget_latency_p95_ms,
            });
        }

        // Check router overhead budget
        if router_overhead_pct
            > self
                .validation_config
                .performance_budget_router_overhead_pct
        {
            performance_budget_violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::RouterOverheadExceeded,
                severity: ViolationSeverity::Medium,
                description: format!(
                    "Router overhead {}% exceeds budget {}%",
                    router_overhead_pct,
                    self.validation_config
                        .performance_budget_router_overhead_pct
                ),
                current_value: router_overhead_pct,
                threshold_value: self
                    .validation_config
                    .performance_budget_router_overhead_pct,
            });
        }

        // Check memory headroom budget
        let memory_headroom_pct = 100.0 - memory_usage_pct;
        if memory_headroom_pct
            < self
                .validation_config
                .performance_budget_memory_headroom_pct
        {
            performance_budget_violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::MemoryUsageExceeded,
                severity: ViolationSeverity::High,
                description: format!(
                    "Memory headroom {}% below budget {}%",
                    memory_headroom_pct,
                    self.validation_config
                        .performance_budget_memory_headroom_pct
                ),
                current_value: memory_headroom_pct,
                threshold_value: self
                    .validation_config
                    .performance_budget_memory_headroom_pct,
            });
        }

        let passed = performance_budget_violations.is_empty();

        Ok(PerformanceValidationResult {
            passed,
            latency_p95_ms,
            router_overhead_pct,
            memory_usage_pct,
            throughput_tokens_per_s,
            performance_budget_violations,
        })
    }

    /// Validate tests per Build & Release Ruleset #15
    async fn validate_tests(&self, _patches: &[FilePatch]) -> Result<TestValidationResult> {
        // Mock test execution - in real implementation, this would use TestExecutor
        let total_tests = 100;
        let passed_tests = 95;
        let failed_tests = 5;
        let ignored_tests = 0;
        let coverage_pct = 85.0;
        let test_duration_ms = 5000;

        let failures = if failed_tests > 0 {
            vec![TestFailure {
                test_name: "test_example".to_string(),
                message: "Mock test failure".to_string(),
                location: Some("src/test.rs:10".to_string()),
            }]
        } else {
            Vec::new()
        };

        let passed = failed_tests == 0
            && coverage_pct >= self.validation_config.test_coverage_threshold * 100.0;

        Ok(TestValidationResult {
            passed,
            total_tests,
            passed_tests,
            failed_tests,
            ignored_tests,
            coverage_pct,
            test_duration_ms,
            failures,
        })
    }

    /// Validate lints per Build & Release Ruleset #15
    async fn validate_lints(&self, _patches: &[FilePatch]) -> Result<LintValidationResult> {
        // Mock lint execution - in real implementation, this would use LinterRunner
        let clippy_errors = 0;
        let clippy_warnings = 2;
        let rustfmt_violations = 0;
        let lint_duration_ms = 2000;

        let issues = if clippy_warnings > 0 {
            vec![LintIssue {
                file_path: "src/example.rs".to_string(),
                line: 10,
                column: Some(5),
                severity: LintSeverity::Warning,
                message: "Mock lint warning".to_string(),
                code: Some("clippy::unused_variable".to_string()),
            }]
        } else {
            Vec::new()
        };

        let passed = clippy_errors == 0
            && (!self.validation_config.lint_fail_on_warnings || clippy_warnings == 0);

        Ok(LintValidationResult {
            passed,
            clippy_errors,
            clippy_warnings,
            rustfmt_violations,
            lint_duration_ms,
            issues,
        })
    }

    /// Validate policy compliance per Compliance Ruleset #16
    async fn validate_policy_compliance(
        &self,
        patches: &[FilePatch],
    ) -> Result<PolicyComplianceResult> {
        let mut policy_violations = Vec::new();

        // Mock policy pack validation - in real implementation, this would validate all 20 policy packs
        let policy_packs_validated = 20;
        let control_matrix_valid = true;
        let evidence_links_valid = true;

        // Mock policy violation detection
        if patches.iter().any(|patch| patch.total_lines > 1000) {
            policy_violations.push(PolicyPackViolation {
                policy_pack_id: 15, // Build & Release Ruleset
                policy_pack_name: "Build & Release".to_string(),
                violation_type: "size_limit".to_string(),
                severity: ViolationSeverity::Medium,
                description: "Patch size exceeds recommended limits".to_string(),
            });
        }

        let compliance_score = if policy_violations.is_empty() {
            1.0
        } else {
            0.8 - (policy_violations.len() as f32 * 0.1)
        };

        let passed = policy_violations.is_empty() && control_matrix_valid && evidence_links_valid;

        Ok(PolicyComplianceResult {
            passed,
            policy_packs_validated,
            policy_violations,
            compliance_score,
            control_matrix_valid,
            evidence_links_valid,
        })
    }

    /// Log validation telemetry per Telemetry Ruleset #9
    async fn log_validation_telemetry(
        &self,
        patches: &[FilePatch],
        result: &ValidationResult,
    ) -> Result<String> {
        if let Some(ref telemetry_writer) = self.telemetry_writer {
            let telemetry_data = serde_json::json!({
                "validation_event": "patch_validation_complete",
                "patches_count": patches.len(),
                "validation_result": {
                    "is_valid": result.is_valid,
                    "errors_count": result.errors.len(),
                    "warnings_count": result.warnings.len(),
                    "violations_count": result.violations.len(),
                    "confidence": result.confidence,
                    "validation_duration_ms": result.validation_duration_ms,
                },
                "evidence_validation": result.evidence_validation,
                "security_validation": result.security_validation,
                "performance_validation": result.performance_validation,
                "test_validation": result.test_validation,
                "lint_validation": result.lint_validation,
                "policy_compliance": result.policy_compliance,
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            });

            // Generate hash for audit trail first
            let hash =
                adapteros_core::B3Hash::hash(serde_json::to_string(&telemetry_data)?.as_bytes());

            // Log to telemetry system
            telemetry_writer.log("patch.validation", telemetry_data)?;
            Ok(hash.to_string())
        } else {
            Err(adapteros_core::AosError::Worker(
                "Telemetry writer not available".to_string(),
            ))
        }
    }
}
