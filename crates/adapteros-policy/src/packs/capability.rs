//! Capability Policy Pack
//!
//! Detects and prevents hallucinated capability claims in responses.
//! Validates that response content does not claim actions the system cannot perform.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Patterns that indicate a capability claim (e.g., "I opened your account")
static CAPABILITY_CLAIM_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // "I [action] your [object]" patterns
        Regex::new(r"(?i)\bI\s+(have\s+)?(opened|accessed|logged\s+into|signed\s+into)\s+(your|the)\s+account\b").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(purchased|bought|ordered)\s+").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(sent|emailed|mailed)\s+(you|your|the|an?)\s+(email|message|letter)\b").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(deleted|removed|erased)\s+(your|the)\s+(file|data|record|account)\b").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(edited|modified|changed|updated)\s+(your|the)\s+(image|photo|picture|file|document)\b").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(transferred|moved|wired)\s+(the\s+)?(money|funds|payment)\b").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(created|made|set\s+up)\s+(a\s+|an\s+|your\s+)?(new\s+)?account\b").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(called|phoned|contacted)\s+").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(booked|reserved|scheduled)\s+(your|the|a)\b").unwrap(),
        Regex::new(r"(?i)\bI\s+(have\s+)?(cancelled|canceled)\s+(your|the)\b").unwrap(),
        // Past tense completions
        Regex::new(r"(?i)\bI('ve|\s+have)\s+successfully\s+(completed|finished|processed)\s+(the|your)\b").unwrap(),
        Regex::new(r"(?i)\bthe\s+(transaction|payment|transfer|order)\s+(has\s+been|was)\s+(completed|processed|confirmed)\b").unwrap(),
    ]
});

/// System capabilities whitelist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCapabilities {
    /// Can generate text responses
    pub can_generate_text: bool,
    /// Can search/retrieve documents
    pub can_search_documents: bool,
    /// Can analyze content
    pub can_analyze_content: bool,
    /// Can format/transform text
    pub can_format_text: bool,
    /// Can provide instructions
    pub can_provide_instructions: bool,
}

impl Default for SystemCapabilities {
    fn default() -> Self {
        Self {
            can_generate_text: true,
            can_search_documents: true,
            can_analyze_content: true,
            can_format_text: true,
            can_provide_instructions: true,
        }
    }
}

/// Capability policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityConfig {
    /// Enable capability claim detection
    pub enabled: bool,
    /// System capabilities
    pub capabilities: SystemCapabilities,
    /// Additional forbidden claim patterns (regex strings)
    pub forbidden_patterns: Vec<String>,
    /// Replacement message for invalid claims
    pub replacement_template: String,
}

impl Default for CapabilityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            capabilities: SystemCapabilities::default(),
            forbidden_patterns: vec![],
            replacement_template:
                "I cannot perform this action directly. Here's how you can do it yourself:"
                    .to_string(),
        }
    }
}

/// Detected capability claim
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityClaim {
    /// The matched text
    pub matched_text: String,
    /// Position in the content
    pub position: usize,
    /// The claimed action
    pub claimed_action: String,
}

/// Result of capability validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityValidationResult {
    /// Whether the content is valid (no hallucinated claims)
    pub is_valid: bool,
    /// Detected invalid claims
    pub invalid_claims: Vec<CapabilityClaim>,
    /// Suggested replacements
    pub suggested_replacements: Vec<String>,
}

/// Capability policy implementation
pub struct CapabilityPolicy {
    config: CapabilityConfig,
    custom_patterns: Vec<Regex>,
}

impl CapabilityPolicy {
    /// Create new capability policy
    pub fn new(config: CapabilityConfig) -> Self {
        let custom_patterns = config
            .forbidden_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self {
            config,
            custom_patterns,
        }
    }

    /// Detect capability claims in content
    pub fn detect_claims(&self, content: &str) -> Vec<CapabilityClaim> {
        if !self.config.enabled {
            return vec![];
        }

        let mut claims = Vec::new();
        let mut seen_positions: HashSet<usize> = HashSet::new();

        // Check built-in patterns
        for pattern in CAPABILITY_CLAIM_PATTERNS.iter() {
            for mat in pattern.find_iter(content) {
                if !seen_positions.contains(&mat.start()) {
                    seen_positions.insert(mat.start());
                    claims.push(CapabilityClaim {
                        matched_text: mat.as_str().to_string(),
                        position: mat.start(),
                        claimed_action: Self::extract_action(mat.as_str()),
                    });
                }
            }
        }

        // Check custom patterns
        for pattern in &self.custom_patterns {
            for mat in pattern.find_iter(content) {
                if !seen_positions.contains(&mat.start()) {
                    seen_positions.insert(mat.start());
                    claims.push(CapabilityClaim {
                        matched_text: mat.as_str().to_string(),
                        position: mat.start(),
                        claimed_action: Self::extract_action(mat.as_str()),
                    });
                }
            }
        }

        claims
    }

    /// Extract the action verb from a matched claim
    fn extract_action(text: &str) -> String {
        let action_verbs = [
            "opened",
            "accessed",
            "logged into",
            "signed into",
            "purchased",
            "bought",
            "ordered",
            "sent",
            "emailed",
            "mailed",
            "deleted",
            "removed",
            "erased",
            "edited",
            "modified",
            "changed",
            "updated",
            "transferred",
            "moved",
            "wired",
            "created",
            "made",
            "set up",
            "called",
            "phoned",
            "contacted",
            "booked",
            "reserved",
            "scheduled",
            "cancelled",
            "canceled",
            "completed",
            "finished",
            "processed",
        ];

        let lower = text.to_lowercase();
        for verb in action_verbs {
            if lower.contains(verb) {
                return verb.to_string();
            }
        }

        "unknown action".to_string()
    }

    /// Validate content for capability claims
    pub fn validate_content(&self, content: &str) -> CapabilityValidationResult {
        let claims = self.detect_claims(content);
        let is_valid = claims.is_empty();

        let suggested_replacements = claims
            .iter()
            .map(|claim| {
                format!(
                    "{} (Instead of claiming '{}')",
                    self.config.replacement_template, claim.claimed_action
                )
            })
            .collect();

        CapabilityValidationResult {
            is_valid,
            invalid_claims: claims,
            suggested_replacements,
        }
    }

    /// Filter content to remove or replace invalid claims
    pub fn filter_content(&self, content: &str) -> String {
        if !self.config.enabled {
            return content.to_string();
        }

        let mut result = content.to_string();
        let claims = self.detect_claims(content);

        // Sort by position descending to replace from end to start
        let mut sorted_claims = claims;
        sorted_claims.sort_by(|a, b| b.position.cmp(&a.position));

        for claim in sorted_claims {
            let replacement = format!(
                "[Note: {} Here's how you can do this yourself:]",
                self.config.replacement_template
            );
            result.replace_range(
                claim.position..claim.position + claim.matched_text.len(),
                &replacement,
            );
        }

        result
    }
}

/// Context for capability policy enforcement
#[derive(Debug)]
pub struct CapabilityContext {
    pub content: String,
    pub tenant_id: String,
}

impl PolicyContext for CapabilityContext {
    fn context_type(&self) -> &str {
        "capability"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for CapabilityPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Capability
    }

    fn name(&self) -> &'static str {
        "Capability"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let cap_ctx = ctx
            .as_any()
            .downcast_ref::<CapabilityContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid capability context".to_string()))?;

        let validation = self.validate_content(&cap_ctx.content);

        let violations: Vec<Violation> = validation
            .invalid_claims
            .iter()
            .map(|claim| Violation {
                severity: Severity::High,
                message: format!(
                    "Hallucinated capability claim detected: '{}'",
                    claim.matched_text
                ),
                details: Some(format!(
                    "Position: {}, Claimed action: {}",
                    claim.position, claim.claimed_action
                )),
            })
            .collect();

        Ok(Audit {
            policy_id: PolicyId::Capability,
            passed: violations.is_empty(),
            violations,
            warnings: validation.suggested_replacements,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_account_access_claims() {
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "I have opened your account and reviewed the transactions.";
        let claims = policy.detect_claims(content);

        assert!(!claims.is_empty());
        assert!(claims[0].claimed_action.contains("opened"));
    }

    #[test]
    fn test_detect_purchase_claims() {
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "I purchased the item for you and it will arrive tomorrow.";
        let claims = policy.detect_claims(content);

        assert!(!claims.is_empty());
        assert!(claims[0].claimed_action.contains("purchased"));
    }

    #[test]
    fn test_detect_email_claims() {
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "I have sent an email to confirm the details.";
        let claims = policy.detect_claims(content);

        assert!(!claims.is_empty());
        assert!(claims[0].claimed_action.contains("sent"));
    }

    #[test]
    fn test_no_false_positives() {
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        // These should NOT trigger
        let safe_content = "I can help you understand how to open an account. Here are the steps you need to follow.";
        let claims = policy.detect_claims(safe_content);

        assert!(claims.is_empty());
    }

    #[test]
    fn test_filter_content() {
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "I have opened your account. You can now view your balance.";
        let filtered = policy.filter_content(content);

        assert!(!filtered.contains("I have opened your account"));
        assert!(filtered.contains("Here's how you can do this yourself"));
    }

    #[test]
    fn test_validation_result() {
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "I deleted your file and sent an email to confirm.";
        let result = policy.validate_content(content);

        assert!(!result.is_valid);
        assert!(result.invalid_claims.len() >= 1);
        assert!(!result.suggested_replacements.is_empty());
    }

    #[test]
    fn test_disabled_policy() {
        let config = CapabilityConfig {
            enabled: false,
            ..Default::default()
        };
        let policy = CapabilityPolicy::new(config);

        let content = "I purchased everything for you.";
        let claims = policy.detect_claims(content);

        assert!(claims.is_empty());
    }

    // === EDGE CASES ===

    #[test]
    fn test_edge_case_quoted_speech() {
        // EDGE CASE: Should NOT flag when user quotes what they want AI to say
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        // This is describing capability, not claiming it
        let content = "The user said 'I opened your account' in their message.";
        let claims = policy.detect_claims(content);

        // KNOWN LIMITATION: Currently triggers false positive
        // TODO: Add quote detection to reduce false positives
        assert!(!claims.is_empty()); // Documents current behavior
    }

    #[test]
    fn test_edge_case_hypothetical() {
        // EDGE CASE: Hypothetical/conditional statements
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        // "If I could" or "I would" patterns - not actual claims
        let content = "If I could access your account, I would help you.";
        let _claims = policy.detect_claims(content);

        // KNOWN LIMITATION: May trigger on hypotheticals
        // Current patterns don't distinguish hypothetical from actual
    }

    #[test]
    fn test_edge_case_negation() {
        // EDGE CASE: Negated claims should NOT trigger
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "I cannot purchase items for you.";
        let claims = policy.detect_claims(content);

        // Should not trigger because it's a negation
        assert!(claims.is_empty());
    }

    #[test]
    fn test_edge_case_empty_content() {
        // EDGE CASE: Empty string
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let claims = policy.detect_claims("");
        assert!(claims.is_empty());
    }

    #[test]
    fn test_edge_case_unicode() {
        // EDGE CASE: Unicode characters in content
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "I have purchased 日本語 items for you.";
        let claims = policy.detect_claims(content);

        assert!(!claims.is_empty());
    }

    #[test]
    fn test_edge_case_multiline() {
        // EDGE CASE: Claims split across lines
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        // This content has "I have" and "purchased" on same line, just newline in text
        let content = "I have\npurchased the item.";
        let claims = policy.detect_claims(content);

        // Current behavior: \s+ in regex matches newlines, so claim IS detected
        // This is actually desirable - prevents evasion via line breaks
        assert!(!claims.is_empty());
    }

    #[test]
    fn test_edge_case_legitimate_instruction() {
        // EDGE CASE: Instructing user how to do something
        let policy = CapabilityPolicy::new(CapabilityConfig::default());

        let content = "To open your account, click the login button.";
        let claims = policy.detect_claims(content);

        // Should not flag instructions TO the user
        assert!(claims.is_empty());
    }
}
