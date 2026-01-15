#![cfg(all(test, feature = "extended-tests"))]
//! Evidence-Grounded Response Testing Utilities
//!
//! This module provides utilities for testing evidence-grounded response generation
//! in the adapterOS system, ensuring that responses are properly backed by evidence
//! and meet quality and relevance criteria.
//!
//! ## Key Features
//!
//! - **Evidence Validation**: Verify evidence spans and citations
//! - **Response Quality**: Assess response accuracy and relevance
//! - **Citation Verification**: Check citation correctness and completeness
//! - **Confidence Scoring**: Validate confidence metrics
//! - **Deterministic Testing**: Reproducible evidence evaluation
//!
//! ## Usage
//!
//! ```rust
//! use tests_unit::evidence::*;
//!
//! #[test]
//! fn test_evidence_grounded_response() {
//!     let validator = EvidenceValidator::new();
//!     let response = generate_response_with_evidence();
//!     let result = validator.validate_response(&response);
//!     assert!(result.is_valid);
//! }
//! ```

use std::collections::HashMap;
use adapteros_core::{B3Hash, derive_seed, derive_seed_indexed};

/// Evidence validator for testing response grounding
pub struct EvidenceValidator {
    validation_rules: Vec<Box<dyn ValidationRule + Send + Sync>>,
    seed: B3Hash,
}

impl EvidenceValidator {
    /// Create a new evidence validator
    pub fn new() -> Self {
        Self {
            validation_rules: Vec::new(),
            seed: B3Hash::hash(b"default_evidence_seed"),
        }
    }

    /// Create a validator with a specific seed for deterministic testing
    pub fn with_seed(seed: u64) -> Self {
        Self {
            validation_rules: Vec::new(),
            seed: B3Hash::hash(&seed.to_le_bytes()),
        }
    }

    /// Add a validation rule
    pub fn add_rule<R: ValidationRule + Send + Sync + 'static>(mut self, rule: R) -> Self {
        self.validation_rules.push(Box::new(rule));
        self
    }

    /// Validate a response with evidence
    pub fn validate_response(&self, response: &EvidenceResponse) -> ValidationResult {
        let mut issues = Vec::new();
        let mut score = 1.0;

        for rule in &self.validation_rules {
            let result = rule.validate(response);
            if !result.is_valid {
                issues.extend(result.issues);
                score *= result.penalty;
            }
        }

        // Apply default validation rules if none specified
        if self.validation_rules.is_empty() {
            let default_result = self.apply_default_validation(response);
            issues.extend(default_result.issues);
            score *= default_result.penalty;
        }

        ValidationResult {
            is_valid: issues.is_empty(),
            score,
            issues,
        }
    }

    /// Apply default validation rules
    fn apply_default_validation(&self, response: &EvidenceResponse) -> RuleValidationResult {
        let mut issues = Vec::new();
        let mut penalty = 1.0;

        // Check minimum evidence spans
        if response.evidence_spans.len() < 3 {
            issues.push(ValidationIssue::InsufficientEvidence(
                format!("Only {} evidence spans, minimum required is 3", response.evidence_spans.len())
            ));
            penalty *= 0.8;
        }

        // Check confidence score
        if response.confidence < 0.7 {
            issues.push(ValidationIssue::LowConfidence(response.confidence));
            penalty *= response.confidence;
        }

        // Check citation coverage
        let total_text_length = response.response_text.len();
        let cited_length: usize = response.evidence_spans.iter()
            .map(|span| span.end - span.start)
            .sum();

        let coverage_ratio = cited_length as f64 / total_text_length as f64;
        if coverage_ratio < 0.5 {
            issues.push(ValidationIssue::LowCitationCoverage(coverage_ratio));
            penalty *= coverage_ratio.max(0.1);
        }

        RuleValidationResult {
            is_valid: issues.is_empty(),
            issues,
            penalty,
        }
    }

    /// Validate evidence spans for correctness
    pub fn validate_evidence_spans(&self, response: &EvidenceResponse, source_text: &str) -> SpanValidationResult {
        let mut valid_spans = Vec::new();
        let mut invalid_spans = Vec::new();

        for span in &response.evidence_spans {
            if span.start >= span.end || span.end > source_text.len() {
                invalid_spans.push(span.clone());
            } else {
                let span_text = &source_text[span.start..span.end];
                // Check if span text matches the cited content
                if self.text_matches_evidence(span_text, &span.evidence_text) {
                    valid_spans.push(span.clone());
                } else {
                    invalid_spans.push(span.clone());
                }
            }
        }

        SpanValidationResult {
            valid_spans,
            invalid_spans,
            coverage_ratio: valid_spans.len() as f64 / response.evidence_spans.len() as f64,
        }
    }

    /// Check if span text matches evidence (simple implementation)
    fn text_matches_evidence(&self, span_text: &str, evidence_text: &str) -> bool {
        // Simple substring check - in practice, this would use more sophisticated matching
        span_text.contains(evidence_text) || evidence_text.contains(span_text)
    }
}

/// Trait for validation rules
pub trait ValidationRule {
    /// Validate a response and return result
    fn validate(&self, response: &EvidenceResponse) -> RuleValidationResult;
}

/// Result of rule validation
#[derive(Debug, Clone)]
pub struct RuleValidationResult {
    pub is_valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub penalty: f64,
}

/// Overall validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub score: f64,
    pub issues: Vec<ValidationIssue>,
}

/// Evidence span validation result
#[derive(Debug, Clone)]
pub struct SpanValidationResult {
    pub valid_spans: Vec<EvidenceSpan>,
    pub invalid_spans: Vec<EvidenceSpan>,
    pub coverage_ratio: f64,
}

/// Validation issues
#[derive(Debug, Clone)]
pub enum ValidationIssue {
    InsufficientEvidence(String),
    LowConfidence(f64),
    LowCitationCoverage(f64),
    InvalidSpan(String),
    MissingCitation(String),
    InaccurateEvidence(String),
}

/// Evidence response structure
#[derive(Debug, Clone)]
pub struct EvidenceResponse {
    pub response_text: String,
    pub evidence_spans: Vec<EvidenceSpan>,
    pub confidence: f64,
    pub citations: Vec<Citation>,
    pub metadata: HashMap<String, String>,
}

/// Evidence span
#[derive(Debug, Clone)]
pub struct EvidenceSpan {
    pub start: usize,
    pub end: usize,
    pub evidence_text: String,
    pub source: String,
    pub confidence: f64,
}

/// Citation
#[derive(Debug, Clone)]
pub struct Citation {
    pub id: String,
    pub source: String,
    pub span: Option<(usize, usize)>,
    pub relevance_score: f64,
}

/// Evidence generator for creating test data
pub struct EvidenceGenerator {
    seed: B3Hash,
    counter: u64,
}

impl EvidenceGenerator {
    /// Create a new evidence generator
    pub fn new(seed: u64) -> Self {
        Self {
            seed: B3Hash::hash(&seed.to_le_bytes()),
            counter: 0,
        }
    }

    /// Generate a deterministic evidence response
    pub fn generate_response(&mut self, source_text: &str, prompt: &str) -> EvidenceResponse {
        self.counter += 1;
        let derived_seed = derive_seed_indexed(&self.seed, "response", self.counter);

        // Generate response text based on prompt
        let response_text = self.generate_response_text(prompt, &derived_seed);

        // Generate evidence spans
        let evidence_spans = self.generate_evidence_spans(source_text, &response_text, &derived_seed);

        // Generate citations
        let citations = self.generate_citations(&evidence_spans, &derived_seed);

        // Calculate confidence
        let confidence = self.calculate_confidence(&evidence_spans, &derived_seed);

        EvidenceResponse {
            response_text,
            evidence_spans,
            confidence,
            citations,
            metadata: self.generate_metadata(&derived_seed),
        }
    }

    /// Generate response text
    fn generate_response_text(&self, prompt: &str, seed: &[u8; 32]) -> String {
        // Simple deterministic text generation based on seed
        let words = ["The", "function", "takes", "parameters", "and", "returns", "a", "value", "based", "on", "input"];
        let mut result = prompt.to_string() + " ";

        for i in 0..(seed[0] % 10 + 5) { // 5-14 words
            let word_index = seed[i as usize % 32] as usize % words.len();
            result.push_str(words[word_index]);
            result.push(' ');
        }

        result.trim().to_string()
    }

    /// Generate evidence spans
    fn generate_evidence_spans(&self, source_text: &str, response_text: &str, seed: &[u8; 32]) -> Vec<EvidenceSpan> {
        let mut spans = Vec::new();
        let num_spans = (seed[1] % 5 + 1) as usize; // 1-5 spans

        for i in 0..num_spans {
            let span_seed = derive_seed_indexed(&B3Hash::from(*seed), &format!("span_{}", i), i as u64);
            let start = (span_seed[0] as usize * span_seed[1] as usize) % source_text.len().max(1);
            let length = (span_seed[2] as usize % 50) + 10; // 10-59 characters
            let end = (start + length).min(source_text.len());

            if end > start {
                let evidence_text = source_text[start..end].to_string();
                let confidence = (span_seed[3] as f64) / 255.0; // 0-1

                spans.push(EvidenceSpan {
                    start,
                    end,
                    evidence_text,
                    source: format!("source_{}", i),
                    confidence,
                });
            }
        }

        spans
    }

    /// Generate citations
    fn generate_citations(&self, spans: &[EvidenceSpan], seed: &[u8; 32]) -> Vec<Citation> {
        spans.iter().enumerate().map(|(i, span)| {
            let citation_seed = derive_seed_indexed(&B3Hash::from(*seed), &format!("citation_{}", i), i as u64);
            let relevance = (citation_seed[0] as f64) / 255.0;

            Citation {
                id: format!("citation_{}", i),
                source: span.source.clone(),
                span: Some((span.start, span.end)),
                relevance_score: relevance,
            }
        }).collect()
    }

    /// Calculate confidence score
    fn calculate_confidence(&self, spans: &[EvidenceSpan], seed: &[u8; 32]) -> f64 {
        if spans.is_empty() {
            return 0.0;
        }

        let avg_span_confidence = spans.iter().map(|s| s.confidence).sum::<f64>() / spans.len() as f64;
        let seed_confidence = (seed[0] as f64) / 255.0;

        (avg_span_confidence + seed_confidence) / 2.0
    }

    /// Generate metadata
    fn generate_metadata(&self, seed: &[u8; 32]) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("generator_version".to_string(), "1.0.0".to_string());
        metadata.insert("seed_hash".to_string(), B3Hash::from(*seed).to_hex());
        metadata.insert("deterministic".to_string(), "true".to_string());
        metadata
    }
}

/// Response quality assessor
pub struct ResponseQualityAssessor {
    quality_metrics: Vec<Box<dyn QualityMetric + Send + Sync>>,
}

impl ResponseQualityAssessor {
    /// Create a new quality assessor
    pub fn new() -> Self {
        Self {
            quality_metrics: Vec::new(),
        }
    }

    /// Add a quality metric
    pub fn add_metric<M: QualityMetric + Send + Sync + 'static>(mut self, metric: M) -> Self {
        self.quality_metrics.push(Box::new(metric));
        self
    }

    /// Assess response quality
    pub fn assess_quality(&self, response: &EvidenceResponse) -> QualityAssessment {
        let mut scores = HashMap::new();
        let mut overall_score = 1.0;

        for metric in &self.quality_metrics {
            let score = metric.score(response);
            scores.insert(metric.name().to_string(), score);
            overall_score *= score;
        }

        // Apply default metrics if none specified
        if self.quality_metrics.is_empty() {
            let default_scores = self.apply_default_metrics(response);
            for (name, score) in default_scores {
                scores.insert(name, score);
                overall_score *= score;
            }
        }

        QualityAssessment {
            overall_score,
            metric_scores: scores,
        }
    }

    /// Apply default quality metrics
    fn apply_default_metrics(&self, response: &EvidenceResponse) -> HashMap<String, f64> {
        let mut scores = HashMap::new();

        // Evidence density
        let evidence_density = response.evidence_spans.len() as f64 / response.response_text.len() as f64 * 1000.0;
        scores.insert("evidence_density".to_string(), (evidence_density / 10.0).min(1.0));

        // Confidence score
        scores.insert("confidence".to_string(), response.confidence);

        // Citation completeness
        let citation_completeness = response.citations.len() as f64 / response.evidence_spans.len() as f64;
        scores.insert("citation_completeness".to_string(), citation_completeness.min(1.0));

        scores
    }
}

/// Trait for quality metrics
pub trait QualityMetric {
    /// Calculate quality score (0.0 to 1.0)
    fn score(&self, response: &EvidenceResponse) -> f64;

    /// Get metric name
    fn name(&self) -> &str;
}

/// Quality assessment result
#[derive(Debug, Clone)]
pub struct QualityAssessment {
    pub overall_score: f64,
    pub metric_scores: HashMap<String, f64>,
}

/// Evidence-based test runner
pub struct EvidenceTestRunner {
    generator: EvidenceGenerator,
    validator: EvidenceValidator,
    assessor: ResponseQualityAssessor,
    test_cases: Vec<EvidenceTestCase>,
}

#[derive(Debug, Clone)]
pub struct EvidenceTestCase {
    pub name: String,
    pub source_text: String,
    pub prompt: String,
    pub expected_min_confidence: f64,
    pub expected_min_spans: usize,
}

impl EvidenceTestRunner {
    /// Create a new evidence test runner
    pub fn new(seed: u64) -> Self {
        Self {
            generator: EvidenceGenerator::new(seed),
            validator: EvidenceValidator::with_seed(seed),
            assessor: ResponseQualityAssessor::new(),
            test_cases: Vec::new(),
        }
    }

    /// Add a test case
    pub fn add_test_case(&mut self, test_case: EvidenceTestCase) {
        self.test_cases.push(test_case);
    }

    /// Run all test cases
    pub fn run_tests(&mut self) -> Vec<TestResult> {
        self.test_cases.iter().map(|test_case| {
            let response = self.generator.generate_response(&test_case.source_text, &test_case.prompt);
            let validation = self.validator.validate_response(&response);
            let quality = self.assessor.assess_quality(&response);

            let passed = validation.is_valid &&
                        response.confidence >= test_case.expected_min_confidence &&
                        response.evidence_spans.len() >= test_case.expected_min_spans;

            TestResult {
                test_name: test_case.name.clone(),
                passed,
                validation_result: validation,
                quality_assessment: quality,
                response: response.clone(),
            }
        }).collect()
    }
}

/// Test result
#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_name: String,
    pub passed: bool,
    pub validation_result: ValidationResult,
    pub quality_assessment: QualityAssessment,
    pub response: EvidenceResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evidence_validator() {
        let validator = EvidenceValidator::with_seed(42);

        let response = EvidenceResponse {
            response_text: "This is a test response with some evidence.".to_string(),
            evidence_spans: vec![
                EvidenceSpan {
                    start: 0,
                    end: 10,
                    evidence_text: "This is a".to_string(),
                    source: "test".to_string(),
                    confidence: 0.9,
                },
                EvidenceSpan {
                    start: 10,
                    end: 20,
                    evidence_text: "test response".to_string(),
                    source: "test".to_string(),
                    confidence: 0.8,
                },
                EvidenceSpan {
                    start: 20,
                    end: 30,
                    evidence_text: "with some".to_string(),
                    source: "test".to_string(),
                    confidence: 0.7,
                },
            ],
            confidence: 0.85,
            citations: vec![],
            metadata: HashMap::new(),
        };

        let result = validator.validate_response(&response);
        assert!(result.is_valid, "Response should be valid: {:?}", result.issues);
        assert!(result.score > 0.7, "Score should be reasonable: {}", result.score);
    }

    #[test]
    fn test_evidence_generator_determinism() {
        let mut gen1 = EvidenceGenerator::new(123);
        let mut gen2 = EvidenceGenerator::new(123);

        let source_text = "This is some source text for testing evidence generation.";
        let prompt = "Explain this function";

        let response1 = gen1.generate_response(source_text, prompt);
        let response2 = gen2.generate_response(source_text, prompt);

        // Should be identical due to same seed
        assert_eq!(response1.response_text, response2.response_text);
        assert_eq!(response1.confidence, response2.confidence);
        assert_eq!(response1.evidence_spans.len(), response2.evidence_spans.len());
    }

    #[test]
    fn test_response_quality_assessment() {
        let assessor = ResponseQualityAssessor::new();

        let response = EvidenceResponse {
            response_text: "Short response".to_string(),
            evidence_spans: vec![EvidenceSpan {
                start: 0,
                end: 6,
                evidence_text: "Short".to_string(),
                source: "test".to_string(),
                confidence: 0.8,
            }],
            confidence: 0.9,
            citations: vec![Citation {
                id: "1".to_string(),
                source: "test".to_string(),
                span: Some((0, 6)),
                relevance_score: 0.9,
            }],
            metadata: HashMap::new(),
        };

        let assessment = assessor.assess_quality(&response);
        assert!(assessment.overall_score > 0.0, "Should have some quality score");
        assert!(!assessment.metric_scores.is_empty(), "Should have metric scores");
    }

    #[test]
    fn test_evidence_test_runner() {
        let mut runner = EvidenceTestRunner::new(456);

        runner.add_test_case(EvidenceTestCase {
            name: "basic_evidence_test".to_string(),
            source_text: "This is a comprehensive source text with lots of information for testing.".to_string(),
            prompt: "What does this text say?".to_string(),
            expected_min_confidence: 0.5,
            expected_min_spans: 1,
        });

        let results = runner.run_tests();
        assert_eq!(results.len(), 1);

        let result = &results[0];
        assert!(result.test_name == "basic_evidence_test");
        // Test may or may not pass depending on generated response quality
    }

    #[test]
    fn test_span_validation() {
        let validator = EvidenceValidator::with_seed(789);
        let source_text = "This is the source text for validation testing.";

        let response = EvidenceResponse {
            response_text: "Response based on source".to_string(),
            evidence_spans: vec![
                EvidenceSpan {
                    start: 0,
                    end: 10,
                    evidence_text: "This is the".to_string(),
                    source: "test".to_string(),
                    confidence: 0.9,
                },
                EvidenceSpan {
                    start: 50, // Invalid - beyond source text length
                    end: 60,
                    evidence_text: "invalid".to_string(),
                    source: "test".to_string(),
                    confidence: 0.5,
                },
            ],
            confidence: 0.8,
            citations: vec![],
            metadata: HashMap::new(),
        };

        let span_result = validator.validate_evidence_spans(&response, source_text);
        assert_eq!(span_result.valid_spans.len(), 1);
        assert_eq!(span_result.invalid_spans.len(), 1);
        assert_eq!(span_result.coverage_ratio, 0.5);
    }
}</code>
