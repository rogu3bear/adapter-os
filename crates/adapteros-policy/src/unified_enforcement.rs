//! Unified policy enforcement interface for AdapterOS
//!
//! Provides a centralized interface for enforcing all 20 policy packs
//! across the system with consistent validation and reporting.
//!
//! # Citations
//! - Policy Pack #1-20: All policy packs enforced through unified interface
//! - CLAUDE.md L142: "Policy Engine: Enforces 20 policy packs"

use crate::policy_packs::{
    PolicyViolation as PackPolicyViolation, PolicyWarning as PackPolicyWarning,
};
use adapteros_core::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info};

pub use crate::policy_packs::ViolationSeverity;
pub type PolicyViolation = PackPolicyViolation;
pub type PolicyWarning = PackPolicyWarning;

/// Unified policy enforcement interface
pub trait PolicyEnforcer {
    /// Validate a request against all applicable policies
    #[allow(async_fn_in_trait)]
    async fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult>;

    /// Check if an operation is allowed
    #[allow(async_fn_in_trait)]
    async fn is_operation_allowed(&self, operation: &Operation) -> Result<bool>;

    /// Get policy violations for an operation
    #[allow(async_fn_in_trait)]
    async fn get_violations(&self, operation: &Operation) -> Result<Vec<PolicyViolation>>;

    /// Apply policy enforcement to an operation
    #[allow(async_fn_in_trait)]
    async fn enforce_policy(&self, operation: &Operation) -> Result<PolicyEnforcementResult>;

    /// Get policy compliance report
    #[allow(async_fn_in_trait)]
    async fn get_compliance_report(&self) -> Result<PolicyComplianceReport>;
}

/// Policy request for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRequest {
    /// Request identifier
    pub request_id: String,

    /// Request type
    pub request_type: RequestType,

    /// Tenant ID
    pub tenant_id: Option<String>,

    /// User ID
    pub user_id: Option<String>,

    /// Request context
    pub context: PolicyContext,

    /// Request metadata
    pub metadata: Option<serde_json::Value>,
}

/// Request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestType {
    /// Inference request
    Inference,

    /// Adapter operation
    AdapterOperation,

    /// Memory operation
    MemoryOperation,

    /// Training operation
    TrainingOperation,

    /// Policy update
    PolicyUpdate,

    /// System operation
    SystemOperation,

    /// User operation
    UserOperation,

    /// Network operation
    NetworkOperation,

    /// File operation
    FileOperation,

    /// Database operation
    DatabaseOperation,
}

/// Policy context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// Component generating the request
    pub component: String,

    /// Operation being performed
    pub operation: String,

    /// Additional context data
    pub data: Option<serde_json::Value>,

    /// Request priority
    pub priority: Priority,
}

/// Request priority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Priority {
    /// Low priority
    Low,

    /// Normal priority
    Normal,

    /// High priority
    High,

    /// Critical priority
    Critical,
}

/// Policy validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyValidationResult {
    /// Whether the request is valid
    pub valid: bool,

    /// Policy violations found
    pub violations: Vec<PolicyViolation>,

    /// Warnings
    pub warnings: Vec<PolicyWarning>,

    /// Validation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Validation duration
    pub duration_ms: u64,
}

/// Operation for policy enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// Operation identifier
    pub operation_id: String,

    /// Operation type
    pub operation_type: OperationType,

    /// Operation parameters
    pub parameters: HashMap<String, serde_json::Value>,

    /// Operation context
    pub context: PolicyContext,

    /// Operation metadata
    pub metadata: Option<serde_json::Value>,
}

/// Operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    /// Load adapter
    LoadAdapter,

    /// Evict adapter
    EvictAdapter,

    /// Pin adapter
    PinAdapter,

    /// Start training
    StartTraining,

    /// Stop training
    StopTraining,

    /// Allocate memory
    AllocateMemory,

    /// Deallocate memory
    DeallocateMemory,

    /// Perform inference
    PerformInference,

    /// Update policy
    UpdatePolicy,

    /// System operation
    SystemOperation,
}

/// Policy enforcement result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEnforcementResult {
    /// Whether the operation is allowed
    pub allowed: bool,

    /// Enforcement actions taken
    pub actions: Vec<EnforcementAction>,

    /// Policy violations
    pub violations: Vec<PolicyViolation>,

    /// Enforcement timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Enforcement duration
    pub duration_ms: u64,
}

/// Enforcement actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementAction {
    /// Allow operation
    Allow,

    /// Deny operation
    Deny,

    /// Modify operation
    Modify {
        modifications: HashMap<String, serde_json::Value>,
    },

    /// Require additional validation
    RequireValidation { validation_type: String },

    /// Log violation
    LogViolation { violation: PolicyViolation },

    /// Send alert
    SendAlert { alert_type: String, message: String },
}

/// Policy compliance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyComplianceReport {
    /// Overall compliance score
    pub compliance_score: f64,

    /// Policy pack compliance
    pub policy_pack_compliance: HashMap<String, PolicyPackCompliance>,

    /// Recent violations
    pub recent_violations: Vec<PolicyViolation>,

    /// Compliance trends
    pub compliance_trends: Vec<ComplianceTrend>,

    /// Report timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Policy pack compliance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPackCompliance {
    /// Policy pack name
    pub policy_pack: String,

    /// Compliance score
    pub compliance_score: f64,

    /// Number of violations
    pub violation_count: u32,

    /// Last violation time
    pub last_violation: Option<chrono::DateTime<chrono::Utc>>,

    /// Compliance status
    pub status: ComplianceStatus,
}

/// Compliance status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceStatus {
    /// Compliant
    Compliant,

    /// Non-compliant
    NonCompliant,

    /// Warning
    Warning,

    /// Unknown
    Unknown,
}

/// Compliance trend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceTrend {
    /// Time period
    pub period: String,

    /// Compliance score
    pub compliance_score: f64,

    /// Trend direction
    pub trend: TrendDirection,
}

/// Trend direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    /// Improving
    Improving,

    /// Declining
    Declining,

    /// Stable
    Stable,
}

/// Unified policy enforcer implementation
#[derive(Debug)]
pub struct UnifiedPolicyEnforcer {
    /// Policy packs
    policy_packs: HashMap<String, Box<dyn PolicyPack + Send + Sync>>,

    /// Enforcement rules
    enforcement_rules: HashMap<String, EnforcementRule>,

    /// Violation history
    violation_history: Vec<PolicyViolation>,
}

impl Default for UnifiedPolicyEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl UnifiedPolicyEnforcer {
    /// Create a new unified policy enforcer
    pub fn new() -> Self {
        Self {
            policy_packs: HashMap::new(),
            enforcement_rules: HashMap::new(),
            violation_history: Vec::new(),
        }
    }

    /// Add a policy pack
    pub fn add_policy_pack(
        &mut self,
        name: String,
        policy_pack: Box<dyn PolicyPack + Send + Sync>,
    ) {
        self.policy_packs.insert(name, policy_pack);
    }

    /// Add an enforcement rule
    pub fn add_enforcement_rule(&mut self, name: String, rule: EnforcementRule) {
        self.enforcement_rules.insert(name, rule);
    }

    /// Record a policy violation
    pub fn record_violation(&mut self, violation: PolicyViolation) {
        self.violation_history.push(violation);

        // Keep only recent violations (last 1000)
        if self.violation_history.len() > 1000 {
            self.violation_history.remove(0);
        }
    }
}

impl PolicyEnforcer for UnifiedPolicyEnforcer {
    async fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let start_time = std::time::Instant::now();
        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Validate against all applicable policy packs
        for (pack_name, policy_pack) in &self.policy_packs {
            match policy_pack.validate_request(request) {
                Ok(validation) => {
                    violations.extend(validation.violations);
                    warnings.extend(validation.warnings);
                }
                Err(e) => {
                    error!(
                        policy_pack = pack_name,
                        error = %e,
                        "Policy pack validation failed"
                    );

                    violations.push(PolicyViolation {
                        violation_id: uuid::Uuid::new_v4().to_string(),
                        policy_pack: pack_name.clone(),
                        severity: ViolationSeverity::Error,
                        message: format!("Policy pack validation failed: {}", e),
                        details: None,
                        remediation: Some(vec!["Check policy pack configuration".to_string()]),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        let duration = start_time.elapsed();
        let valid = violations.is_empty();

        info!(
            request_id = %request.request_id,
            valid = valid,
            violations = violations.len(),
            warnings = warnings.len(),
            duration_ms = duration.as_millis(),
            "Policy validation completed"
        );

        Ok(PolicyValidationResult {
            valid,
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: duration.as_millis() as u64,
        })
    }

    async fn is_operation_allowed(&self, operation: &Operation) -> Result<bool> {
        let validation_result = self
            .validate_request(&PolicyRequest {
                request_id: operation.operation_id.clone(),
                request_type: match operation.operation_type {
                    OperationType::LoadAdapter => RequestType::AdapterOperation,
                    OperationType::EvictAdapter => RequestType::AdapterOperation,
                    OperationType::PinAdapter => RequestType::AdapterOperation,
                    OperationType::StartTraining => RequestType::TrainingOperation,
                    OperationType::StopTraining => RequestType::TrainingOperation,
                    OperationType::AllocateMemory => RequestType::MemoryOperation,
                    OperationType::DeallocateMemory => RequestType::MemoryOperation,
                    OperationType::PerformInference => RequestType::Inference,
                    OperationType::UpdatePolicy => RequestType::PolicyUpdate,
                    OperationType::SystemOperation => RequestType::SystemOperation,
                },
                tenant_id: None,
                user_id: None,
                context: operation.context.clone(),
                metadata: operation.metadata.clone(),
            })
            .await?;

        Ok(validation_result.valid)
    }

    async fn get_violations(&self, operation: &Operation) -> Result<Vec<PolicyViolation>> {
        let validation_result = self
            .validate_request(&PolicyRequest {
                request_id: operation.operation_id.clone(),
                request_type: match operation.operation_type {
                    OperationType::LoadAdapter => RequestType::AdapterOperation,
                    OperationType::EvictAdapter => RequestType::AdapterOperation,
                    OperationType::PinAdapter => RequestType::AdapterOperation,
                    OperationType::StartTraining => RequestType::TrainingOperation,
                    OperationType::StopTraining => RequestType::TrainingOperation,
                    OperationType::AllocateMemory => RequestType::MemoryOperation,
                    OperationType::DeallocateMemory => RequestType::MemoryOperation,
                    OperationType::PerformInference => RequestType::Inference,
                    OperationType::UpdatePolicy => RequestType::PolicyUpdate,
                    OperationType::SystemOperation => RequestType::SystemOperation,
                },
                tenant_id: None,
                user_id: None,
                context: operation.context.clone(),
                metadata: operation.metadata.clone(),
            })
            .await?;

        Ok(validation_result.violations)
    }

    async fn enforce_policy(&self, operation: &Operation) -> Result<PolicyEnforcementResult> {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();

        // Check if operation is allowed
        let allowed = self.is_operation_allowed(operation).await?;

        if allowed {
            actions.push(EnforcementAction::Allow);
        } else {
            actions.push(EnforcementAction::Deny);

            // Get violations
            let violations = self.get_violations(operation).await?;

            // Log violations
            for violation in &violations {
                actions.push(EnforcementAction::LogViolation {
                    violation: violation.clone(),
                });
            }
        }

        let duration = start_time.elapsed();

        info!(
            operation_id = %operation.operation_id,
            allowed = allowed,
            actions = actions.len(),
            duration_ms = duration.as_millis(),
            "Policy enforcement completed"
        );

        Ok(PolicyEnforcementResult {
            allowed,
            actions,
            violations: self.get_violations(operation).await?,
            timestamp: Utc::now(),
            duration_ms: duration.as_millis() as u64,
        })
    }

    async fn get_compliance_report(&self) -> Result<PolicyComplianceReport> {
        let mut policy_pack_compliance = HashMap::new();
        let mut total_violations = 0;

        // Calculate compliance for each policy pack
        for pack_name in self.policy_packs.keys() {
            let violations: Vec<_> = self
                .violation_history
                .iter()
                .filter(|v| v.policy_pack == *pack_name)
                .collect();

            let violation_count = violations.len() as u32;
            total_violations += violation_count;

            let compliance_score = if violation_count == 0 {
                100.0
            } else {
                (100.0 - (violation_count as f64 * 10.0)).max(0.0)
            };

            let status = if compliance_score >= 95.0 {
                ComplianceStatus::Compliant
            } else if compliance_score >= 80.0 {
                ComplianceStatus::Warning
            } else {
                ComplianceStatus::NonCompliant
            };

            policy_pack_compliance.insert(
                pack_name.clone(),
                PolicyPackCompliance {
                    policy_pack: pack_name.clone(),
                    compliance_score,
                    violation_count,
                    last_violation: violations.last().map(|_v| Utc::now()), // TODO: Use actual timestamp
                    status,
                },
            );
        }

        let overall_compliance_score = if total_violations == 0 {
            100.0
        } else {
            (100.0 - (total_violations as f64 * 5.0)).max(0.0)
        };

        Ok(PolicyComplianceReport {
            compliance_score: overall_compliance_score,
            policy_pack_compliance,
            recent_violations: self
                .violation_history
                .iter()
                .rev()
                .take(10)
                .cloned()
                .collect(),
            compliance_trends: Vec::new(), // TODO: Implement trend calculation
            timestamp: Utc::now(),
        })
    }
}

/// Policy pack trait
pub trait PolicyPack: Send + Sync + std::fmt::Debug {
    /// Validate a request against this policy pack
    fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult>;

    /// Get policy pack name
    fn get_name(&self) -> &str;

    /// Get policy pack version
    fn get_version(&self) -> &str;
}

/// Enforcement rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementRule {
    /// Rule name
    pub name: String,

    /// Rule conditions
    pub conditions: Vec<RuleCondition>,

    /// Rule actions
    pub actions: Vec<EnforcementAction>,

    /// Rule priority
    pub priority: u32,
}

/// Rule condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCondition {
    /// Condition field
    pub field: String,

    /// Condition operator
    pub operator: ConditionOperator,

    /// Condition value
    pub value: serde_json::Value,
}

/// Condition operators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionOperator {
    /// Equals
    Equals,

    /// Not equals
    NotEquals,

    /// Greater than
    GreaterThan,

    /// Less than
    LessThan,

    /// Contains
    Contains,

    /// Not contains
    NotContains,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_policy_enforcer_creation() {
        let enforcer = UnifiedPolicyEnforcer::new();
        assert!(enforcer.policy_packs.is_empty());
        assert!(enforcer.enforcement_rules.is_empty());
    }

    #[tokio::test]
    async fn test_policy_validation() {
        let enforcer = UnifiedPolicyEnforcer::new();

        let request = PolicyRequest {
            request_id: "test-request".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("default".to_string()),
            user_id: Some("user1".to_string()),
            context: PolicyContext {
                component: "test-component".to_string(),
                operation: "test-operation".to_string(),
                data: None,
                priority: Priority::Normal,
            },
            metadata: None,
        };

        let result = enforcer.validate_request(&request).await.unwrap();
        assert!(result.valid);
        assert!(result.violations.is_empty());
    }
}
