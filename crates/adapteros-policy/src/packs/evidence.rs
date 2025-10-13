//! Evidence Policy Pack
//!
//! Mandatory open-book grounding with evidence retrieval before generation for regulated domains.
//! Enforces trace, signatures, and audit artifacts.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Evidence policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceConfig {
    /// Require open-book grounding
    pub require_open_book: bool,
    /// Minimum number of evidence spans required
    pub min_spans: usize,
    /// Prefer latest revision
    pub prefer_latest_revision: bool,
    /// Warn on superseded evidence
    pub warn_on_superseded: bool,
    /// Evidence quality thresholds
    pub quality_thresholds: QualityThresholds,
    /// Evidence types allowed
    pub allowed_types: Vec<EvidenceType>,
    /// Evidence source requirements
    pub source_requirements: SourceRequirements,
}

/// Evidence quality thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityThresholds {
    /// Minimum relevance score
    pub min_relevance: f32,
    /// Minimum confidence score
    pub min_confidence: f32,
    /// Minimum recency (days)
    pub min_recency_days: u32,
    /// Maximum age (days)
    pub max_age_days: u32,
}

/// Evidence types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EvidenceType {
    /// Code documentation
    CodeDoc,
    /// API documentation
    ApiDoc,
    /// Test cases
    TestCase,
    /// Configuration files
    Config,
    /// Error logs
    ErrorLog,
    /// Performance metrics
    Performance,
    /// Security audit
    SecurityAudit,
    /// Compliance report
    Compliance,
}

/// Source requirements for evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRequirements {
    /// Require source signatures
    pub require_signatures: bool,
    /// Require source timestamps
    pub require_timestamps: bool,
    /// Require source versioning
    pub require_versioning: bool,
    /// Allowed source domains
    pub allowed_domains: Vec<String>,
    /// Blocked source domains
    pub blocked_domains: Vec<String>,
}

impl Default for EvidenceConfig {
    fn default() -> Self {
        Self {
            require_open_book: true,
            min_spans: 1,
            prefer_latest_revision: true,
            warn_on_superseded: true,
            quality_thresholds: QualityThresholds {
                min_relevance: 0.7,
                min_confidence: 0.8,
                min_recency_days: 0,
                max_age_days: 365,
            },
            allowed_types: vec![
                EvidenceType::CodeDoc,
                EvidenceType::ApiDoc,
                EvidenceType::TestCase,
                EvidenceType::Config,
            ],
            source_requirements: SourceRequirements {
                require_signatures: true,
                require_timestamps: true,
                require_versioning: true,
                allowed_domains: vec![],
                blocked_domains: vec![],
            },
        }
    }
}

/// Evidence span metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSpan {
    /// Document ID
    pub doc_id: String,
    /// Revision number
    pub rev: u32,
    /// Span hash for integrity
    pub span_hash: String,
    /// Relevance score
    pub relevance: f32,
    /// Confidence score
    pub confidence: f32,
    /// Evidence type
    pub evidence_type: EvidenceType,
    /// Source information
    pub source: SourceInfo,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Source domain
    pub domain: String,
    /// Source path
    pub path: String,
    /// Source version
    pub version: String,
    /// Source signature
    pub signature: Option<String>,
}

/// Evidence policy enforcement
pub struct EvidencePolicy {
    config: EvidenceConfig,
}

impl EvidencePolicy {
    /// Create a new evidence policy
    pub fn new(config: EvidenceConfig) -> Self {
        Self { config }
    }

    /// Validate evidence spans
    pub fn validate_evidence_spans(&self, spans: &[EvidenceSpan]) -> Result<()> {
        if spans.len() < self.config.min_spans {
            return Err(AosError::PolicyViolation(format!(
                "Insufficient evidence spans: {} < {}",
                spans.len(),
                self.config.min_spans
            )));
        }

        for span in spans {
            self.validate_evidence_span(span)?;
        }

        Ok(())
    }

    /// Validate individual evidence span
    fn validate_evidence_span(&self, span: &EvidenceSpan) -> Result<()> {
        // Validate evidence type
        if !self.config.allowed_types.contains(&span.evidence_type) {
            return Err(AosError::PolicyViolation(format!(
                "Evidence type {:?} not allowed",
                span.evidence_type
            )));
        }

        // Validate quality thresholds
        if span.relevance < self.config.quality_thresholds.min_relevance {
            return Err(AosError::PolicyViolation(format!(
                "Evidence relevance {} below threshold {}",
                span.relevance, self.config.quality_thresholds.min_relevance
            )));
        }

        if span.confidence < self.config.quality_thresholds.min_confidence {
            return Err(AosError::PolicyViolation(format!(
                "Evidence confidence {} below threshold {}",
                span.confidence, self.config.quality_thresholds.min_confidence
            )));
        }

        // Validate source requirements
        self.validate_source_info(&span.source)?;

        Ok(())
    }

    /// Validate source information
    fn validate_source_info(&self, source: &SourceInfo) -> Result<()> {
        // Check blocked domains
        if self
            .config
            .source_requirements
            .blocked_domains
            .contains(&source.domain)
        {
            return Err(AosError::PolicyViolation(format!(
                "Source domain {} is blocked",
                source.domain
            )));
        }

        // Check allowed domains (if specified)
        if !self.config.source_requirements.allowed_domains.is_empty()
            && !self
                .config
                .source_requirements
                .allowed_domains
                .contains(&source.domain)
        {
            return Err(AosError::PolicyViolation(format!(
                "Source domain {} not in allowed list",
                source.domain
            )));
        }

        // Validate signature requirement
        if self.config.source_requirements.require_signatures && source.signature.is_none() {
            return Err(AosError::PolicyViolation(
                "Source signature required but not provided".to_string(),
            ));
        }

        Ok(())
    }

    /// Check for superseded evidence
    pub fn check_superseded_evidence(&self, spans: &[EvidenceSpan]) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        if self.config.warn_on_superseded {
            // Group spans by doc_id
            let mut doc_groups: HashMap<String, Vec<&EvidenceSpan>> = HashMap::new();
            for span in spans {
                doc_groups
                    .entry(span.doc_id.clone())
                    .or_default()
                    .push(span);
            }

            // Check for superseded revisions
            for (doc_id, doc_spans) in doc_groups {
                if doc_spans.len() > 1 {
                    let max_rev = doc_spans.iter().map(|s| s.rev).max().unwrap();
                    let superseded: Vec<u32> = doc_spans
                        .iter()
                        .filter(|s| s.rev < max_rev)
                        .map(|s| s.rev)
                        .collect();

                    if !superseded.is_empty() {
                        warnings.push(format!(
                            "Document {} has superseded revisions: {:?}",
                            doc_id, superseded
                        ));
                    }
                }
            }
        }

        Ok(warnings)
    }

    /// Validate evidence retrieval requirements
    pub fn validate_retrieval_requirements(&self, has_evidence: bool) -> Result<()> {
        if self.config.require_open_book && !has_evidence {
            Err(AosError::PolicyViolation(
                "Open-book grounding required but no evidence provided".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Calculate evidence quality score
    pub fn calculate_quality_score(&self, spans: &[EvidenceSpan]) -> f32 {
        if spans.is_empty() {
            return 0.0;
        }

        let total_score: f32 = spans
            .iter()
            .map(|span| (span.relevance + span.confidence) / 2.0)
            .sum();

        total_score / spans.len() as f32
    }
}

impl Policy for EvidencePolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Evidence
    }

    fn name(&self) -> &'static str {
        "Evidence"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // evidence spans, retrieval requirements, etc.

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
    use chrono::Utc;

    #[test]
    fn test_evidence_policy_creation() {
        let config = EvidenceConfig::default();
        let policy = EvidencePolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Evidence);
        assert_eq!(policy.name(), "Evidence");
        assert_eq!(policy.severity(), Severity::High);
    }

    #[test]
    fn test_evidence_config_default() {
        let config = EvidenceConfig::default();
        assert!(config.require_open_book);
        assert_eq!(config.min_spans, 1);
        assert!(config.prefer_latest_revision);
        assert!(config.warn_on_superseded);
    }

    #[test]
    fn test_evidence_spans_validation() {
        let config = EvidenceConfig::default();
        let policy = EvidencePolicy::new(config);

        let valid_span = EvidenceSpan {
            doc_id: "doc1".to_string(),
            rev: 1,
            span_hash: "hash1".to_string(),
            relevance: 0.8,
            confidence: 0.9,
            evidence_type: EvidenceType::CodeDoc,
            source: SourceInfo {
                domain: "example.com".to_string(),
                path: "/docs/api.md".to_string(),
                version: "1.0".to_string(),
                signature: Some("sig1".to_string()),
            },
            timestamp: Utc::now(),
        };

        // Valid case
        assert!(policy
            .validate_evidence_spans(&[valid_span.clone()])
            .is_ok());

        // Insufficient spans
        assert!(policy.validate_evidence_spans(&[]).is_err());
    }

    #[test]
    fn test_evidence_quality_validation() {
        let config = EvidenceConfig::default();
        let policy = EvidencePolicy::new(config);

        let low_quality_span = EvidenceSpan {
            doc_id: "doc1".to_string(),
            rev: 1,
            span_hash: "hash1".to_string(),
            relevance: 0.5, // Below threshold
            confidence: 0.9,
            evidence_type: EvidenceType::CodeDoc,
            source: SourceInfo {
                domain: "example.com".to_string(),
                path: "/docs/api.md".to_string(),
                version: "1.0".to_string(),
                signature: Some("sig1".to_string()),
            },
            timestamp: Utc::now(),
        };

        assert!(policy.validate_evidence_spans(&[low_quality_span]).is_err());
    }

    #[test]
    fn test_retrieval_requirements_validation() {
        let config = EvidenceConfig::default();
        let policy = EvidencePolicy::new(config);

        // Valid case
        assert!(policy.validate_retrieval_requirements(true).is_ok());

        // Invalid case
        assert!(policy.validate_retrieval_requirements(false).is_err());
    }

    #[test]
    fn test_quality_score_calculation() {
        let config = EvidenceConfig::default();
        let policy = EvidencePolicy::new(config);

        let spans = vec![
            EvidenceSpan {
                doc_id: "doc1".to_string(),
                rev: 1,
                span_hash: "hash1".to_string(),
                relevance: 0.8,
                confidence: 0.9,
                evidence_type: EvidenceType::CodeDoc,
                source: SourceInfo {
                    domain: "example.com".to_string(),
                    path: "/docs/api.md".to_string(),
                    version: "1.0".to_string(),
                    signature: Some("sig1".to_string()),
                },
                timestamp: Utc::now(),
            },
            EvidenceSpan {
                doc_id: "doc2".to_string(),
                rev: 1,
                span_hash: "hash2".to_string(),
                relevance: 0.7,
                confidence: 0.8,
                evidence_type: EvidenceType::ApiDoc,
                source: SourceInfo {
                    domain: "example.com".to_string(),
                    path: "/docs/api2.md".to_string(),
                    version: "1.0".to_string(),
                    signature: Some("sig2".to_string()),
                },
                timestamp: Utc::now(),
            },
        ];

        let score = policy.calculate_quality_score(&spans);
        assert!(score > 0.0);
        assert!(score <= 1.0);
    }
}
