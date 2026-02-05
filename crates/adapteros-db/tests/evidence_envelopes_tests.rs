//! Database integration tests for evidence_envelopes module
//!
//! Tests the full lifecycle of evidence envelope storage, chain validation,
//! retrieval, and verification including:
//! - Chain-linked envelope storage with validation
//! - Chain tail retrieval
//! - Chain divergence detection
//! - Cross-tenant isolation
//! - Query filtering (by tenant, scope, sequence)
//! - Evidence chain verification
//! - CRUD operations (get, delete, count)
//! - JSON serialization roundtrip
#![allow(unused_imports)]

use adapteros_core::evidence_envelope::{
    BundleMetadataRef, EvidenceEnvelope, InferenceReceiptRef, PolicyAuditRef,
};
use adapteros_core::evidence_verifier::{
    evidence_chain_divergence, is_evidence_chain_divergence, EvidenceVerifier,
};
use adapteros_core::{AosError, B3Hash, EvidenceScope};
use adapteros_db::{Db, EvidenceEnvelopeFilter};

/// Helper: Create test tenant in database
async fn insert_test_tenant(db: &Db, tenant_id: &str) {
    adapteros_db::sqlx::query(
        r#"
        INSERT INTO tenants (id, name, created_at)
        VALUES (?, ?, datetime('now'))
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(tenant_id)
    .bind(format!("Tenant {}", tenant_id))
    .execute(db.pool())
    .await
    .unwrap();
}

/// Helper: Create sample telemetry bundle reference
fn sample_bundle_ref(sequence: u64) -> BundleMetadataRef {
    BundleMetadataRef {
        bundle_hash: B3Hash::hash(format!("bundle-{}", sequence).as_bytes()),
        merkle_root: B3Hash::hash(format!("merkle-{}", sequence).as_bytes()),
        event_count: 100 + sequence as u32,
        cpid: Some(format!("cp-{:03}", sequence)),
        sequence_no: Some(sequence),
    }
}

/// Helper: Create sample policy audit reference
fn sample_policy_ref(sequence: i64) -> PolicyAuditRef {
    PolicyAuditRef {
        decision_id: format!("dec-{:03}", sequence),
        entry_hash: B3Hash::hash(format!("policy-entry-{}", sequence).as_bytes()),
        chain_sequence: sequence,
        policy_pack_id: "pack-egress".to_string(),
        hook: "OnBeforeInference".to_string(),
        decision: "allow".to_string(),
    }
}

/// Helper: Create sample inference receipt reference
fn sample_inference_ref(sequence: u32) -> InferenceReceiptRef {
    InferenceReceiptRef {
        trace_id: format!("trace-{:03}", sequence),
        run_head_hash: B3Hash::hash(format!("run-head-{}", sequence).as_bytes()),
        output_digest: B3Hash::hash(format!("output-{}", sequence).as_bytes()),
        receipt_digest: B3Hash::hash(format!("receipt-{}", sequence).as_bytes()),
        logical_prompt_tokens: 100 + sequence,
        prefix_cached_token_count: 20,
        billed_input_tokens: 80 + sequence,
        logical_output_tokens: 50 + sequence,
        billed_output_tokens: 50 + sequence,
        stop_reason_code: Some("end_turn".to_string()),
        stop_reason_token_index: Some(49 + sequence),
        stop_policy_digest_b3: Some(B3Hash::hash(b"stop-policy")),
        model_cache_identity_v2_digest_b3: Some(B3Hash::hash(b"model-cache-id")),
        backend_used: "mock".to_string(),
        backend_attestation_b3: Some(B3Hash::hash(b"mock-attestation")),
        ..Default::default()
    }
}

// ============================================================================
// Test 1: Store evidence envelope with chain validation
// ============================================================================

#[tokio::test]
async fn test_store_evidence_envelope_with_chain_validation() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Store first envelope (no previous_root)
    let env1 = EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(1), None);
    let id1 = db.store_evidence_envelope(&env1).await.unwrap();
    assert!(!id1.is_empty());

    // Store second envelope (links to first)
    let env2 = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(2),
        Some(env1.root),
    );
    let id2 = db.store_evidence_envelope(&env2).await.unwrap();
    assert!(!id2.is_empty());
    assert_ne!(id1, id2);

    // Store third envelope (links to second)
    let env3 = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(3),
        Some(env2.root),
    );
    let id3 = db.store_evidence_envelope(&env3).await.unwrap();
    assert!(!id3.is_empty());

    // Verify chain sequence numbers are assigned correctly
    let retrieved_env1 = db.get_evidence_envelope(&id1).await.unwrap().unwrap();
    let retrieved_env2 = db.get_evidence_envelope(&id2).await.unwrap().unwrap();
    let retrieved_env3 = db.get_evidence_envelope(&id3).await.unwrap().unwrap();

    assert_eq!(retrieved_env1.tenant_id, "tenant-1");
    assert_eq!(retrieved_env2.previous_root, Some(env1.root));
    assert_eq!(retrieved_env3.previous_root, Some(env2.root));
}

// ============================================================================
// Test 2: Evidence chain tail retrieval
// ============================================================================

#[tokio::test]
async fn test_get_evidence_chain_tail() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // No chain yet - should return None
    let tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await
        .unwrap();
    assert!(tail.is_none());

    // Store first envelope
    let env1 = EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(1), None);
    db.store_evidence_envelope(&env1).await.unwrap();

    // Tail should be the first envelope with sequence 1
    let tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await
        .unwrap();
    assert!(tail.is_some());
    let (root, seq) = tail.unwrap();
    assert_eq!(root, env1.root);
    assert_eq!(seq, 1);

    // Store second envelope
    let env2 = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(2),
        Some(env1.root),
    );
    db.store_evidence_envelope(&env2).await.unwrap();

    // Tail should now be the second envelope with sequence 2
    let tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await
        .unwrap();
    assert!(tail.is_some());
    let (root, seq) = tail.unwrap();
    assert_eq!(root, env2.root);
    assert_eq!(seq, 2);
}

// ============================================================================
// Test 3: Chain divergence detection on mismatch
// ============================================================================

#[tokio::test]
async fn test_chain_divergence_detection_on_mismatch() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Store first envelope
    let env1 = EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(1), None);
    db.store_evidence_envelope(&env1).await.unwrap();

    // Try to store second envelope with WRONG previous_root
    let wrong_root = B3Hash::hash(b"wrong-previous-root");
    let env2 = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(2),
        Some(wrong_root),
    );

    let err = db.store_evidence_envelope(&env2).await.unwrap_err();
    assert!(is_evidence_chain_divergence(&err));

    match err {
        AosError::Validation(msg) => {
            assert!(msg.contains("EVIDENCE_CHAIN_DIVERGED"));
            assert!(msg.contains("previous_root mismatch"));
        }
        _ => panic!("Expected validation error for chain divergence"),
    }
}

#[tokio::test]
async fn test_chain_divergence_first_envelope_with_previous() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Try to store first envelope with a previous_root (invalid)
    let wrong_root = B3Hash::hash(b"should-not-exist");
    let env = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(1),
        Some(wrong_root),
    );

    let err = db.store_evidence_envelope(&env).await.unwrap_err();
    assert!(is_evidence_chain_divergence(&err));

    match err {
        AosError::Validation(msg) => {
            assert!(msg.contains("EVIDENCE_CHAIN_DIVERGED"));
            assert!(msg.contains("unexpected previous_root"));
        }
        _ => panic!("Expected validation error for chain divergence"),
    }
}

#[tokio::test]
async fn test_chain_divergence_missing_previous() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Store first envelope
    let env1 = EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(1), None);
    db.store_evidence_envelope(&env1).await.unwrap();

    // Try to store second envelope without previous_root (invalid)
    let env2 = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(2),
        None, // Should have previous_root
    );

    let err = db.store_evidence_envelope(&env2).await.unwrap_err();
    assert!(is_evidence_chain_divergence(&err));

    match err {
        AosError::Validation(msg) => {
            assert!(msg.contains("EVIDENCE_CHAIN_DIVERGED"));
            assert!(msg.contains("expected previous_root"));
        }
        _ => panic!("Expected validation error for chain divergence"),
    }
}

// ============================================================================
// Test 4: Evidence envelope query filtering
// ============================================================================

#[tokio::test]
async fn test_query_evidence_envelopes_by_tenant() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;
    insert_test_tenant(&db, "tenant-2").await;

    // Store envelopes for tenant-1
    let env1 = EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(1), None);
    db.store_evidence_envelope(&env1).await.unwrap();

    let env2 = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(2),
        Some(env1.root),
    );
    db.store_evidence_envelope(&env2).await.unwrap();

    // Store envelope for tenant-2
    let env3 = EvidenceEnvelope::new_telemetry("tenant-2".to_string(), sample_bundle_ref(1), None);
    db.store_evidence_envelope(&env3).await.unwrap();

    // Query tenant-1 only
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|e| e.tenant_id == "tenant-1"));

    // Query tenant-2 only
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-2".to_string()),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tenant_id, "tenant-2");
}

#[tokio::test]
async fn test_query_evidence_envelopes_by_scope() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Store telemetry envelopes
    let telem1 =
        EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(1), None);
    db.store_evidence_envelope(&telem1).await.unwrap();

    let telem2 = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        sample_bundle_ref(2),
        Some(telem1.root),
    );
    db.store_evidence_envelope(&telem2).await.unwrap();

    // Store policy envelope
    let policy1 = EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(1), None);
    db.store_evidence_envelope(&policy1).await.unwrap();

    // Store inference envelope
    let infer1 =
        EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(1), None);
    db.store_evidence_envelope(&infer1).await.unwrap();

    // Query telemetry only
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        scope: Some(EvidenceScope::Telemetry),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|e| e.scope == EvidenceScope::Telemetry));

    // Query policy only
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        scope: Some(EvidenceScope::Policy),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].scope, EvidenceScope::Policy);

    // Query inference only
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        scope: Some(EvidenceScope::Inference),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].scope, EvidenceScope::Inference);
}

#[tokio::test]
async fn test_query_evidence_envelopes_by_sequence_range() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Store 5 envelopes in chain
    let mut prev_root = None;
    for i in 1..=5 {
        let env = EvidenceEnvelope::new_telemetry(
            "tenant-1".to_string(),
            sample_bundle_ref(i),
            prev_root,
        );
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root = Some(env.root);
    }

    // Query sequence 2-4 (inclusive)
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        from_sequence: Some(2),
        to_sequence: Some(4),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 3);

    // Query from sequence 3 onwards
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        from_sequence: Some(3),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 3);

    // Query up to sequence 2
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        to_sequence: Some(2),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_query_evidence_envelopes_with_limit_offset() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Store 10 envelopes
    let mut prev_root = None;
    for i in 1..=10 {
        let env = EvidenceEnvelope::new_telemetry(
            "tenant-1".to_string(),
            sample_bundle_ref(i),
            prev_root,
        );
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root = Some(env.root);
    }

    // Query with limit
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        limit: Some(3),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 3);

    // Query with offset
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        offset: Some(5),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 5);

    // Query with limit and offset (pagination)
    let filter = EvidenceEnvelopeFilter {
        tenant_id: Some("tenant-1".to_string()),
        limit: Some(3),
        offset: Some(3),
        ..Default::default()
    };
    let results = db.query_evidence_envelopes(filter).await.unwrap();
    assert_eq!(results.len(), 3);
}

// ============================================================================
// Test 5: Cross-tenant isolation for envelopes
// ============================================================================

#[tokio::test]
async fn test_cross_tenant_isolation_for_envelopes() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;
    insert_test_tenant(&db, "tenant-2").await;

    // Create independent chains for each tenant with same scope
    let env1_t1 = EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(1), None);
    db.store_evidence_envelope(&env1_t1).await.unwrap();

    let env1_t2 = EvidenceEnvelope::new_policy("tenant-2".to_string(), sample_policy_ref(10), None);
    db.store_evidence_envelope(&env1_t2).await.unwrap();

    // Each tenant should have independent chain tails
    let tail_t1 = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Policy)
        .await
        .unwrap()
        .unwrap();
    let tail_t2 = db
        .get_evidence_chain_tail("tenant-2", EvidenceScope::Policy)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(tail_t1.0, env1_t1.root);
    assert_eq!(tail_t2.0, env1_t2.root);
    assert_ne!(tail_t1.0, tail_t2.0);

    // Add second envelope to each chain
    let env2_t1 = EvidenceEnvelope::new_policy(
        "tenant-1".to_string(),
        sample_policy_ref(2),
        Some(env1_t1.root),
    );
    db.store_evidence_envelope(&env2_t1).await.unwrap();

    let env2_t2 = EvidenceEnvelope::new_policy(
        "tenant-2".to_string(),
        sample_policy_ref(11),
        Some(env1_t2.root),
    );
    db.store_evidence_envelope(&env2_t2).await.unwrap();

    // Verify each tenant's query returns only their envelopes
    let results_t1 = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-1".to_string()),
            scope: Some(EvidenceScope::Policy),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(results_t1.len(), 2);
    assert!(results_t1.iter().all(|e| e.tenant_id == "tenant-1"));

    let results_t2 = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-2".to_string()),
            scope: Some(EvidenceScope::Policy),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(results_t2.len(), 2);
    assert!(results_t2.iter().all(|e| e.tenant_id == "tenant-2"));

    // Verify counts are isolated
    let count_t1 = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Policy))
        .await
        .unwrap();
    let count_t2 = db
        .count_evidence_envelopes("tenant-2", Some(EvidenceScope::Policy))
        .await
        .unwrap();
    assert_eq!(count_t1, 2);
    assert_eq!(count_t2, 2);
}

// ============================================================================
// Test 6: Evidence chain verification
// ============================================================================

#[tokio::test]
async fn test_verify_evidence_chain_valid() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Build a valid chain of 5 inference envelopes
    let mut prev_root = None;
    for i in 1..=5 {
        let env = EvidenceEnvelope::new_inference(
            "tenant-1".to_string(),
            sample_inference_ref(i as u32),
            prev_root,
        );
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root = Some(env.root);
    }

    // Verify the chain
    let result = db
        .verify_evidence_chain("tenant-1", EvidenceScope::Inference)
        .await
        .unwrap();

    assert!(result.is_valid);
    assert_eq!(result.envelopes_checked, 5);
    assert!(!result.divergence_detected);
    assert!(result.first_invalid_index.is_none());
    assert!(result.error_message.is_none());
}

#[tokio::test]
async fn test_verify_evidence_chain_empty() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Verify empty chain
    let result = db
        .verify_evidence_chain("tenant-1", EvidenceScope::Telemetry)
        .await
        .unwrap();

    assert!(result.is_valid);
    assert_eq!(result.envelopes_checked, 0);
    assert!(!result.divergence_detected);
}

#[tokio::test]
async fn test_verify_evidence_chain_detects_corruption() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Build a chain of 3 envelopes
    let mut prev_root = None;
    for i in 1..=3 {
        let env = EvidenceEnvelope::new_telemetry(
            "tenant-1".to_string(),
            sample_bundle_ref(i),
            prev_root,
        );
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root = Some(env.root);
    }

    // Corrupt the second envelope's root in the database
    adapteros_db::sqlx::query(
        "UPDATE evidence_envelopes SET root = ? WHERE tenant_id = ? AND chain_sequence = ?",
    )
    .bind(B3Hash::hash(b"corrupted-root").to_hex())
    .bind("tenant-1")
    .bind(2)
    .execute(db.pool())
    .await
    .unwrap();

    // Verification should detect the corruption
    let result = db
        .verify_evidence_chain("tenant-1", EvidenceScope::Telemetry)
        .await
        .unwrap();

    assert!(!result.is_valid);
    assert!(result.first_invalid_index.is_some());
    // The corruption is detected when verifying the envelope at index 1 (second envelope)
    // because its root doesn't match the computed root
    assert!(result.error_message.is_some());
}

// ============================================================================
// Test 7: Get evidence envelope by ID
// ============================================================================

#[tokio::test]
async fn test_get_evidence_envelope_by_id() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Store envelope
    let original = EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(1), None);
    let id = db.store_evidence_envelope(&original).await.unwrap();

    // Retrieve by ID
    let retrieved = db.get_evidence_envelope(&id).await.unwrap();
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.tenant_id, original.tenant_id);
    assert_eq!(retrieved.scope, original.scope);
    assert_eq!(retrieved.root, original.root);
    assert_eq!(retrieved.previous_root, original.previous_root);

    // Non-existent ID should return None
    let not_found = db.get_evidence_envelope("non-existent-id").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_get_evidence_envelope_preserves_all_scopes() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Test telemetry envelope
    let telem = EvidenceEnvelope::new_telemetry("tenant-1".to_string(), sample_bundle_ref(1), None);
    let telem_id = db.store_evidence_envelope(&telem).await.unwrap();
    let retrieved_telem = db.get_evidence_envelope(&telem_id).await.unwrap().unwrap();
    assert_eq!(retrieved_telem.scope, EvidenceScope::Telemetry);
    assert!(retrieved_telem.bundle_metadata_ref.is_some());
    assert!(retrieved_telem.policy_audit_ref.is_none());
    assert!(retrieved_telem.inference_receipt_ref.is_none());

    // Test policy envelope
    let policy = EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(1), None);
    let policy_id = db.store_evidence_envelope(&policy).await.unwrap();
    let retrieved_policy = db.get_evidence_envelope(&policy_id).await.unwrap().unwrap();
    assert_eq!(retrieved_policy.scope, EvidenceScope::Policy);
    assert!(retrieved_policy.bundle_metadata_ref.is_none());
    assert!(retrieved_policy.policy_audit_ref.is_some());
    assert!(retrieved_policy.inference_receipt_ref.is_none());

    // Test inference envelope
    let infer =
        EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(1), None);
    let infer_id = db.store_evidence_envelope(&infer).await.unwrap();
    let retrieved_infer = db.get_evidence_envelope(&infer_id).await.unwrap().unwrap();
    assert_eq!(retrieved_infer.scope, EvidenceScope::Inference);
    assert!(retrieved_infer.bundle_metadata_ref.is_none());
    assert!(retrieved_infer.policy_audit_ref.is_none());
    assert!(retrieved_infer.inference_receipt_ref.is_some());
}

// ============================================================================
// Test 8: Delete tenant evidence envelopes
// ============================================================================

#[tokio::test]
async fn test_delete_tenant_evidence_envelopes() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;
    insert_test_tenant(&db, "tenant-2").await;

    // Store envelopes for both tenants
    let mut prev_root_t1 = None;
    for i in 1..=3 {
        let env = EvidenceEnvelope::new_telemetry(
            "tenant-1".to_string(),
            sample_bundle_ref(i),
            prev_root_t1,
        );
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root_t1 = Some(env.root);
    }

    let mut prev_root_t2 = None;
    for i in 1..=2 {
        let env = EvidenceEnvelope::new_telemetry(
            "tenant-2".to_string(),
            sample_bundle_ref(i),
            prev_root_t2,
        );
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root_t2 = Some(env.root);
    }

    // Verify initial counts
    let count_t1_before = db.count_evidence_envelopes("tenant-1", None).await.unwrap();
    let count_t2_before = db.count_evidence_envelopes("tenant-2", None).await.unwrap();
    assert_eq!(count_t1_before, 3);
    assert_eq!(count_t2_before, 2);

    // Delete tenant-1 envelopes
    let deleted = db
        .delete_tenant_evidence_envelopes("tenant-1")
        .await
        .unwrap();
    assert_eq!(deleted, 3);

    // Verify tenant-1 has no envelopes
    let count_t1_after = db.count_evidence_envelopes("tenant-1", None).await.unwrap();
    assert_eq!(count_t1_after, 0);

    // Verify tenant-2 still has envelopes
    let count_t2_after = db.count_evidence_envelopes("tenant-2", None).await.unwrap();
    assert_eq!(count_t2_after, 2);

    // Delete non-existent tenant should return 0
    let deleted = db
        .delete_tenant_evidence_envelopes("non-existent")
        .await
        .unwrap();
    assert_eq!(deleted, 0);
}

// ============================================================================
// Test 9: Count evidence envelopes
// ============================================================================

#[tokio::test]
async fn test_count_evidence_envelopes() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Initially no envelopes
    let count = db.count_evidence_envelopes("tenant-1", None).await.unwrap();
    assert_eq!(count, 0);

    // Add 3 telemetry envelopes
    let mut prev_root = None;
    for i in 1..=3 {
        let env = EvidenceEnvelope::new_telemetry(
            "tenant-1".to_string(),
            sample_bundle_ref(i),
            prev_root,
        );
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root = Some(env.root);
    }

    // Add 2 policy envelopes
    let mut prev_root = None;
    for i in 1..=2 {
        let env =
            EvidenceEnvelope::new_policy("tenant-1".to_string(), sample_policy_ref(i), prev_root);
        db.store_evidence_envelope(&env).await.unwrap();
        prev_root = Some(env.root);
    }

    // Count all envelopes
    let total_count = db.count_evidence_envelopes("tenant-1", None).await.unwrap();
    assert_eq!(total_count, 5);

    // Count telemetry only
    let telem_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Telemetry))
        .await
        .unwrap();
    assert_eq!(telem_count, 3);

    // Count policy only
    let policy_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Policy))
        .await
        .unwrap();
    assert_eq!(policy_count, 2);

    // Count inference (should be 0)
    let infer_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Inference))
        .await
        .unwrap();
    assert_eq!(infer_count, 0);
}

// ============================================================================
// Test 10: Envelope serialization JSON roundtrip
// ============================================================================

#[tokio::test]
async fn test_envelope_serialization_json_roundtrip() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Seed the chain so we can exercise previous_root linkage
    let seed =
        EvidenceEnvelope::new_inference("tenant-1".to_string(), sample_inference_ref(41), None);
    db.store_evidence_envelope(&seed).await.unwrap();

    // Create an inference envelope with all fields populated
    let original = EvidenceEnvelope::new_inference(
        "tenant-1".to_string(),
        sample_inference_ref(42),
        Some(seed.root),
    );

    // Store and retrieve
    let id = db.store_evidence_envelope(&original).await.unwrap();
    let retrieved = db.get_evidence_envelope(&id).await.unwrap().unwrap();

    // Verify all fields match
    assert_eq!(retrieved.schema_version, original.schema_version);
    assert_eq!(retrieved.tenant_id, original.tenant_id);
    assert_eq!(retrieved.scope, original.scope);
    assert_eq!(retrieved.previous_root, original.previous_root);
    assert_eq!(retrieved.root, original.root);
    assert_eq!(retrieved.created_at, original.created_at);

    // Verify payload data is preserved
    let original_ref = original.inference_receipt_ref.as_ref().unwrap();
    let retrieved_ref = retrieved.inference_receipt_ref.as_ref().unwrap();

    assert_eq!(retrieved_ref.trace_id, original_ref.trace_id);
    assert_eq!(retrieved_ref.run_head_hash, original_ref.run_head_hash);
    assert_eq!(retrieved_ref.output_digest, original_ref.output_digest);
    assert_eq!(retrieved_ref.receipt_digest, original_ref.receipt_digest);
    assert_eq!(
        retrieved_ref.logical_prompt_tokens,
        original_ref.logical_prompt_tokens
    );
    assert_eq!(
        retrieved_ref.prefix_cached_token_count,
        original_ref.prefix_cached_token_count
    );
    assert_eq!(
        retrieved_ref.billed_input_tokens,
        original_ref.billed_input_tokens
    );
    assert_eq!(
        retrieved_ref.logical_output_tokens,
        original_ref.logical_output_tokens
    );
    assert_eq!(
        retrieved_ref.billed_output_tokens,
        original_ref.billed_output_tokens
    );
    assert_eq!(
        retrieved_ref.stop_reason_code,
        original_ref.stop_reason_code
    );
    assert_eq!(
        retrieved_ref.stop_reason_token_index,
        original_ref.stop_reason_token_index
    );
    assert_eq!(
        retrieved_ref.stop_policy_digest_b3,
        original_ref.stop_policy_digest_b3
    );
    assert_eq!(
        retrieved_ref.model_cache_identity_v2_digest_b3,
        original_ref.model_cache_identity_v2_digest_b3
    );
}

#[tokio::test]
async fn test_envelope_json_roundtrip_all_scopes() {
    let db = Db::new_in_memory().await.unwrap();
    insert_test_tenant(&db, "tenant-1").await;

    // Test telemetry roundtrip
    let telem_original = EvidenceEnvelope::new_telemetry(
        "tenant-1".to_string(),
        BundleMetadataRef {
            bundle_hash: B3Hash::hash(b"test-bundle"),
            merkle_root: B3Hash::hash(b"test-merkle"),
            event_count: 999,
            cpid: Some("test-cpid".to_string()),
            sequence_no: Some(123),
        },
        None,
    );
    let telem_id = db.store_evidence_envelope(&telem_original).await.unwrap();
    let telem_retrieved = db.get_evidence_envelope(&telem_id).await.unwrap().unwrap();

    let telem_orig_ref = telem_original.bundle_metadata_ref.as_ref().unwrap();
    let telem_retr_ref = telem_retrieved.bundle_metadata_ref.as_ref().unwrap();
    assert_eq!(telem_retr_ref.bundle_hash, telem_orig_ref.bundle_hash);
    assert_eq!(telem_retr_ref.merkle_root, telem_orig_ref.merkle_root);
    assert_eq!(telem_retr_ref.event_count, telem_orig_ref.event_count);
    assert_eq!(telem_retr_ref.cpid, telem_orig_ref.cpid);
    assert_eq!(telem_retr_ref.sequence_no, telem_orig_ref.sequence_no);

    // Test policy roundtrip
    let policy_original = EvidenceEnvelope::new_policy(
        "tenant-1".to_string(),
        PolicyAuditRef {
            decision_id: "test-decision".to_string(),
            entry_hash: B3Hash::hash(b"test-entry"),
            chain_sequence: 456,
            policy_pack_id: "test-pack".to_string(),
            hook: "TestHook".to_string(),
            decision: "deny".to_string(),
        },
        None,
    );
    let policy_id = db.store_evidence_envelope(&policy_original).await.unwrap();
    let policy_retrieved = db.get_evidence_envelope(&policy_id).await.unwrap().unwrap();

    let policy_orig_ref = policy_original.policy_audit_ref.as_ref().unwrap();
    let policy_retr_ref = policy_retrieved.policy_audit_ref.as_ref().unwrap();
    assert_eq!(policy_retr_ref.decision_id, policy_orig_ref.decision_id);
    assert_eq!(policy_retr_ref.entry_hash, policy_orig_ref.entry_hash);
    assert_eq!(
        policy_retr_ref.chain_sequence,
        policy_orig_ref.chain_sequence
    );
    assert_eq!(
        policy_retr_ref.policy_pack_id,
        policy_orig_ref.policy_pack_id
    );
    assert_eq!(policy_retr_ref.hook, policy_orig_ref.hook);
    assert_eq!(policy_retr_ref.decision, policy_orig_ref.decision);
}
