use crate::security_response::{ResponsePlan, SecurityResponseEngine};
use crate::threat_detection::{
    ThreatAssessment, ThreatDetectionEngine, ThreatSeverity, ThreatSignal,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Consolidated security report combining detection and response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    pub assessment: ThreatAssessment,
    pub response_plan: ResponsePlan,
    pub compliant: bool,
}

/// End-to-end security monitoring service that ties together detection and response.
#[derive(Debug)]
pub struct SecurityMonitoringService {
    detector: ThreatDetectionEngine,
    responder: SecurityResponseEngine,
    retention: Duration,
}

impl SecurityMonitoringService {
    pub fn new(
        detector: ThreatDetectionEngine,
        responder: SecurityResponseEngine,
        retention: Duration,
    ) -> Self {
        Self {
            detector,
            responder,
            retention,
        }
    }

    /// Process an individual security signal and generate a report.
    pub fn process_signal(&mut self, signal: ThreatSignal) -> SecurityReport {
        let assessment = self.detector.ingest(signal);
        let response_plan = self.responder.execute(&assessment);
        self.detector.prune(self.retention);

        let compliant = matches!(assessment.severity, ThreatSeverity::Low)
            && response_plan.actions.iter().all(|action| {
                matches!(
                    action,
                    crate::security_response::ResponseAction::Notify { .. }
                )
            });

        SecurityReport {
            assessment,
            response_plan,
            compliant,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security_response::{ResponseAction, ResponsePolicy};

    #[test]
    fn integrates_detection_and_response() {
        let mut detector = ThreatDetectionEngine::new(8);
        detector.update_baseline("auth", 5.0);
        detector.register_pattern("auth-spike", "auth", 30.0, 4, ThreatSeverity::High);

        let mut responder = SecurityResponseEngine::new(vec!["oncall".into()]);
        responder.register_policy(ResponsePolicy {
            name: "lockdown".into(),
            min_severity: ThreatSeverity::High,
            actions: vec![ResponseAction::Escalate {
                team: "incident-response".into(),
            }],
            escalation_after: Duration::from_secs(60),
        });

        let mut service =
            SecurityMonitoringService::new(detector, responder, Duration::from_secs(1));
        let signal = ThreatSignal::new("auth", 20.0, serde_json::json!({"tenant": "acme"}));
        let report = service.process_signal(signal);

        assert!(report.assessment.severity >= ThreatSeverity::Medium);
        assert!(!report.response_plan.actions.is_empty());
    }
}
