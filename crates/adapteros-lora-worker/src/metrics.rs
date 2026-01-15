//! Quality metrics for model evaluation
//!
//! Implements metrics from the adapterOS specification:
//! - ARR: Abstention Rate (Responsible)
//! - ECS: Evidence Coverage Score
//! - HLR: Hallucination Rate
//! - CR: Citation Rate
//! - NAR: Numeric Accuracy Rate
//! - PAR: Policy Adherence Rate

use adapteros_lora_rag::EvidenceSpan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Abstention Rate (Responsible) - fraction of queries where system abstained
    pub arr: f32,

    /// Evidence Coverage Score @ K - fraction of responses with >=K evidence spans
    pub ecs_5: f32,

    /// Hallucination Rate - estimated fraction of unsupported claims
    pub hlr: f32,

    /// Citation Rate - fraction of factual claims with citations
    pub cr: f32,

    /// Numeric Accuracy Rate - fraction of numeric claims with correct units
    pub nar: f32,

    /// Policy Adherence Rate - fraction meeting all policy requirements
    pub par: f32,
}

impl QualityMetrics {
    /// Check if metrics meet production thresholds
    pub fn meets_thresholds(&self, thresholds: &QualityThresholds) -> bool {
        self.arr >= thresholds.arr_min
            && self.ecs_5 >= thresholds.ecs5_min
            && self.hlr <= thresholds.hlr_max
            && self.cr >= thresholds.cr_min
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityThresholds {
    pub arr_min: f32,
    pub ecs5_min: f32,
    pub hlr_max: f32,
    pub cr_min: f32,
}

impl Default for QualityThresholds {
    fn default() -> Self {
        // From specification: build_release policy
        Self {
            arr_min: 0.95,
            ecs5_min: 0.75,
            hlr_max: 0.03,
            cr_min: 0.01,
        }
    }
}

/// Compute quality metrics from a set of inference results
pub struct MetricsCalculator {
    _thresholds: QualityThresholds,
}

impl MetricsCalculator {
    pub fn new(thresholds: QualityThresholds) -> Self {
        Self {
            _thresholds: thresholds,
        }
    }

    /// Calculate ARR (Abstention Rate)
    pub fn calculate_arr(&self, results: &[InferenceResult]) -> f32 {
        if results.is_empty() {
            return 0.0;
        }

        let abstained = results
            .iter()
            .filter(|r| r.status == "insufficient_evidence" || r.status == "low_confidence")
            .count();

        abstained as f32 / results.len() as f32
    }

    /// Calculate ECS@5 (Evidence Coverage Score at 5 spans)
    pub fn calculate_ecs_5(&self, results: &[InferenceResult]) -> f32 {
        if results.is_empty() {
            return 0.0;
        }

        let with_sufficient_evidence = results.iter().filter(|r| r.evidence.len() >= 5).count();

        with_sufficient_evidence as f32 / results.len() as f32
    }

    /// Calculate HLR (Hallucination Rate) - simplified heuristic
    ///
    /// In production, this would use:
    /// - Consistency checking across sources
    /// - Factuality verification against knowledge base
    /// - Temporal consistency checks
    pub fn calculate_hlr(&self, results: &[InferenceResult]) -> f32 {
        if results.is_empty() {
            return 0.0;
        }

        let mut hallucination_count = 0;

        for result in results {
            // Heuristic: responses without evidence are potential hallucinations
            if result.text.is_some() && result.evidence.is_empty() {
                hallucination_count += 1;
            }

            // Check for numeric claims without units (policy violation)
            if let Some(ref text) = result.text {
                if self.has_unsupported_numeric_claims(text) {
                    hallucination_count += 1;
                }
            }
        }

        hallucination_count as f32 / results.len() as f32
    }

    /// Calculate CR (Citation Rate)
    pub fn calculate_cr(&self, results: &[InferenceResult]) -> f32 {
        if results.is_empty() {
            return 0.0;
        }

        let with_citations = results
            .iter()
            .filter(|r| r.text.is_some() && !r.evidence.is_empty())
            .count();

        with_citations as f32 / results.len() as f32
    }

    /// Calculate NAR (Numeric Accuracy Rate)
    pub fn calculate_nar(&self, results: &[InferenceResult]) -> f32 {
        if results.is_empty() {
            return 1.0; // No numerics = 100% accuracy
        }

        let mut numeric_claims = 0;
        let mut correct_units = 0;

        for result in results {
            if let Some(ref text) = result.text {
                let (claims, correct) = self.analyze_numeric_claims(text);
                numeric_claims += claims;
                correct_units += correct;
            }
        }

        if numeric_claims == 0 {
            return 1.0;
        }

        correct_units as f32 / numeric_claims as f32
    }

    /// Calculate PAR (Policy Adherence Rate)
    pub fn calculate_par(&self, results: &[InferenceResult]) -> f32 {
        if results.is_empty() {
            return 0.0;
        }

        let policy_compliant = results
            .iter()
            .filter(|r| {
                // Check evidence requirement
                let evidence_ok = !r.evidence.is_empty() || r.status != "ok";

                // Check no unsupported claims
                let no_unsupported = if let Some(ref text) = r.text {
                    !self.has_unsupported_claims(text, &r.evidence)
                } else {
                    true
                };

                evidence_ok && no_unsupported
            })
            .count();

        policy_compliant as f32 / results.len() as f32
    }

    /// Calculate all metrics at once
    pub fn calculate_all(&self, results: &[InferenceResult]) -> QualityMetrics {
        QualityMetrics {
            arr: self.calculate_arr(results),
            ecs_5: self.calculate_ecs_5(results),
            hlr: self.calculate_hlr(results),
            cr: self.calculate_cr(results),
            nar: self.calculate_nar(results),
            par: self.calculate_par(results),
        }
    }

    // Helper methods for heuristic checks

    fn has_unsupported_numeric_claims(&self, text: &str) -> bool {
        // Simple heuristic: look for numbers without nearby unit indicators
        // In production, this would use NLP to extract numeric entities
        let has_number = text.chars().any(|c| c.is_numeric());
        let has_unit = text.contains("psi") || text.contains("lbf") || text.contains("in");

        has_number && !has_unit
    }

    fn analyze_numeric_claims(&self, text: &str) -> (usize, usize) {
        // Simplified: count numbers and check for unit keywords nearby
        let mut claims = 0;
        let mut correct = 0;

        // In production, use proper tokenization and entity extraction
        for word in text.split_whitespace() {
            if word.chars().any(|c| c.is_numeric()) {
                claims += 1;
                // Check if next few words contain unit
                if text.contains("psi") || text.contains("lbf") || text.contains("in") {
                    correct += 1;
                }
            }
        }

        (claims, correct)
    }

    fn has_unsupported_claims(&self, text: &str, evidence: &[EvidenceSpan]) -> bool {
        // Heuristic: if response is substantive but no evidence, flag it
        text.split_whitespace().count() > 20 && evidence.is_empty()
    }
}

impl Default for MetricsCalculator {
    fn default() -> Self {
        Self::new(QualityThresholds::default())
    }
}

/// Inference result for metrics calculation
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub text: Option<String>,
    pub status: String,
    pub evidence: Vec<EvidenceSpan>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arr_calculation() {
        let calculator = MetricsCalculator::default();

        let results = vec![
            InferenceResult {
                text: Some("answer".to_string()),
                status: "ok".to_string(),
                evidence: vec![],
            },
            InferenceResult {
                text: None,
                status: "insufficient_evidence".to_string(),
                evidence: vec![],
            },
            InferenceResult {
                text: None,
                status: "low_confidence".to_string(),
                evidence: vec![],
            },
        ];

        let arr = calculator.calculate_arr(&results);
        assert!((arr - 0.666).abs() < 0.01); // 2 out of 3 abstained
    }

    #[test]
    fn test_metrics_meet_thresholds() {
        let thresholds = QualityThresholds::default();

        let good_metrics = QualityMetrics {
            arr: 0.96,
            ecs_5: 0.80,
            hlr: 0.02,
            cr: 0.95,
            nar: 0.98,
            par: 0.95,
        };

        assert!(good_metrics.meets_thresholds(&thresholds));

        let bad_metrics = QualityMetrics {
            arr: 0.90, // Below threshold
            ecs_5: 0.80,
            hlr: 0.02,
            cr: 0.95,
            nar: 0.98,
            par: 0.95,
        };

        assert!(!bad_metrics.meets_thresholds(&thresholds));
    }
}
