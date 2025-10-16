//! Unified verification and validation framework for AdapterOS
//!
//! Provides a centralized framework for verifying and validating all aspects
//! of the system including code quality, security, performance, and compliance.
//!
//! # Citations
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
//! - CLAUDE.md L50-55: "Verification and validation with deterministic execution"

use crate::{
    code_quality::{CodeQualityResult, CodeQualityVerifier},
    performance::{PerformanceResult, PerformanceVerifier},
    security::{SecurityResult, SecurityVerifier},
};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use blake3::hash as blake3_hash;
use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use sha2::{Sha256, Sha512};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::{info, warn};

/// Unified verification and validation framework interface
#[async_trait]
pub trait VerificationFramework {
    /// Verify code quality
    async fn verify_code_quality(&self, config: &CodeQualityConfig) -> Result<CodeQualityReport>;

    /// Verify security compliance
    async fn verify_security(&self, config: &SecurityConfig) -> Result<SecurityReport>;

    /// Verify performance requirements
    async fn verify_performance(&self, config: &PerformanceConfig) -> Result<PerformanceReport>;

    /// Verify compliance requirements
    async fn verify_compliance(&self, config: &ComplianceConfig) -> Result<ComplianceReport>;

    /// Verify system integrity
    async fn verify_system_integrity(&self, config: &IntegrityConfig) -> Result<IntegrityReport>;

    /// Verify deployment readiness
    async fn verify_deployment_readiness(
        &self,
        config: &DeploymentConfig,
    ) -> Result<DeploymentReport>;

    /// Run comprehensive verification
    async fn run_comprehensive_verification(
        &self,
        config: &VerificationConfig,
    ) -> Result<ComprehensiveReport>;
}

/// Verification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Code quality configuration
    pub code_quality: CodeQualityConfig,

    /// Security configuration
    pub security: SecurityConfig,

    /// Performance configuration
    pub performance: PerformanceConfig,

    /// Compliance configuration
    pub compliance: ComplianceConfig,

    /// Integrity configuration
    pub integrity: IntegrityConfig,

    /// Deployment configuration
    pub deployment: DeploymentConfig,

    /// Verification timeout in seconds
    pub timeout_seconds: u64,

    /// Enable parallel verification
    pub enable_parallel: bool,

    /// Verification output format
    pub output_format: OutputFormat,

    /// Additional configuration
    pub additional_config: HashMap<String, serde_json::Value>,
}

/// Code quality configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQualityConfig {
    /// Enable clippy checks
    pub enable_clippy: bool,

    /// Enable format checks
    pub enable_format: bool,

    /// Enable test coverage checks
    pub enable_coverage: bool,

    /// Minimum test coverage percentage
    pub min_coverage_percentage: f64,

    /// Enable complexity analysis
    pub enable_complexity: bool,

    /// Maximum cyclomatic complexity
    pub max_cyclomatic_complexity: u32,

    /// Enable documentation checks
    pub enable_documentation: bool,

    /// Enable dead code detection
    pub enable_dead_code: bool,

    /// Additional quality checks
    pub additional_checks: Vec<String>,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable vulnerability scanning
    pub enable_vulnerability_scanning: bool,

    /// Enable dependency scanning
    pub enable_dependency_scanning: bool,

    /// Enable secret detection
    pub enable_secret_detection: bool,

    /// Enable SAST (Static Application Security Testing)
    pub enable_sast: bool,

    /// Enable DAST (Dynamic Application Security Testing)
    pub enable_dast: bool,

    /// Enable container security scanning
    pub enable_container_scanning: bool,

    /// Security severity thresholds
    pub severity_thresholds: HashMap<String, SecuritySeverity>,

    /// Security policies
    pub security_policies: Vec<String>,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable performance testing
    pub enable_performance_testing: bool,

    /// Performance test scenarios
    pub test_scenarios: Vec<PerformanceScenario>,

    /// Performance thresholds
    pub performance_thresholds: HashMap<String, PerformanceThreshold>,

    /// Enable load testing
    pub enable_load_testing: bool,

    /// Enable stress testing
    pub enable_stress_testing: bool,

    /// Enable memory profiling
    pub enable_memory_profiling: bool,

    /// Enable CPU profiling
    pub enable_cpu_profiling: bool,
}

/// Compliance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    /// Compliance standards
    pub compliance_standards: Vec<ComplianceStandard>,

    /// Compliance policies
    pub compliance_policies: Vec<CompliancePolicy>,

    /// Compliance checks
    pub compliance_checks: Vec<ComplianceCheck>,

    /// Compliance reporting
    pub compliance_reporting: ComplianceReporting,

    /// Compliance thresholds
    pub compliance_thresholds: HashMap<String, f64>,
}

/// Integrity configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityConfig {
    /// Enable file integrity checks
    pub enable_file_integrity: bool,

    /// Enable checksum verification
    pub enable_checksum_verification: bool,

    /// Enable signature verification
    pub enable_signature_verification: bool,

    /// Enable dependency integrity checks
    pub enable_dependency_integrity: bool,

    /// Integrity check algorithms
    pub integrity_algorithms: Vec<IntegrityAlgorithm>,

    /// Integrity check paths
    pub integrity_paths: Vec<String>,
}

/// Deployment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    /// Enable deployment readiness checks
    pub enable_readiness_checks: bool,

    /// Enable health checks
    pub enable_health_checks: bool,

    /// Enable configuration validation
    pub enable_config_validation: bool,

    /// Enable resource validation
    pub enable_resource_validation: bool,

    /// Enable dependency validation
    pub enable_dependency_validation: bool,

    /// Deployment environment
    pub deployment_environment: DeploymentEnvironment,

    /// Deployment targets
    pub deployment_targets: Vec<DeploymentTarget>,
}

/// Output formats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    /// JSON format
    Json,

    /// XML format
    Xml,

    /// HTML format
    Html,

    /// Plain text format
    PlainText,

    /// Markdown format
    Markdown,
}

/// Security severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecuritySeverity {
    /// Critical severity
    Critical,

    /// High severity
    High,

    /// Medium severity
    Medium,

    /// Low severity
    Low,

    /// Info severity
    Info,
}

/// Performance scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceScenario {
    /// Scenario name
    pub name: String,

    /// Scenario description
    pub description: Option<String>,

    /// Scenario type
    pub scenario_type: PerformanceScenarioType,

    /// Scenario parameters
    pub parameters: HashMap<String, serde_json::Value>,

    /// Scenario duration in seconds
    pub duration_seconds: u64,

    /// Scenario concurrency
    pub concurrency: u32,
}

/// Performance scenario types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceScenarioType {
    /// Load test scenario
    LoadTest,

    /// Stress test scenario
    StressTest,

    /// Endurance test scenario
    EnduranceTest,

    /// Spike test scenario
    SpikeTest,

    /// Volume test scenario
    VolumeTest,
}

/// Performance thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThreshold {
    /// Threshold name
    pub name: String,

    /// Threshold value
    pub value: f64,

    /// Threshold unit
    pub unit: String,

    /// Threshold operator
    pub operator: ThresholdOperator,
}

/// Threshold operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThresholdOperator {
    /// Less than
    LessThan,

    /// Less than or equal
    LessThanOrEqual,

    /// Greater than
    GreaterThan,

    /// Greater than or equal
    GreaterThanOrEqual,

    /// Equal
    Equal,

    /// Not equal
    NotEqual,
}

/// Compliance standards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceStandard {
    /// Standard name
    pub name: String,

    /// Standard version
    pub version: String,

    /// Standard description
    pub description: Option<String>,

    /// Standard requirements
    pub requirements: Vec<ComplianceRequirement>,
}

/// Compliance requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRequirement {
    /// Requirement identifier
    pub id: String,

    /// Requirement name
    pub name: String,

    /// Requirement description
    pub description: Option<String>,

    /// Requirement type
    pub requirement_type: ComplianceRequirementType,

    /// Requirement severity
    pub severity: ComplianceSeverity,
}

/// Compliance requirement types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceRequirementType {
    /// Mandatory requirement
    Mandatory,

    /// Recommended requirement
    Recommended,

    /// Optional requirement
    Optional,
}

/// Compliance severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceSeverity {
    /// Critical severity
    Critical,

    /// High severity
    High,

    /// Medium severity
    Medium,

    /// Low severity
    Low,
}

/// Compliance policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompliancePolicy {
    /// Policy name
    pub name: String,

    /// Policy description
    pub description: Option<String>,

    /// Policy rules
    pub rules: Vec<ComplianceRule>,

    /// Policy enforcement
    pub enforcement: ComplianceEnforcement,
}

/// Compliance rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRule {
    /// Rule identifier
    pub id: String,

    /// Rule name
    pub name: String,

    /// Rule description
    pub description: Option<String>,

    /// Rule conditions
    pub conditions: Vec<ComplianceCondition>,

    /// Rule actions
    pub actions: Vec<ComplianceAction>,
}

/// Compliance conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCondition {
    /// Condition field
    pub field: String,

    /// Condition operator
    pub operator: ComplianceOperator,

    /// Condition value
    pub value: serde_json::Value,
}

/// Compliance operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceOperator {
    /// Equals
    Equals,

    /// Not equals
    NotEquals,

    /// Contains
    Contains,

    /// Not contains
    NotContains,

    /// Greater than
    GreaterThan,

    /// Less than
    LessThan,
}

/// Compliance actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceAction {
    /// Allow action
    Allow,

    /// Deny action
    Deny,

    /// Warn action
    Warn,

    /// Log action
    Log,

    /// Notify action
    Notify { notification_type: String },
}

/// Compliance enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceEnforcement {
    /// Strict enforcement
    Strict,

    /// Lenient enforcement
    Lenient,

    /// Warning only
    WarningOnly,
}

/// Compliance checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCheck {
    /// Check identifier
    pub id: String,

    /// Check name
    pub name: String,

    /// Check description
    pub description: Option<String>,

    /// Check type
    pub check_type: ComplianceCheckType,

    /// Check parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Compliance check types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceCheckType {
    /// Code quality check
    CodeQuality,

    /// Security check
    Security,

    /// Performance check
    Performance,

    /// Documentation check
    Documentation,

    /// License check
    License,

    /// Dependency check
    Dependency,
}

/// Compliance reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReporting {
    /// Enable compliance reporting
    pub enable_reporting: bool,

    /// Report format
    pub report_format: OutputFormat,

    /// Report output path
    pub report_output_path: Option<String>,

    /// Report templates
    pub report_templates: Vec<String>,

    /// Report scheduling
    pub report_scheduling: Option<ReportScheduling>,
}

/// Report scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportScheduling {
    /// Schedule type
    pub schedule_type: ScheduleType,

    /// Schedule parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Schedule types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScheduleType {
    /// Daily schedule
    Daily,

    /// Weekly schedule
    Weekly,

    /// Monthly schedule
    Monthly,

    /// Custom schedule
    Custom { cron_expression: String },
}

/// Integrity algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntegrityAlgorithm {
    /// SHA-256 algorithm
    Sha256,

    /// SHA-512 algorithm
    Sha512,

    /// BLAKE3 algorithm
    Blake3,

    /// MD5 algorithm
    Md5,
}

/// Deployment environments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeploymentEnvironment {
    /// Development environment
    Development,

    /// Staging environment
    Staging,

    /// Production environment
    Production,

    /// Testing environment
    Testing,
}

/// Deployment targets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentTarget {
    /// Target name
    pub name: String,

    /// Target type
    pub target_type: DeploymentTargetType,

    /// Target configuration
    pub configuration: HashMap<String, serde_json::Value>,
}

/// Deployment target types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeploymentTargetType {
    /// Kubernetes deployment
    Kubernetes,

    /// Docker deployment
    Docker,

    /// Bare metal deployment
    BareMetal,

    /// Cloud deployment
    Cloud { provider: String },
}

/// Code quality report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQualityReport {
    /// Overall quality score
    pub overall_score: f64,

    /// Quality metrics
    pub metrics: HashMap<String, QualityMetric>,

    /// Quality issues
    pub issues: Vec<QualityIssue>,

    /// Quality recommendations
    pub recommendations: Vec<QualityRecommendation>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetric {
    /// Metric name
    pub name: String,

    /// Metric value
    pub value: f64,

    /// Metric unit
    pub unit: String,

    /// Metric threshold
    pub threshold: Option<f64>,

    /// Metric status
    pub status: MetricStatus,
}

/// Metric statuses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricStatus {
    /// Pass status
    Pass,

    /// Fail status
    Fail,

    /// Warning status
    Warning,

    /// Unknown status
    Unknown,
}

/// Quality issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    /// Issue identifier
    pub id: String,

    /// Issue type
    pub issue_type: String,

    /// Issue severity
    pub severity: IssueSeverity,

    /// Issue message
    pub message: String,

    /// Issue location
    pub location: Option<IssueLocation>,

    /// Issue details
    pub details: Option<serde_json::Value>,
}

/// Issue severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueSeverity {
    /// Critical severity
    Critical,

    /// High severity
    High,

    /// Medium severity
    Medium,

    /// Low severity
    Low,

    /// Info severity
    Info,
}

/// Issue locations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLocation {
    /// File path
    pub file_path: String,

    /// Line number
    pub line_number: Option<u32>,

    /// Column number
    pub column_number: Option<u32>,

    /// Function name
    pub function_name: Option<String>,
}

/// Quality recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityRecommendation {
    /// Recommendation identifier
    pub id: String,

    /// Recommendation type
    pub recommendation_type: String,

    /// Recommendation message
    pub message: String,

    /// Recommendation priority
    pub priority: RecommendationPriority,

    /// Recommendation details
    pub details: Option<serde_json::Value>,
}

/// Recommendation priorities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    /// High priority
    High,

    /// Medium priority
    Medium,

    /// Low priority
    Low,
}

/// Security report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    /// Overall security score
    pub overall_score: f64,

    /// Security vulnerabilities
    pub vulnerabilities: Vec<SecurityVulnerability>,

    /// Security recommendations
    pub recommendations: Vec<SecurityRecommendation>,

    /// Security metrics
    pub metrics: HashMap<String, SecurityMetric>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Security vulnerabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityVulnerability {
    /// Vulnerability identifier
    pub id: String,

    /// Vulnerability name
    pub name: String,

    /// Vulnerability description
    pub description: Option<String>,

    /// Vulnerability severity
    pub severity: SecuritySeverity,

    /// Vulnerability type
    pub vulnerability_type: String,

    /// Vulnerability location
    pub location: Option<IssueLocation>,

    /// Vulnerability details
    pub details: Option<serde_json::Value>,
}

/// Security recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRecommendation {
    /// Recommendation identifier
    pub id: String,

    /// Recommendation type
    pub recommendation_type: String,

    /// Recommendation message
    pub message: String,

    /// Recommendation priority
    pub priority: RecommendationPriority,

    /// Recommendation details
    pub details: Option<serde_json::Value>,
}

/// Security metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMetric {
    /// Metric name
    pub name: String,

    /// Metric value
    pub value: f64,

    /// Metric unit
    pub unit: String,

    /// Metric threshold
    pub threshold: Option<f64>,

    /// Metric status
    pub status: MetricStatus,
}

/// Performance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    /// Overall performance score
    pub overall_score: f64,

    /// Performance metrics
    pub metrics: HashMap<String, PerformanceMetric>,

    /// Performance issues
    pub issues: Vec<PerformanceIssue>,

    /// Performance recommendations
    pub recommendations: Vec<PerformanceRecommendation>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetric {
    /// Metric name
    pub name: String,

    /// Metric value
    pub value: f64,

    /// Metric unit
    pub unit: String,

    /// Metric threshold
    pub threshold: Option<f64>,

    /// Metric status
    pub status: MetricStatus,
}

/// Performance issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceIssue {
    /// Issue identifier
    pub id: String,

    /// Issue type
    pub issue_type: String,

    /// Issue severity
    pub severity: IssueSeverity,

    /// Issue message
    pub message: String,

    /// Issue location
    pub location: Option<IssueLocation>,

    /// Issue details
    pub details: Option<serde_json::Value>,
}

/// Performance recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecommendation {
    /// Recommendation identifier
    pub id: String,

    /// Recommendation type
    pub recommendation_type: String,

    /// Recommendation message
    pub message: String,

    /// Recommendation priority
    pub priority: RecommendationPriority,

    /// Recommendation details
    pub details: Option<serde_json::Value>,
}

/// Compliance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// Overall compliance score
    pub overall_score: f64,

    /// Compliance status
    pub compliance_status: ComplianceStatus,

    /// Compliance violations
    pub violations: Vec<ComplianceViolation>,

    /// Compliance recommendations
    pub recommendations: Vec<ComplianceRecommendation>,

    /// Compliance metrics
    pub metrics: HashMap<String, ComplianceMetric>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Compliance statuses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComplianceStatus {
    /// Compliant status
    Compliant,

    /// Non-compliant status
    NonCompliant,

    /// Partially compliant status
    PartiallyCompliant,

    /// Unknown status
    Unknown,
}

/// Compliance violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceViolation {
    /// Violation identifier
    pub id: String,

    /// Violation type
    pub violation_type: String,

    /// Violation severity
    pub severity: ComplianceSeverity,

    /// Violation message
    pub message: String,

    /// Violation location
    pub location: Option<IssueLocation>,

    /// Violation details
    pub details: Option<serde_json::Value>,
}

/// Compliance recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRecommendation {
    /// Recommendation identifier
    pub id: String,

    /// Recommendation type
    pub recommendation_type: String,

    /// Recommendation message
    pub message: String,

    /// Recommendation priority
    pub priority: RecommendationPriority,

    /// Recommendation details
    pub details: Option<serde_json::Value>,
}

/// Compliance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceMetric {
    /// Metric name
    pub name: String,

    /// Metric value
    pub value: f64,

    /// Metric unit
    pub unit: String,

    /// Metric threshold
    pub threshold: Option<f64>,

    /// Metric status
    pub status: MetricStatus,
}

/// Integrity report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    /// Overall integrity score
    pub overall_score: f64,

    /// Integrity checks
    pub integrity_checks: Vec<IntegrityCheck>,

    /// Integrity violations
    pub violations: Vec<IntegrityViolation>,

    /// Integrity recommendations
    pub recommendations: Vec<IntegrityRecommendation>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Integrity checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheck {
    /// Check identifier
    pub id: String,

    /// Check name
    pub name: String,

    /// Check status
    pub status: CheckStatus,

    /// Check result
    pub result: Option<String>,

    /// Check details
    pub details: Option<serde_json::Value>,
}

/// Check statuses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckStatus {
    /// Pass status
    Pass,

    /// Fail status
    Fail,

    /// Warning status
    Warning,

    /// Unknown status
    Unknown,
}

/// Integrity violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityViolation {
    /// Violation identifier
    pub id: String,

    /// Violation type
    pub violation_type: String,

    /// Violation severity
    pub severity: IssueSeverity,

    /// Violation message
    pub message: String,

    /// Violation location
    pub location: Option<IssueLocation>,

    /// Violation details
    pub details: Option<serde_json::Value>,
}

/// Integrity recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityRecommendation {
    /// Recommendation identifier
    pub id: String,

    /// Recommendation type
    pub recommendation_type: String,

    /// Recommendation message
    pub message: String,

    /// Recommendation priority
    pub priority: RecommendationPriority,

    /// Recommendation details
    pub details: Option<serde_json::Value>,
}

/// Deployment report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentReport {
    /// Overall deployment readiness score
    pub overall_score: f64,

    /// Deployment readiness status
    pub readiness_status: DeploymentReadinessStatus,

    /// Deployment checks
    pub deployment_checks: Vec<DeploymentCheck>,

    /// Deployment issues
    pub issues: Vec<DeploymentIssue>,

    /// Deployment recommendations
    pub recommendations: Vec<DeploymentRecommendation>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Deployment readiness statuses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeploymentReadinessStatus {
    /// Ready status
    Ready,

    /// Not ready status
    NotReady,

    /// Partially ready status
    PartiallyReady,

    /// Unknown status
    Unknown,
}

/// Deployment checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentCheck {
    /// Check identifier
    pub id: String,

    /// Check name
    pub name: String,

    /// Check status
    pub status: CheckStatus,

    /// Check result
    pub result: Option<String>,

    /// Check details
    pub details: Option<serde_json::Value>,
}

/// Deployment issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentIssue {
    /// Issue identifier
    pub id: String,

    /// Issue type
    pub issue_type: String,

    /// Issue severity
    pub severity: IssueSeverity,

    /// Issue message
    pub message: String,

    /// Issue location
    pub location: Option<IssueLocation>,

    /// Issue details
    pub details: Option<serde_json::Value>,
}

/// Deployment recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecommendation {
    /// Recommendation identifier
    pub id: String,

    /// Recommendation type
    pub recommendation_type: String,

    /// Recommendation message
    pub message: String,

    /// Recommendation priority
    pub priority: RecommendationPriority,

    /// Recommendation details
    pub details: Option<serde_json::Value>,
}

/// Comprehensive report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComprehensiveReport {
    /// Overall verification score
    pub overall_score: f64,

    /// Verification status
    pub verification_status: VerificationStatus,

    /// Code quality report
    pub code_quality: CodeQualityReport,

    /// Security report
    pub security: SecurityReport,

    /// Performance report
    pub performance: PerformanceReport,

    /// Compliance report
    pub compliance: ComplianceReport,

    /// Integrity report
    pub integrity: IntegrityReport,

    /// Deployment report
    pub deployment: DeploymentReport,

    /// Verification summary
    pub summary: VerificationSummary,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Verification statuses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VerificationStatus {
    /// Pass status
    Pass,

    /// Fail status
    Fail,

    /// Warning status
    Warning,

    /// Unknown status
    Unknown,
}

/// Verification summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationSummary {
    /// Total checks performed
    pub total_checks: u32,

    /// Passed checks
    pub passed_checks: u32,

    /// Failed checks
    pub failed_checks: u32,

    /// Warning checks
    pub warning_checks: u32,

    /// Success rate
    pub success_rate: f64,

    /// Verification duration in milliseconds
    pub verification_duration_ms: u64,
}

impl VerificationStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pass" | "passed" => Some(VerificationStatus::Pass),
            "warning" | "warn" => Some(VerificationStatus::Warning),
            "fail" | "failed" => Some(VerificationStatus::Fail),
            _ => Some(VerificationStatus::Unknown),
        }
    }
}

impl IssueSeverity {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "critical" => Some(IssueSeverity::Critical),
            "high" => Some(IssueSeverity::High),
            "medium" => Some(IssueSeverity::Medium),
            "low" => Some(IssueSeverity::Low),
            "info" => Some(IssueSeverity::Info),
            _ => None,
        }
    }
}

impl SecuritySeverity {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "critical" => Some(SecuritySeverity::Critical),
            "high" => Some(SecuritySeverity::High),
            "medium" => Some(SecuritySeverity::Medium),
            "low" => Some(SecuritySeverity::Low),
            "info" => Some(SecuritySeverity::Info),
            _ => None,
        }
    }
}

impl RecommendationPriority {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "high" => Some(RecommendationPriority::High),
            "medium" => Some(RecommendationPriority::Medium),
            "low" => Some(RecommendationPriority::Low),
            _ => None,
        }
    }
}

/// Unified verification framework implementation
pub struct UnifiedVerificationFramework {
    /// Verification configuration
    #[allow(dead_code)]
    config: VerificationConfig,

    /// Verification history
    #[allow(dead_code)]
    verification_history: Vec<ComprehensiveReport>,

    /// Workspace root used for executing verification tooling
    workspace_root: PathBuf,
}

impl UnifiedVerificationFramework {
    /// Create a new unified verification framework
    pub fn new(config: VerificationConfig) -> Self {
        let workspace_root = std::env::var("AOS_WORKSPACE_ROOT")
            .map(PathBuf::from)
            .or_else(|_| std::env::current_dir())
            .unwrap_or_else(|_| PathBuf::from("."));

        Self {
            config,
            verification_history: Vec::new(),
            workspace_root,
        }
    }

    /// Create a new framework with an explicit workspace root. Primarily used for testing.
    pub fn with_workspace_root(
        config: VerificationConfig,
        workspace_root: impl AsRef<Path>,
    ) -> Self {
        Self {
            config,
            verification_history: Vec::new(),
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.workspace_root().join(candidate)
        }
    }

    fn build_code_quality_metrics(
        &self,
        result: &CodeQualityResult,
        config: &CodeQualityConfig,
    ) -> HashMap<String, QualityMetric> {
        let mut metrics = HashMap::new();

        metrics.insert(
            "clippy_warnings".to_string(),
            QualityMetric {
                name: "Clippy Warnings".to_string(),
                value: result.clippy_results.warnings as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if !config.enable_clippy {
                    MetricStatus::Unknown
                } else if result.clippy_results.warnings == 0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics.insert(
            "clippy_errors".to_string(),
            QualityMetric {
                name: "Clippy Errors".to_string(),
                value: result.clippy_results.errors as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if !config.enable_clippy {
                    MetricStatus::Unknown
                } else if result.clippy_results.errors == 0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Fail
                },
            },
        );

        metrics.insert(
            "formatting".to_string(),
            QualityMetric {
                name: "Rustfmt Compliance".to_string(),
                value: if result.format_results.passed {
                    1.0
                } else {
                    0.0
                },
                unit: "pass".to_string(),
                threshold: Some(1.0),
                status: if !config.enable_format {
                    MetricStatus::Unknown
                } else if result.format_results.passed {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Fail
                },
            },
        );

        let coverage_threshold = if config.enable_coverage {
            Some(config.min_coverage_percentage)
        } else {
            None
        };
        metrics.insert(
            "coverage_overall".to_string(),
            QualityMetric {
                name: "Overall Test Coverage".to_string(),
                value: result.coverage_results.overall_coverage,
                unit: "%".to_string(),
                threshold: coverage_threshold,
                status: if !config.enable_coverage {
                    MetricStatus::Unknown
                } else if result.coverage_results.overall_coverage >= config.min_coverage_percentage
                {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Fail
                },
            },
        );

        metrics.insert(
            "complexity_peak".to_string(),
            QualityMetric {
                name: "Max Cyclomatic Complexity".to_string(),
                value: result.complexity_results.max_cyclomatic_complexity as f64,
                unit: "score".to_string(),
                threshold: if config.enable_complexity {
                    Some(config.max_cyclomatic_complexity as f64)
                } else {
                    None
                },
                status: if !config.enable_complexity {
                    MetricStatus::Unknown
                } else if result.complexity_results.max_cyclomatic_complexity
                    <= config.max_cyclomatic_complexity
                {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics.insert(
            "documentation_coverage".to_string(),
            QualityMetric {
                name: "Documentation Coverage".to_string(),
                value: result.documentation_results.coverage,
                unit: "%".to_string(),
                threshold: Some(80.0),
                status: if !config.enable_documentation {
                    MetricStatus::Unknown
                } else if result.documentation_results.coverage >= 80.0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics.insert(
            "dead_code".to_string(),
            QualityMetric {
                name: "Dead Code Percentage".to_string(),
                value: result.dead_code_results.percentage,
                unit: "%".to_string(),
                threshold: Some(5.0),
                status: if !config.enable_dead_code {
                    MetricStatus::Unknown
                } else if result.dead_code_results.percentage <= 5.0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics
    }

    fn build_security_metrics(&self, result: &SecurityResult) -> HashMap<String, SecurityMetric> {
        let mut metrics = HashMap::new();
        let vulnerabilities = &result.vulnerability_results;
        let dependencies = &result.dependency_results;
        let code_security = &result.code_security_results;
        let compliance = &result.compliance_results;

        metrics.insert(
            "critical_vulnerabilities".to_string(),
            SecurityMetric {
                name: "Critical Vulnerabilities".to_string(),
                value: vulnerabilities.critical as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if vulnerabilities.critical == 0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Fail
                },
            },
        );

        metrics.insert(
            "high_vulnerabilities".to_string(),
            SecurityMetric {
                name: "High Vulnerabilities".to_string(),
                value: vulnerabilities.high as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if vulnerabilities.high == 0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics.insert(
            "vulnerable_dependencies".to_string(),
            SecurityMetric {
                name: "Vulnerable Dependencies".to_string(),
                value: dependencies.vulnerable_dependencies as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if dependencies.vulnerable_dependencies == 0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics.insert(
            "outdated_dependencies".to_string(),
            SecurityMetric {
                name: "Outdated Dependencies".to_string(),
                value: dependencies.outdated_dependencies as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if dependencies.outdated_dependencies == 0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics.insert(
            "unsafe_code".to_string(),
            SecurityMetric {
                name: "Unsafe Code Blocks".to_string(),
                value: code_security.unsafe_code_count as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if code_security.unsafe_code_count == 0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics.insert(
            "security_compliance".to_string(),
            SecurityMetric {
                name: format!("{} Compliance Score", compliance.standard),
                value: compliance.score,
                unit: "score".to_string(),
                threshold: Some(90.0),
                status: if compliance.score >= 90.0 {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        metrics
    }

    fn build_performance_metrics(
        &self,
        result: &PerformanceResult,
        config: &PerformanceConfig,
    ) -> HashMap<String, PerformanceMetric> {
        let mut metrics = HashMap::new();
        let thresholds = &config.performance_thresholds;

        let pass_rate = if result.benchmark_results.total_benchmarks == 0 {
            100.0
        } else {
            (result.benchmark_results.passed_benchmarks as f64
                / result.benchmark_results.total_benchmarks as f64)
                * 100.0
        };
        metrics.insert(
            "benchmark_pass_rate".to_string(),
            self.performance_metric(
                "Benchmark Pass Rate",
                pass_rate,
                "%",
                thresholds.get("benchmark_pass_rate"),
            ),
        );

        metrics.insert(
            "avg_execution_time_ms".to_string(),
            self.performance_metric(
                "Average Execution Time",
                result.benchmark_results.avg_execution_time_ms,
                "ms",
                thresholds.get("avg_execution_time_ms"),
            ),
        );

        metrics.insert(
            "latency_p95_ms".to_string(),
            self.performance_metric(
                "P95 Latency",
                result.latency_results.p95_latency_ms,
                "ms",
                thresholds.get("latency_p95_ms"),
            ),
        );

        metrics.insert(
            "throughput_ops".to_string(),
            self.performance_metric(
                "Throughput",
                result.throughput_results.ops_per_second,
                "ops/s",
                thresholds.get("throughput_ops"),
            ),
        );

        metrics.insert(
            "cpu_utilization".to_string(),
            self.performance_metric(
                "CPU Utilization",
                result.resource_results.cpu_utilization_percent,
                "%",
                thresholds.get("cpu_utilization"),
            ),
        );

        metrics.insert(
            "memory_utilization".to_string(),
            self.performance_metric(
                "Memory Utilization",
                result.resource_results.memory_utilization_percent,
                "%",
                thresholds.get("memory_utilization"),
            ),
        );

        metrics
    }

    fn performance_metric(
        &self,
        display_name: &str,
        value: f64,
        unit: &str,
        threshold: Option<&PerformanceThreshold>,
    ) -> PerformanceMetric {
        if let Some(threshold) = threshold {
            PerformanceMetric {
                name: display_name.to_string(),
                value,
                unit: unit.to_string(),
                threshold: Some(threshold.value),
                status: self.evaluate_performance_threshold(value, threshold),
            }
        } else {
            PerformanceMetric {
                name: display_name.to_string(),
                value,
                unit: unit.to_string(),
                threshold: None,
                status: MetricStatus::Unknown,
            }
        }
    }

    fn evaluate_performance_threshold(
        &self,
        value: f64,
        threshold: &PerformanceThreshold,
    ) -> MetricStatus {
        let meets = match threshold.operator {
            ThresholdOperator::LessThan => value < threshold.value,
            ThresholdOperator::LessThanOrEqual => value <= threshold.value,
            ThresholdOperator::GreaterThan => value > threshold.value,
            ThresholdOperator::GreaterThanOrEqual => value >= threshold.value,
            ThresholdOperator::Equal => (value - threshold.value).abs() < f64::EPSILON,
            ThresholdOperator::NotEqual => (value - threshold.value).abs() >= f64::EPSILON,
        };

        if meets {
            MetricStatus::Pass
        } else {
            MetricStatus::Fail
        }
    }

    fn priority_from_issue_severity(&self, severity: IssueSeverity) -> RecommendationPriority {
        match severity {
            IssueSeverity::Critical | IssueSeverity::High => RecommendationPriority::High,
            IssueSeverity::Medium => RecommendationPriority::Medium,
            IssueSeverity::Low | IssueSeverity::Info => RecommendationPriority::Low,
        }
    }

    fn priority_from_compliance_severity(
        &self,
        severity: &ComplianceSeverity,
    ) -> RecommendationPriority {
        match severity {
            ComplianceSeverity::Critical | ComplianceSeverity::High => RecommendationPriority::High,
            ComplianceSeverity::Medium => RecommendationPriority::Medium,
            ComplianceSeverity::Low => RecommendationPriority::Low,
        }
    }

    fn compliance_requirement_type_name(
        &self,
        requirement_type: &ComplianceRequirementType,
    ) -> &'static str {
        match requirement_type {
            ComplianceRequirementType::Mandatory => "mandatory",
            ComplianceRequirementType::Recommended => "recommended",
            ComplianceRequirementType::Optional => "optional",
        }
    }

    fn clamp_score(&self, score: f64) -> f64 {
        score.max(0.0).min(100.0)
    }
}

fn integrity_algorithm_name(algorithm: &IntegrityAlgorithm) -> &'static str {
    match algorithm {
        IntegrityAlgorithm::Sha256 => "sha256",
        IntegrityAlgorithm::Sha512 => "sha512",
        IntegrityAlgorithm::Blake3 => "blake3",
        IntegrityAlgorithm::Md5 => "md5",
    }
}

fn compute_integrity_digest(path: &Path, algorithm: &IntegrityAlgorithm) -> Result<String> {
    let data = fs::read(path)
        .map_err(|err| AosError::Io(format!("Failed to read {}: {}", path.display(), err)))?;

    let digest = match algorithm {
        IntegrityAlgorithm::Sha256 => format!("{:x}", Sha256::digest(&data)),
        IntegrityAlgorithm::Sha512 => format!("{:x}", Sha512::digest(&data)),
        IntegrityAlgorithm::Blake3 => blake3_hash(&data).to_hex().to_string(),
        IntegrityAlgorithm::Md5 => {
            let mut hasher = Md5::new();
            hasher.update(&data);
            format!("{:x}", hasher.finalize())
        }
    };

    Ok(digest)
}

#[async_trait]
impl VerificationFramework for UnifiedVerificationFramework {
    async fn verify_code_quality(&self, config: &CodeQualityConfig) -> Result<CodeQualityReport> {
        info!("Starting code quality verification");

        let verifier = CodeQualityVerifier::new(self.workspace_root());
        let result = verifier.verify(config).await?;

        let metrics = self.build_code_quality_metrics(&result, config);
        let issues = result
            .issues
            .iter()
            .enumerate()
            .map(|(idx, issue)| QualityIssue {
                id: format!("code-quality-issue-{}", idx + 1),
                issue_type: issue.issue_type.clone(),
                severity: IssueSeverity::from_str(&issue.severity).unwrap_or(IssueSeverity::Medium),
                message: issue.description.clone(),
                location: Some(IssueLocation {
                    file_path: issue.file.clone(),
                    line_number: Some(issue.line),
                    column_number: Some(issue.column),
                    function_name: None,
                }),
                details: issue
                    .suggestion
                    .as_ref()
                    .map(|suggestion| json!({ "suggestion": suggestion })),
            })
            .collect();

        let recommendations = result
            .recommendations
            .iter()
            .enumerate()
            .map(|(idx, message)| QualityRecommendation {
                id: format!("code-quality-rec-{}", idx + 1),
                recommendation_type: "code_quality".to_string(),
                message: message.clone(),
                priority: RecommendationPriority::Medium,
                details: None,
            })
            .collect();

        let report = CodeQualityReport {
            overall_score: result.score,
            metrics,
            issues,
            recommendations,
            timestamp: result.timestamp,
        };

        info!(
            overall_score = report.overall_score,
            "Code quality verification completed"
        );
        Ok(report)
    }

    async fn verify_security(&self, config: &SecurityConfig) -> Result<SecurityReport> {
        info!("Starting security verification");

        let verifier = SecurityVerifier::new(self.workspace_root());
        let result = verifier.verify(config).await?;

        let vulnerabilities = result
            .vulnerability_results
            .vulnerabilities
            .iter()
            .enumerate()
            .map(|(idx, vuln)| SecurityVulnerability {
                id: if vuln.id.is_empty() {
                    format!("security-vuln-{}", idx + 1)
                } else {
                    vuln.id.clone()
                },
                name: vuln.title.clone(),
                description: Some(vuln.description.clone()),
                severity: SecuritySeverity::from_str(&vuln.severity)
                    .unwrap_or(SecuritySeverity::Medium),
                vulnerability_type: "dependency".to_string(),
                location: Some(IssueLocation {
                    file_path: vuln.package.clone(),
                    line_number: None,
                    column_number: None,
                    function_name: None,
                }),
                details: Some(json!({
                    "package": vuln.package,
                    "version": vuln.version,
                    "fixed_version": vuln.fixed_version,
                    "references": vuln.references,
                })),
            })
            .collect();

        let recommendations = result
            .recommendations
            .iter()
            .enumerate()
            .map(|(idx, message)| SecurityRecommendation {
                id: format!("security-rec-{}", idx + 1),
                recommendation_type: "security".to_string(),
                message: message.clone(),
                priority: RecommendationPriority::High,
                details: None,
            })
            .collect();

        let metrics = self.build_security_metrics(&result);

        let report = SecurityReport {
            overall_score: result.score,
            vulnerabilities,
            recommendations,
            metrics,
            timestamp: result.timestamp,
        };

        info!(
            overall_score = report.overall_score,
            "Security verification completed"
        );
        Ok(report)
    }

    async fn verify_performance(&self, config: &PerformanceConfig) -> Result<PerformanceReport> {
        info!("Starting performance verification");

        let verifier = PerformanceVerifier::new(self.workspace_root());
        let result = verifier.verify(config).await?;

        let metrics = self.build_performance_metrics(&result, config);
        let issues = result
            .issues
            .iter()
            .enumerate()
            .map(|(idx, issue)| PerformanceIssue {
                id: format!("performance-issue-{}", idx + 1),
                issue_type: issue.issue_type.clone(),
                severity: IssueSeverity::from_str(&issue.severity).unwrap_or(IssueSeverity::Medium),
                message: issue.description.clone(),
                location: Some(IssueLocation {
                    file_path: issue.component.clone(),
                    line_number: None,
                    column_number: None,
                    function_name: None,
                }),
                details: Some(json!({
                    "performance_impact": issue.performance_impact,
                    "suggestion": issue.suggestion,
                })),
            })
            .collect();

        let recommendations = result
            .recommendations
            .iter()
            .enumerate()
            .map(|(idx, message)| PerformanceRecommendation {
                id: format!("performance-rec-{}", idx + 1),
                recommendation_type: "performance".to_string(),
                message: message.clone(),
                priority: RecommendationPriority::Medium,
                details: None,
            })
            .collect();

        let report = PerformanceReport {
            overall_score: result.score,
            metrics,
            issues,
            recommendations,
            timestamp: result.timestamp,
        };

        info!(
            overall_score = report.overall_score,
            "Performance verification completed"
        );
        Ok(report)
    }

    async fn verify_compliance(&self, config: &ComplianceConfig) -> Result<ComplianceReport> {
        info!("Starting compliance verification");

        let mut violations = Vec::new();
        let mut recommendations = Vec::new();
        let mut metrics = HashMap::new();

        let mut total_requirements = 0usize;
        let mut satisfied_requirements = 0usize;

        for standard in &config.compliance_standards {
            let mut satisfied_for_standard = 0usize;

            for requirement in &standard.requirements {
                total_requirements += 1;
                let matched = config.compliance_checks.iter().any(|check| {
                    check.id == requirement.id
                        || check
                            .name
                            .to_lowercase()
                            .contains(&requirement.name.to_lowercase())
                });

                if matched {
                    satisfied_requirements += 1;
                    satisfied_for_standard += 1;
                } else {
                    let violation_id = format!(
                        "{}-{}",
                        standard.name.to_lowercase().replace(' ', "_"),
                        requirement.id
                    );
                    violations.push(ComplianceViolation {
                        id: violation_id.clone(),
                        violation_type: self
                            .compliance_requirement_type_name(&requirement.requirement_type)
                            .to_string(),
                        severity: requirement.severity.clone(),
                        message: requirement.description.clone().unwrap_or_else(|| {
                            format!("Requirement '{}' not satisfied", requirement.name)
                        }),
                        location: None,
                        details: Some(json!({
                            "standard": standard.name,
                            "requirement_id": requirement.id,
                            "requirement_name": requirement.name,
                        })),
                    });

                    recommendations.push(ComplianceRecommendation {
                        id: format!("rec-{}", violation_id),
                        recommendation_type: "compliance".to_string(),
                        message: format!(
                            "Implement control '{}' for standard '{}'",
                            requirement.name, standard.name
                        ),
                        priority: self.priority_from_compliance_severity(&requirement.severity),
                        details: Some(json!({
                            "requirement_id": requirement.id,
                            "standard": standard.name,
                        })),
                    });
                }
            }

            let coverage = if standard.requirements.is_empty() {
                100.0
            } else {
                (satisfied_for_standard as f64 / standard.requirements.len() as f64) * 100.0
            };

            let threshold = config.compliance_thresholds.get("coverage").copied();
            metrics.insert(
                format!(
                    "{}_coverage",
                    standard.name.to_lowercase().replace(' ', "_")
                ),
                ComplianceMetric {
                    name: format!("{} Coverage", standard.name),
                    value: coverage,
                    unit: "%".to_string(),
                    threshold,
                    status: if standard.requirements.is_empty() {
                        MetricStatus::Unknown
                    } else if let Some(threshold) = threshold {
                        if coverage >= threshold {
                            MetricStatus::Pass
                        } else {
                            MetricStatus::Warning
                        }
                    } else if coverage >= 80.0 {
                        MetricStatus::Pass
                    } else {
                        MetricStatus::Warning
                    },
                },
            );
        }

        if config.compliance_policies.is_empty() {
            violations.push(ComplianceViolation {
                id: "missing-policies".to_string(),
                violation_type: "policy".to_string(),
                severity: ComplianceSeverity::High,
                message: "No compliance policies defined".to_string(),
                location: None,
                details: None,
            });
            recommendations.push(ComplianceRecommendation {
                id: "rec-missing-policies".to_string(),
                recommendation_type: "policy".to_string(),
                message: "Define baseline compliance policies".to_string(),
                priority: RecommendationPriority::High,
                details: None,
            });
        }

        if config.compliance_checks.is_empty() {
            violations.push(ComplianceViolation {
                id: "missing-checks".to_string(),
                violation_type: "validation".to_string(),
                severity: ComplianceSeverity::Medium,
                message: "No compliance checks configured".to_string(),
                location: None,
                details: None,
            });
            recommendations.push(ComplianceRecommendation {
                id: "rec-missing-checks".to_string(),
                recommendation_type: "validation".to_string(),
                message: "Configure automated compliance checks".to_string(),
                priority: RecommendationPriority::Medium,
                details: None,
            });
        }

        metrics.insert(
            "policy_count".to_string(),
            ComplianceMetric {
                name: "Compliance Policies".to_string(),
                value: config.compliance_policies.len() as f64,
                unit: "count".to_string(),
                threshold: Some(1.0),
                status: if config.compliance_policies.is_empty() {
                    MetricStatus::Fail
                } else {
                    MetricStatus::Pass
                },
            },
        );

        metrics.insert(
            "check_count".to_string(),
            ComplianceMetric {
                name: "Compliance Checks".to_string(),
                value: config.compliance_checks.len() as f64,
                unit: "count".to_string(),
                threshold: Some(1.0),
                status: if config.compliance_checks.is_empty() {
                    MetricStatus::Warning
                } else {
                    MetricStatus::Pass
                },
            },
        );

        metrics.insert(
            "violations".to_string(),
            ComplianceMetric {
                name: "Compliance Violations".to_string(),
                value: violations.len() as f64,
                unit: "count".to_string(),
                threshold: Some(0.0),
                status: if violations.is_empty() {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Warning
                },
            },
        );

        if config.compliance_reporting.enable_reporting {
            metrics.insert(
                "reporting".to_string(),
                ComplianceMetric {
                    name: "Compliance Reporting Enabled".to_string(),
                    value: 1.0,
                    unit: "flag".to_string(),
                    threshold: Some(1.0),
                    status: MetricStatus::Pass,
                },
            );
        }

        let overall_score = if total_requirements == 0 {
            100.0
        } else {
            (satisfied_requirements as f64 / total_requirements as f64) * 100.0
        };

        let pass_threshold = config
            .compliance_thresholds
            .get("overall_score")
            .copied()
            .unwrap_or(85.0);
        let warn_threshold = config
            .compliance_thresholds
            .get("warning_score")
            .copied()
            .unwrap_or(70.0);

        let compliance_status = if overall_score >= pass_threshold {
            ComplianceStatus::Compliant
        } else if overall_score >= warn_threshold {
            ComplianceStatus::PartiallyCompliant
        } else {
            ComplianceStatus::NonCompliant
        };

        let report = ComplianceReport {
            overall_score: self.clamp_score(overall_score),
            compliance_status,
            violations,
            recommendations,
            metrics,
            timestamp: chrono::Utc::now(),
        };

        info!(
            overall_score = report.overall_score,
            "Compliance verification completed"
        );
        Ok(report)
    }

    async fn verify_system_integrity(&self, config: &IntegrityConfig) -> Result<IntegrityReport> {
        info!("Starting system integrity verification");

        let mut checks = Vec::new();
        let mut violations = Vec::new();
        let mut score = 100.0;

        if config.integrity_paths.is_empty() {
            checks.push(IntegrityCheck {
                id: "integrity-no-paths".to_string(),
                name: "Integrity paths configured".to_string(),
                status: CheckStatus::Warning,
                result: Some("No integrity paths provided".to_string()),
                details: None,
            });
        }

        for (idx, path) in config.integrity_paths.iter().enumerate() {
            let resolved = self.resolve_path(path);
            let mut status = if config.enable_file_integrity {
                CheckStatus::Pass
            } else {
                CheckStatus::Unknown
            };
            let mut details = JsonMap::new();
            details.insert(
                "path".to_string(),
                JsonValue::String(resolved.display().to_string()),
            );

            if !config.enable_file_integrity {
                details.insert(
                    "note".to_string(),
                    JsonValue::String("File integrity checks disabled".to_string()),
                );
            } else if !resolved.exists() {
                status = CheckStatus::Fail;
                score -= 15.0;
                violations.push(IntegrityViolation {
                    id: format!("missing-{}", idx + 1),
                    violation_type: "file".to_string(),
                    severity: IssueSeverity::High,
                    message: format!("Integrity path '{}' is missing", path),
                    location: Some(IssueLocation {
                        file_path: path.clone(),
                        line_number: None,
                        column_number: None,
                        function_name: None,
                    }),
                    details: Some(JsonValue::String(resolved.display().to_string())),
                });
            } else {
                if let Ok(metadata) = fs::metadata(&resolved) {
                    details.insert("size_bytes".to_string(), json!(metadata.len()));
                }

                if config.enable_checksum_verification {
                    let mut digest_details = JsonMap::new();
                    for algorithm in &config.integrity_algorithms {
                        match compute_integrity_digest(&resolved, algorithm) {
                            Ok(digest) => {
                                digest_details.insert(
                                    integrity_algorithm_name(algorithm).to_string(),
                                    JsonValue::String(digest),
                                );
                            }
                            Err(err) => {
                                status = CheckStatus::Warning;
                                warn!(
                                    path = %resolved.display(),
                                    algorithm = integrity_algorithm_name(algorithm),
                                    error = %err,
                                    "Failed to compute integrity digest"
                                );
                                digest_details.insert(
                                    format!("{}_error", integrity_algorithm_name(algorithm)),
                                    JsonValue::String(err.to_string()),
                                );
                            }
                        }
                    }
                    if !digest_details.is_empty() {
                        details.insert("digests".to_string(), JsonValue::Object(digest_details));
                    }
                }

                if config.enable_signature_verification {
                    let mut signature_path = resolved.clone();
                    let new_extension = resolved
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| format!("{}.sig", ext))
                        .unwrap_or_else(|| "sig".to_string());
                    signature_path.set_extension(new_extension);

                    if signature_path.exists() {
                        details.insert(
                            "signature_path".to_string(),
                            JsonValue::String(signature_path.display().to_string()),
                        );
                    } else {
                        status = CheckStatus::Warning;
                        score -= 5.0;
                        violations.push(IntegrityViolation {
                            id: format!("missing-signature-{}", idx + 1),
                            violation_type: "signature".to_string(),
                            severity: IssueSeverity::Medium,
                            message: format!("Signature file missing for '{}'", path),
                            location: Some(IssueLocation {
                                file_path: path.clone(),
                                line_number: None,
                                column_number: None,
                                function_name: None,
                            }),
                            details: Some(JsonValue::String(signature_path.display().to_string())),
                        });
                    }
                }
            }

            checks.push(IntegrityCheck {
                id: format!("integrity-check-{}", idx + 1),
                name: format!("Integrity validation for {}", path),
                status: status.clone(),
                result: Some(match status {
                    CheckStatus::Pass => "verified".to_string(),
                    CheckStatus::Warning => "verified_with_warnings".to_string(),
                    CheckStatus::Fail => "failed".to_string(),
                    CheckStatus::Unknown => "not_run".to_string(),
                }),
                details: if details.is_empty() {
                    None
                } else {
                    Some(JsonValue::Object(details))
                },
            });
        }

        if config.enable_dependency_integrity {
            let lock_path = self.workspace_root().join("Cargo.lock");
            let mut details = JsonMap::new();
            details.insert(
                "path".to_string(),
                JsonValue::String(lock_path.display().to_string()),
            );

            let status = if lock_path.exists() {
                CheckStatus::Pass
            } else {
                score -= 15.0;
                violations.push(IntegrityViolation {
                    id: "missing-lockfile".to_string(),
                    violation_type: "dependency".to_string(),
                    severity: IssueSeverity::High,
                    message: "Cargo.lock not found".to_string(),
                    location: Some(IssueLocation {
                        file_path: lock_path.display().to_string(),
                        line_number: None,
                        column_number: None,
                        function_name: None,
                    }),
                    details: None,
                });
                CheckStatus::Fail
            };

            checks.push(IntegrityCheck {
                id: "integrity-dependency-lockfile".to_string(),
                name: "Dependency lockfile integrity".to_string(),
                status: status.clone(),
                result: Some(match status {
                    CheckStatus::Pass => "verified".to_string(),
                    CheckStatus::Warning => "verified_with_warnings".to_string(),
                    CheckStatus::Fail => "missing".to_string(),
                    CheckStatus::Unknown => "not_run".to_string(),
                }),
                details: Some(JsonValue::Object(details)),
            });
        }

        let recommendations: Vec<IntegrityRecommendation> = violations
            .iter()
            .enumerate()
            .map(|(idx, violation)| IntegrityRecommendation {
                id: format!("integrity-rec-{}", idx + 1),
                recommendation_type: violation.violation_type.clone(),
                message: violation.message.clone(),
                priority: self.priority_from_issue_severity(violation.severity.clone()),
                details: violation.details.clone(),
            })
            .collect();

        let report = IntegrityReport {
            overall_score: self.clamp_score(score),
            integrity_checks: checks,
            violations,
            recommendations,
            timestamp: chrono::Utc::now(),
        };

        info!(
            overall_score = report.overall_score,
            "System integrity verification completed"
        );
        Ok(report)
    }

    async fn verify_deployment_readiness(
        &self,
        config: &DeploymentConfig,
    ) -> Result<DeploymentReport> {
        info!("Starting deployment readiness verification");

        let mut checks = Vec::new();
        let mut issues = Vec::new();
        let mut score = 100.0;

        let targets_check_status = if config.deployment_targets.is_empty() {
            score -= 25.0;
            issues.push(DeploymentIssue {
                id: "deployment-no-targets".to_string(),
                issue_type: "targets".to_string(),
                severity: IssueSeverity::High,
                message: "No deployment targets configured".to_string(),
                location: None,
                details: Some(json!({ "environment": config.deployment_environment })),
            });
            CheckStatus::Fail
        } else {
            CheckStatus::Pass
        };

        checks.push(DeploymentCheck {
            id: "targets-defined".to_string(),
            name: "Deployment targets defined".to_string(),
            status: targets_check_status.clone(),
            result: Some(match targets_check_status {
                CheckStatus::Pass => format!("{} targets", config.deployment_targets.len()),
                _ => "missing targets".to_string(),
            }),
            details: Some(json!({
                "target_count": config.deployment_targets.len(),
                "environment": config.deployment_environment,
            })),
        });

        if config.enable_health_checks {
            let all_targets_have_health = config
                .deployment_targets
                .iter()
                .all(|target| target.configuration.contains_key("health_endpoint"));
            let status = if all_targets_have_health {
                CheckStatus::Pass
            } else {
                score -= 10.0;
                issues.push(DeploymentIssue {
                    id: "deployment-health-checks".to_string(),
                    issue_type: "health".to_string(),
                    severity: IssueSeverity::Medium,
                    message: "Missing health checks for one or more targets".to_string(),
                    location: None,
                    details: Some(json!({ "all_targets_configured": all_targets_have_health })),
                });
                CheckStatus::Warning
            };

            checks.push(DeploymentCheck {
                id: "health-checks".to_string(),
                name: "Health checks configured".to_string(),
                status: status.clone(),
                result: Some(match status {
                    CheckStatus::Pass => "configured".to_string(),
                    CheckStatus::Warning => "missing targets".to_string(),
                    CheckStatus::Fail => "not configured".to_string(),
                    CheckStatus::Unknown => "unknown".to_string(),
                }),
                details: Some(json!({ "all_targets_configured": all_targets_have_health })),
            });
        }

        if config.enable_config_validation {
            let missing_configs = config
                .deployment_targets
                .iter()
                .filter(|target| target.configuration.is_empty())
                .count();
            let status = if missing_configs == 0 {
                CheckStatus::Pass
            } else {
                score -= 10.0;
                issues.push(DeploymentIssue {
                    id: "deployment-config".to_string(),
                    issue_type: "configuration".to_string(),
                    severity: IssueSeverity::Medium,
                    message: "Some targets are missing configuration".to_string(),
                    location: None,
                    details: Some(json!({ "targets_missing": missing_configs })),
                });
                CheckStatus::Warning
            };

            checks.push(DeploymentCheck {
                id: "config-validation".to_string(),
                name: "Target configuration validation".to_string(),
                status: status.clone(),
                result: Some(match status {
                    CheckStatus::Pass => "valid".to_string(),
                    CheckStatus::Warning => "missing".to_string(),
                    CheckStatus::Fail => "invalid".to_string(),
                    CheckStatus::Unknown => "unknown".to_string(),
                }),
                details: Some(json!({ "targets_missing": missing_configs })),
            });
        }

        if config.enable_resource_validation {
            let resource_ready = config.deployment_targets.iter().all(|target| {
                target
                    .configuration
                    .get("resources")
                    .map(|value| !value.is_null())
                    .unwrap_or(false)
            });
            let status = if resource_ready {
                CheckStatus::Pass
            } else {
                score -= 10.0;
                issues.push(DeploymentIssue {
                    id: "deployment-resources".to_string(),
                    issue_type: "resources".to_string(),
                    severity: IssueSeverity::Medium,
                    message: "Resource requirements not defined for all targets".to_string(),
                    location: None,
                    details: Some(json!({ "resource_ready": resource_ready })),
                });
                CheckStatus::Warning
            };

            checks.push(DeploymentCheck {
                id: "resource-validation".to_string(),
                name: "Resource definitions present".to_string(),
                status: status.clone(),
                result: Some(match status {
                    CheckStatus::Pass => "ready".to_string(),
                    CheckStatus::Warning => "incomplete".to_string(),
                    CheckStatus::Fail => "missing".to_string(),
                    CheckStatus::Unknown => "unknown".to_string(),
                }),
                details: Some(json!({ "resource_ready": resource_ready })),
            });
        }

        if config.enable_dependency_validation {
            let lock_path = self.workspace_root().join("Cargo.lock");
            let status = if lock_path.exists() {
                CheckStatus::Pass
            } else {
                score -= 10.0;
                issues.push(DeploymentIssue {
                    id: "deployment-dependencies".to_string(),
                    issue_type: "dependencies".to_string(),
                    severity: IssueSeverity::High,
                    message: "Dependency lockfile missing".to_string(),
                    location: Some(IssueLocation {
                        file_path: lock_path.display().to_string(),
                        line_number: None,
                        column_number: None,
                        function_name: None,
                    }),
                    details: None,
                });
                CheckStatus::Fail
            };

            checks.push(DeploymentCheck {
                id: "dependency-validation".to_string(),
                name: "Dependency lockfile present".to_string(),
                status: status.clone(),
                result: Some(match status {
                    CheckStatus::Pass => "lockfile present".to_string(),
                    CheckStatus::Warning => "lockfile warning".to_string(),
                    CheckStatus::Fail => "lockfile missing".to_string(),
                    CheckStatus::Unknown => "unknown".to_string(),
                }),
                details: Some(json!({ "path": lock_path.display().to_string() })),
            });
        }

        let readiness_status = if score >= 90.0 {
            DeploymentReadinessStatus::Ready
        } else if score >= 70.0 {
            DeploymentReadinessStatus::PartiallyReady
        } else {
            DeploymentReadinessStatus::NotReady
        };

        let recommendations: Vec<DeploymentRecommendation> = issues
            .iter()
            .enumerate()
            .map(|(idx, issue)| DeploymentRecommendation {
                id: format!("deployment-rec-{}", idx + 1),
                recommendation_type: issue.issue_type.clone(),
                message: issue.message.clone(),
                priority: self.priority_from_issue_severity(issue.severity.clone()),
                details: issue.details.clone(),
            })
            .collect();

        let report = DeploymentReport {
            overall_score: self.clamp_score(score),
            readiness_status,
            deployment_checks: checks,
            issues,
            recommendations,
            timestamp: chrono::Utc::now(),
        };

        info!(
            overall_score = report.overall_score,
            readiness_status = ?report.readiness_status,
            "Deployment readiness verification completed"
        );
        Ok(report)
    }

    async fn run_comprehensive_verification(
        &self,
        config: &VerificationConfig,
    ) -> Result<ComprehensiveReport> {
        let _start_time = chrono::Utc::now();
        let start_instant = std::time::Instant::now();

        info!("Starting comprehensive verification");

        // Run all verification checks
        let code_quality = self.verify_code_quality(&config.code_quality).await?;
        let security = self.verify_security(&config.security).await?;
        let performance = self.verify_performance(&config.performance).await?;
        let compliance = self.verify_compliance(&config.compliance).await?;
        let integrity = self.verify_system_integrity(&config.integrity).await?;
        let deployment = self.verify_deployment_readiness(&config.deployment).await?;

        let end_time = chrono::Utc::now();
        let duration = start_instant.elapsed();

        // Calculate overall score
        let overall_score = (code_quality.overall_score
            + security.overall_score
            + performance.overall_score
            + compliance.overall_score
            + integrity.overall_score
            + deployment.overall_score)
            / 6.0;

        // Determine verification status
        let verification_status = if overall_score >= 90.0 {
            VerificationStatus::Pass
        } else if overall_score >= 70.0 {
            VerificationStatus::Warning
        } else {
            VerificationStatus::Fail
        };

        let summary = VerificationSummary {
            total_checks: 6,
            passed_checks: if verification_status == VerificationStatus::Pass {
                6
            } else {
                0
            },
            failed_checks: if verification_status == VerificationStatus::Fail {
                6
            } else {
                0
            },
            warning_checks: if verification_status == VerificationStatus::Warning {
                6
            } else {
                0
            },
            success_rate: overall_score / 100.0,
            verification_duration_ms: duration.as_millis() as u64,
        };

        let report = ComprehensiveReport {
            overall_score,
            verification_status: verification_status.clone(),
            code_quality,
            security,
            performance,
            compliance,
            integrity,
            deployment,
            summary,
            timestamp: end_time,
        };

        info!(
            overall_score = overall_score,
            verification_status = ?verification_status,
            verification_duration_ms = duration.as_millis(),
            "Comprehensive verification completed"
        );

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn base_config() -> VerificationConfig {
        let mut compliance_thresholds = HashMap::new();
        compliance_thresholds.insert("coverage".to_string(), 80.0);
        compliance_thresholds.insert("overall_score".to_string(), 85.0);
        compliance_thresholds.insert("warning_score".to_string(), 70.0);

        let mut deployment_target_config = HashMap::new();
        deployment_target_config.insert("health_endpoint".to_string(), json!("/health"));
        deployment_target_config.insert(
            "resources".to_string(),
            json!({ "cpu": "500m", "memory": "512Mi" }),
        );

        VerificationConfig {
            code_quality: CodeQualityConfig {
                enable_clippy: false,
                enable_format: false,
                enable_coverage: false,
                min_coverage_percentage: 80.0,
                enable_complexity: false,
                max_cyclomatic_complexity: 10,
                enable_documentation: false,
                enable_dead_code: false,
                additional_checks: Vec::new(),
            },
            security: SecurityConfig {
                enable_vulnerability_scanning: false,
                enable_dependency_scanning: false,
                enable_secret_detection: false,
                enable_sast: false,
                enable_dast: false,
                enable_container_scanning: false,
                severity_thresholds: HashMap::new(),
                security_policies: vec!["baseline".to_string()],
            },
            performance: PerformanceConfig {
                enable_performance_testing: false,
                test_scenarios: Vec::new(),
                performance_thresholds: HashMap::new(),
                enable_load_testing: false,
                enable_stress_testing: false,
                enable_memory_profiling: false,
                enable_cpu_profiling: false,
            },
            compliance: ComplianceConfig {
                compliance_standards: vec![ComplianceStandard {
                    name: "SOC2".to_string(),
                    version: "1.0".to_string(),
                    description: None,
                    requirements: vec![ComplianceRequirement {
                        id: "SOC2-1".to_string(),
                        name: "Document security policy".to_string(),
                        description: None,
                        requirement_type: ComplianceRequirementType::Mandatory,
                        severity: ComplianceSeverity::High,
                    }],
                }],
                compliance_policies: vec![CompliancePolicy {
                    name: "Baseline".to_string(),
                    description: None,
                    rules: vec![ComplianceRule {
                        id: "rule-1".to_string(),
                        name: "Baseline".to_string(),
                        description: None,
                        conditions: Vec::new(),
                        actions: vec![ComplianceAction::Allow],
                    }],
                    enforcement: ComplianceEnforcement::Strict,
                }],
                compliance_checks: vec![ComplianceCheck {
                    id: "check-1".to_string(),
                    name: "Document security policy".to_string(),
                    description: None,
                    check_type: ComplianceCheckType::Documentation,
                    parameters: HashMap::new(),
                }],
                compliance_reporting: ComplianceReporting {
                    enable_reporting: true,
                    report_format: OutputFormat::Json,
                    report_output_path: None,
                    report_templates: Vec::new(),
                    report_scheduling: None,
                },
                compliance_thresholds,
            },
            integrity: IntegrityConfig {
                enable_file_integrity: true,
                enable_checksum_verification: false,
                enable_signature_verification: false,
                enable_dependency_integrity: false,
                integrity_algorithms: vec![IntegrityAlgorithm::Sha256],
                integrity_paths: Vec::new(),
            },
            deployment: DeploymentConfig {
                enable_readiness_checks: true,
                enable_health_checks: true,
                enable_config_validation: true,
                enable_resource_validation: true,
                enable_dependency_validation: false,
                deployment_environment: DeploymentEnvironment::Development,
                deployment_targets: vec![DeploymentTarget {
                    name: "local".to_string(),
                    target_type: DeploymentTargetType::Docker,
                    configuration: deployment_target_config,
                }],
            },
            timeout_seconds: 60,
            enable_parallel: false,
            output_format: OutputFormat::Json,
            additional_config: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_verification_framework_creation() {
        let temp_dir = tempdir().unwrap();
        let config = base_config();
        let framework = UnifiedVerificationFramework::with_workspace_root(config, temp_dir.path());
        assert!(framework.verification_history.is_empty());
    }

    #[tokio::test]
    async fn test_code_quality_verification() {
        let temp_dir = tempdir().unwrap();
        let config = base_config();
        let framework = UnifiedVerificationFramework::with_workspace_root(config, temp_dir.path());
        let report = framework
            .verify_code_quality(&framework.config.code_quality)
            .await
            .unwrap();
        assert!(report.metrics.contains_key("clippy_warnings"));
    }

    #[tokio::test]
    async fn test_security_verification_metrics() {
        let temp_dir = tempdir().unwrap();
        let config = base_config();
        let framework = UnifiedVerificationFramework::with_workspace_root(config, temp_dir.path());
        let report = framework
            .verify_security(&framework.config.security)
            .await
            .unwrap();
        assert!(report.metrics.contains_key("critical_vulnerabilities"));
    }

    #[tokio::test]
    async fn test_performance_verification_metrics() {
        let temp_dir = tempdir().unwrap();
        let config = base_config();
        let framework = UnifiedVerificationFramework::with_workspace_root(config, temp_dir.path());
        let report = framework
            .verify_performance(&framework.config.performance)
            .await
            .unwrap();
        assert!(report.metrics.contains_key("benchmark_pass_rate"));
    }

    #[tokio::test]
    async fn test_compliance_verification_detects_violation() {
        let temp_dir = tempdir().unwrap();
        let mut config = base_config();
        config.compliance.compliance_checks.clear();
        let framework = UnifiedVerificationFramework::with_workspace_root(config, temp_dir.path());
        let report = framework
            .verify_compliance(&framework.config.compliance)
            .await
            .unwrap();
        assert!(!report.violations.is_empty());
        assert_ne!(report.compliance_status, ComplianceStatus::Compliant);
    }

    #[tokio::test]
    async fn test_integrity_verification_reports_missing_file() {
        let temp_dir = tempdir().unwrap();
        let mut config = base_config();
        config.integrity.integrity_paths = vec!["missing-file.bin".to_string()];
        config.integrity.enable_checksum_verification = true;
        let framework = UnifiedVerificationFramework::with_workspace_root(config, temp_dir.path());
        let report = framework
            .verify_system_integrity(&framework.config.integrity)
            .await
            .unwrap();
        assert!(!report.violations.is_empty());
        assert!(report.overall_score < 100.0);
    }

    #[tokio::test]
    async fn test_deployment_verification_status() {
        let temp_dir = tempdir().unwrap();
        let config = base_config();
        let framework = UnifiedVerificationFramework::with_workspace_root(config, temp_dir.path());
        let report = framework
            .verify_deployment_readiness(&framework.config.deployment)
            .await
            .unwrap();
        assert_eq!(report.readiness_status, DeploymentReadinessStatus::Ready);
    }
}
