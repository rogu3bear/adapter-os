//! Refusal Policy Pack
//!
//! Abstains when evidence spans are insufficient or confidence falls below threshold.
//! Denies unsafe operations and redacts outputs.
//!
//! Implements best-effort response mode: instead of asking clarifying questions,
//! proceeds with stated assumptions when confidence is moderate.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Response mode determined by confidence level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResponseMode {
    /// Full answer with high confidence - no caveats needed
    Complete,
    /// Proceed with stated assumptions when confidence is moderate
    /// Instead of asking clarifying questions, deliver partial results with explicit assumptions
    BestEffort {
        /// Explicit assumptions made to proceed
        assumptions: Vec<String>,
    },
    /// Hard abstain - only for safety violations or very low confidence
    Abstain,
}

/// Best-effort response with stated assumptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestEffortResponse {
    /// The content being delivered
    pub content: String,
    /// Assumptions made to produce this response
    pub stated_assumptions: Vec<String>,
    /// Confidence level of the response
    pub confidence: f32,
    /// Areas where user might want to verify or provide more info
    pub verification_hints: Vec<String>,
}

/// Refusal policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefusalConfig {
    /// Abstain threshold for confidence (below this = hard abstain)
    pub abstain_threshold: f32,
    /// Best-effort threshold (between abstain_threshold and this = best-effort mode)
    /// Above this threshold = complete response
    pub best_effort_threshold: f32,
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
    /// Check for self-harm content
    pub check_self_harm: bool,
    /// Safety thresholds
    pub thresholds: SafetyThresholds,
    /// Self-harm detection patterns
    pub self_harm_patterns: Vec<String>,
    /// High-stakes domain configuration
    pub high_stakes_config: HighStakesConfig,
}

/// High-stakes domain types requiring elevated confidence thresholds
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HighStakesDomain {
    /// Medical advice domain
    Medical,
    /// Legal advice domain
    Legal,
    /// Financial advice domain
    Financial,
    /// No high-stakes domain detected
    None,
}

/// Configuration for high-stakes domain handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighStakesConfig {
    /// Confidence threshold for medical domain (default: 0.85)
    pub medical_threshold: f32,
    /// Confidence threshold for legal domain (default: 0.80)
    pub legal_threshold: f32,
    /// Confidence threshold for financial domain (default: 0.80)
    pub financial_threshold: f32,
    /// Keywords to detect medical domain
    pub medical_keywords: Vec<String>,
    /// Keywords to detect legal domain
    pub legal_keywords: Vec<String>,
    /// Keywords to detect financial domain
    pub financial_keywords: Vec<String>,
    /// Disclaimer for medical advice
    pub medical_disclaimer: String,
    /// Disclaimer for legal advice
    pub legal_disclaimer: String,
    /// Disclaimer for financial advice
    pub financial_disclaimer: String,
}

impl Default for HighStakesConfig {
    fn default() -> Self {
        Self {
            medical_threshold: 0.85,
            legal_threshold: 0.80,
            financial_threshold: 0.80,
            medical_keywords: vec![
                "diagnosis".to_string(),
                "treatment".to_string(),
                "medication".to_string(),
                "dosage".to_string(),
                "symptoms".to_string(),
                "prescription".to_string(),
                "medical advice".to_string(),
                "health condition".to_string(),
            ],
            legal_keywords: vec![
                "legal advice".to_string(),
                "lawsuit".to_string(),
                "liability".to_string(),
                "contract".to_string(),
                "sue".to_string(),
                "attorney".to_string(),
                "court".to_string(),
                "legal rights".to_string(),
            ],
            financial_keywords: vec![
                "investment advice".to_string(),
                "stock".to_string(),
                "portfolio".to_string(),
                "retirement".to_string(),
                "tax advice".to_string(),
                "financial planning".to_string(),
                "trading".to_string(),
                "securities".to_string(),
            ],
            medical_disclaimer: "This information is not medical advice. Please consult a qualified healthcare professional for medical guidance.".to_string(),
            legal_disclaimer: "This information is not legal advice. Please consult a licensed attorney for legal guidance.".to_string(),
            financial_disclaimer: "This information is not financial advice. Please consult a qualified financial advisor for investment decisions.".to_string(),
        }
    }
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
            abstain_threshold: 0.40,
            best_effort_threshold: 0.70,
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
                    RedactionPattern {
                        name: "ssn".to_string(),
                        pattern: r"\b\d{3}-\d{2}-\d{4}\b".to_string(),
                        replacement: "[SSN]".to_string(),
                    },
                    RedactionPattern {
                        name: "credit_card".to_string(),
                        pattern: r"\b(?:\d{4}[-\s]?){3}\d{4}\b".to_string(),
                        replacement: "[CREDIT_CARD]".to_string(),
                    },
                ],
            },
            safety_checks: SafetyChecks {
                check_harmful: true,
                check_biased: true,
                check_misleading: true,
                check_illegal: true,
                check_self_harm: true,
                thresholds: SafetyThresholds {
                    harmful_threshold: 0.8,
                    biased_threshold: 0.7,
                    misleading_threshold: 0.6,
                    illegal_threshold: 0.9,
                },
                self_harm_patterns: vec![
                    "suicide".to_string(),
                    "kill myself".to_string(),
                    "end my life".to_string(),
                    "self-harm".to_string(),
                    "self harm".to_string(),
                    "hurt myself".to_string(),
                    "want to die".to_string(),
                    "don't want to live".to_string(),
                    "no reason to live".to_string(),
                    "better off dead".to_string(),
                    "cutting myself".to_string(),
                    "overdose".to_string(),
                ],
                high_stakes_config: HighStakesConfig::default(),
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
    /// Self-harm concern detected - requires supportive response with crisis resources
    SelfHarmConcern,
    /// High-stakes domain requiring elevated confidence (medical/legal/financial)
    HighStakesDomain,
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

    /// Determine response mode based on confidence level
    /// - confidence >= best_effort_threshold: Complete (full answer)
    /// - confidence >= abstain_threshold: BestEffort (proceed with assumptions)
    /// - confidence < abstain_threshold: Abstain (hard refusal)
    pub fn determine_response_mode(
        &self,
        confidence: f32,
        context_hints: Option<&[String]>,
    ) -> ResponseMode {
        if confidence >= self.config.best_effort_threshold {
            ResponseMode::Complete
        } else if confidence >= self.config.abstain_threshold {
            // Generate assumptions based on what's uncertain
            let assumptions = self.generate_assumptions(confidence, context_hints);
            ResponseMode::BestEffort { assumptions }
        } else {
            ResponseMode::Abstain
        }
    }

    /// Generate assumptions for best-effort mode
    fn generate_assumptions(
        &self,
        confidence: f32,
        context_hints: Option<&[String]>,
    ) -> Vec<String> {
        let mut assumptions = Vec::new();

        // Add confidence-based assumption
        if confidence < 0.6 {
            assumptions
                .push("Assuming the query refers to the most common interpretation".to_string());
        }

        // Add context-based assumptions if provided
        if let Some(hints) = context_hints {
            for hint in hints {
                assumptions.push(format!("Assuming: {}", hint));
            }
        }

        // Add general assumption about scope
        if assumptions.is_empty() {
            assumptions.push(
                "Proceeding with general interpretation; please clarify if specific context is needed".to_string(),
            );
        }

        assumptions
    }

    /// Create a best-effort response with stated assumptions
    pub fn create_best_effort_response(
        &self,
        content: String,
        confidence: f32,
        context_hints: Option<&[String]>,
    ) -> BestEffortResponse {
        let assumptions = self.generate_assumptions(confidence, context_hints);

        let verification_hints = vec![
            "Consider verifying specific details if accuracy is critical".to_string(),
            "Additional context may improve response quality".to_string(),
        ];

        BestEffortResponse {
            content,
            stated_assumptions: assumptions,
            confidence,
            verification_hints,
        }
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
            RefusalReason::SelfHarmConcern => vec![
                "If you're in crisis, please call 988 (Suicide & Crisis Lifeline)".to_string(),
                "Text HOME to 741741 (Crisis Text Line)".to_string(),
                "Call SAMHSA at 1-800-662-4357 for mental health support".to_string(),
                "You're not alone—please reach out to a trusted person or professional".to_string(),
            ],
            RefusalReason::HighStakesDomain => vec![
                "This topic requires professional guidance".to_string(),
                "Please consult a qualified professional for personalized advice".to_string(),
                "The information provided should not replace professional consultation".to_string(),
            ],
            RefusalReason::SafetyViolation => vec![
                "This request cannot be fulfilled for safety reasons".to_string(),
                "Consider rephrasing to focus on educational or informational aspects".to_string(),
            ],
            RefusalReason::PolicyViolation => vec![
                "This request violates usage policies".to_string(),
                "Review the acceptable use guidelines for more information".to_string(),
            ],
            RefusalReason::UnsupportedOperation => vec![
                "This operation is not currently supported".to_string(),
                "Check available features or contact support for alternatives".to_string(),
            ],
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

    /// Check content for self-harm indicators
    ///
    /// Returns true if the content contains patterns that suggest self-harm ideation
    /// or planning. When detected, responses should include crisis resources
    /// (988 Suicide & Crisis Lifeline, Crisis Text Line, SAMHSA).
    pub fn check_self_harm(&self, content: &str) -> bool {
        if !self.config.safety_checks.check_self_harm {
            return false;
        }

        let content_lower = content.to_lowercase();
        self.config
            .safety_checks
            .self_harm_patterns
            .iter()
            .any(|pattern| content_lower.contains(&pattern.to_lowercase()))
    }

    /// Detect if content relates to a high-stakes domain requiring elevated confidence
    ///
    /// Returns the detected domain type (Medical, Legal, Financial, or None).
    /// When a high-stakes domain is detected, elevated confidence thresholds should apply.
    pub fn detect_high_stakes_domain(&self, content: &str) -> HighStakesDomain {
        let content_lower = content.to_lowercase();
        let config = &self.config.safety_checks.high_stakes_config;

        // Check medical keywords
        if config
            .medical_keywords
            .iter()
            .any(|kw| content_lower.contains(&kw.to_lowercase()))
        {
            return HighStakesDomain::Medical;
        }

        // Check legal keywords
        if config
            .legal_keywords
            .iter()
            .any(|kw| content_lower.contains(&kw.to_lowercase()))
        {
            return HighStakesDomain::Legal;
        }

        // Check financial keywords
        if config
            .financial_keywords
            .iter()
            .any(|kw| content_lower.contains(&kw.to_lowercase()))
        {
            return HighStakesDomain::Financial;
        }

        HighStakesDomain::None
    }

    /// Get the confidence threshold for a high-stakes domain
    pub fn get_domain_threshold(&self, domain: &HighStakesDomain) -> f32 {
        let config = &self.config.safety_checks.high_stakes_config;
        match domain {
            HighStakesDomain::Medical => config.medical_threshold,
            HighStakesDomain::Legal => config.legal_threshold,
            HighStakesDomain::Financial => config.financial_threshold,
            HighStakesDomain::None => self.config.abstain_threshold,
        }
    }

    /// Get the disclaimer for a high-stakes domain
    pub fn get_domain_disclaimer(&self, domain: &HighStakesDomain) -> Option<&str> {
        let config = &self.config.safety_checks.high_stakes_config;
        match domain {
            HighStakesDomain::Medical => Some(&config.medical_disclaimer),
            HighStakesDomain::Legal => Some(&config.legal_disclaimer),
            HighStakesDomain::Financial => Some(&config.financial_disclaimer),
            HighStakesDomain::None => None,
        }
    }

    /// Check if should abstain for a high-stakes domain
    ///
    /// Uses elevated confidence thresholds for medical, legal, and financial domains.
    pub fn should_abstain_high_stakes(&self, content: &str, confidence: f32) -> bool {
        let domain = self.detect_high_stakes_domain(content);
        let threshold = self.get_domain_threshold(&domain);
        confidence < threshold
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
        assert_eq!(config.abstain_threshold, 0.40);
        assert_eq!(config.best_effort_threshold, 0.70);
        assert!(config.redaction_rules.redact_sensitive);
        assert!(config.safety_checks.check_harmful);
    }

    #[test]
    fn test_should_abstain() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Should abstain (below 0.40)
        assert!(policy.should_abstain(0.3));

        // Should not abstain (above 0.40)
        assert!(!policy.should_abstain(0.5));
    }

    #[test]
    fn test_determine_response_mode_complete() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // High confidence -> Complete
        let mode = policy.determine_response_mode(0.85, None);
        assert_eq!(mode, ResponseMode::Complete);
    }

    #[test]
    fn test_determine_response_mode_best_effort() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Moderate confidence -> BestEffort
        let mode = policy.determine_response_mode(0.55, None);
        match mode {
            ResponseMode::BestEffort { assumptions } => {
                assert!(!assumptions.is_empty());
            }
            _ => panic!("Expected BestEffort mode"),
        }
    }

    #[test]
    fn test_determine_response_mode_abstain() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Low confidence -> Abstain
        let mode = policy.determine_response_mode(0.25, None);
        assert_eq!(mode, ResponseMode::Abstain);
    }

    #[test]
    fn test_create_best_effort_response() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        let response = policy.create_best_effort_response(
            "Here is the answer based on available context.".to_string(),
            0.55,
            Some(&["using default settings".to_string()]),
        );

        assert!(!response.stated_assumptions.is_empty());
        assert!(response
            .stated_assumptions
            .iter()
            .any(|a| a.contains("default settings")));
        assert_eq!(response.confidence, 0.55);
        assert!(!response.verification_hints.is_empty());
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

    #[test]
    fn test_check_self_harm_detection() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Should detect self-harm patterns
        assert!(policy.check_self_harm("I want to kill myself"));
        assert!(policy.check_self_harm("thinking about suicide"));
        assert!(policy.check_self_harm("I want to end my life"));
        assert!(policy.check_self_harm("I've been self-harm cutting"));

        // Should not flag benign content
        assert!(!policy.check_self_harm("I had a great day today"));
        assert!(!policy.check_self_harm("Working on a programming project"));
    }

    #[test]
    fn test_self_harm_response_includes_crisis_resources() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        let response =
            policy.generate_refusal_response(RefusalReason::SelfHarmConcern, None, 0.0, None);

        assert_eq!(response.reason, RefusalReason::SelfHarmConcern);
        // Should include 988 crisis line
        assert!(response.suggested_actions.iter().any(|a| a.contains("988")));
        // Should include Crisis Text Line
        assert!(response
            .suggested_actions
            .iter()
            .any(|a| a.contains("741741")));
        // Should include SAMHSA
        assert!(response
            .suggested_actions
            .iter()
            .any(|a| a.contains("SAMHSA")));
    }

    #[test]
    fn test_detect_high_stakes_domain() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Medical domain
        assert_eq!(
            policy.detect_high_stakes_domain("What is the recommended dosage?"),
            HighStakesDomain::Medical
        );
        assert_eq!(
            policy.detect_high_stakes_domain("I need medical advice for my symptoms"),
            HighStakesDomain::Medical
        );

        // Legal domain
        assert_eq!(
            policy.detect_high_stakes_domain("Can I sue for this?"),
            HighStakesDomain::Legal
        );
        assert_eq!(
            policy.detect_high_stakes_domain("I need legal advice about the contract"),
            HighStakesDomain::Legal
        );

        // Financial domain
        assert_eq!(
            policy.detect_high_stakes_domain("What stock should I invest in?"),
            HighStakesDomain::Financial
        );
        assert_eq!(
            policy.detect_high_stakes_domain("Give me investment advice for my portfolio"),
            HighStakesDomain::Financial
        );

        // No high-stakes domain
        assert_eq!(
            policy.detect_high_stakes_domain("How do I write a for loop?"),
            HighStakesDomain::None
        );
    }

    #[test]
    fn test_high_stakes_domain_thresholds() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Medical has highest threshold (0.85)
        assert_eq!(
            policy.get_domain_threshold(&HighStakesDomain::Medical),
            0.85
        );
        // Legal and Financial have 0.80
        assert_eq!(policy.get_domain_threshold(&HighStakesDomain::Legal), 0.80);
        assert_eq!(
            policy.get_domain_threshold(&HighStakesDomain::Financial),
            0.80
        );
        // None uses default abstain threshold (0.40)
        assert_eq!(policy.get_domain_threshold(&HighStakesDomain::None), 0.40);
    }

    #[test]
    fn test_should_abstain_high_stakes() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        // Medical content with confidence below 0.85 should abstain
        assert!(policy.should_abstain_high_stakes("What is the dosage?", 0.80));

        // Medical content with confidence above 0.85 should not abstain
        assert!(!policy.should_abstain_high_stakes("What is the dosage?", 0.90));

        // Non-high-stakes content with confidence above 0.55 should not abstain
        assert!(!policy.should_abstain_high_stakes("How do I write code?", 0.60));
    }

    #[test]
    fn test_high_stakes_domain_disclaimers() {
        let config = RefusalConfig::default();
        let policy = RefusalPolicy::new(config);

        assert!(policy
            .get_domain_disclaimer(&HighStakesDomain::Medical)
            .unwrap()
            .contains("medical advice"));
        assert!(policy
            .get_domain_disclaimer(&HighStakesDomain::Legal)
            .unwrap()
            .contains("legal advice"));
        assert!(policy
            .get_domain_disclaimer(&HighStakesDomain::Financial)
            .unwrap()
            .contains("financial advice"));
        assert!(policy
            .get_domain_disclaimer(&HighStakesDomain::None)
            .is_none());
    }
}
