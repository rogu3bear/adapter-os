/// Tests for PRD 2: Deterministic Tenant Snapshot & Hydration
///
/// Validates:
/// 1. Deterministic hash computation (same DB → same hash)
/// 2. Hash changes when state changes
/// 3. Validation of DB consistency (stack references)
/// 4. Idempotent hydration (no duplicates)
use adapteros_core::{SnapshotHash, TenantStateSnapshot};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::traits::CreateStackRequest;
use adapteros_db::Db;
use anyhow::Result;

/// Test: Build same fixture DB twice, hydrate twice, assert equal hash
#[tokio::test]
async fn test_deterministic_hash() -> Result<()> {
    // Create two identical in-memory databases
    let db1 = Db::new_in_memory().await?;
    let db2 = Db::new_in_memory().await?;

    // Create tenant in both DBs
    let tenant_id = "test-tenant";
    db1.create_tenant(tenant_id, false).await?;
    db2.create_tenant(tenant_id, false).await?;

    // Register identical adapters in both DBs
    let adapter1_params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id("adapter-1")
        .name("Test Adapter 1")
        .hash_b3("hash1")
        .rank(8)
        .tier("warm")
        .build()?;

    let adapter2_params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id("adapter-2")
        .name("Test Adapter 2")
        .hash_b3("hash2")
        .rank(16)
        .tier("hot")
        .build()?;

    let id1 = db1.register_adapter(adapter1_params.clone()).await?;
    let id2 = db1.register_adapter(adapter2_params.clone()).await?;

    db2.register_adapter(adapter1_params).await?;
    db2.register_adapter(adapter2_params).await?;

    // Create identical stacks in both DBs
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: "test-stack".to_string(),
        description: Some("Test Stack".to_string()),
        adapter_ids: vec![id1.clone(), id2.clone()],
        workflow_type: None,
    };

    db1.insert_stack(&stack_req).await?;
    db2.insert_stack(&stack_req).await?;

    // Hydrate both DBs
    let hash1 = db1.hydrate_from_db(tenant_id).await?;
    let hash2 = db2.hydrate_from_db(tenant_id).await?;

    // Assert hashes are identical (deterministic)
    assert_eq!(
        hash1.state_hash, hash2.state_hash,
        "Same DB contents must yield identical hash"
    );

    // Hydrate again (idempotency test)
    let hash1_again = db1.hydrate_from_db(tenant_id).await?;
    assert_eq!(
        hash1.state_hash, hash1_again.state_hash,
        "Re-hydrating must yield same hash (idempotent)"
    );

    Ok(())
}

/// Test: Remove one adapter, hydrate again, assert hash changes
#[tokio::test]
async fn test_hash_changes_on_state_change() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = "test-tenant";
    db.create_tenant(tenant_id, false).await?;

    // Register adapters
    let adapter1_params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id("adapter-1")
        .name("Test Adapter 1")
        .hash_b3("hash1")
        .rank(8)
        .build()?;

    let adapter2_params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id("adapter-2")
        .name("Test Adapter 2")
        .hash_b3("hash2")
        .rank(16)
        .build()?;

    let id1 = db.register_adapter(adapter1_params).await?;
    let id2 = db.register_adapter(adapter2_params).await?;

    // Hydrate and get initial hash
    let hash1 = db.hydrate_from_db(tenant_id).await?;

    // Delete one adapter
    db.delete_adapter(&id2).await?;

    // Hydrate again
    let hash2 = db.hydrate_from_db(tenant_id).await?;

    // Assert hash changed
    assert_ne!(
        hash1.state_hash, hash2.state_hash,
        "Hash must change when state changes"
    );

    Ok(())
}

/// Test: Inject invalid stack reference, assert hydration fails with 409
#[tokio::test]
async fn test_validation_fails_on_invalid_stack_reference() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = "test-tenant";
    db.create_tenant(tenant_id, false).await?;

    // Register one adapter
    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id("adapter-1")
        .name("Test Adapter 1")
        .hash_b3("hash1")
        .rank(8)
        .build()?;

    let id1 = db.register_adapter(adapter_params).await?;

    // Create stack with missing adapter reference
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: "invalid-stack".to_string(),
        description: Some("Stack with invalid reference".to_string()),
        adapter_ids: vec![id1, "non-existent-adapter".to_string()],
        workflow_type: None,
    };

    db.insert_stack(&stack_req).await?;

    // Get hash before hydration attempt
    let hash_before = db.get_tenant_snapshot_hash(tenant_id).await?;

    // Attempt hydration - should fail
    let result = db.hydrate_from_db(tenant_id).await;

    assert!(result.is_err(), "Hydration should fail with invalid reference");

    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("references invalid or cross-tenant adapters"),
        "Error should indicate invalid adapter reference, got: {}",
        err_msg
    );

    // Verify no hash was stored (no partial writes)
    let hash_after = db.get_tenant_snapshot_hash(tenant_id).await?;
    assert_eq!(
        hash_before, hash_after,
        "Hash should not change on validation failure"
    );

    Ok(())
}

/// Test: Canonical ordering produces consistent results
#[tokio::test]
async fn test_canonical_ordering() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = "test-tenant";
    db.create_tenant(tenant_id, false).await?;

    // Register adapters in different order
    let ids: Vec<String> = vec!["c", "a", "b"]
        .into_iter()
        .map(|name| async {
            let params = AdapterRegistrationBuilder::new()
                .tenant_id(tenant_id)
                .adapter_id(format!("adapter-{}", name))
                .name(format!("Adapter {}", name))
                .hash_b3(format!("hash-{}", name))
                .rank(8)
                .build()
                .unwrap();
            db.register_adapter(params).await.unwrap()
        })
        .collect::<Vec<_>>();

    // Wait for all registrations
    let mut registered_ids = Vec::new();
    for fut in ids {
        registered_ids.push(fut.await);
    }

    // Build snapshot
    let snapshot = db.build_tenant_snapshot(tenant_id).await?;

    // Verify adapters are sorted by ID (canonical ordering)
    for i in 0..snapshot.adapters.len() - 1 {
        assert!(
            snapshot.adapters[i].id <= snapshot.adapters[i + 1].id,
            "Adapters must be sorted by ID"
        );
    }

    Ok(())
}

/// Test: Snapshot serialization is deterministic
#[test]
fn test_snapshot_compute_hash_determinism() {
    use adapteros_core::{AdapterInfo, PolicyInfo, StackInfo};
    use chrono::Utc;
    use std::collections::BTreeMap;

    // Create snapshot with unsorted data
    let snapshot1 = TenantStateSnapshot {
        tenant_id: "tenant-1".to_string(),
        adapters: vec![
            AdapterInfo {
                id: "c".to_string(),
                name: "C".to_string(),
                rank: 1,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "a".to_string(),
                name: "A".to_string(),
                rank: 2,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "b".to_string(),
                name: "B".to_string(),
                rank: 3,
                version: "1.0".to_string(),
            },
        ],
        stacks: vec![StackInfo {
            name: "stack-1".to_string(),
            adapter_ids: vec!["c".to_string(), "a".to_string(), "b".to_string()],
        }],
        router_policies: vec![PolicyInfo {
            name: "policy-1".to_string(),
            rules: vec!["rule-c".to_string(), "rule-a".to_string()],
        }],
        plugin_configs: BTreeMap::new(),
        feature_flags: BTreeMap::new(),
        configs: BTreeMap::new(),
        snapshot_timestamp: Utc::now(),
    };

    // Create identical snapshot with different order
    let snapshot2 = TenantStateSnapshot {
        tenant_id: "tenant-1".to_string(),
        adapters: vec![
            AdapterInfo {
                id: "a".to_string(),
                name: "A".to_string(),
                rank: 2,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "b".to_string(),
                name: "B".to_string(),
                rank: 3,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "c".to_string(),
                name: "C".to_string(),
                rank: 1,
                version: "1.0".to_string(),
            },
        ],
        stacks: vec![StackInfo {
            name: "stack-1".to_string(),
            adapter_ids: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        }],
        router_policies: vec![PolicyInfo {
            name: "policy-1".to_string(),
            rules: vec!["rule-a".to_string(), "rule-c".to_string()],
        }],
        plugin_configs: BTreeMap::new(),
        feature_flags: BTreeMap::new(),
        configs: BTreeMap::new(),
        snapshot_timestamp: snapshot1.snapshot_timestamp, // Same timestamp
    };

    // Compute hashes - should be identical due to canonical ordering
    let hash1 = snapshot1.compute_hash();
    let hash2 = snapshot2.compute_hash();

    assert_eq!(
        hash1, hash2,
        "Snapshots with same content but different input order must yield identical hash"
    );
}
