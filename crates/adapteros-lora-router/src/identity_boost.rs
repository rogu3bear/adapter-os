//! Identity-based routing for adapter selection
//!
//! Applies conditional boosts to adapter scores based on identity dataset rules.
//! Identity datasets define personas with routing rules that match against
//! inference context (prompt content, user attributes, etc.).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Identity configuration for a dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// Type identifier (always "identity")
    pub r#type: String,
    /// Persona definition
    pub persona: PersonaDefinition,
    /// Routing rules for this identity
    pub routing_rules: Vec<RoutingRule>,
}

/// Persona definition for identity datasets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaDefinition {
    /// Persona name
    pub name: String,
    /// Persona traits/characteristics
    #[serde(default)]
    pub traits: Vec<String>,
    /// Areas of expertise
    #[serde(default)]
    pub expertise: Vec<String>,
}

/// Routing rule for conditional adapter boosting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Condition to match
    pub condition: RuleCondition,
    /// Boost multiplier to apply when condition matches
    pub boost: f32,
}

/// Condition types for routing rules
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleCondition {
    /// Match if prompt contains specified text
    PromptContains { text: String },
    /// Match if any of the keywords are in prompt
    PromptKeywords { keywords: Vec<String> },
    /// Match specific user attributes
    UserAttribute { key: String, value: String },
    /// Always match (unconditional boost)
    Always,
}

impl RuleCondition {
    /// Check if this condition matches the given context
    pub fn matches(&self, context: &RouterContext) -> bool {
        match self {
            RuleCondition::PromptContains { text } => {
                context.prompt.to_lowercase().contains(&text.to_lowercase())
            }
            RuleCondition::PromptKeywords { keywords } => {
                let prompt_lower = context.prompt.to_lowercase();
                keywords
                    .iter()
                    .any(|kw| prompt_lower.contains(&kw.to_lowercase()))
            }
            RuleCondition::UserAttribute { key, value } => {
                context.user_attributes.get(key) == Some(value)
            }
            RuleCondition::Always => true,
        }
    }
}

/// Router context for evaluating identity rules
#[derive(Debug, Clone)]
pub struct RouterContext {
    /// The inference prompt
    pub prompt: String,
    /// User attributes for matching
    pub user_attributes: HashMap<String, String>,
}

/// Identity booster for adapter routing
pub struct IdentityBooster {
    /// Identity configurations indexed by adapter index
    identity_configs: HashMap<u16, IdentityConfig>,
}

impl IdentityBooster {
    /// Create new identity booster with configurations
    pub fn new(identity_configs: HashMap<u16, IdentityConfig>) -> Self {
        Self { identity_configs }
    }

    /// Apply identity-based boosts to adapter scores
    pub fn apply_boosts(&self, context: &RouterContext, scores: &mut [f32]) {
        for (adapter_idx, config) in &self.identity_configs {
            let idx = *adapter_idx as usize;
            if idx < scores.len() {
                for rule in &config.routing_rules {
                    if rule.condition.matches(context) {
                        scores[idx] *= rule.boost;
                        tracing::debug!(
                            adapter_idx = idx,
                            boost = rule.boost,
                            persona = %config.persona.name,
                            "Applied identity boost"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_contains_condition() {
        let condition = RuleCondition::PromptContains {
            text: "documentation".to_string(),
        };

        let context = RouterContext {
            prompt: "Write documentation for this API".to_string(),
            user_attributes: HashMap::new(),
        };

        assert!(condition.matches(&context));
    }

    #[test]
    fn test_identity_boost_application() {
        let mut config_map = HashMap::new();
        config_map.insert(
            0,
            IdentityConfig {
                r#type: "identity".to_string(),
                persona: PersonaDefinition {
                    name: "Technical Writer".to_string(),
                    traits: vec!["precise".to_string()],
                    expertise: vec!["documentation".to_string()],
                },
                routing_rules: vec![RoutingRule {
                    condition: RuleCondition::PromptContains {
                        text: "documentation".to_string(),
                    },
                    boost: 2.0,
                }],
            },
        );

        let booster = IdentityBooster::new(config_map);
        let context = RouterContext {
            prompt: "Write documentation".to_string(),
            user_attributes: HashMap::new(),
        };

        let mut scores = vec![1.0, 1.0, 1.0];
        booster.apply_boosts(&context, &mut scores);

        assert_eq!(scores[0], 2.0); // Boosted
        assert_eq!(scores[1], 1.0); // Not boosted
        assert_eq!(scores[2], 1.0); // Not boosted
    }
}
