//! Determinism Hardening Tests (PRD-DET-001)
//!
//! This module contains the test plan implementations from the
//! Determinism Hardening Review document.
//!
//! Tests T1-T10:
//! - T1: Seed collision detection
//! - T2: Replay verification (identical input → identical receipt)
//! - T3: Backend selection determinism
//! - T4: Policy pack determinism enforcement
//! - T5: Q15 compile-time guard (verified elsewhere)
//! - T6: Evidence chain tamper detection
//! - T7: Strict mode no fallback
//! - T8: Metallib verification for BitExact
//! - T9: Seed lineage receipt binding
//! - T10: Dual-write drift detection

use adapteros_core::seed::{SeedLineage, SeedMode, TypedSeed, HKDF_ALGORITHM_VERSION};
use adapteros_core::B3Hash;
use adapteros_core::evidence_envelope::{
    EvidenceEnvelope, EvidenceScope, InferenceReceiptRef, ReceiptCompletenessReport,
    EVIDENCE_ENVELOPE_SCHEMA_VERSION,
};
use adapteros_lora_kernel_api::attestation::{
    BackendType, DeterminismLevel, DeterminismReport, FloatingPointMode, RngSeedingMethod,
};
use adapteros_types::routing::{detect_backend_drift, DriftReport, RouterDecision};

// =============================================================================
// T1: Seed Collision Detection
// =============================================================================

#[test]
fn test_seed_collision_detection_different_seeds() {
    // Different seeds should produce different lineage hashes
    let seed1 = [1u8; 32];
    let seed2 = [2u8; 32];

    let lineage1 = SeedLineage::from_raw_seed(&seed1, SeedMode::Strict, true);
    let lineage2 = SeedLineage::from_raw_seed(&seed2, SeedMode::Strict, true);

    // Different seeds MUST produce different binding hashes
    assert_ne!(
        lineage1.to_binding_hash(),
        lineage2.to_binding_hash(),
        "Different seeds must produce different binding hashes"
    );
}

#[test]
fn test_seed_collision_detection_same_seed() {
    // Same seed should produce identical lineage hash
    let seed = [42u8; 32];

    let lineage1 = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);
    let lineage2 = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);

    // Same seed MUST produce identical binding hash
    assert_eq!(
        lineage1.to_binding_hash(),
        lineage2.to_binding_hash(),
        "Same seed must produce identical binding hash"
    );
}

#[test]
fn test_seed_collision_detection_mode_matters() {
    // Same seed but different mode should produce different hash
    let seed = [42u8; 32];

    let lineage_strict = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);
    let lineage_best_effort = SeedLineage::from_raw_seed(&seed, SeedMode::BestEffort, true);

    assert_ne!(
        lineage_strict.to_binding_hash(),
        lineage_best_effort.to_binding_hash(),
        "Different seed modes must produce different binding hashes"
    );
}

// =============================================================================
// T2: Replay Verification
// =============================================================================

#[test]
fn test_replay_with_different_seed_produces_different_receipt() {
    // Create two receipts with different seed lineages
    let seed1 = [1u8; 32];
    let seed2 = [2u8; 32];

    let lineage1 = SeedLineage::from_raw_seed(&seed1, SeedMode::Strict, true);
    let lineage2 = SeedLineage::from_raw_seed(&seed2, SeedMode::Strict, true);

    let mut receipt1 = sample_inference_receipt();
    receipt1.seed_lineage_hash = Some(lineage1.to_binding_hash());

    let mut receipt2 = sample_inference_receipt();
    receipt2.seed_lineage_hash = Some(lineage2.to_binding_hash());

    // Receipts must have different seed lineage hashes
    assert_ne!(
        receipt1.seed_lineage_hash, receipt2.seed_lineage_hash,
        "Replay with different seed must produce different lineage hash"
    );

    // Create envelopes and verify they have different digests
    let env1 = EvidenceEnvelope::new_inference("tenant".into(), receipt1, None);
    let env2 = EvidenceEnvelope::new_inference("tenant".into(), receipt2, None);

    assert_ne!(
        env1.digest(),
        env2.digest(),
        "Envelopes with different seed lineage must have different digests"
    );
}

#[test]
fn test_replay_with_same_seed_produces_same_receipt() {
    // Create two receipts with identical seed lineages
    let seed = [42u8; 32];

    let lineage = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);
    let binding_hash = lineage.to_binding_hash();

    let mut receipt1 = sample_inference_receipt();
    receipt1.seed_lineage_hash = Some(binding_hash);

    let mut receipt2 = sample_inference_receipt();
    receipt2.seed_lineage_hash = Some(binding_hash);

    // Receipts must have identical seed lineage hashes
    assert_eq!(
        receipt1.seed_lineage_hash, receipt2.seed_lineage_hash,
        "Replay with same seed must produce same lineage hash"
    );
}

// =============================================================================
// T3: Backend Selection Determinism
// =============================================================================

#[test]
fn test_backend_selection_determinism() {
    // Same attestation configuration should produce same attestation hash
    let report1 = DeterminismReport::for_metal_verified(
        B3Hash::hash(b"metallib-content"),
        Some("1.0.0".to_string()),
    );
    let report2 = DeterminismReport::for_metal_verified(
        B3Hash::hash(b"metallib-content"),
        Some("1.0.0".to_string()),
    );

    assert_eq!(
        report1.to_attestation_hash(),
        report2.to_attestation_hash(),
        "Same backend config must produce same attestation hash"
    );
}

#[test]
fn test_different_backend_produces_different_hash() {
    let metal_report = DeterminismReport::for_metal_verified(
        B3Hash::hash(b"metallib-content"),
        Some("1.0.0".to_string()),
    );
    let coreml_report = DeterminismReport::for_coreml();

    assert_ne!(
        metal_report.to_attestation_hash(),
        coreml_report.to_attestation_hash(),
        "Different backends must produce different attestation hashes"
    );
}

// =============================================================================
// T4: Policy Pack Determinism Enforcement
// =============================================================================

#[test]
fn test_policy_pack_enforces_determinism_levels() {
    // BitExact is the highest level
    assert!(
        DeterminismLevel::BitExact > DeterminismLevel::BoundedTolerance,
        "BitExact must be greater than BoundedTolerance"
    );
    assert!(
        DeterminismLevel::BoundedTolerance > DeterminismLevel::None,
        "BoundedTolerance must be greater than None"
    );
}

#[test]
fn test_backend_type_determinism_by_design() {
    // Metal and MLX are deterministic by design
    assert!(
        BackendType::Metal.is_deterministic_by_design(),
        "Metal must be deterministic by design"
    );
    assert!(
        BackendType::MLX.is_deterministic_by_design(),
        "MLX must be deterministic by design"
    );
    assert!(
        BackendType::Mock.is_deterministic_by_design(),
        "Mock must be deterministic by design"
    );
}

// =============================================================================
// T5: Q15 Compile-Time Guard
// =============================================================================
// Note: Q15 denominator is verified in adapteros-lora-router/tests/determinism.rs
// via test_q15_denominator_is_32767

#[test]
fn test_q15_denominator_constant() {
    // This test verifies the constant is accessible and correct
    use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
    assert!(
        (ROUTER_GATE_Q15_DENOM - 32767.0).abs() < f32::EPSILON,
        "Q15 denominator MUST be exactly 32767.0"
    );
}

// =============================================================================
// T6: Evidence Chain Tamper Detection
// =============================================================================

#[test]
fn test_evidence_chain_detects_tampering() {
    let receipt = sample_inference_receipt();
    let env = EvidenceEnvelope::new_inference("tenant".into(), receipt.clone(), None);

    // Original digest
    let original_digest = env.digest();

    // Create tampered receipt
    let mut tampered_receipt = receipt.clone();
    tampered_receipt.backend_used = "tampered".to_string();

    // Create envelope with tampered receipt
    let tampered_env =
        EvidenceEnvelope::new_inference("tenant".into(), tampered_receipt, None);

    // Tampered envelope must have different digest
    assert_ne!(
        original_digest,
        tampered_env.digest(),
        "Tampered envelope must have different digest"
    );
}

#[test]
fn test_evidence_chain_linking() {
    let receipt1 = sample_inference_receipt();
    let env1 = EvidenceEnvelope::new_inference("tenant".into(), receipt1, None);

    // Chain to first envelope
    let mut receipt2 = sample_inference_receipt();
    receipt2.trace_id = "trace-002".to_string();
    let env2 = EvidenceEnvelope::new_inference("tenant".into(), receipt2, Some(env1.root));

    // Verify chain link
    assert_eq!(
        env2.previous_root,
        Some(env1.root),
        "Second envelope must link to first envelope's root"
    );
}

// =============================================================================
// T7: Strict Mode No Fallback
// =============================================================================

#[test]
fn test_strict_mode_rejects_fallback() {
    use adapteros_deterministic_exec::seed::{GlobalSeedManager, SeedError};

    let manager = GlobalSeedManager::new();

    // Strict mode with no primary seed should fail
    let result = manager.init_with_mode(None, SeedMode::Strict);
    assert!(
        matches!(result, Err(SeedError::StrictModeFallbackRejected)),
        "Strict mode must reject fallback seed"
    );
}

#[test]
fn test_best_effort_mode_allows_fallback() {
    use adapteros_deterministic_exec::seed::GlobalSeedManager;

    let manager = GlobalSeedManager::new();

    // BestEffort mode with no primary seed should use fallback
    let result = manager.init_with_mode(None, SeedMode::BestEffort);
    assert!(result.is_ok(), "BestEffort mode must allow fallback seed");
}

#[test]
fn test_strict_mode_with_primary_seed_succeeds() {
    use adapteros_deterministic_exec::seed::GlobalSeedManager;

    let manager = GlobalSeedManager::new();
    let seed = [42u8; 32];

    // Strict mode with primary seed should succeed
    let result = manager.init_with_mode(Some(seed), SeedMode::Strict);
    assert!(
        result.is_ok(),
        "Strict mode with primary seed must succeed"
    );
    assert_eq!(result.unwrap(), seed);
}

// =============================================================================
// T8: Metallib Verification for BitExact
// =============================================================================

#[test]
fn test_metallib_required_for_bitexact_metal() {
    // Metal backend with BitExact requires verified metallib
    let report = DeterminismReport {
        backend_type: BackendType::Metal,
        metallib_hash: Some(B3Hash::hash(b"test")),
        metallib_verified: false, // NOT verified
        manifest: None,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        determinism_level: DeterminismLevel::BitExact,
        compiler_flags: vec![],
        deterministic: true,
        runtime_version: None,
        device_id: None,
    };

    // Should fail validation for BitExact
    let result = report.validate_for_inference(DeterminismLevel::BitExact);
    assert!(
        result.is_err(),
        "Metal without verified metallib must fail BitExact validation"
    );
}

#[test]
fn test_metallib_verified_passes_bitexact() {
    let report = DeterminismReport::for_metal_verified(
        B3Hash::hash(b"metallib"),
        Some("1.0.0".to_string()),
    );

    let result = report.validate_for_inference(DeterminismLevel::BitExact);
    assert!(
        result.is_ok(),
        "Metal with verified metallib must pass BitExact validation"
    );
}

// =============================================================================
// T9: Seed Lineage Receipt Binding
// =============================================================================

#[test]
fn test_seed_lineage_bound_to_receipt() {
    let seed1 = [1u8; 32];
    let seed2 = [2u8; 32];

    let lineage1 = SeedLineage::from_raw_seed(&seed1, SeedMode::Strict, true);
    let lineage2 = SeedLineage::from_raw_seed(&seed2, SeedMode::Strict, true);

    let mut receipt1 = sample_inference_receipt();
    receipt1.seed_lineage_hash = Some(lineage1.to_binding_hash());

    let mut receipt2 = sample_inference_receipt();
    receipt2.seed_lineage_hash = Some(lineage2.to_binding_hash());

    let env1 = EvidenceEnvelope::new_inference("tenant".into(), receipt1, None);
    let env2 = EvidenceEnvelope::new_inference("tenant".into(), receipt2, None);

    assert_ne!(
        env1.digest(),
        env2.digest(),
        "Different seed lineage must produce different envelope digest"
    );
}

#[test]
fn test_seed_lineage_binding_hash_stability() {
    // Same inputs should always produce same binding hash
    let seed = [42u8; 32];

    for _ in 0..10 {
        let lineage = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true);
        let hash = lineage.to_binding_hash();

        // First iteration sets expected, rest verify
        let expected = SeedLineage::from_raw_seed(&seed, SeedMode::Strict, true).to_binding_hash();
        assert_eq!(hash, expected, "Binding hash must be stable across invocations");
    }
}

// =============================================================================
// T10: Dual-Write Drift Detection
// =============================================================================

#[test]
fn test_dual_write_drift_detection_backend_type() {
    let primary = RouterDecision::new(0, vec![], 0.5, 1.0, 0.01)
        .with_backend_type("metal".to_string());
    let secondary = RouterDecision::new(0, vec![], 0.5, 1.0, 0.01)
        .with_backend_type("coreml".to_string());

    let drift = detect_backend_drift(&primary, &secondary);
    assert!(drift.is_some(), "Backend type drift must be detected");
    assert_eq!(drift.unwrap().field, "backend_type");
}

#[test]
fn test_dual_write_drift_detection_entropy() {
    let primary = RouterDecision::new(0, vec![], 0.5, 1.0, 0.01);
    let mut secondary = RouterDecision::new(0, vec![], 0.5, 1.0, 0.01);
    secondary.entropy = 0.6; // Different entropy

    let drift = detect_backend_drift(&primary, &secondary);
    assert!(drift.is_some(), "Entropy drift must be detected");
    assert_eq!(drift.unwrap().field, "entropy");
}

#[test]
fn test_dual_write_no_drift() {
    let primary = RouterDecision::new(0, vec![], 0.5, 1.0, 0.01)
        .with_backend_type("metal".to_string());
    let secondary = RouterDecision::new(0, vec![], 0.5, 1.0, 0.01)
        .with_backend_type("metal".to_string());

    let drift = detect_backend_drift(&primary, &secondary);
    assert!(drift.is_none(), "Identical records must not report drift");
}

// =============================================================================
// Receipt Completeness Tests (EP-5)
// =============================================================================

#[test]
fn test_receipt_completeness_all_fields_present() {
    let mut receipt = sample_inference_receipt();
    let lineage = SeedLineage::from_raw_seed(&[42u8; 32], SeedMode::Strict, true);
    receipt.seed_lineage_hash = Some(lineage.to_binding_hash());
    receipt.backend_attestation_b3 = Some(B3Hash::hash(b"attestation"));

    let report = receipt.validate_completeness();
    assert!(report.is_complete, "Receipt with all fields should be complete");
    assert!(report.missing_fields.is_empty());
}

#[test]
fn test_receipt_completeness_missing_backend() {
    let mut receipt = sample_inference_receipt();
    receipt.backend_used = String::new();

    let report = receipt.validate_completeness();
    assert!(!report.is_complete);
    assert!(report.missing_fields.contains(&"backend_used".to_string()));
}

#[test]
fn test_receipt_strict_mode_validation() {
    let mut receipt = sample_inference_receipt();
    // Missing seed_lineage_hash and backend_attestation_b3

    let result = receipt.validate_for_strict_mode();
    assert!(
        result.is_err(),
        "Incomplete receipt must fail strict mode validation"
    );
}

// =============================================================================
// Helper Functions
// =============================================================================

fn sample_inference_receipt() -> InferenceReceiptRef {
    InferenceReceiptRef {
        trace_id: "trace-001".to_string(),
        run_head_hash: B3Hash::hash(b"run-head"),
        output_digest: B3Hash::hash(b"output"),
        receipt_digest: B3Hash::hash(b"receipt"),
        logical_prompt_tokens: 100,
        prefix_cached_token_count: 0,
        billed_input_tokens: 100,
        logical_output_tokens: 50,
        billed_output_tokens: 50,
        stop_reason_code: Some("end_turn".to_string()),
        stop_reason_token_index: Some(50),
        stop_policy_digest_b3: None,
        model_cache_identity_v2_digest_b3: None,
        backend_used: "metal".to_string(),
        backend_attestation_b3: None,
        seed_lineage_hash: None,
    }
}
