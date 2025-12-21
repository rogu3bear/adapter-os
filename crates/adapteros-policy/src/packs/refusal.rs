//! Refusal Policy Pack
//!
//! Abstains when evidence spans are insufficient or confidence falls below threshold.
//! Denies unsafe operations and redacts outputs.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Refusal policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefusalConfig {
    /// Abstain threshold for confidence
    pub abstain_threshold: f32,
    /// Missing fields templates for different domains
    pub missing_fields_templates: HashMap<String, Vec<String>>,
    /// Refusal reasons configuration
    pub refusal_reasons: RefusalReasons,
    /// Redaction rules
    pub redaction_rules: RedactionRules,
    /// Safety checks
    pub safety_checks: SafetyChecks,
}

/// Refusal reasons configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefusalReasons {
    /// Insufficient evidence
    pub insufficient_evidence: String,
    /// Low confidence
    pub low_confidence: String,
    /// Safety violation
    pub safety_violation: String,
    /// Policy violation
    pub policy_violation: String,
    /// Unsupported operation
    pub unsupported_operation: String,
}

/// Redaction rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionRules {
    /// Redact sensitive information
    pub redact_sensitive: bool,
    /// Redact personal information
    pub redact_personal: bool,
    /// Redact financial information
    pub redact_financial: bool,
    /// Redact health information
    pub redact_health: bool,
    /// Redaction patterns
    pub patterns: Vec<RedactionPattern>,
}

/// Redaction pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPattern {
    /// Pattern name
    pub name: String,
    /// Regex pattern
    pub pattern: String,
    /// Replacement text
    pub replacement: String,
}

/// Safety checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyChecks {
    /// Check for harmful content
    pub check_harmful: bool,
    /// Check for biased content
    pub check_biased: bool,
    /// Check for misleading content
    pub check_misleading: bool,
    /// Check for illegal content
    pub check_illegal: bool,
    /// Safety thresholds
    pub thresholds: SafetyThresholds,
}

/// Safety thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyThresholds {
    /// Harmful content threshold
    pub harmful_threshold: f32,
    /// Biased content threshold
    pub biased_threshold: f32,
    /// Misleading content threshold
    pub misleading_threshold: f32,
    /// Illegal content threshold
    pub illegal_threshold: f32,
}

impl Default for RefusalConfig {
    fn default() -> Self {
        let mut missing_fields_templates = HashMap::new();
        missing_fields_templates.insert(
            "torque_spec".to_string(),
            vec![
                "aircraft_effectivity".to_string(),
                "component_pn".to_string(),
            ],
        );
        missing_fields_templates.insert(
            "code_review".to_string(),
            vec!["reviewer".to_string(), "approval_status".to_string()],
        );

        Self {
            abstain_threshold: 0.55,
            missing_fields_templates,
            refusal_reasons: RefusalReasons {
                insufficient_evidence: "Insufficient evidence to provide a reliable answer"
                    .to_string(),
                low_confidence: "Confidence level below required threshold".to_string(),
                safety_violation: "Request violates safety guidelines".to_string(),
                policy_violation: "Request violates policy requirements".to_string(),
                unsupported_operation: "Operation not supported in current context".to_string(),
            },
            redaction_rules: RedactionRules {
                redact_sensitive: true,
                redact_personal: true,
                redact_financial: true,
                redact_health: true,
                patterns: vec![
                    RedactionPattern {
                        name: "email".to_string(),
                        pattern: r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b".to_string(),
                        replacement: "[EMAIL]".to_string(),
                    },
                    RedactionPattern {
                        name: "phone".to_string(),
                        pattern: r"\b\d{3}-\d{3}-\d{4}\b".to_string(),
                        replacement: "[PHONE]".to_string(),
                    },
                ],
            },
            safety_checks: SafetyChecks {
                check_harmful: true,
                check_biased: true,
                check_misleading: true,
                check_illegal: true,
                thresholds: SafetyThresholds {
                    harmful_threshold: 0.8,
                    biased_threshold: 0.7,
                    misleading_threshold: 0.6,
                    illegal_threshold: 0.9,
                },
            },
        }
    }
}

/// Refusal response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefusalResponse {
    /// Refusal reason
    pub reason: RefusalReason,
    /// Missing fields (if applicable)
    pub missing_fields: Option<Vec<String>>,
    /// Suggested actions
    pub suggested_actions: Vec<String>,
    /// Confidence score
    pub confidence: f32,
    /// Safety scores
    pub safety_scores: Option<SafetyScores>,
}

/// Refusal reason
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RefusalReason {
    /// Insufficient evidence
    InsufficientEvidence,
    /// Low confidence
    LowConfidence,
    /// Safety violation
    SafetyViolation,
    /// Policy violation
    PolicyViolation,
    /// Unsupported operation
    UnsupportedOperation,
    /// Missing required fields
    MissingFields,
}

/// Safety scores
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyScores {
    /// Harmful content score
    pub harmful: f32,
    /// Biased content score
    pub biased: f32,
    /// Misleading content score
    pub misleading: f32,
    /// Illegal content score
    pub illegal: f32,
}

/// Refusal policy enforcement
pub struct RefusalPolicy {
    config: RefusalConfig,
}

impl RefusalPolicy {
    /// Create a new refusal policy
    pub fn new(config: RefusalConfig) -> Self {
        Self { config }
    }

    /// Check if should abstain based on confidence
    pub fn should_abstain(&self, confidence: f32) -> bool {
        confidence < self.config.abstain_threshold
    }

    /// Check if should abstain based on evidence spans
    pub fn should_abstain_evidence(&self, span_count: usize, min_spans: usize) -> bool {
        span_count < min_spans
    }

    /// Check safety scores
    pub fn check_safety_scores(&self, scores: &SafetyScores) -> Result<()> {
        if self.config.safety_checks.check_harmful
            && scores.harmful > self.config.safety_checks.thresholds.harmful_threshold
        {
            return Err(AosError::PolicyViolation(
                "Harmful content detected".to_string(),
            ));
        }

        if self.config.safety_checks.check_biased
            && scores.biased > self.config.safety_checks.thresholds.biased_threshold
        {
            return Err(AosError::PolicyViolation(
                "Biased content detected".to_string(),
            ));
        }

        if self.config.safety_checks.check_misleading
            && scores.misleading > self.config.safety_checks.thresholds.misleading_threshold
        {
            return Err(AosError::PolicyViolation(
                "Misleading content detected".to_string(),
            ));
        }

        if self.config.safety_checks.check_illegal
            && scores.illegal > self.config.safety_checks.thresholds.illegal_threshold
        {
            return Err(AosError::PolicyViolation(
                "Illegal content detected".to_string(),
            ));
        }

        Ok(())
    }

    /// Generate refusal response
    pub fn generate_refusal_response(
        &self,
        reason: RefusalReason,
        missing_fields: Option<Vec<String>>,
        confidence: f32,
        safety_scores: Option<SafetyScores>,
    ) -> RefusalResponse {
        let suggested_actions = match reason {
            RefusalReason::InsufficientEvidence => vec![
                "Provide more specific context".to_string(),
                "Include relevant documentation".to_string(),
            ],
            RefusalReason::LowConfidence => vec![
                "Request additional information".to_string(),
                "Clarify the question".to_string(),
            ],
            RefusalReason::MissingFields => {
                if let Some(fields) = &missing_fields {
                    fields.iter().map(|f| format!("Provide {}", f)).collect()
                } else {
                    vec!["Provide required information".to_string()]
                }
            }
            _ => vec!["Contact support for assistance".to_string()],
        };

        RefusalResponse {
            reason,
            missing_fields,
            suggested_actions,
            confidence,
            safety_scores,
        }
    }

    /// Apply redaction rules
    pub fn apply_redaction(&self, text: &str) -> String {
        let mut result = text.to_string();

        for pattern in &self.config.redaction_rules.patterns {
            if let Ok(regex) = regex::Regex::new(&pattern.pattern) {
                result = regex.replace_all(&result, &pattern.replacement).to_string();
            }
        }

        result
    }

    /// Check missing fields for domain
    pub fn check_missing_fields(
        &self,
        domain: &str,
        provided_fields: &[String],
    ) -> Option<Vec<String>> {
        if let Some(required_fields) = self.config.missing_fields_templates.get(domain) {
            let missing: Vec<String> = required_fields
                .iter()
                .filter(|field| !provided_fields.contains(field))
                .cloned()
                .collect();

            if missing.is_empty() {
                None
            } else {
                Some(missing)
            }
        } else {
            None
        }
    }
}

impl Policy for RefusalPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Refusal
    }

    fn name(&self) -> &'static str {
        "Refusal"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // specific policy requirements

        if violations.is_empty() {
            Ok(Audit::passed(self.id()))
        } else {
            Ok(Audit::failed(self.id(), violations))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refusal_policy_creation() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Refusal);
        assert_eq!(policy.name(), "Refusal");
        assert_eq!(policy.severity(), Severity::High);
    }

    #[test]
    fn test_refusal_config_default() {
        let config = RefusalConfig::default();
        assert_eq!(config.abstain_threshold, 0.55);
        assert!(config.redaction_rules.redact_sensitive);
        assert!(config.safety_checks.check_harmful);
    }

    #[test]
    fn test_should_abstain() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Should abstain
        assert!(policy.should_abstain(0.4));

        // Should not abstain
        assert!(!policy.should_abstain(0.7));
    }

    #[test]
    fn test_should_abstain_evidence() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Should abstain
        assert!(policy.should_abstain_evidence(0, 1));

        // Should not abstain
        assert!(!policy.should_abstain_evidence(2, 1));
    }

    #[test]
    fn test_safety_scores_check() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        let safe_scores = SafetyScores {
            harmful: 0.1,
            biased: 0.2,
            misleading: 0.3,
            illegal: 0.1,
        };

        assert!(policy.check_safety_scores(&safe_scores).is_ok());

        let unsafe_scores = SafetyScores {
            harmful: 0.9, // Above threshold
            biased: 0.2,
            misleading: 0.3,
            illegal: 0.1,
        };

        assert!(policy.check_safety_scores(&unsafe_scores).is_err());
    }

    #[test]
    fn test_generate_refusal_response() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        let response =
            policy.generate_refusal_response(RefusalReason::InsufficientEvidence, None, 0.3, None);

        assert_eq!(response.reason, RefusalReason::InsufficientEvidence);
        assert_eq!(response.confidence, 0.3);
        assert!(!response.suggested_actions.is_empty());
    }

    #[test]
    fn test_apply_redaction() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        let text = "Contact me at john@example.com or call 555-123-4567";
        let redacted = policy.apply_redaction(text);

        assert!(redacted.contains("[EMAIL]"));
        assert!(redacted.contains("[PHONE]"));
    }

    #[test]
    fn test_check_missing_fields() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Check missing fields
        let missing =
            policy.check_missing_fields("torque_spec", &["aircraft_effectivity".to_string()]);
        assert!(missing.is_some());
        assert!(missing.unwrap().contains(&"component_pn".to_string()));

        // Check no missing fields
        let no_missing = policy.check_missing_fields(
            "torque_spec",
            &[
                "aircraft_effectivity".to_string(),
                "component_pn".to_string(),
            ],
        );
        assert!(no_missing.is_none());
    }
}
