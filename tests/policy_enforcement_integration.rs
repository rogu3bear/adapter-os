//! Integration tests for policy enforcement.
//!
//! This file intentionally avoids external services (DB, supervisor, network)
//! and instead validates enforcement behavior via the `adapteros-policy` crate's
//! `Policy` trait contracts.

use adapteros_policy::packs::{EvidenceConfig, EvidencePolicy, RouterConfig, RouterPolicy};
use adapteros_policy::{DeterminismConfig, DeterminismPolicy, Policy, PolicyContext};
use std::collections::HashMap;

/// Minimal PolicyContext for integration tests (metadata-only).
struct TestContext {
    metadata: HashMap<String, String>,
}

impl TestContext {
    fn new(metadata: HashMap<String, String>) -> Self {
        Self { metadata }
    }
}

impl PolicyContext for TestContext {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}

#[test]
fn test_policy_enforcement_basic() {
    let ctx = TestContext::new(HashMap::new());
    let policy = RouterPolicy::new(RouterConfig::default());
    let audit = policy
        .enforce(&ctx)
        .expect("policy enforcement should not error");
    assert!(audit.passed, "router policy should pass basic enforcement");
}

#[test]
fn test_policy_enforcement_determinism() {
    let mut metadata = HashMap::new();
    metadata.insert(
        "rng_seeding_method".to_string(),
        "system_entropy".to_string(),
    );
    metadata.insert("floating_point_mode".to_string(), "ieee754".to_string());
    metadata.insert("deterministic".to_string(), "true".to_string());
    metadata.insert("backend_type".to_string(), "metal".to_string());

    let ctx = TestContext::new(metadata);
    let policy = DeterminismPolicy::new(DeterminismConfig::default());
    let audit = policy
        .enforce(&ctx)
        .expect("policy enforcement should not error");

    assert!(
        !audit.passed,
        "system_entropy seeding should violate determinism policy"
    );
    assert!(
        audit
            .violations
            .iter()
            .any(|v| v.message.contains("Non-deterministic RNG seeding")),
        "expected RNG seeding violation; got: {:?}",
        audit.violations
    );
}

#[test]
fn test_policy_enforcement_evidence() {
    let mut metadata = HashMap::new();
    metadata.insert("tier".to_string(), "tier_1".to_string());
    metadata.insert("evidence_count".to_string(), "1".to_string());

    // Missing primary_dataset_id should be rejected for T1 adapters.
    let ctx = TestContext::new(metadata);
    let policy = EvidencePolicy::new(EvidenceConfig::default());
    let audit = policy
        .enforce(&ctx)
        .expect("policy enforcement should not error");
    assert!(!audit.passed);
    assert!(
        audit
            .violations
            .iter()
            .any(|v| v.message.contains("missing primary dataset")),
        "expected primary dataset violation; got: {:?}",
        audit.violations
    );
}

#[test]
fn test_policy_enforcement_evidence_passes_with_required_fields() {
    let mut metadata = HashMap::new();
    metadata.insert("tier".to_string(), "tier_1".to_string());
    metadata.insert("primary_dataset_id".to_string(), "dataset-123".to_string());
    metadata.insert("evidence_count".to_string(), "1".to_string());
    metadata.insert("has_evidence".to_string(), "true".to_string());

    let ctx = TestContext::new(metadata);
    let policy = EvidencePolicy::new(EvidenceConfig::default());
    let audit = policy
        .enforce(&ctx)
        .expect("policy enforcement should not error");
    assert!(
        audit.passed,
        "expected evidence policy to pass with required metadata"
    );
}
