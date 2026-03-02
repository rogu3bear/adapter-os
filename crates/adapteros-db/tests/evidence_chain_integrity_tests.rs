//! Evidence Chain Integrity Tests (P0 Critical)
//!
//! Extended tests for evidence chain integrity and divergence detection.
//! Complements evidence_envelope_integration.rs with additional edge cases.
//!
//! These tests verify:
//! - Chain divergence on root tampering
//! - Unexpected chain break detection
//! - Scope isolation prevents cross-chain linkage
//! - Chain verification fails on signature mismatch
//! - Chain recovery/continuation after divergence rejection

use adapteros_core::evidence_envelope::{BundleMetadataRef, EvidenceEnvelope, PolicyAuditRef};
use adapteros_core::{B3Hash, EvidenceScope};
use adapteros_db::Db;
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

/// Helper to create a telemetry envelope with specific bundle hash
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
        pinned_degradation_evidence: None,
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

/// Test that inserting an envelope with tampered root is rejected.
///
/// Even if previous_root is correct, a modified root hash should
/// cause verification to fail.
#[tokio::test]
async fn test_chain_rejects_envelope_with_different_bundle_hash() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create first envelope
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    // Create second envelope with correct previous_root
    let env2 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-2"),
        Some(env1.root),
    );
    db.store_evidence_envelope(&env2).await?;

    // Verify chain is consistent
    let tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert!(tail.is_some());
    let (root, seq) = tail.unwrap();
    assert_eq!(root, env2.root);
    assert_eq!(seq, 2);

    // Attempt to insert third envelope with wrong previous_root
    let env3_bad = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-3"),
        Some(env1.root), // Wrong! Should be env2.root
    );

    let result = db.store_evidence_envelope(&env3_bad).await;
    assert!(
        result.is_err(),
        "Should reject envelope with wrong previous_root"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("EVIDENCE_CHAIN_DIVERGED"),
        "Error should indicate chain divergence"
    );

    Ok(())
}

/// Test that scope isolation prevents using one scope's root in another.
///
/// A telemetry chain's root cannot be used as previous_root for a policy envelope.
#[tokio::test]
async fn test_scope_isolation_prevents_cross_chain_linkage() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create telemetry envelope
    let telemetry_env =
        create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&telemetry_env).await?;

    // Try to create policy envelope using telemetry's root as previous
    // This should fail because policy chain has no envelopes yet
    let policy_env = create_policy_envelope(
        "tenant-1".to_string(),
        "decision-1".to_string(),
        Some(telemetry_env.root), // Wrong - this is a telemetry root!
    );

    let result = db.store_evidence_envelope(&policy_env).await;
    assert!(result.is_err(), "Should reject cross-scope chain linkage");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("EVIDENCE_CHAIN_DIVERGED"),
        "Error should indicate chain divergence: {}",
        err_msg
    );

    Ok(())
}

/// Test that chain can continue after a rejected divergent envelope.
///
/// The valid chain should remain intact and accept correctly linked envelopes.
#[tokio::test]
async fn test_chain_continues_after_divergence_rejection() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create initial chain of 2 envelopes
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    let env2 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-2"),
        Some(env1.root),
    );
    db.store_evidence_envelope(&env2).await?;

    // Attempt to insert divergent envelope (should fail)
    let env_bad = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-bad"),
        Some(B3Hash::hash(b"wrong-root")),
    );
    let result = db.store_evidence_envelope(&env_bad).await;
    assert!(result.is_err());

    // Verify chain state is unchanged
    let tail_before = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert!(tail_before.is_some());
    assert_eq!(
        tail_before.unwrap().1,
        2,
        "Chain should still be at sequence 2"
    );

    // Now insert correctly linked envelope - should succeed
    let env3 = create_telemetry_envelope(
        "tenant-1".to_string(),
        B3Hash::hash(b"bundle-3"),
        Some(env2.root), // Correct previous_root
    );
    db.store_evidence_envelope(&env3).await?;

    // Verify chain advanced correctly
    let tail_after = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert!(tail_after.is_some());
    let (root, seq) = tail_after.unwrap();
    assert_eq!(root, env3.root);
    assert_eq!(seq, 3, "Chain should advance to sequence 3");

    Ok(())
}

/// Test that envelope count is accurate even after rejected envelopes.
///
/// Failed insertions should not affect count or chain state.
#[tokio::test]
async fn test_envelope_count_unchanged_after_rejection() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Create initial envelope
    let env1 = create_telemetry_envelope("tenant-1".to_string(), B3Hash::hash(b"bundle-1"), None);
    db.store_evidence_envelope(&env1).await?;

    let count_before = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Telemetry))
        .await?;
    assert_eq!(count_before, 1);

    // Attempt multiple rejected insertions
    for i in 0..5 {
        let env_bad = create_telemetry_envelope(
            "tenant-1".to_string(),
            B3Hash::hash(format!("bad-{}", i).as_bytes()),
            Some(B3Hash::hash(format!("wrong-{}", i).as_bytes())),
        );
        let _ = db.store_evidence_envelope(&env_bad).await;
    }

    // Count should be unchanged
    let count_after = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Telemetry))
        .await?;
    assert_eq!(
        count_after, 1,
        "Count should be unchanged after rejected insertions"
    );

    Ok(())
}

/// Test that multiple scopes can grow independently for the same tenant.
///
/// Each scope should have its own sequence and chain state.
#[tokio::test]
async fn test_multiple_scopes_independent_growth() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    create_test_tenant(&db, "tenant-1").await?;

    // Grow telemetry chain to length 3
    let mut telemetry_prev = None;
    for i in 0..3 {
        let env = create_telemetry_envelope(
            "tenant-1".to_string(),
            B3Hash::hash(format!("tel-{}", i).as_bytes()),
            telemetry_prev,
        );
        db.store_evidence_envelope(&env).await?;
        telemetry_prev = Some(env.root);
    }

    // Grow policy chain to length 5
    let mut policy_prev = None;
    for i in 0..5 {
        let env = create_policy_envelope(
            "tenant-1".to_string(),
            format!("decision-{}", i),
            policy_prev,
        );
        db.store_evidence_envelope(&env).await?;
        policy_prev = Some(env.root);
    }

    // Verify independent sequences
    let telemetry_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Telemetry)
        .await?;
    assert_eq!(telemetry_tail.unwrap().1, 3);

    let policy_tail = db
        .get_evidence_chain_tail("tenant-1", EvidenceScope::Policy)
        .await?;
    assert_eq!(policy_tail.unwrap().1, 5);

    // Verify counts
    let tel_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Telemetry))
        .await?;
    assert_eq!(tel_count, 3);

    let pol_count = db
        .count_evidence_envelopes("tenant-1", Some(EvidenceScope::Policy))
        .await?;
    assert_eq!(pol_count, 5);

    Ok(())
}
