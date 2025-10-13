//! Compliance Policy Pack
//!
//! Enforces compliance requirements including control matrix mapping,
//! evidence linking, and ITAR isolation verification.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Compliance policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    /// Control matrix hash
    pub control_matrix_hash: String,
    /// Whether evidence links are required
    pub require_evidence_links: bool,
    /// Whether ITAR suite must be green
    pub require_itar_suite_green: bool,
    /// Required compliance frameworks
    pub required_frameworks: Vec<ComplianceFramework>,
    /// Evidence retention period in days
    pub evidence_retention_days: u64,
    /// Audit trail requirements
    pub audit_trail_requirements: AuditTrailRequirements,
}

/// Compliance frameworks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceFramework {
    /// SOC 2 Type II
    Soc2Type2,
    /// ISO 27001
    Iso27001,
    /// PCI DSS
    PciDss,
    /// HIPAA
    Hipaa,
    /// ITAR
    Itar,
    /// GDPR
    Gdpr,
}

/// Audit trail requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrailRequirements {
    /// Whether audit trails are required
    pub require_audit_trails: bool,
    /// Minimum retention period in days
    pub min_retention_days: u64,
    /// Required audit events
    pub required_events: Vec<AuditEventType>,
    /// Whether immutable audit logs are required
    pub require_immutable_logs: bool,
}

/// Types of audit events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuditEventType {
    /// User authentication
    UserAuthentication,
    /// Data access
    DataAccess,
    /// Configuration changes
    ConfigurationChange,
    /// Policy violations
    PolicyViolation,
    /// System events
    SystemEvent,
    /// Security events
    SecurityEvent,
}

impl Default for AuditTrailRequirements {
    fn default() -> Self {
        Self {
            require_audit_trails: true,
            min_retention_days: 2555, // 7 years
            required_events: vec![
                AuditEventType::UserAuthentication,
                AuditEventType::DataAccess,
                AuditEventType::ConfigurationChange,
                AuditEventType::PolicyViolation,
                AuditEventType::SecurityEvent,
            ],
            require_immutable_logs: true,
        }
    }
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            control_matrix_hash: "b3:default_control_matrix_hash".to_string(),
            require_evidence_links: true,
            require_itar_suite_green: true,
            required_frameworks: vec![ComplianceFramework::Soc2Type2],
            evidence_retention_days: 2555, // 7 years
            audit_trail_requirements: AuditTrailRequirements::default(),
        }
    }
}

/// Control matrix entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlMatrixEntry {
    pub control_id: String,
    pub control_name: String,
    pub framework: ComplianceFramework,
    pub evidence_file: Option<String>,
    pub evidence_hash: Option<String>,
    pub last_verified: Option<u64>,
    pub verification_status: VerificationStatus,
}

/// Verification status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// Not verified
    NotVerified,
    /// Verified
    Verified,
    /// Verification failed
    Failed,
    /// Verification expired
    Expired,
}

/// Evidence entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEntry {
    pub evidence_id: String,
    pub evidence_type: EvidenceType,
    pub file_path: String,
    pub file_hash: String,
    pub created_at: u64,
    pub created_by: String,
    pub controls: Vec<String>, // Control IDs this evidence supports
    pub is_valid: bool,
}

/// Types of evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvidenceType {
    /// Policy document
    PolicyDocument,
    /// Audit report
    AuditReport,
    /// Test results
    TestResults,
    /// Configuration file
    ConfigurationFile,
    /// Log file
    LogFile,
    /// Certificate
    Certificate,
}

/// ITAR test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItarTestResults {
    pub test_id: String,
    pub test_name: String,
    pub test_status: ItarTestStatus,
    pub violations_found: u64,
    pub test_duration_ms: u64,
    pub timestamp: u64,
}

/// ITAR test status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItarTestStatus {
    /// Test passed
    Passed,
    /// Test failed
    Failed,
    /// Test in progress
    InProgress,
    /// Test skipped
    Skipped,
}

/// Compliance policy implementation
pub struct CompliancePolicy {
    config: ComplianceConfig,
}

impl CompliancePolicy {
    /// Create new compliance policy
    pub fn new(config: ComplianceConfig) -> Self {
        Self { config }
    }

    /// Validate control matrix
    pub fn validate_control_matrix(&self, controls: &[ControlMatrixEntry]) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        if self.config.require_evidence_links {
            for control in controls {
                if control.evidence_file.is_none() {
                    errors.push(format!(
                        "Control {} lacks evidence file",
                        control.control_id
                    ));
                }

                if control.evidence_hash.is_none() {
                    errors.push(format!(
                        "Control {} lacks evidence hash",
                        control.control_id
                    ));
                }

                match control.verification_status {
                    VerificationStatus::NotVerified => {
                        errors.push(format!("Control {} not verified", control.control_id));
                    }
                    VerificationStatus::Failed => {
                        errors.push(format!(
                            "Control {} verification failed",
                            control.control_id
                        ));
                    }
                    VerificationStatus::Expired => {
                        errors.push(format!(
                            "Control {} verification expired",
                            control.control_id
                        ));
                    }
                    VerificationStatus::Verified => {
                        // Check if verification is recent enough
                        if let Some(last_verified) = control.last_verified {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();

                            if now - last_verified > self.config.evidence_retention_days * 86400 {
                                errors.push(format!(
                                    "Control {} verification too old",
                                    control.control_id
                                ));
                            }
                        }
                    }
                }
            }
        }

        Ok(errors)
    }

    /// Validate evidence entries
    pub fn validate_evidence(&self, evidence: &[EvidenceEntry]) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        for entry in evidence {
            if !entry.is_valid {
                errors.push(format!(
                    "Evidence {} is marked as invalid",
                    entry.evidence_id
                ));
            }

            if entry.file_hash.is_empty() {
                errors.push(format!("Evidence {} has empty hash", entry.evidence_id));
            }

            if entry.controls.is_empty() {
                errors.push(format!(
                    "Evidence {} has no associated controls",
                    entry.evidence_id
                ));
            }

            // Check evidence age
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now - entry.created_at > self.config.evidence_retention_days * 86400 {
                errors.push(format!("Evidence {} is too old", entry.evidence_id));
            }
        }

        Ok(errors)
    }

    /// Validate ITAR test results
    pub fn validate_itar_results(&self, results: &[ItarTestResults]) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        if self.config.require_itar_suite_green {
            for result in results {
                match result.test_status {
                    ItarTestStatus::Failed => {
                        errors.push(format!(
                            "ITAR test {} failed with {} violations",
                            result.test_name, result.violations_found
                        ));
                    }
                    ItarTestStatus::InProgress => {
                        errors.push(format!("ITAR test {} still in progress", result.test_name));
                    }
                    ItarTestStatus::Skipped => {
                        errors.push(format!("ITAR test {} was skipped", result.test_name));
                    }
                    ItarTestStatus::Passed => {
                        // Test passed, no error
                    }
                }
            }
        }

        Ok(errors)
    }

    /// Validate audit trail requirements
    pub fn validate_audit_trails(&self, audit_events: &[AuditEventType]) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        if self.config.audit_trail_requirements.require_audit_trails {
            for required_event in &self.config.audit_trail_requirements.required_events {
                if !audit_events.contains(required_event) {
                    errors.push(format!(
                        "Required audit event {:?} not found",
                        required_event
                    ));
                }
            }
        }

        Ok(errors)
    }

    /// Validate compliance configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.control_matrix_hash.is_empty() {
            return Err(AosError::PolicyViolation(
                "Control matrix hash cannot be empty".to_string(),
            ));
        }

        if self.config.evidence_retention_days == 0 {
            return Err(AosError::PolicyViolation(
                "Evidence retention period must be greater than 0".to_string(),
            ));
        }

        if self.config.required_frameworks.is_empty() {
            return Err(AosError::PolicyViolation(
                "At least one compliance framework must be required".to_string(),
            ));
        }

        if self.config.audit_trail_requirements.min_retention_days == 0 {
            return Err(AosError::PolicyViolation(
                "Audit trail minimum retention must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for compliance policy enforcement
#[derive(Debug)]
pub struct ComplianceContext {
    pub control_matrix: Vec<ControlMatrixEntry>,
    pub evidence: Vec<EvidenceEntry>,
    pub itar_results: Vec<ItarTestResults>,
    pub audit_events: Vec<AuditEventType>,
    pub tenant_id: String,
    pub operation: ComplianceOperation,
}

/// Types of compliance operations
#[derive(Debug)]
pub enum ComplianceOperation {
    /// Compliance audit
    Audit,
    /// Evidence collection
    EvidenceCollection,
    /// Control verification
    ControlVerification,
    /// ITAR testing
    ItarTesting,
    /// Compliance reporting
    Reporting,
}

impl PolicyContext for ComplianceContext {
    fn context_type(&self) -> &str {
        "compliance"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for CompliancePolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Compliance
    }

    fn name(&self) -> &'static str {
        "Compliance"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let compliance_ctx = ctx
            .as_any()
            .downcast_ref::<ComplianceContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid compliance context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Validate control matrix
        match self.validate_control_matrix(&compliance_ctx.control_matrix) {
            Ok(errors) => {
                for error in errors {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: format!("Control matrix validation failed: {}", error),
                        details: Some("Control matrix entry validation failed".to_string()),
                    });
                }
            }
            Err(e) => {
                violations.push(Violation {
                    severity: Severity::High,
                    message: "Control matrix validation error".to_string(),
                    details: Some(e.to_string()),
                });
            }
        }

        // Validate evidence
        match self.validate_evidence(&compliance_ctx.evidence) {
            Ok(errors) => {
                for error in errors {
                    violations.push(Violation {
                        severity: Severity::Medium,
                        message: format!("Evidence validation failed: {}", error),
                        details: Some("Evidence entry validation failed".to_string()),
                    });
                }
            }
            Err(e) => {
                violations.push(Violation {
                    severity: Severity::Medium,
                    message: "Evidence validation error".to_string(),
                    details: Some(e.to_string()),
                });
            }
        }

        // Validate ITAR results
        match self.validate_itar_results(&compliance_ctx.itar_results) {
            Ok(errors) => {
                for error in errors {
                    violations.push(Violation {
                        severity: Severity::Critical,
                        message: format!("ITAR test failed: {}", error),
                        details: Some("ITAR compliance test failed".to_string()),
                    });
                }
            }
            Err(e) => {
                violations.push(Violation {
                    severity: Severity::Critical,
                    message: "ITAR results validation error".to_string(),
                    details: Some(e.to_string()),
                });
            }
        }

        // Validate audit trails
        match self.validate_audit_trails(&compliance_ctx.audit_events) {
            Ok(errors) => {
                for error in errors {
                    violations.push(Violation {
                        severity: Severity::Medium,
                        message: format!("Audit trail validation failed: {}", error),
                        details: Some("Audit trail requirement not met".to_string()),
                    });
                }
            }
            Err(e) => {
                violations.push(Violation {
                    severity: Severity::Medium,
                    message: "Audit trail validation error".to_string(),
                    details: Some(e.to_string()),
                });
            }
        }

        // Add warnings for missing data
        if compliance_ctx.control_matrix.is_empty() {
            warnings.push("Control matrix is empty".to_string());
        }

        if compliance_ctx.evidence.is_empty() {
            warnings.push("No evidence provided".to_string());
        }

        if compliance_ctx.itar_results.is_empty() {
            warnings.push("No ITAR test results provided".to_string());
        }

        Ok(Audit {
            policy_id: PolicyId::Compliance,
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
    fn test_compliance_config_default() {
        let config = ComplianceConfig::default();
        assert!(config.require_evidence_links);
        assert!(config.require_itar_suite_green);
        assert!(!config.required_frameworks.is_empty());
    }

    #[test]
    fn test_compliance_policy_creation() {
        let config = ComplianceConfig::default();
        let policy = CompliancePolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Compliance);
    }

    #[test]
    fn test_control_matrix_validation() {
        let config = ComplianceConfig::default();
        let policy = CompliancePolicy::new(config);

        let controls = vec![ControlMatrixEntry {
            control_id: "control1".to_string(),
            control_name: "Test Control".to_string(),
            framework: ComplianceFramework::Soc2Type2,
            evidence_file: None, // Should fail
            evidence_hash: None, // Should fail
            last_verified: None,
            verification_status: VerificationStatus::NotVerified, // Should fail
        }];

        let errors = policy.validate_control_matrix(&controls).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("evidence file")));
        assert!(errors.iter().any(|e| e.contains("evidence hash")));
        assert!(errors.iter().any(|e| e.contains("not verified")));
    }

    #[test]
    fn test_evidence_validation() {
        let config = ComplianceConfig::default();
        let policy = CompliancePolicy::new(config);

        let evidence = vec![EvidenceEntry {
            evidence_id: "evidence1".to_string(),
            evidence_type: EvidenceType::PolicyDocument,
            file_path: "/path/to/evidence".to_string(),
            file_hash: "".to_string(), // Should fail
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            created_by: "test_user".to_string(),
            controls: vec![], // Should fail
            is_valid: false,  // Should fail
        }];

        let errors = policy.validate_evidence(&evidence).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("empty hash")));
        assert!(errors.iter().any(|e| e.contains("no associated controls")));
        assert!(errors.iter().any(|e| e.contains("marked as invalid")));
    }

    #[test]
    fn test_itar_results_validation() {
        let config = ComplianceConfig::default();
        let policy = CompliancePolicy::new(config);

        let results = vec![ItarTestResults {
            test_id: "test1".to_string(),
            test_name: "ITAR Test".to_string(),
            test_status: ItarTestStatus::Failed, // Should fail
            violations_found: 5,
            test_duration_ms: 1000,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }];

        let errors = policy.validate_itar_results(&results).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("failed")));
    }

    #[test]
    fn test_compliance_config_validation() {
        let mut config = ComplianceConfig::default();
        config.control_matrix_hash = "".to_string(); // Invalid
        let policy = CompliancePolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
