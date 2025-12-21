use crate::threat_detection::{ThreatAssessment, ThreatSeverity};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// Response actions executed by the security response engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResponseAction {
    Notify { channel: String, message: String },
    Quarantine { resource: String },
    Escalate { team: String },
    Audit { record: String },
}

/// Policy definition for mapping severity to actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePolicy {
    pub name: String,
    pub min_severity: ThreatSeverity,
    pub actions: Vec<ResponseAction>,
    pub escalation_after: Duration,
}

/// Response plan produced for a threat assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsePlan {
    pub severity: ThreatSeverity,
    pub actions: Vec<ResponseAction>,
    pub audit_trail: Vec<AuditEntry>,
}

/// Audit entry capturing security response actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: SystemTime,
    pub action: ResponseAction,
    pub notes: String,
}

/// Executes response policies and maintains audit trails.
#[derive(Debug, Default)]
pub struct SecurityResponseEngine {
    policies: Vec<ResponsePolicy>,
    audit_trail: Vec<AuditEntry>,
    default_channels: Vec<String>,
}

impl SecurityResponseEngine {
    pub fn new(default_channels: Vec<String>) -> Self {
        Self {
            policies: Vec::new(),
            audit_trail: Vec::new(),
            default_channels,
        }
    }

    pub fn register_policy(&mut self, policy: ResponsePolicy) {
        self.policies.push(policy);
        self.policies
            .sort_by(|a, b| a.min_severity.cmp(&b.min_severity));
    }

    /// Execute response plan for a threat assessment.
    pub fn execute(&mut self, assessment: &ThreatAssessment) -> ResponsePlan {
        let mut actions = Vec::new();
        let mut audit_entries = Vec::new();

        for policy in &self.policies {
            if severity_at_least(assessment.severity, policy.min_severity) {
                for action in &policy.actions {
                    actions.push(action.clone());
                    audit_entries.push(AuditEntry {
                        timestamp: SystemTime::now(),
                        action: action.clone(),
                        notes: format!("Policy {} triggered", policy.name),
                    });
                }
            }
        }

        if actions.is_empty() {
            for channel in &self.default_channels {
                let action = ResponseAction::Notify {
                    channel: channel.clone(),
                    message: format!(
                        "Default notification: severity {:?} risk {:.2}",
                        assessment.severity, assessment.risk_score
                    ),
                };
                audit_entries.push(AuditEntry {
                    timestamp: SystemTime::now(),
                    action: action.clone(),
                    notes: "Default notification".into(),
                });
                actions.push(action);
            }
        }

        self.audit_trail.extend(audit_entries.iter().cloned());

        ResponsePlan {
            severity: assessment.severity,
            actions,
            audit_trail: audit_entries,
        }
    }

    /// Retrieve immutable audit trail for compliance checks.
    pub fn audit_trail(&self) -> &[AuditEntry] {
        &self.audit_trail
    }
}

fn severity_at_least(actual: ThreatSeverity, expected: ThreatSeverity) -> bool {
    use ThreatSeverity::*;
    match (actual, expected) {
        (Critical, _) => true,
        (High, Low | Medium | High) => true,
        (Medium, Low | Medium) => true,
        (Low, Low) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_policies_and_audit_trail() {
        let mut engine = SecurityResponseEngine::new(vec!["secops".into()]);
        engine.register_policy(ResponsePolicy {
            name: "contain-egress".into(),
            min_severity: ThreatSeverity::High,
            actions: vec![ResponseAction::Quarantine {
                resource: "egress-gateway".into(),
            }],
            escalation_after: Duration::from_secs(300),
        });

        let assessment = ThreatAssessment {
            risk_score: 0.9,
            severity: ThreatSeverity::High,
            matched_patterns: vec!["egress".into()],
            anomalies: vec![],
            evidence: vec![serde_json::json!({})],
        };

        let plan = engine.execute(&assessment);
        assert_eq!(plan.actions.len(), 1);
        assert!(matches!(plan.actions[0], ResponseAction::Quarantine { .. }));
        assert!(!engine.audit_trail().is_empty());
    }

    #[test]
    fn defaults_to_notification() {
        let mut engine = SecurityResponseEngine::new(vec!["oncall".into()]);
        let assessment = ThreatAssessment::compliant();
        let plan = engine.execute(&assessment);
        assert!(matches!(plan.actions[0], ResponseAction::Notify { .. }));
    }
}
