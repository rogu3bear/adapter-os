//! Unified verification and validation framework for AdapterOS
//!
//! Provides a centralized framework for verifying and validating all aspects
//! of the system including code quality, security, performance, and compliance.

#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::should_implement_trait)]

pub mod code_quality;
pub mod performance;
pub mod security;
pub mod unified_validation;

// Re-export unified validation types
pub use unified_validation::{
    CheckStatus, CodeQualityConfig, CodeQualityReport, ComplianceAction, ComplianceCheck,
    ComplianceCheckType, ComplianceCondition, ComplianceConfig, ComplianceEnforcement,
    ComplianceMetric, ComplianceOperator, CompliancePolicy, ComplianceRecommendation,
    ComplianceReport, ComplianceReporting, ComplianceRequirement, ComplianceRequirementType,
    ComplianceRule, ComplianceSeverity, ComplianceStandard, ComplianceStatus, ComplianceViolation,
    ComprehensiveReport, DeploymentCheck, DeploymentConfig, DeploymentEnvironment, DeploymentIssue,
    DeploymentReadinessStatus, DeploymentRecommendation, DeploymentReport, DeploymentTarget,
    DeploymentTargetType, IntegrityAlgorithm, IntegrityCheck, IntegrityConfig,
    IntegrityRecommendation, IntegrityReport, IntegrityViolation, IssueLocation, IssueSeverity,
    MetricStatus, OutputFormat, PerformanceConfig, PerformanceIssue, PerformanceMetric,
    PerformanceRecommendation, PerformanceReport, PerformanceScenario, PerformanceScenarioType,
    PerformanceThreshold, QualityIssue, QualityMetric, QualityRecommendation,
    RecommendationPriority, ReportScheduling, ScheduleType, SecurityConfig, SecurityMetric,
    SecurityRecommendation, SecurityReport, SecuritySeverity, SecurityVulnerability,
    ThresholdOperator, UnifiedVerificationFramework, VerificationConfig, VerificationFramework,
    VerificationStatus, VerificationSummary,
};

// Re-export implementation modules
pub use code_quality::{CodeQualityResult, CodeQualityVerifier};
pub use performance::{PerformanceResult, PerformanceVerifier};
pub use security::{SecurityResult, SecurityVerifier};
