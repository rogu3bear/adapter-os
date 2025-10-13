//! Build & Release Policy Pack
//!
//! Enforces build and release gates including determinism replay,
//! hallucination metrics, and rollback requirements.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Build & Release policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildReleaseConfig {
    /// Whether replay zero diff is required
    pub require_replay_zero_diff: bool,
    /// Hallucination thresholds
    pub hallucination_thresholds: HallucinationThresholds,
    /// Whether signed plan is required
    pub require_signed_plan: bool,
    /// Whether rollback plan is required
    pub require_rollback_plan: bool,
    /// Maximum build time in seconds
    pub max_build_time_secs: u64,
    /// Required test coverage percentage
    pub min_test_coverage_pct: f64,
}

/// Hallucination detection thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationThresholds {
    /// Minimum Answer Relevance Rate
    pub arr_min: f64,
    /// Minimum Evidence Citation Score at 5
    pub ecs5_min: f64,
    /// Maximum Hallucination Likelihood Rate
    pub hlr_max: f64,
    /// Maximum Contradiction Rate
    pub cr_max: f64,
}

impl Default for HallucinationThresholds {
    fn default() -> Self {
        Self {
            arr_min: 0.95,
            ecs5_min: 0.75,
            hlr_max: 0.03,
            cr_max: 0.01,
        }
    }
}

impl Default for BuildReleaseConfig {
    fn default() -> Self {
        Self {
            require_replay_zero_diff: true,
            hallucination_thresholds: HallucinationThresholds::default(),
            require_signed_plan: true,
            require_rollback_plan: true,
            max_build_time_secs: 3600, // 1 hour
            min_test_coverage_pct: 80.0,
        }
    }
}

/// Build metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetrics {
    pub build_id: String,
    pub build_time_secs: u64,
    pub test_coverage_pct: f64,
    pub tests_passed: u64,
    pub tests_failed: u64,
    pub tests_total: u64,
    pub build_status: BuildStatus,
    pub timestamp: u64,
}

/// Build status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildStatus {
    /// Build successful
    Success,
    /// Build failed
    Failed,
    /// Build in progress
    InProgress,
    /// Build cancelled
    Cancelled,
}

/// Replay test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayTestResults {
    pub test_id: String,
    pub prompt_hash: String,
    pub expected_output_hash: String,
    pub actual_output_hash: String,
    pub diff_count: u64,
    pub is_zero_diff: bool,
    pub execution_time_ms: u64,
    pub timestamp: u64,
}

/// Hallucination test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationTestResults {
    pub test_id: String,
    pub arr_score: f64,
    pub ecs5_score: f64,
    pub hlr_score: f64,
    pub cr_score: f64,
    pub total_tests: u64,
    pub passed_tests: u64,
    pub failed_tests: u64,
    pub timestamp: u64,
}

/// Plan metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanMetadata {
    pub plan_id: String,
    pub version: String,
    pub is_signed: bool,
    pub signature_hash: Option<String>,
    pub rollback_plan_id: Option<String>,
    pub created_at: u64,
    pub created_by: String,
}

/// Build & Release policy implementation
pub struct BuildReleasePolicy {
    config: BuildReleaseConfig,
}

impl BuildReleasePolicy {
    /// Create new build & release policy
    pub fn new(config: BuildReleaseConfig) -> Self {
        Self { config }
    }

    /// Validate build metrics
    pub fn validate_build_metrics(&self, metrics: &BuildMetrics) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        // Check build time
        if metrics.build_time_secs > self.config.max_build_time_secs {
            errors.push(format!(
                "Build time {}s exceeds maximum {}s",
                metrics.build_time_secs, self.config.max_build_time_secs
            ));
        }

        // Check test coverage
        if metrics.test_coverage_pct < self.config.min_test_coverage_pct {
            errors.push(format!(
                "Test coverage {:.2}% below minimum {:.2}%",
                metrics.test_coverage_pct, self.config.min_test_coverage_pct
            ));
        }

        // Check build status
        match metrics.build_status {
            BuildStatus::Failed => {
                errors.push("Build failed".to_string());
            }
            BuildStatus::Cancelled => {
                errors.push("Build cancelled".to_string());
            }
            _ => {}
        }

        // Check test results
        if metrics.tests_failed > 0 {
            errors.push(format!(
                "{} tests failed out of {} total tests",
                metrics.tests_failed, metrics.tests_total
            ));
        }

        Ok(errors)
    }

    /// Validate replay test results
    pub fn validate_replay_results(&self, results: &[ReplayTestResults]) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        if self.config.require_replay_zero_diff {
            for result in results {
                if !result.is_zero_diff {
                    errors.push(format!(
                        "Replay test {} failed: {} differences found",
                        result.test_id, result.diff_count
                    ));
                }
            }
        }

        Ok(errors)
    }

    /// Validate hallucination test results
    pub fn validate_hallucination_results(
        &self,
        results: &HallucinationTestResults,
    ) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        // Check ARR threshold
        if results.arr_score < self.config.hallucination_thresholds.arr_min {
            errors.push(format!(
                "ARR score {:.4} below minimum {:.4}",
                results.arr_score, self.config.hallucination_thresholds.arr_min
            ));
        }

        // Check ECS@5 threshold
        if results.ecs5_score < self.config.hallucination_thresholds.ecs5_min {
            errors.push(format!(
                "ECS@5 score {:.4} below minimum {:.4}",
                results.ecs5_score, self.config.hallucination_thresholds.ecs5_min
            ));
        }

        // Check HLR threshold
        if results.hlr_score > self.config.hallucination_thresholds.hlr_max {
            errors.push(format!(
                "HLR score {:.4} above maximum {:.4}",
                results.hlr_score, self.config.hallucination_thresholds.hlr_max
            ));
        }

        // Check CR threshold
        if results.cr_score > self.config.hallucination_thresholds.cr_max {
            errors.push(format!(
                "CR score {:.4} above maximum {:.4}",
                results.cr_score, self.config.hallucination_thresholds.cr_max
            ));
        }

        Ok(errors)
    }

    /// Validate plan metadata
    pub fn validate_plan_metadata(&self, plan: &PlanMetadata) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        // Check signature requirement
        if self.config.require_signed_plan && !plan.is_signed {
            errors.push("Plan signature is required but not provided".to_string());
        }

        // Check rollback plan requirement
        if self.config.require_rollback_plan && plan.rollback_plan_id.is_none() {
            errors.push("Rollback plan is required but not provided".to_string());
        }

        // Check signature hash
        if plan.is_signed && plan.signature_hash.is_none() {
            errors.push("Plan is marked as signed but signature hash is missing".to_string());
        }

        Ok(errors)
    }

    /// Validate build & release configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.max_build_time_secs == 0 {
            return Err(AosError::PolicyViolation(
                "Maximum build time must be greater than 0".to_string(),
            ));
        }

        if self.config.min_test_coverage_pct < 0.0 || self.config.min_test_coverage_pct > 100.0 {
            return Err(AosError::PolicyViolation(
                "Minimum test coverage must be between 0 and 100".to_string(),
            ));
        }

        // Validate hallucination thresholds
        if self.config.hallucination_thresholds.arr_min < 0.0
            || self.config.hallucination_thresholds.arr_min > 1.0
        {
            return Err(AosError::PolicyViolation(
                "ARR minimum must be between 0 and 1".to_string(),
            ));
        }

        if self.config.hallucination_thresholds.ecs5_min < 0.0
            || self.config.hallucination_thresholds.ecs5_min > 1.0
        {
            return Err(AosError::PolicyViolation(
                "ECS@5 minimum must be between 0 and 1".to_string(),
            ));
        }

        if self.config.hallucination_thresholds.hlr_max < 0.0
            || self.config.hallucination_thresholds.hlr_max > 1.0
        {
            return Err(AosError::PolicyViolation(
                "HLR maximum must be between 0 and 1".to_string(),
            ));
        }

        if self.config.hallucination_thresholds.cr_max < 0.0
            || self.config.hallucination_thresholds.cr_max > 1.0
        {
            return Err(AosError::PolicyViolation(
                "CR maximum must be between 0 and 1".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for build & release policy enforcement
#[derive(Debug)]
pub struct BuildReleaseContext {
    pub build_metrics: Option<BuildMetrics>,
    pub replay_results: Vec<ReplayTestResults>,
    pub hallucination_results: Option<HallucinationTestResults>,
    pub plan_metadata: Option<PlanMetadata>,
    pub tenant_id: String,
    pub operation: BuildReleaseOperation,
}

/// Types of build & release operations
#[derive(Debug)]
pub enum BuildReleaseOperation {
    /// Build operation
    Build,
    /// Release operation
    Release,
    /// Promotion operation
    Promotion,
    /// Rollback operation
    Rollback,
    /// Validation operation
    Validation,
}

impl PolicyContext for BuildReleaseContext {
    fn context_type(&self) -> &str {
        "build_release"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for BuildReleasePolicy {
    fn id(&self) -> PolicyId {
        PolicyId::BuildRelease
    }

    fn name(&self) -> &'static str {
        "Build & Release"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let build_ctx = ctx
            .as_any()
            .downcast_ref::<BuildReleaseContext>()
            .ok_or_else(|| {
                AosError::PolicyViolation("Invalid build & release context".to_string())
            })?;

        // Validate configuration
        self.validate_config()?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Validate build metrics
        if let Some(metrics) = &build_ctx.build_metrics {
            match self.validate_build_metrics(metrics) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::High,
                            message: format!("Build validation failed: {}", error),
                            details: Some(format!("Build ID: {}", metrics.build_id)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: "Build metrics validation error".to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }
        }

        // Validate replay results
        match self.validate_replay_results(&build_ctx.replay_results) {
            Ok(errors) => {
                for error in errors {
                    violations.push(Violation {
                        severity: Severity::Critical,
                        message: format!("Replay test failed: {}", error),
                        details: Some("Determinism replay test failed".to_string()),
                    });
                }
            }
            Err(e) => {
                violations.push(Violation {
                    severity: Severity::High,
                    message: "Replay results validation error".to_string(),
                    details: Some(e.to_string()),
                });
            }
        }

        // Validate hallucination results
        if let Some(results) = &build_ctx.hallucination_results {
            match self.validate_hallucination_results(results) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::High,
                            message: format!("Hallucination test failed: {}", error),
                            details: Some(format!("Test ID: {}", results.test_id)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: "Hallucination results validation error".to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }
        }

        // Validate plan metadata
        if let Some(plan) = &build_ctx.plan_metadata {
            match self.validate_plan_metadata(plan) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::High,
                            message: format!("Plan validation failed: {}", error),
                            details: Some(format!("Plan ID: {}", plan.plan_id)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: "Plan metadata validation error".to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }
        }

        // Add warnings for missing data
        if build_ctx.build_metrics.is_none() {
            warnings.push("Build metrics not provided".to_string());
        }

        if build_ctx.hallucination_results.is_none() {
            warnings.push("Hallucination test results not provided".to_string());
        }

        if build_ctx.plan_metadata.is_none() {
            warnings.push("Plan metadata not provided".to_string());
        }

        Ok(Audit {
            policy_id: PolicyId::BuildRelease,
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_release_config_default() {
        let config = BuildReleaseConfig::default();
        assert!(config.require_replay_zero_diff);
        assert!(config.require_signed_plan);
        assert!(config.require_rollback_plan);
        assert_eq!(config.max_build_time_secs, 3600);
        assert_eq!(config.min_test_coverage_pct, 80.0);
    }

    #[test]
    fn test_build_release_policy_creation() {
        let config = BuildReleaseConfig::default();
        let policy = BuildReleasePolicy::new(config);
        assert_eq!(policy.id(), PolicyId::BuildRelease);
    }

    #[test]
    fn test_build_metrics_validation() {
        let config = BuildReleaseConfig::default();
        let policy = BuildReleasePolicy::new(config);

        let metrics = BuildMetrics {
            build_id: "test_build".to_string(),
            build_time_secs: 4000,   // Exceeds 1 hour (3600s) limit
            test_coverage_pct: 70.0, // Below 80% minimum
            tests_passed: 80,
            tests_failed: 5, // Some failures
            tests_total: 85,
            build_status: BuildStatus::Success,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let errors = policy.validate_build_metrics(&metrics).unwrap();
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.contains("build time") || e.contains("Build time")));
        assert!(errors
            .iter()
            .any(|e| e.contains("test coverage") || e.contains("Test coverage")));
    }

    #[test]
    fn test_replay_results_validation() {
        let config = BuildReleaseConfig::default();
        let policy = BuildReleasePolicy::new(config);

        let results = vec![ReplayTestResults {
            test_id: "test1".to_string(),
            prompt_hash: "hash1".to_string(),
            expected_output_hash: "hash2".to_string(),
            actual_output_hash: "hash3".to_string(), // Different hash
            diff_count: 5,
            is_zero_diff: false, // Should fail
            execution_time_ms: 1000,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }];

        let errors = policy.validate_replay_results(&results).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("differences found")));
    }

    #[test]
    fn test_hallucination_results_validation() {
        let config = BuildReleaseConfig::default();
        let policy = BuildReleasePolicy::new(config);

        let results = HallucinationTestResults {
            test_id: "test1".to_string(),
            arr_score: 0.90,  // Below 0.95 minimum
            ecs5_score: 0.70, // Below 0.75 minimum
            hlr_score: 0.05,  // Above 0.03 maximum
            cr_score: 0.02,   // Above 0.01 maximum
            total_tests: 100,
            passed_tests: 85,
            failed_tests: 15,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let errors = policy.validate_hallucination_results(&results).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("ARR")));
        assert!(errors.iter().any(|e| e.contains("ECS@5")));
        assert!(errors.iter().any(|e| e.contains("HLR")));
        assert!(errors.iter().any(|e| e.contains("CR")));
    }

    #[test]
    fn test_plan_metadata_validation() {
        let config = BuildReleaseConfig::default();
        let policy = BuildReleasePolicy::new(config);

        let plan = PlanMetadata {
            plan_id: "test_plan".to_string(),
            version: "1.0.0".to_string(),
            is_signed: false, // Should fail
            signature_hash: None,
            rollback_plan_id: None, // Should fail
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            created_by: "test_user".to_string(),
        };

        let errors = policy.validate_plan_metadata(&plan).unwrap();
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.to_lowercase().contains("signature")));
        assert!(errors.iter().any(|e| e.to_lowercase().contains("rollback")));
    }

    #[test]
    fn test_build_release_config_validation() {
        let mut config = BuildReleaseConfig::default();
        config.max_build_time_secs = 0; // Invalid
        let policy = BuildReleasePolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
