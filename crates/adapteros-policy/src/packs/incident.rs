//! Incident Policy Pack
//!
//! Enforces incident response procedures including memory pressure,
//! router skew, determinism failures, and policy violations.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Incident policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentConfig {
    /// Memory pressure response procedures
    pub memory_procedures: Vec<MemoryProcedure>,
    /// Router skew response procedures
    pub router_skew_procedures: Vec<RouterSkewProcedure>,
    /// Determinism failure response procedures
    pub determinism_procedures: Vec<DeterminismProcedure>,
    /// Policy violation response procedures
    pub violation_procedures: Vec<ViolationProcedure>,
    /// Incident escalation thresholds
    pub escalation_thresholds: EscalationThresholds,
    /// Incident response timeouts
    pub response_timeouts: ResponseTimeouts,
}

/// Memory pressure response procedures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryProcedure {
    /// Drop ephemeral adapters
    DropEphemeral,
    /// Reduce K parameter
    ReduceK,
    /// Evict cold adapters
    EvictCold,
    /// Deny new sessions
    DenyNewSessions,
}

/// Router skew response procedures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RouterSkewProcedure {
    /// Enable entropy floor
    EnableEntropyFloor,
    /// Cap per-adapter activation
    CapActivation,
    /// Recalibrate router
    Recalibrate,
    /// Rebuild plan
    RebuildPlan,
}

/// Determinism failure response procedures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeterminismProcedure {
    /// Freeze serving plan
    FreezePlan,
    /// Export bundle
    ExportBundle,
    /// Diff kernel hashes
    DiffHashes,
    /// Rollback to last CP
    Rollback,
}

/// Policy violation response procedures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationProcedure {
    /// Isolate process
    Isolate,
    /// Export audit bundle
    ExportBundle,
    /// Rotate keys
    RotateKeys,
    /// Open incident ticket
    OpenTicket,
}

/// Escalation thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationThresholds {
    /// Memory pressure threshold percentage
    pub memory_pressure_threshold: f64,
    /// Router skew threshold
    pub router_skew_threshold: f64,
    /// Determinism failure threshold
    pub determinism_failure_threshold: u64,
    /// Policy violation threshold
    pub policy_violation_threshold: u64,
}

/// Response timeouts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTimeouts {
    /// Memory pressure response timeout in seconds
    pub memory_response_timeout_secs: u64,
    /// Router skew response timeout in seconds
    pub router_skew_response_timeout_secs: u64,
    /// Determinism failure response timeout in seconds
    pub determinism_response_timeout_secs: u64,
    /// Policy violation response timeout in seconds
    pub violation_response_timeout_secs: u64,
}

impl Default for EscalationThresholds {
    fn default() -> Self {
        Self {
            memory_pressure_threshold: 85.0,
            router_skew_threshold: 0.1,
            determinism_failure_threshold: 1,
            policy_violation_threshold: 5,
        }
    }
}

impl Default for ResponseTimeouts {
    fn default() -> Self {
        Self {
            memory_response_timeout_secs: 300,      // 5 minutes
            router_skew_response_timeout_secs: 600, // 10 minutes
            determinism_response_timeout_secs: 60,  // 1 minute
            violation_response_timeout_secs: 1800,  // 30 minutes
        }
    }
}

impl Default for IncidentConfig {
    fn default() -> Self {
        Self {
            memory_procedures: vec![
                MemoryProcedure::DropEphemeral,
                MemoryProcedure::ReduceK,
                MemoryProcedure::EvictCold,
                MemoryProcedure::DenyNewSessions,
            ],
            router_skew_procedures: vec![
                RouterSkewProcedure::EnableEntropyFloor,
                RouterSkewProcedure::CapActivation,
                RouterSkewProcedure::Recalibrate,
                RouterSkewProcedure::RebuildPlan,
            ],
            determinism_procedures: vec![
                DeterminismProcedure::FreezePlan,
                DeterminismProcedure::ExportBundle,
                DeterminismProcedure::DiffHashes,
                DeterminismProcedure::Rollback,
            ],
            violation_procedures: vec![
                ViolationProcedure::Isolate,
                ViolationProcedure::ExportBundle,
                ViolationProcedure::RotateKeys,
                ViolationProcedure::OpenTicket,
            ],
            escalation_thresholds: EscalationThresholds::default(),
            response_timeouts: ResponseTimeouts::default(),
        }
    }
}

/// Incident event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentEvent {
    pub incident_id: String,
    pub incident_type: IncidentType,
    pub severity: IncidentSeverity,
    pub description: String,
    pub detected_at: u64,
    pub resolved_at: Option<u64>,
    pub procedures_executed: Vec<String>,
    pub status: IncidentStatus,
}

/// Types of incidents
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IncidentType {
    /// Memory pressure incident
    MemoryPressure,
    /// Router skew incident
    RouterSkew,
    /// Determinism failure incident
    DeterminismFailure,
    /// Policy violation incident
    PolicyViolation,
    /// System failure incident
    SystemFailure,
    /// Security incident
    SecurityIncident,
}

/// Incident severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IncidentSeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Incident status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IncidentStatus {
    /// Incident detected
    Detected,
    /// Incident in progress
    InProgress,
    /// Incident resolved
    Resolved,
    /// Incident escalated
    Escalated,
    /// Incident closed
    Closed,
}

/// Incident response plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentResponsePlan {
    pub incident_type: IncidentType,
    pub procedures: Vec<String>,
    pub expected_duration_secs: u64,
    pub escalation_criteria: Vec<String>,
    pub rollback_procedures: Vec<String>,
}

/// Incident policy implementation
pub struct IncidentPolicy {
    config: IncidentConfig,
}

impl IncidentPolicy {
    /// Create new incident policy
    pub fn new(config: IncidentConfig) -> Self {
        Self { config }
    }

    /// Generate incident response plan
    pub fn generate_response_plan(&self, incident_type: &IncidentType) -> IncidentResponsePlan {
        match incident_type {
            IncidentType::MemoryPressure => IncidentResponsePlan {
                incident_type: incident_type.clone(),
                procedures: self
                    .config
                    .memory_procedures
                    .iter()
                    .map(|p| format!("{:?}", p))
                    .collect(),
                expected_duration_secs: self.config.response_timeouts.memory_response_timeout_secs,
                escalation_criteria: vec![
                    "Memory usage exceeds 95%".to_string(),
                    "Response time exceeds 5 minutes".to_string(),
                ],
                rollback_procedures: vec![
                    "Restore previous K value".to_string(),
                    "Reload evicted adapters".to_string(),
                ],
            },
            IncidentType::RouterSkew => IncidentResponsePlan {
                incident_type: incident_type.clone(),
                procedures: self
                    .config
                    .router_skew_procedures
                    .iter()
                    .map(|p| format!("{:?}", p))
                    .collect(),
                expected_duration_secs: self
                    .config
                    .response_timeouts
                    .router_skew_response_timeout_secs,
                escalation_criteria: vec![
                    "Router skew exceeds 0.2".to_string(),
                    "Response time exceeds 10 minutes".to_string(),
                ],
                rollback_procedures: vec![
                    "Disable entropy floor".to_string(),
                    "Restore previous calibration".to_string(),
                ],
            },
            IncidentType::DeterminismFailure => IncidentResponsePlan {
                incident_type: incident_type.clone(),
                procedures: self
                    .config
                    .determinism_procedures
                    .iter()
                    .map(|p| format!("{:?}", p))
                    .collect(),
                expected_duration_secs: self
                    .config
                    .response_timeouts
                    .determinism_response_timeout_secs,
                escalation_criteria: vec![
                    "Multiple determinism failures".to_string(),
                    "Response time exceeds 1 minute".to_string(),
                ],
                rollback_procedures: vec![
                    "Restore previous plan".to_string(),
                    "Reload previous kernels".to_string(),
                ],
            },
            IncidentType::PolicyViolation => IncidentResponsePlan {
                incident_type: incident_type.clone(),
                procedures: self
                    .config
                    .violation_procedures
                    .iter()
                    .map(|p| format!("{:?}", p))
                    .collect(),
                expected_duration_secs: self
                    .config
                    .response_timeouts
                    .violation_response_timeout_secs,
                escalation_criteria: vec![
                    "Multiple policy violations".to_string(),
                    "Response time exceeds 30 minutes".to_string(),
                ],
                rollback_procedures: vec![
                    "Restore previous policy".to_string(),
                    "Reload previous configuration".to_string(),
                ],
            },
            _ => IncidentResponsePlan {
                incident_type: incident_type.clone(),
                procedures: vec!["Manual intervention required".to_string()],
                expected_duration_secs: 3600, // 1 hour default
                escalation_criteria: vec!["Escalate to on-call engineer".to_string()],
                rollback_procedures: vec!["Manual rollback required".to_string()],
            },
        }
    }

    /// Check if incident should be escalated
    pub fn should_escalate(&self, incident: &IncidentEvent) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let duration = now - incident.detected_at;

        match incident.incident_type {
            IncidentType::MemoryPressure => {
                duration > self.config.response_timeouts.memory_response_timeout_secs
            }
            IncidentType::RouterSkew => {
                duration
                    > self
                        .config
                        .response_timeouts
                        .router_skew_response_timeout_secs
            }
            IncidentType::DeterminismFailure => {
                duration
                    > self
                        .config
                        .response_timeouts
                        .determinism_response_timeout_secs
            }
            IncidentType::PolicyViolation => {
                duration
                    > self
                        .config
                        .response_timeouts
                        .violation_response_timeout_secs
            }
            _ => duration > 3600, // 1 hour default
        }
    }

    /// Validate incident response procedures
    pub fn validate_procedures(
        &self,
        incident_type: &IncidentType,
        procedures: &[String],
    ) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        let expected_procedures = match incident_type {
            IncidentType::MemoryPressure => self
                .config
                .memory_procedures
                .iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<_>>(),
            IncidentType::RouterSkew => self
                .config
                .router_skew_procedures
                .iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<_>>(),
            IncidentType::DeterminismFailure => self
                .config
                .determinism_procedures
                .iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<_>>(),
            IncidentType::PolicyViolation => self
                .config
                .violation_procedures
                .iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<_>>(),
            _ => vec!["Manual intervention required".to_string()],
        };

        for expected in &expected_procedures {
            if !procedures.contains(expected) {
                errors.push(format!("Required procedure {} not executed", expected));
            }
        }

        Ok(errors)
    }

    /// Validate incident configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.memory_procedures.is_empty() {
            return Err(AosError::PolicyViolation(
                "Memory procedures cannot be empty".to_string(),
            ));
        }

        if self.config.router_skew_procedures.is_empty() {
            return Err(AosError::PolicyViolation(
                "Router skew procedures cannot be empty".to_string(),
            ));
        }

        if self.config.determinism_procedures.is_empty() {
            return Err(AosError::PolicyViolation(
                "Determinism procedures cannot be empty".to_string(),
            ));
        }

        if self.config.violation_procedures.is_empty() {
            return Err(AosError::PolicyViolation(
                "Violation procedures cannot be empty".to_string(),
            ));
        }

        if self.config.escalation_thresholds.memory_pressure_threshold < 0.0
            || self.config.escalation_thresholds.memory_pressure_threshold > 100.0
        {
            return Err(AosError::PolicyViolation(
                "Memory pressure threshold must be between 0 and 100".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for incident policy enforcement
#[derive(Debug)]
pub struct IncidentContext {
    pub incidents: Vec<IncidentEvent>,
    pub executed_procedures: HashMap<String, Vec<String>>,
    pub tenant_id: String,
    pub operation: IncidentOperation,
}

/// Types of incident operations
#[derive(Debug)]
pub enum IncidentOperation {
    /// Incident detection
    Detection,
    /// Incident response
    Response,
    /// Incident escalation
    Escalation,
    /// Incident resolution
    Resolution,
    /// Incident audit
    Audit,
}

impl PolicyContext for IncidentContext {
    fn context_type(&self) -> &str {
        "incident"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for IncidentPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Incident
    }

    fn name(&self) -> &'static str {
        "Incident"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let incident_ctx = ctx
            .as_any()
            .downcast_ref::<IncidentContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid incident context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Check incident response procedures
        for incident in &incident_ctx.incidents {
            if let Some(procedures) = incident_ctx.executed_procedures.get(&incident.incident_id) {
                match self.validate_procedures(&incident.incident_type, procedures) {
                    Ok(errors) => {
                        for error in errors {
                            violations.push(Violation {
                                severity: Severity::High,
                                message: format!(
                                    "Incident {} procedure validation failed: {}",
                                    incident.incident_id, error
                                ),
                                details: Some(format!(
                                    "Incident type: {:?}",
                                    incident.incident_type
                                )),
                            });
                        }
                    }
                    Err(e) => {
                        violations.push(Violation {
                            severity: Severity::High,
                            message: format!(
                                "Incident {} procedure validation error",
                                incident.incident_id
                            ),
                            details: Some(e.to_string()),
                        });
                    }
                }
            }

            // Check if incident should be escalated
            if self.should_escalate(incident) {
                warnings.push(format!(
                    "Incident {} should be escalated",
                    incident.incident_id
                ));
            }

            // Check incident status
            match incident.status {
                IncidentStatus::Detected => {
                    warnings.push(format!(
                        "Incident {} still in detected status",
                        incident.incident_id
                    ));
                }
                IncidentStatus::InProgress => {
                    // Check if it's been too long
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    if now - incident.detected_at > 3600 {
                        // 1 hour
                        warnings.push(format!(
                            "Incident {} has been in progress for over 1 hour",
                            incident.incident_id
                        ));
                    }
                }
                _ => {}
            }
        }

        // Check for unresolved incidents
        let unresolved_count = incident_ctx
            .incidents
            .iter()
            .filter(|i| {
                matches!(
                    i.status,
                    IncidentStatus::Detected | IncidentStatus::InProgress
                )
            })
            .count();

        if unresolved_count > 0 {
            warnings.push(format!(
                "{} unresolved incidents detected",
                unresolved_count
            ));
        }

        Ok(Audit {
            policy_id: PolicyId::Incident,
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
    fn test_incident_config_default() {
        let config = IncidentConfig::default();
        assert!(!config.memory_procedures.is_empty());
        assert!(!config.router_skew_procedures.is_empty());
        assert!(!config.determinism_procedures.is_empty());
        assert!(!config.violation_procedures.is_empty());
    }

    #[test]
    fn test_incident_policy_creation() {
        let config = IncidentConfig::default();
        let policy = IncidentPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Incident);
    }

    #[test]
    fn test_response_plan_generation() {
        let config = IncidentConfig::default();
        let policy = IncidentPolicy::new(config);

        let plan = policy.generate_response_plan(&IncidentType::MemoryPressure);
        assert_eq!(plan.incident_type, IncidentType::MemoryPressure);
        assert!(!plan.procedures.is_empty());
        assert!(plan.expected_duration_secs > 0);
    }

    #[test]
    fn test_incident_escalation() {
        let config = IncidentConfig::default();
        let policy = IncidentPolicy::new(config);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let incident = IncidentEvent {
            incident_id: "test_incident".to_string(),
            incident_type: IncidentType::MemoryPressure,
            severity: IncidentSeverity::High,
            description: "Test incident".to_string(),
            detected_at: now - 400, // 400 seconds ago (exceeds 300s timeout)
            resolved_at: None,
            procedures_executed: vec![],
            status: IncidentStatus::InProgress,
        };

        assert!(policy.should_escalate(&incident));
    }

    #[test]
    fn test_procedure_validation() {
        let config = IncidentConfig::default();
        let policy = IncidentPolicy::new(config);

        let procedures = vec!["DropEphemeral".to_string()];
        let errors = policy
            .validate_procedures(&IncidentType::MemoryPressure, &procedures)
            .unwrap();
        assert!(!errors.is_empty()); // Should fail because not all procedures were executed
    }

    #[test]
    fn test_incident_config_validation() {
        let mut config = IncidentConfig::default();
        config.memory_procedures.clear(); // Invalid
        let policy = IncidentPolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
