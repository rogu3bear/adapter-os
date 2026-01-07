//! Integration tests for evidence envelope storage and verification
//!
//! Tests the complete lifecycle of evidence envelopes including:
//! - Chain initialization and linkage
//! - Multi-scope isolation (telemetry, policy, inference)
//! - Chain verification and divergence detection
//! - Tenant isolation
//! - Query filtering and pagination

use adapteros_core::evidence_envelope::{
    BundleMetadataRef, EvidenceEnvelope, InferenceReceiptRef, PolicyAuditRef,
};
use adapteros_core::{B3Hash, EvidenceScope};
use adapteros_db::{Db, EvidenceEnvelopeFilter};
use std::sync::Arc;

/// Helper to create a test tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind(format!("Test Tenant {}", tenant_id))
        .execute(db.pool())
        .await?;
    Ok(())
}

/// Helper to create a telemetry envelope
fn create_telemetry_envelope(
    tenant_id: String,
    bundle_hash: B3Hash,
    previous_root: Option<B3Hash>,
) -> EvidenceEnvelope {
    let bundle_ref = BundleMetadataRef {
        bundle_hash,
        merkle_root: B3Hash::hash(b"merkle"),
        event_count: 100,
        cpid: Some("cp-001".to_string()),
        sequence_no: Some(1),
    };

    EvidenceEnvelope::new_telemetry(tenant_id, bundle_ref, previous_root)
}

/// Helper to create a policy envelope
fn create_policy_envelope(
    tenant_id: String,
    decision_id: String,
    previous_root: Option<B3Hash>,
) -> EvidenceEnvelope {
    let policy_ref = PolicyAuditRef {
        decision_id,
        entry_hash: B3Hash::hash(b"decision"),
        chain_sequence: 1,
        policy_pack_id: "core".to_string(),
        hook: "OnBeforeInference".to_string(),
        decision: "allow".to_string(),
    };

    EvidenceEnvelope::new_policy(tenant_id, policy_ref, previous_root)
}

/// Helper to create an inference envelope
fn create_inference_envelope(
    tenant_id: String,
    trace_id: String,
    previous_root: Option<B3Hash>,
) -> EvidenceEnvelope {
    let receipt_ref = InferenceReceiptRef {
        trace_id,
        run_head_hash: B3Hash::hash(b"run_head"),
        output_digest: B3Hash::hash(b"output"),
        receipt_digest: B3Hash::hash(b"receipt"),
        logical_prompt_tokens: 50,
        prefix_cached_token_count: 0,
        billed_input_tokens: 50,
        logical_output_tokens: 20,
        billed_output_tokens: 20,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        model_cache_identity_v2_digest_b3: None,
        backend_used: "mock".to_string(),
        backend_attestation_b3: None,
    };

    EvidenceEnvelope::new_inference(tenant_id, receipt_ref, previous_root)
}

#[tokio::test]
async fn test_chain_initialization() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // First envelope in chain should have no previous_root
    let envelope =
        create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);

    let id = db.store_evidence_envelope(&envelope).await?;
    assert!(!id.is_empty());

    // Verify chain tail
    let tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert!(tail.is_some());
    let (root, seq) = tail.unwrap();
    assert_eq!(root, envelope.root);
    assert_eq!(seq, 1);

    Ok(())
}

#[tokio::test]
async fn test_chain_linkage() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create chain of 3 envelopes
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    let env2 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-2"),
        Some(env1.root),
    );
    db.store_evidence_envelope(&env2).await?;

    let env3 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-3"),
        Some(env2.root),
    );
    db.store_evidence_envelope(&env3).await?;

    // Verify chain tail points to the last envelope
    let tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert!(tail.is_some());
    let (root, seq) = tail.unwrap();
    assert_eq!(root, env3.root);
    assert_eq!(seq, 3);

    Ok(())
}

#[tokio::test]
async fn test_chain_divergence_detection() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create first envelope
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    // Try to store an envelope with wrong previous_root
    let env2_bad = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-2"),
        Some(B3Hash::hash(b"wrong-root")),
    );

    let result = db.store_evidence_envelope(&env2_bad).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("EVIDENCE_CHAIN_DIVERGED"));
    assert!(err_msg.contains("previous_root mismatch"));

    // Try to store an envelope claiming to be first when chain exists
    let env2_no_prev =
        create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-2"), None);

    let result = db.store_evidence_envelope(&env2_no_prev).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("EVIDENCE_CHAIN_DIVERGED"));
    assert!(err_msg.contains("expected previous_root"));

    Ok(())
}

#[tokio::test]
async fn test_scope_isolation() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create envelopes for different scopes - each scope has its own chain
    let telemetry_env =
        create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&telemetry_env).await?;

    let policy_env = create_policy_envelope("tenant-1".to_string(), "decision-1".to_string(), None);
    db.store_evidence_envelope(&policy_env).await?;

    let inference_env =
        create_inference_envelope("tenant-1".to_string(), "trace-1".to_string(), None);
    db.store_evidence_envelope(&inference_env).await?;

    // Verify each scope has its own chain
    let telemetry_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert!(telemetry_tail.is_some());
    assert_eq!(telemetry_tail.unwrap().1, 1);

    let policy_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Policy)
        .await?;
    assert!(policy_tail.is_some());
    assert_eq!(policy_tail.unwrap().1, 1);

    let inference_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Inference)
        .await?;
    assert!(inference_tail.is_some());
    assert_eq!(inference_tail.unwrap().1, 1);

    Ok(())
}

#[tokio::test]
async fn test_tenant_isolation() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;
    create_test_tenant(&db, "tenant-2").await?;

    // Create envelopes for two different tenants
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    let env2 = create_telemetry_envelope("tenant-2".to_string(), B3Hash::hash(b"bundle-2"), None);
    db.store_evidence_envelope(&env2).await?;

    // Verify each tenant has its own chain
    let tenant1_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert!(tenant1_tail.is_some());
    assert_eq!(tenant1_tail.unwrap().0, env1.root);

    let tenant2_tail = db
        .get_evidence_chain_tail("tenant-2", EvidenceScope::Telemetry)
        .await?;
    assert!(tenant2_tail.is_some());
    assert_eq!(tenant2_tail.unwrap().0, env2.root);

    // Count should be isolated
    let count1 = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Telemetry))
        .await?;
    assert_eq!(count1, 1);

    let count2 = db
        .count_evidence_envelopes("tenant-2", Some(EvidenceScope::Telemetry))
        .await?;
    assert_eq!(count2, 1);

    Ok(())
}

#[tokio::test]
async fn test_query_filtering() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create a chain of 5 envelopes
    let mut prev_root = None;
    for i in 0..5 {
        let env = create_telemetry_envelope(
            "tenant-1".to_string(),
            B3Hash::hash(format!("bundle-{}", i).as_bytes()),
            prev_root,
        );
        db.store_evidence_envelope(&env).await?;
        prev_root = Some(env.root);
    }

    // Query all envelopes
    let all = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-1".to_string()),
            scope: Some(EvidenceScope::Telemetry),
            ..Default::default()
        })
        .await?;
    assert_eq!(all.len(), 5);

    // Query with sequence range
    let range = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-1".to_string()),
            scope: Some(EvidenceScope::Telemetry),
            from_sequence: Some(2),
            to_sequence: Some(4),
            ..Default::default()
        })
        .await?;
    assert_eq!(range.len(), 3);

    // Query with limit
    let limited = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-1".to_string()),
            scope: Some(EvidenceScope::Telemetry),
            limit: Some(2),
            ..Default::default()
        })
        .await?;
    assert_eq!(limited.len(), 2);

    // Query with offset and limit (pagination)
    let page = db
        .query_evidence_envelopes(EvidenceEnvelopeFilter {
            tenant_id: Some("tenant-1".to_string()),
            scope: Some(EvidenceScope::Telemetry),
            offset: Some(2),
            limit: Some(2),
            ..Default::default()
        })
        .await?;
    assert_eq!(page.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_chain_verification() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create a valid chain of 3 envelopes
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    let env2 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-2"),
        Some(env1.root),
    );
    db.store_evidence_envelope(&env2).await?;

    let env3 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-3"),
        Some(env2.root),
    );
    db.store_evidence_envelope(&env3).await?;

    // Verify the chain
    let result = db
        .verify_evidence_chain("tenant-1", EvidenceScope::Telemetry)
        .await?;

    assert!(result.is_valid);
    assert_eq!(result.envelopes_checked, 3);
    assert!(result.first_invalid_index.is_none());
    assert!(!result.divergence_detected);
    assert!(result.error_message.is_none());

    Ok(())
}

#[tokio::test]
async fn test_empty_chain_verification() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Verify an empty chain
    let result = db
        .verify_evidence_chain("tenant-1", EvidenceScope::Telemetry)
        .await?;

    assert!(result.is_valid);
    assert_eq!(result.envelopes_checked, 0);
    assert!(result.first_invalid_index.is_none());
    assert!(!result.divergence_detected);

    Ok(())
}

#[tokio::test]
async fn test_get_envelope_by_id() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create and store an envelope
    let original =
        create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    let id = db.store_evidence_envelope(&original).await?;

    // Retrieve it by ID
    let retrieved = db.get_evidence_envelope(&id).await?;
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.tenant_id, original.tenant_id);
    assert_eq!(retrieved.scope, original.scope);
    assert_eq!(retrieved.root, original.root);
    assert_eq!(retrieved.previous_root, original.previous_root);

    // Try to get a non-existent envelope
    let not_found = db.get_evidence_envelope("non-existent-id").await?;
    assert!(not_found.is_none());

    Ok(())
}

#[tokio::test]
async fn test_count_envelopes() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create envelopes across different scopes
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    let env2 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-2"),
        Some(env1.root),
    );
    db.store_evidence_envelope(&env2).await?;

    let env3 = create_policy_envelope("tenant-1".to_string(), "decision-1".to_string(), None);
    db.store_evidence_envelope(&env3).await?;

    // Count telemetry envelopes
    let telemetry_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Telemetry))
        .await?;
    assert_eq!(telemetry_count, 2);

    // Count policy envelopes
    let policy_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Policy))
        .await?;
    assert_eq!(policy_count, 1);

    // Count all envelopes (no scope filter)
    let total_count = db.count_evidence_envelopes("tenant-1", None).await?;
    assert_eq!(total_count, 3);

    Ok(())
}

#[tokio::test]
async fn test_delete_tenant_envelopes() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;
    create_test_tenant(&db, "tenant-2").await?;

    // Create envelopes for two tenants
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    let env2 = create_telemetry_envelope("tenant-2".to_string(), B3Hash::hash(b"bundle-2"), None);
    db.store_evidence_envelope(&env2).await?;

    // Delete tenant-1's envelopes
    let deleted = db.delete_tenant_evidence_envelopes("tenant-1").await?;
    assert_eq!(deleted, 1);

    // Verify tenant-1 has no envelopes
    let count1 = db.count_evidence_envelopes("tenant-1", None).await?;
    assert_eq!(count1, 0);

    // Verify tenant-2 still has envelopes
    let count2 = db.count_evidence_envelopes("tenant-2", None).await?;
    assert_eq!(count2, 1);

    Ok(())
}

#[tokio::test]
async fn test_foreign_key_constraint() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);

    // Try to create an envelope for a non-existent tenant
    let env = create_telemetry_envelope(
        "non-existent-tenant".to_string(),
        B3Hash::hash(b"bundle-1"),
        None,
    );

    let result = db.store_evidence_envelope(&env).await;
    assert!(result.is_err());
    // SQLite will return a foreign key constraint error

    Ok(())
}

#[tokio::test]
async fn test_concurrent_chains() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create chains for all three scopes concurrently
    let mut prev_telemetry = None;
    let mut prev_policy = None;
    let mut prev_inference = None;

    for i in 0..3 {
        // Telemetry chain
        let telem_env = create_telemetry_envelope(
            "tenant-1".to_string(),
            B3Hash::hash(format!("bundle-{}", i).as_bytes()),
            prev_telemetry,
        );
        db.store_evidence_envelope(&telem_env).await?;
        prev_telemetry = Some(telem_env.root);

        // Policy chain
        let policy_env = create_policy_envelope(
            "tenant-1".to_string(),
            format!("decision-{}", i),
            prev_policy,
        );
        db.store_evidence_envelope(&policy_env).await?;
        prev_policy = Some(policy_env.root);

        // Inference chain
        let infer_env = create_inference_envelope(
            "tenant-1".to_string(),
            format!("trace-{}", i),
            prev_inference,
        );
        db.store_evidence_envelope(&infer_env).await?;
        prev_inference = Some(infer_env.root);
    }

    // Verify all chains have the correct length
    let telem_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Telemetry))
        .await?;
    assert_eq!(telem_count, 3);

    let policy_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Policy))
        .await?;
    assert_eq!(policy_count, 3);

    let infer_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Inference))
        .await?;
    assert_eq!(infer_count, 3);

    // Verify each chain's tail
    let telem_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert_eq!(telem_tail.unwrap().1, 3);

    let policy_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Policy)
        .await?;
    assert_eq!(policy_tail.unwrap().1, 3);

    let infer_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Inference)
        .await?;
    assert_eq!(infer_tail.unwrap().1, 3);

    Ok(())
}
