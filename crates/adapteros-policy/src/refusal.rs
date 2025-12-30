//! Refusal response generation
//!
//! This module provides refusal responses that always include actionable
//! alternatives to help users understand how to proceed when a request
//! cannot be fulfilled.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefusalReason {
    InsufficientEvidence {
        needed: usize,
        found: usize,
    },
    LowConfidence {
        threshold: f32,
        actual: f32,
    },
    MissingFields {
        template: String,
        fields: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefusalResponse {
    pub status: String,
    pub reason: RefusalReason,
    pub message: String,
    /// Suggested actions the user can take to resolve the refusal.
    /// Always populated with at least one actionable alternative.
    pub suggested_actions: Vec<String>,
}

impl RefusalResponse {
    pub fn insufficient_evidence(needed: usize, found: usize) -> Self {
        Self {
            status: "insufficient_evidence".to_string(),
            reason: RefusalReason::InsufficientEvidence { needed, found },
            message: format!(
                "Cannot provide answer with sufficient confidence. Found {} evidence spans, need {}.",
                found, needed
            ),
            suggested_actions: vec![
                "Provide more specific context in your query".to_string(),
                "Include relevant documentation or references".to_string(),
                format!("Add at least {} more evidence sources to the knowledge base", needed.saturating_sub(found)),
            ],
        }
    }

    pub fn low_confidence(threshold: f32, actual: f32) -> Self {
        Self {
            status: "low_confidence".to_string(),
            reason: RefusalReason::LowConfidence { threshold, actual },
            message: format!(
                "Confidence {} is below required threshold {}",
                actual, threshold
            ),
            suggested_actions: vec![
                "Rephrase your question to be more specific".to_string(),
                "Provide additional context or constraints".to_string(),
                "Consider breaking the question into smaller, focused queries".to_string(),
            ],
        }
    }

    pub fn missing_fields(template: String, fields: Vec<String>) -> Self {
        let field_suggestions: Vec<String> = fields
            .iter()
            .map(|f| format!("Provide the required field: {}", f))
            .collect();

        Self {
            status: "missing_fields".to_string(),
            reason: RefusalReason::MissingFields {
                template: template.clone(),
                fields: fields.clone(),
            },
            message: format!(
                "Cannot provide complete answer. Missing required fields for {}: {:?}",
                template, fields
            ),
            suggested_actions: if field_suggestions.is_empty() {
                vec!["Provide all required information for the request".to_string()]
            } else {
                field_suggestions
            },
        }
    }
}
