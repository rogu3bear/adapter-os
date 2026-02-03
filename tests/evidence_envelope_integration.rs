//! End-to-end integration tests for evidence envelope flow during inference
//!
//! Tests cover:
//! 1. Full inference → receipt → envelope creation flow
//! 2. Envelope storage during actual inference request
//! 3. Chain linking across multiple inference requests
//! 4. Receipt digest includes evidence envelope reference
//! 5. Audit chain state verification after inference
//! 6. Telemetry envelope creation during request lifecycle
//! 7. Policy envelope creation when policies are evaluated
//! 8. Envelope canonical bytes are deterministic

use adapteros_api_types::inference::PolicyOverrideFlags;
use adapteros_core::evidence_envelope::{
    BundleMetadataRef, EvidenceEnvelope, InferenceReceiptRef, PolicyAuditRef,
};
use adapteros_core::{B3Hash, EvidenceScope, EvidenceVerifier};
use adapteros_db::{
    Db, EvidenceEnvelopeFilter, SqlTraceSink, TraceFinalization, TraceSink, TraceStart,
    TraceTokenInput,
};
use std::sync::Arc;

// =============================================================================
// Test Utilities
// =============================================================================

/// Initialize in-memory database with required schema
async fn init_test_db() -> anyhow::Result<Arc<Db>> {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Arc::new(Db::new_in_memory().await?);

    // Create test tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-test', 'Test Tenant')")
        .execute(db.pool())
        .await?;

    Ok(db)
}

/// Create a test inference trace with tokens
async fn create_test_trace(
    db: &Arc<Db>,
    trace_id: &str,
    tenant_id: &str,
    token_count: usize,
) -> anyhow::Result<adapteros_db::TraceReceipt> {
    let context_digest = B3Hash::hash(format!("context-{}", trace_id).as_bytes()).to_bytes();

    let start = TraceStart {
        trace_id: trace_id.to_string(),
        tenant_id: tenant_id.to_string(),
        request_id: Some(format!("req-{}", trace_id)),
        context_digest,
        stack_id: None,
        model_id: None,
        policy_id: None,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 32).await?;

    // Record tokens
    for i in 0..token_count {
        let token = TraceTokenInput {
            token_index: i as u32,
            adapter_ids: vec![format!("adapter-{}", i % 3)],
            gates_q15: vec![(100 + i * 10) as i16],
            policy_mask_digest_b3: Some(B3Hash::hash(b"policy-mask").to_bytes()),
            allowed_mask: Some(vec![true]),
            policy_overrides_applied: Some(PolicyOverrideFlags {
                allow_list: true,
                deny_list: false,
                trust_state: false,
            }),
            backend_id: Some("coreml".to_string()),
            kernel_version_id: Some("v1".to_string()),
        };
        sink.record_token(token).await?;
    }

    // Finalize with output tokens
    let output_tokens: Vec<u32> = (0..token_count).map(|i| 1000 + i as u32).collect();
    let receipt = sink
        .finalize(TraceFinalization {
            output_tokens: &output_tokens,
            logical_prompt_tokens: 10,
            prefix_cached_token_count: 0,
            billed_input_tokens: 10,
            logical_output_tokens: output_tokens.len() as u32,
            billed_output_tokens: output_tokens.len() as u32,
            stop_reason_code: Some("end_turn".to_string()),
            stop_reason_token_index: Some((token_count - 1) as u32),
            stop_policy_digest_b3: Some(B3Hash::hash(b"stop-policy")),
            tenant_kv_quota_bytes: 0,
            tenant_kv_bytes_used: 0,
            kv_evictions: 0,
            kv_residency_policy_id: None,
            kv_quota_enforced: false,
            prefix_kv_key_b3: None,
            prefix_cache_hit: false,
            prefix_kv_bytes: 0,
            model_cache_identity_v2_digest_b3: Some(B3Hash::hash(b"model-id")),
            attestation: None,
            equipment_profile: None,
            // Phase 3: Crypto Receipt Dual-Write
            crypto_receipt_digest_b3: None,
            receipt_parity_verified: None,
            tenant_id: None,
            // P0-1: Cache attestation (not needed when prefix_cached_token_count = 0)
            cache_attestation: None,
            worker_public_key: None,
            // UMA telemetry (PRD §5.5)
            copy_bytes: None,
        })
        .await?;

    Ok(receipt)
}

/// Convert TraceReceipt to InferenceReceiptRef for envelope creation
fn trace_receipt_to_ref(
    trace_id: String,
    receipt: &adapteros_db::TraceReceipt,
) -> InferenceReceiptRef {
    InferenceReceiptRef {
        trace_id,
        run_head_hash: receipt.run_head_hash,
        output_digest: receipt.output_digest,
        receipt_digest: receipt.receipt_digest,
        logical_prompt_tokens: receipt.logical_prompt_tokens,
        prefix_cached_token_count: receipt.prefix_cached_token_count,
        billed_input_tokens: receipt.billed_input_tokens,
        logical_output_tokens: receipt.logical_output_tokens,
        billed_output_tokens: receipt.billed_output_tokens,
        stop_reason_code: receipt.stop_reason_code.clone(),
        stop_reason_token_index: receipt.stop_reason_token_index,
        stop_policy_digest_b3: receipt.stop_policy_digest_b3,
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: receipt.prefix_cache_hit,
        prefix_kv_bytes: receipt.prefix_kv_bytes,
        model_cache_identity_v2_digest_b3: receipt.model_cache_identity_v2_digest_b3,
        // PRD-DET-001: Backend identity fields default to empty when converting from legacy TraceReceipt
        backend_used: String::new(),
        backend_attestation_b3: None,
        seed_lineage_hash: None,
        adapter_training_lineage_digest: None,
        // V6 cross-run lineage: not available in TraceReceipt, use defaults
        previous_receipt_digest: None,
        session_sequence: 0,
    }
}

// =============================================================================
// Test 1: Full inference → receipt → envelope creation flow
// =============================================================================

#[tokio::test]
async fn test_inference_to_envelope_flow() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create inference trace
    let receipt = create_test_trace(&db, "trace-001", "tenant-test", 5).await?;

    // Convert to envelope reference
    let receipt_ref = trace_receipt_to_ref("trace-001".to_string(), &receipt);

    // Create envelope (first in chain, no previous_root)
    let envelope =
        EvidenceEnvelope::new_inference("tenant-test".to_string(), receipt_ref.clone(), None);

    // Verify envelope structure
    assert_eq!(envelope.scope, EvidenceScope::Inference);
    assert_eq!(envelope.tenant_id, "tenant-test");
    assert!(envelope.previous_root.is_none());
    assert_eq!(envelope.root, receipt.receipt_digest);

    // Validate envelope
    envelope.validate()?;

    // Store envelope
    let envelope_id = db.store_evidence_envelope(&envelope).await?;
    assert!(!envelope_id.is_empty());

    // Retrieve and verify
    let stored = db.get_evidence_envelope(&envelope_id).await?;
    assert!(stored.is_some());
    let stored_envelope = stored.unwrap();

    assert_eq!(stored_envelope.tenant_id, "tenant-test");
    assert_eq!(stored_envelope.root, receipt.receipt_digest);
    assert_eq!(
        stored_envelope
            .inference_receipt_ref
            .as_ref()
            .unwrap()
            .trace_id,
        "trace-001"
    );

    Ok(())
}

// =============================================================================
// Test 2: Envelope storage during actual inference request
// =============================================================================

#[tokio::test]
async fn test_envelope_storage_during_inference() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Simulate inference flow: trace → receipt → envelope
    let receipt = create_test_trace(&db, "trace-inf-001", "tenant-test", 3).await?;

    // Create and store envelope as would happen in inference_core
    let receipt_ref = trace_receipt_to_ref("trace-inf-001".to_string(), &receipt);
    let envelope = EvidenceEnvelope::new_inference("tenant-test".to_string(), receipt_ref, None);

    let _envelope_id = db.store_evidence_envelope(&envelope).await?;

    // Verify envelope was stored with correct sequence
    let envelopes = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-test".to_string()),
            scope: Some(EvidenceScope::Inference),
            ..Default::default()
        })
        .await?;

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].root, receipt.receipt_digest);

    Ok(())
}

// =============================================================================
// Test 3: Chain linking across multiple inference requests
// =============================================================================

#[tokio::test]
async fn test_envelope_chain_linking() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create first inference envelope
    let receipt1 = create_test_trace(&db, "trace-chain-1", "tenant-test", 3).await?;
    let ref1 = trace_receipt_to_ref("trace-chain-1".to_string(), &receipt1);
    let envelope1 = EvidenceEnvelope::new_inference("tenant-test".to_string(), ref1, None);

    db.store_evidence_envelope(&envelope1).await?;

    // Create second inference envelope linked to first
    let receipt2 = create_test_trace(&db, "trace-chain-2", "tenant-test", 4).await?;
    let ref2 = trace_receipt_to_ref("trace-chain-2".to_string(), &receipt2);
    let envelope2 = EvidenceEnvelope::new_inference(
        "tenant-test".to_string(),
        ref2,
        Some(envelope1.root), // Link to previous
    );

    db.store_evidence_envelope(&envelope2).await?;

    // Create third envelope linked to second
    let receipt3 = create_test_trace(&db, "trace-chain-3", "tenant-test", 2).await?;
    let ref3 = trace_receipt_to_ref("trace-chain-3".to_string(), &receipt3);
    let envelope3 = EvidenceEnvelope::new_inference(
        "tenant-test".to_string(),
        ref3,
        Some(envelope2.root), // Link to previous
    );

    db.store_evidence_envelope(&envelope3).await?;

    // Verify chain
    let chain = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-test".to_string()),
            scope: Some(EvidenceScope::Inference),
            ..Default::default()
        })
        .await?;

    assert_eq!(chain.len(), 3);

    // Verify linkage
    assert!(chain[0].previous_root.is_none()); // First in chain
    assert_eq!(chain[1].previous_root, Some(chain[0].root));
    assert_eq!(chain[2].previous_root, Some(chain[1].root));

    Ok(())
}

// =============================================================================
// Test 4: Receipt digest includes evidence envelope reference
// =============================================================================

#[tokio::test]
async fn test_receipt_digest_includes_envelope_ref() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    let receipt = create_test_trace(&db, "trace-digest", "tenant-test", 3).await?;

    // Create envelope from receipt
    let receipt_ref = trace_receipt_to_ref("trace-digest".to_string(), &receipt);
    let envelope =
        EvidenceEnvelope::new_inference("tenant-test".to_string(), receipt_ref.clone(), None);

    // The envelope's root should match the receipt digest
    // This binds the envelope to the exact receipt
    assert_eq!(envelope.root, receipt.receipt_digest);

    // Verify receipt_ref contains all necessary fields
    assert_eq!(receipt_ref.trace_id, "trace-digest");
    assert_eq!(receipt_ref.run_head_hash, receipt.run_head_hash);
    assert_eq!(receipt_ref.output_digest, receipt.output_digest);
    assert_eq!(receipt_ref.receipt_digest, receipt.receipt_digest);

    // Verify token accounting fields are included
    assert_eq!(receipt_ref.logical_prompt_tokens, 10);
    assert_eq!(receipt_ref.prefix_cached_token_count, 0); // No cache credits in this test
    assert_eq!(receipt_ref.billed_input_tokens, 10); // All tokens billed (no cache)

    // Verify stop controller fields are included
    assert_eq!(receipt_ref.stop_reason_code, Some("end_turn".to_string()));
    assert!(receipt_ref.stop_policy_digest_b3.is_some());

    // Verify model identity is included
    assert!(receipt_ref.model_cache_identity_v2_digest_b3.is_some());

    Ok(())
}

// =============================================================================
// Test 5: Audit chain state verification after inference
// =============================================================================

#[tokio::test]
async fn test_audit_chain_verification() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create a chain of 5 inference envelopes
    let mut prev_root: Option<B3Hash> = None;
    for i in 0..5 {
        let trace_id = format!("trace-audit-{}", i);
        let receipt = create_test_trace(&db, &trace_id, "tenant-test", 2).await?;
        let receipt_ref = trace_receipt_to_ref(trace_id.clone(), &receipt);
        let envelope =
            EvidenceEnvelope::new_inference("tenant-test".to_string(), receipt_ref, prev_root);

        db.store_evidence_envelope(&envelope).await?;
        prev_root = Some(envelope.root);
    }

    // Verify chain integrity
    let result = db
        .verify_evidence_chain("tenant-test", EvidenceScope::Inference)
        .await?;

    assert!(result.is_valid, "Chain should be valid");
    assert_eq!(result.envelopes_checked, 5);
    assert!(!result.divergence_detected);
    assert!(result.first_invalid_index.is_none());

    // Get chain tail
    let tail = db
        .get_evidence_chain_tail("tenant-test", EvidenceScope::Inference)
        .await?;
    assert!(tail.is_some());
    let (tail_root, tail_seq) = tail.unwrap();
    assert_eq!(tail_seq, 5); // 5th envelope in sequence
    assert_eq!(tail_root, prev_root.unwrap());

    Ok(())
}

// =============================================================================
// Test 6: Chain divergence detection
// =============================================================================

#[tokio::test]
async fn test_chain_divergence_detection() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create valid chain start
    let receipt1 = create_test_trace(&db, "trace-div-1", "tenant-test", 2).await?;
    let ref1 = trace_receipt_to_ref("trace-div-1".to_string(), &receipt1);
    let envelope1 = EvidenceEnvelope::new_inference("tenant-test".to_string(), ref1, None);
    db.store_evidence_envelope(&envelope1).await?;

    // Try to create envelope with wrong previous_root
    let receipt2 = create_test_trace(&db, "trace-div-2", "tenant-test", 2).await?;
    let ref2 = trace_receipt_to_ref("trace-div-2".to_string(), &receipt2);
    let wrong_prev = B3Hash::hash(b"wrong-previous-root");
    let envelope2 =
        EvidenceEnvelope::new_inference("tenant-test".to_string(), ref2, Some(wrong_prev));

    // Should fail with divergence error
    let result = db.store_evidence_envelope(&envelope2).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("EVIDENCE_CHAIN_DIVERGED"),
        "Error should indicate chain divergence: {}",
        err_msg
    );

    Ok(())
}

// =============================================================================
// Test 7: Telemetry envelope creation during request lifecycle
// =============================================================================

#[tokio::test]
async fn test_telemetry_envelope_creation() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create telemetry bundle reference
    let bundle_ref = BundleMetadataRef {
        bundle_hash: B3Hash::hash(b"telemetry-bundle-001"),
        merkle_root: B3Hash::hash(b"telemetry-merkle-001"),
        event_count: 100,
        cpid: Some("cp-001".to_string()),
        sequence_no: Some(1),
    };

    // Create telemetry envelope
    let envelope = EvidenceEnvelope::new_telemetry("tenant-test".to_string(), bundle_ref, None);

    assert_eq!(envelope.scope, EvidenceScope::Telemetry);
    assert!(envelope.bundle_metadata_ref.is_some());
    assert!(envelope.policy_audit_ref.is_none());
    assert!(envelope.inference_receipt_ref.is_none());

    // Store envelope
    let envelope_id = db.store_evidence_envelope(&envelope).await?;
    assert!(!envelope_id.is_empty());

    // Verify stored
    let stored = db.get_evidence_envelope(&envelope_id).await?;
    assert!(stored.is_some());

    let stored_envelope = stored.unwrap();
    assert_eq!(stored_envelope.scope, EvidenceScope::Telemetry);
    assert_eq!(
        stored_envelope
            .bundle_metadata_ref
            .as_ref()
            .unwrap()
            .event_count,
        100
    );

    Ok(())
}

// =============================================================================
// Test 8: Policy envelope creation when policies are evaluated
// =============================================================================

#[tokio::test]
async fn test_policy_envelope_creation() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create policy audit reference
    let policy_ref = PolicyAuditRef {
        decision_id: "dec-001".to_string(),
        entry_hash: B3Hash::hash(b"policy-decision-001"),
        chain_sequence: 1,
        policy_pack_id: "pack-egress".to_string(),
        hook: "OnBeforeInference".to_string(),
        decision: "allow".to_string(),
    };

    // Create policy envelope
    let envelope = EvidenceEnvelope::new_policy("tenant-test".to_string(), policy_ref, None);

    assert_eq!(envelope.scope, EvidenceScope::Policy);
    assert!(envelope.policy_audit_ref.is_some());
    assert!(envelope.bundle_metadata_ref.is_none());
    assert!(envelope.inference_receipt_ref.is_none());

    // Store envelope
    let envelope_id = db.store_evidence_envelope(&envelope).await?;
    assert!(!envelope_id.is_empty());

    // Verify stored
    let stored = db.get_evidence_envelope(&envelope_id).await?;
    assert!(stored.is_some());

    let stored_envelope = stored.unwrap();
    assert_eq!(stored_envelope.scope, EvidenceScope::Policy);
    assert_eq!(
        stored_envelope.policy_audit_ref.as_ref().unwrap().hook,
        "OnBeforeInference"
    );

    Ok(())
}

// =============================================================================
// Test 9: Mixed chain with different evidence types
// =============================================================================

#[tokio::test]
async fn test_mixed_evidence_chain() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Each scope has its own independent chain
    // Create inference chain
    let receipt1 = create_test_trace(&db, "trace-mixed-1", "tenant-test", 2).await?;
    let ref1 = trace_receipt_to_ref("trace-mixed-1".to_string(), &receipt1);
    let inf_env1 = EvidenceEnvelope::new_inference("tenant-test".to_string(), ref1, None);
    db.store_evidence_envelope(&inf_env1).await?;

    // Create policy chain (separate from inference)
    let policy_ref1 = PolicyAuditRef {
        decision_id: "dec-mixed-1".to_string(),
        entry_hash: B3Hash::hash(b"policy-mixed-1"),
        chain_sequence: 1,
        policy_pack_id: "pack-egress".to_string(),
        hook: "OnBeforeInference".to_string(),
        decision: "allow".to_string(),
    };
    let policy_env1 = EvidenceEnvelope::new_policy("tenant-test".to_string(), policy_ref1, None);
    db.store_evidence_envelope(&policy_env1).await?;

    // Create telemetry chain (separate from both)
    let bundle_ref1 = BundleMetadataRef {
        bundle_hash: B3Hash::hash(b"bundle-mixed-1"),
        merkle_root: B3Hash::hash(b"merkle-mixed-1"),
        event_count: 50,
        cpid: Some("cp-001".to_string()),
        sequence_no: Some(1),
    };
    let telem_env1 = EvidenceEnvelope::new_telemetry("tenant-test".to_string(), bundle_ref1, None);
    db.store_evidence_envelope(&telem_env1).await?;

    // Add second to each chain
    let receipt2 = create_test_trace(&db, "trace-mixed-2", "tenant-test", 2).await?;
    let ref2 = trace_receipt_to_ref("trace-mixed-2".to_string(), &receipt2);
    let inf_env2 =
        EvidenceEnvelope::new_inference("tenant-test".to_string(), ref2, Some(inf_env1.root));
    db.store_evidence_envelope(&inf_env2).await?;

    let policy_ref2 = PolicyAuditRef {
        decision_id: "dec-mixed-2".to_string(),
        entry_hash: B3Hash::hash(b"policy-mixed-2"),
        chain_sequence: 2,
        policy_pack_id: "pack-egress".to_string(),
        hook: "OnAfterInference".to_string(),
        decision: "allow".to_string(),
    };
    let policy_env2 = EvidenceEnvelope::new_policy(
        "tenant-test".to_string(),
        policy_ref2,
        Some(policy_env1.root),
    );
    db.store_evidence_envelope(&policy_env2).await?;

    // Verify each chain independently
    let inf_result = db
        .verify_evidence_chain("tenant-test", EvidenceScope::Inference)
        .await?;
    assert!(inf_result.is_valid);
    assert_eq!(inf_result.envelopes_checked, 2);

    let policy_result = db
        .verify_evidence_chain("tenant-test", EvidenceScope::Policy)
        .await?;
    assert!(policy_result.is_valid);
    assert_eq!(policy_result.envelopes_checked, 2);

    let telem_result = db
        .verify_evidence_chain("tenant-test", EvidenceScope::Telemetry)
        .await?;
    assert!(telem_result.is_valid);
    assert_eq!(telem_result.envelopes_checked, 1);

    Ok(())
}

// =============================================================================
// Test 10: Verify envelope canonical bytes are deterministic
// =============================================================================

#[tokio::test]
async fn test_canonical_bytes_determinism() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create a single receipt and use it to create two envelopes with the same data
    let receipt = create_test_trace(&db, "trace-det-1", "tenant-test", 3).await?;

    // Create envelopes from same receipt data
    let ref1 = trace_receipt_to_ref("trace-det-1".to_string(), &receipt);
    let env1 = EvidenceEnvelope::new_inference("tenant-test".to_string(), ref1.clone(), None);

    // Create second envelope with identical receipt reference
    let mut env2 = EvidenceEnvelope::new_inference("tenant-test".to_string(), ref1, None);

    // Normalize timestamps for determinism check
    env2.created_at = env1.created_at.clone();
    env2.signed_at_us = env1.signed_at_us;

    // Canonical bytes should be identical for identical data
    let bytes1 = env1.to_canonical_bytes();
    let bytes2 = env2.to_canonical_bytes();

    assert_eq!(bytes1, bytes2, "Canonical bytes should be deterministic");

    // Digests should match
    assert_eq!(env1.digest(), env2.digest());

    // Changing tenant_id should change digest
    env2.tenant_id = "different-tenant".to_string();
    let bytes3 = env2.to_canonical_bytes();
    assert_ne!(bytes1, bytes3);
    assert_ne!(env1.digest(), env2.digest());

    Ok(())
}

// =============================================================================
// Test 11: Envelope verification with EvidenceVerifier
// =============================================================================

#[tokio::test]
async fn test_envelope_verification() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create and store a chain of envelopes
    let receipt1 = create_test_trace(&db, "trace-verify-1", "tenant-test", 2).await?;
    let ref1 = trace_receipt_to_ref("trace-verify-1".to_string(), &receipt1);
    let env1 = EvidenceEnvelope::new_inference("tenant-test".to_string(), ref1, None);
    db.store_evidence_envelope(&env1).await?;

    let receipt2 = create_test_trace(&db, "trace-verify-2", "tenant-test", 2).await?;
    let ref2 = trace_receipt_to_ref("trace-verify-2".to_string(), &receipt2);
    let env2 = EvidenceEnvelope::new_inference("tenant-test".to_string(), ref2, Some(env1.root));
    db.store_evidence_envelope(&env2).await?;

    // Verify using EvidenceVerifier
    let verifier = EvidenceVerifier::new();

    // Verify first envelope
    let result1 = verifier.verify_envelope(&env1, None)?;
    assert!(result1.is_valid);
    assert!(result1.schema_version_ok);
    assert!(result1.root_matches);
    assert!(result1.chain_link_valid);
    assert!(result1.payload_valid);

    // Verify second envelope with chain link
    let result2 = verifier.verify_envelope(&env2, Some(&env1.root))?;
    assert!(result2.is_valid);
    assert!(result2.chain_link_valid);

    // Verify chain
    let chain_result = verifier.verify_chain(&[env1, env2])?;
    assert!(chain_result.is_valid);
    assert_eq!(chain_result.envelopes_checked, 2);
    assert!(!chain_result.divergence_detected);

    Ok(())
}

// =============================================================================
// Test 12: Envelope count and query filters
// =============================================================================

#[tokio::test]
async fn test_envelope_query_filters() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create multiple envelopes of different types
    for i in 0..3 {
        let receipt =
            create_test_trace(&db, &format!("trace-query-{}", i), "tenant-test", 2).await?;
        let receipt_ref = trace_receipt_to_ref(format!("trace-query-{}", i), &receipt);

        // Get actual previous root from chain tail
        let prev = if i == 0 {
            None
        } else {
            db.get_evidence_chain_tail("tenant-test", EvidenceScope::Inference)
                .await?
                .map(|(root, _)| root)
        };

        let env = EvidenceEnvelope::new_inference("tenant-test".to_string(), receipt_ref, prev);
        db.store_evidence_envelope(&env).await?;
    }

    // Query all inference envelopes
    let all_inf = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-test".to_string()),
            scope: Some(EvidenceScope::Inference),
            ..Default::default()
        })
        .await?;
    assert_eq!(all_inf.len(), 3);

    // Query with limit
    let limited = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-test".to_string()),
            scope: Some(EvidenceScope::Inference),
            limit: Some(2),
            ..Default::default()
        })
        .await?;
    assert_eq!(limited.len(), 2);

    // Count envelopes
    let count = db
        .count_evidence_envelopes("tenant-test", Some(EvidenceScope::Inference))
        .await?;
    assert_eq!(count, 3);

    Ok(())
}
