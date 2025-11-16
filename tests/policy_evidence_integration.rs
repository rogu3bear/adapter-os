//! Integration tests for policy compliance with evidence tracking
//!
//! These tests validate the Evidence Policy Pack enforcement and
//! evidence-based retrieval requirements for regulatory compliance.

use adapteros_policy::packs::evidence::{
    EvidenceConfig, EvidencePolicy, EvidenceSpan, EvidenceType, QualityThresholds, SourceInfo,
    SourceRequirements,
};
use adapteros_policy::{Audit, Policy, PolicyId, Severity};
use chrono::Utc;
use std::collections::HashMap;

/// Mock policy context for testing
struct TestPolicyContext {
    metadata: HashMap<String, String>,
}

impl adapteros_policy::PolicyContext for TestPolicyContext {
    fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}

#[test]
fn test_evidence_policy_default_configuration() {
    let config = EvidenceConfig::default();
    let policy = EvidencePolicy::new(config);

    assert_eq!(policy.id(), PolicyId::Evidence);
    assert_eq!(policy.name(), "Evidence");
    assert_eq!(policy.severity(), Severity::High);
}

#[test]
fn test_evidence_policy_enforcement_basic() {
    let config = EvidenceConfig::default();
    let policy = EvidencePolicy::new(config);

    let ctx = TestPolicyContext {
        metadata: HashMap::new(),
    };

    let result = policy.enforce(&ctx);
    assert!(result.is_ok());

    let audit = result.unwrap();
    assert!(audit.passed);
}

#[test]
fn test_evidence_spans_minimum_requirement() {
    let mut config = EvidenceConfig::default();
    config.min_spans = 3;

    let policy = EvidencePolicy::new(config);

    // Create valid evidence spans
    let spans = vec![
        create_valid_evidence_span("doc1", 1),
        create_valid_evidence_span("doc2", 1),
        create_valid_evidence_span("doc3", 1),
    ];

    // Should pass with exactly minimum spans
    assert!(policy.validate_evidence_spans(&spans).is_ok());

    // Should fail with fewer spans
    assert!(policy.validate_evidence_spans(&spans[0..2]).is_err());
}

#[test]
fn test_evidence_quality_thresholds() {
    let mut config = EvidenceConfig::default();
    config.quality_thresholds = QualityThresholds {
        min_relevance: 0.8,
        min_confidence: 0.9,
        min_recency_days: 0,
        max_age_days: 365,
    };

    let policy = EvidencePolicy::new(config);

    // High quality span - should pass
    let high_quality_span = EvidenceSpan {
        doc_id: "doc1".to_string(),
        rev: 1,
        span_hash: "hash1".to_string(),
        relevance: 0.9,
        confidence: 0.95,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    };

    assert!(policy.validate_evidence_spans(&[high_quality_span]).is_ok());

    // Low relevance - should fail
    let low_relevance_span = EvidenceSpan {
        doc_id: "doc2".to_string(),
        rev: 1,
        span_hash: "hash2".to_string(),
        relevance: 0.5,
        confidence: 0.95,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    };

    assert!(policy
        .validate_evidence_spans(&[low_relevance_span])
        .is_err());

    // Low confidence - should fail
    let low_confidence_span = EvidenceSpan {
        doc_id: "doc3".to_string(),
        rev: 1,
        span_hash: "hash3".to_string(),
        relevance: 0.9,
        confidence: 0.5,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    };

    assert!(policy
        .validate_evidence_spans(&[low_confidence_span])
        .is_err());
}

#[test]
fn test_evidence_type_restrictions() {
    let mut config = EvidenceConfig::default();
    config.allowed_types = vec![EvidenceType::CodeDoc, EvidenceType::ApiDoc];

    let policy = EvidencePolicy::new(config);

    // Allowed type - should pass
    let allowed_span = EvidenceSpan {
        doc_id: "doc1".to_string(),
        rev: 1,
        span_hash: "hash1".to_string(),
        relevance: 0.8,
        confidence: 0.9,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    };

    assert!(policy.validate_evidence_spans(&[allowed_span]).is_ok());

    // Disallowed type - should fail
    let disallowed_span = EvidenceSpan {
        doc_id: "doc2".to_string(),
        rev: 1,
        span_hash: "hash2".to_string(),
        relevance: 0.8,
        confidence: 0.9,
        evidence_type: EvidenceType::SecurityAudit,
        source: create_valid_source(),
        timestamp: Utc::now(),
    };

    assert!(policy.validate_evidence_spans(&[disallowed_span]).is_err());
}

#[test]
fn test_source_signature_requirements() {
    let mut config = EvidenceConfig::default();
    config.source_requirements.require_signatures = true;

    let policy = EvidencePolicy::new(config);

    // With signature - should pass
    let signed_span = EvidenceSpan {
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
            signature: Some("valid_signature".to_string()),
        },
        timestamp: Utc::now(),
    };

    assert!(policy.validate_evidence_spans(&[signed_span]).is_ok());

    // Without signature - should fail
    let unsigned_span = EvidenceSpan {
        doc_id: "doc2".to_string(),
        rev: 1,
        span_hash: "hash2".to_string(),
        relevance: 0.8,
        confidence: 0.9,
        evidence_type: EvidenceType::CodeDoc,
        source: SourceInfo {
            domain: "example.com".to_string(),
            path: "/docs/api.md".to_string(),
            version: "1.0".to_string(),
            signature: None,
        },
        timestamp: Utc::now(),
    };

    assert!(policy.validate_evidence_spans(&[unsigned_span]).is_err());
}

#[test]
fn test_source_domain_restrictions() {
    let mut config = EvidenceConfig::default();
    config.source_requirements.allowed_domains =
        vec!["trusted.com".to_string(), "verified.com".to_string()];
    config.source_requirements.blocked_domains = vec!["blocked.com".to_string()];

    let policy = EvidencePolicy::new(config);

    // Allowed domain - should pass
    let allowed_span = create_evidence_span_with_domain("doc1", "trusted.com");
    assert!(policy.validate_evidence_spans(&[allowed_span]).is_ok());

    // Blocked domain - should fail
    let blocked_span = create_evidence_span_with_domain("doc2", "blocked.com");
    assert!(policy.validate_evidence_spans(&[blocked_span]).is_err());

    // Not in allowed list - should fail
    let unlisted_span = create_evidence_span_with_domain("doc3", "unknown.com");
    assert!(policy.validate_evidence_spans(&[unlisted_span]).is_err());
}

#[test]
fn test_superseded_evidence_warnings() {
    let mut config = EvidenceConfig::default();
    config.warn_on_superseded = true;

    let policy = EvidencePolicy::new(config);

    // Multiple revisions of same document
    let spans = vec![
        EvidenceSpan {
            doc_id: "doc1".to_string(),
            rev: 1,
            span_hash: "hash1".to_string(),
            relevance: 0.8,
            confidence: 0.9,
            evidence_type: EvidenceType::CodeDoc,
            source: create_valid_source(),
            timestamp: Utc::now(),
        },
        EvidenceSpan {
            doc_id: "doc1".to_string(),
            rev: 3,
            span_hash: "hash3".to_string(),
            relevance: 0.8,
            confidence: 0.9,
            evidence_type: EvidenceType::CodeDoc,
            source: create_valid_source(),
            timestamp: Utc::now(),
        },
        EvidenceSpan {
            doc_id: "doc1".to_string(),
            rev: 2,
            span_hash: "hash2".to_string(),
            relevance: 0.8,
            confidence: 0.9,
            evidence_type: EvidenceType::CodeDoc,
            source: create_valid_source(),
            timestamp: Utc::now(),
        },
    ];

    let warnings = policy.check_superseded_evidence(&spans).unwrap();
    assert!(!warnings.is_empty());
    assert!(warnings[0].contains("doc1"));
    assert!(warnings[0].contains("superseded"));
}

#[test]
fn test_open_book_grounding_requirement() {
    let mut config = EvidenceConfig::default();
    config.require_open_book = true;

    let policy = EvidencePolicy::new(config);

    // With evidence - should pass
    assert!(policy.validate_retrieval_requirements(true).is_ok());

    // Without evidence - should fail
    assert!(policy.validate_retrieval_requirements(false).is_err());

    // Disable open-book requirement
    let mut config_optional = EvidenceConfig::default();
    config_optional.require_open_book = false;
    let policy_optional = EvidencePolicy::new(config_optional);

    // Without evidence but not required - should pass
    assert!(policy_optional
        .validate_retrieval_requirements(false)
        .is_ok());
}

#[test]
fn test_evidence_quality_score_calculation() {
    let config = EvidenceConfig::default();
    let policy = EvidencePolicy::new(config);

    // Empty spans
    assert_eq!(policy.calculate_quality_score(&[]), 0.0);

    // Single span
    let single_span = vec![EvidenceSpan {
        doc_id: "doc1".to_string(),
        rev: 1,
        span_hash: "hash1".to_string(),
        relevance: 0.8,
        confidence: 0.9,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    }];

    let single_score = policy.calculate_quality_score(&single_span);
    assert!((single_score - 0.85).abs() < 0.01); // (0.8 + 0.9) / 2 = 0.85

    // Multiple spans
    let multiple_spans = vec![
        EvidenceSpan {
            doc_id: "doc1".to_string(),
            rev: 1,
            span_hash: "hash1".to_string(),
            relevance: 0.8,
            confidence: 0.9,
            evidence_type: EvidenceType::CodeDoc,
            source: create_valid_source(),
            timestamp: Utc::now(),
        },
        EvidenceSpan {
            doc_id: "doc2".to_string(),
            rev: 1,
            span_hash: "hash2".to_string(),
            relevance: 0.7,
            confidence: 0.8,
            evidence_type: EvidenceType::ApiDoc,
            source: create_valid_source(),
            timestamp: Utc::now(),
        },
    ];

    let multiple_score = policy.calculate_quality_score(&multiple_spans);
    // ((0.8+0.9)/2 + (0.7+0.8)/2) / 2 = (0.85 + 0.75) / 2 = 0.8
    assert!((multiple_score - 0.8).abs() < 0.01);
}

#[test]
fn test_comprehensive_policy_validation() {
    // Create strict policy configuration
    let config = EvidenceConfig {
        require_open_book: true,
        min_spans: 2,
        prefer_latest_revision: true,
        warn_on_superseded: true,
        quality_thresholds: QualityThresholds {
            min_relevance: 0.75,
            min_confidence: 0.85,
            min_recency_days: 0,
            max_age_days: 365,
        },
        allowed_types: vec![
            EvidenceType::CodeDoc,
            EvidenceType::ApiDoc,
            EvidenceType::TestCase,
        ],
        source_requirements: SourceRequirements {
            require_signatures: true,
            require_timestamps: true,
            require_versioning: true,
            allowed_domains: vec![],
            blocked_domains: vec![],
        },
    };

    let policy = EvidencePolicy::new(config);

    // Valid evidence set
    let valid_spans = vec![
        EvidenceSpan {
            doc_id: "doc1".to_string(),
            rev: 1,
            span_hash: "hash1".to_string(),
            relevance: 0.9,
            confidence: 0.95,
            evidence_type: EvidenceType::CodeDoc,
            source: SourceInfo {
                domain: "example.com".to_string(),
                path: "/docs/api.md".to_string(),
                version: "2.0".to_string(),
                signature: Some("sig1".to_string()),
            },
            timestamp: Utc::now(),
        },
        EvidenceSpan {
            doc_id: "doc2".to_string(),
            rev: 1,
            span_hash: "hash2".to_string(),
            relevance: 0.85,
            confidence: 0.9,
            evidence_type: EvidenceType::ApiDoc,
            source: SourceInfo {
                domain: "example.com".to_string(),
                path: "/docs/api2.md".to_string(),
                version: "2.0".to_string(),
                signature: Some("sig2".to_string()),
            },
            timestamp: Utc::now(),
        },
    ];

    // Should pass all validations
    assert!(policy.validate_evidence_spans(&valid_spans).is_ok());
    assert!(policy.validate_retrieval_requirements(true).is_ok());
    let quality_score = policy.calculate_quality_score(&valid_spans);
    assert!(quality_score > 0.8);
}

#[test]
fn test_multiple_evidence_types() {
    let config = EvidenceConfig::default();
    let policy = EvidencePolicy::new(config);

    let mixed_evidence = vec![
        create_evidence_span_with_type("doc1", EvidenceType::CodeDoc),
        create_evidence_span_with_type("doc2", EvidenceType::ApiDoc),
        create_evidence_span_with_type("doc3", EvidenceType::TestCase),
        create_evidence_span_with_type("doc4", EvidenceType::Config),
    ];

    assert!(policy.validate_evidence_spans(&mixed_evidence).is_ok());
}

#[test]
fn test_edge_case_empty_spans() {
    let config = EvidenceConfig::default();
    let policy = EvidencePolicy::new(config);

    // Empty spans should fail minimum requirement
    let result = policy.validate_evidence_spans(&[]);
    assert!(result.is_err());
}

#[test]
fn test_edge_case_boundary_quality_scores() {
    let mut config = EvidenceConfig::default();
    config.quality_thresholds = QualityThresholds {
        min_relevance: 0.7,
        min_confidence: 0.8,
        min_recency_days: 0,
        max_age_days: 365,
    };

    let policy = EvidencePolicy::new(config);

    // Exactly at threshold - should pass
    let boundary_span = EvidenceSpan {
        doc_id: "doc1".to_string(),
        rev: 1,
        span_hash: "hash1".to_string(),
        relevance: 0.7,
        confidence: 0.8,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    };

    assert!(policy.validate_evidence_spans(&[boundary_span]).is_ok());

    // Just below threshold - should fail
    let below_threshold_span = EvidenceSpan {
        doc_id: "doc2".to_string(),
        rev: 1,
        span_hash: "hash2".to_string(),
        relevance: 0.69,
        confidence: 0.79,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    };

    assert!(policy
        .validate_evidence_spans(&[below_threshold_span])
        .is_err());
}

// Helper functions

fn create_valid_evidence_span(doc_id: &str, rev: u32) -> EvidenceSpan {
    EvidenceSpan {
        doc_id: doc_id.to_string(),
        rev,
        span_hash: format!("hash_{}", doc_id),
        relevance: 0.8,
        confidence: 0.9,
        evidence_type: EvidenceType::CodeDoc,
        source: create_valid_source(),
        timestamp: Utc::now(),
    }
}

fn create_valid_source() -> SourceInfo {
    SourceInfo {
        domain: "example.com".to_string(),
        path: "/docs/api.md".to_string(),
        version: "1.0".to_string(),
        signature: Some("valid_signature".to_string()),
    }
}

fn create_evidence_span_with_domain(doc_id: &str, domain: &str) -> EvidenceSpan {
    EvidenceSpan {
        doc_id: doc_id.to_string(),
        rev: 1,
        span_hash: format!("hash_{}", doc_id),
        relevance: 0.8,
        confidence: 0.9,
        evidence_type: EvidenceType::CodeDoc,
        source: SourceInfo {
            domain: domain.to_string(),
            path: "/docs/api.md".to_string(),
            version: "1.0".to_string(),
            signature: Some("sig".to_string()),
        },
        timestamp: Utc::now(),
    }
}

fn create_evidence_span_with_type(doc_id: &str, evidence_type: EvidenceType) -> EvidenceSpan {
    EvidenceSpan {
        doc_id: doc_id.to_string(),
        rev: 1,
        span_hash: format!("hash_{}", doc_id),
        relevance: 0.8,
        confidence: 0.9,
        evidence_type,
        source: create_valid_source(),
        timestamp: Utc::now(),
    }
}
