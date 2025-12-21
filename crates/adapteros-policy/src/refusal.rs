//! Refusal response generation

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
        }
    }

    pub fn missing_fields(template: String, fields: Vec<String>) -> Self {
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
        }
    }
}
