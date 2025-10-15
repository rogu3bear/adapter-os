//! Unified verification and validation framework for AdapterOS
//!
//! Provides a centralized framework for verifying and validating all aspects
//! of the system including code quality, security, performance, and compliance.

pub mod unified_validation;
pub mod code_quality;
pub mod security;
pub mod performance;

// Re-export unified validation types
pub use unified_validation::{
    VerificationFramework, UnifiedVerificationFramework, VerificationConfig, CodeQualityConfig,
    SecurityConfig, PerformanceConfig, ComplianceConfig, IntegrityConfig, DeploymentConfig,
    OutputFormat, SecuritySeverity, PerformanceScenario, PerformanceScenarioType, PerformanceThreshold,
    ThresholdOperator, ComplianceStandard, ComplianceRequirement, ComplianceRequirementType,
    ComplianceSeverity, CompliancePolicy, ComplianceRule, ComplianceCondition, ComplianceOperator,
    ComplianceAction, ComplianceEnforcement, ComplianceCheck, ComplianceCheckType, ComplianceReporting,
    ReportScheduling, ScheduleType, IntegrityAlgorithm, DeploymentEnvironment, DeploymentTarget,
    DeploymentTargetType, CodeQualityReport, QualityMetric, MetricStatus, QualityIssue,
    IssueSeverity, IssueLocation, QualityRecommendation, RecommendationPriority, SecurityReport,
    SecurityVulnerability, SecurityRecommendation, SecurityMetric, PerformanceReport,
    PerformanceMetric, PerformanceIssue, PerformanceRecommendation, ComplianceReport,
    ComplianceStatus, ComplianceViolation, ComplianceRecommendation, ComplianceMetric,
    IntegrityReport, IntegrityCheck, CheckStatus, IntegrityViolation, IntegrityRecommendation,
    DeploymentReport, DeploymentReadinessStatus, DeploymentCheck, DeploymentIssue,
    DeploymentRecommendation, ComprehensiveReport, VerificationStatus, VerificationSummary,
};

// Re-export implementation modules
pub use code_quality::{CodeQualityVerifier, CodeQualityResult};
pub use security::{SecurityVerifier, SecurityResult};
pub use performance::{PerformanceVerifier, PerformanceResult};
