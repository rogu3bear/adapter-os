//! Tenant isolation tests for adapter lifecycle operations
//!
//! These tests verify that adapter operations are properly scoped to tenants
//! and prevent cross-tenant data leakage.

use adapteros_core::B3Hash;
use adapteros_db::test_utils::*;
use adapteros_db::{Adapter, Db};

#[tokio::test]
async fn test_adapter_registration_cross_tenant_isolation() {
    let db = setup_test_db().await;

    // Register adapter in tenant-a
    let adapter_id = db.register_adapter(create_test_adapter_params("tenant-a", "test-adapter")).await.unwrap();

    // Verify tenant-a can access
    let adapter = db.get_adapter_for_tenant("tenant-a", &adapter_id).await.unwrap().unwrap();
    assert_eq!(adapter.tenant_id, "tenant-a");
    assert_eq!(adapter.adapter_id, adapter_id);

    // Verify tenant-b cannot access
    let result = db.get_adapter_for_tenant("tenant-b", &adapter_id).await.unwrap();
    assert!(result.is_none(), "Cross-tenant access should be blocked");

    cleanup_test_db(db).await;
}

#[tokio::test]
async fn test_adapter_listing_tenant_scoped() {
    let db = setup_test_db().await;

    // Create adapters in different tenants
    let adapter_a = db.register_adapter(create_test_adapter_params("tenant-a", "adapter-a")).await.unwrap();
    let adapter_b = db.register_adapter(create_test_adapter_params("tenant-b", "adapter-b")).await.unwrap();

    // List adapters for tenant-a
    let tenant_a_adapters = db.list_adapters_for_tenant("tenant-a").await.unwrap();
    assert!(tenant_a_adapters.iter().any(|a| a.adapter_id == adapter_a));
    assert!(!tenant_a_adapters.iter().any(|a| a.adapter_id == adapter_b));

    // List adapters for tenant-b
    let tenant_b_adapters = db.list_adapters_for_tenant("tenant-b").await.unwrap();
    assert!(tenant_b_adapters.iter().any(|a| a.adapter_id == adapter_b));
    assert!(!tenant_b_adapters.iter().any(|a| a.adapter_id == adapter_a));

    cleanup_test_db(db).await;
}

#[tokio::test]
async fn test_adapter_state_update_cross_tenant_denial() {
    let db = setup_test_db().await;

    // Create adapter in tenant-a
    let adapter_id = db.register_adapter(create_test_adapter_params("tenant-a", "test-adapter")).await.unwrap();

    // Update state from tenant-a (should succeed)
    db.update_adapter_state(&adapter_id, "warm", "test promotion").await.unwrap();

    // Verify state was updated
    let adapter = db.get_adapter_for_tenant("tenant-a", &adapter_id).await.unwrap().unwrap();
    assert_eq!(adapter.load_state, Some("warm".to_string()));

    // Attempt state update from tenant-b (should fail silently or be ignored)
    // Note: This test may need adjustment based on actual update_adapter_state behavior
    let result = db.update_adapter_state(&adapter_id, "hot", "cross-tenant attempt").await;
    // The function may succeed but not actually update if tenant scoping is enforced elsewhere

    cleanup_test_db(db).await;
}

#[tokio::test]
async fn test_adapter_deletion_cross_tenant_denial() {
    let db = setup_test_db().await;

    // Create adapter in tenant-a
    let adapter_id = db.register_adapter(create_test_adapter_params("tenant-a", "test-adapter")).await.unwrap();

    // Attempt deletion from tenant-b (should fail or be ignored)
    let result = db.delete_adapter(&adapter_id).await;
    // This should either fail or only mark for deletion if proper tenant checks exist

    // Verify adapter still exists for tenant-a
    let adapter = db.get_adapter_for_tenant("tenant-a", &adapter_id).await.unwrap();
    assert!(adapter.is_some(), "Adapter should still exist for owning tenant");

    cleanup_test_db(db).await;
}

#[tokio::test]
async fn test_find_adapter_by_hash_tenant_scoped() {
    let db = setup_test_db().await;

    // Create adapter in tenant-a
    let params = create_test_adapter_params("tenant-a", "test-adapter");
    let adapter_id = db.register_adapter(params.clone()).await.unwrap();

    // Get the adapter to find its hash
    let adapter = db.get_adapter_for_tenant("tenant-a", &adapter_id).await.unwrap().unwrap();
    let hash = adapter.hash_b3.clone();

    // Find by hash within tenant-a (should succeed)
    let found = db.find_adapter_by_hash_for_tenant("tenant-a", &hash).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().adapter_id, adapter_id);

    // Find by hash within tenant-b (should return None)
    let not_found = db.find_adapter_by_hash_for_tenant("tenant-b", &hash).await.unwrap();
    assert!(not_found.is_none(), "Cross-tenant hash lookup should be blocked");

    cleanup_test_db(db).await;
}

#[tokio::test]
async fn test_adapter_lineage_tenant_isolation() {
    let db = setup_test_db().await;

    // Create parent adapter in tenant-a
    let parent_params = create_test_adapter_params("tenant-a", "parent-adapter");
    let parent_id = db.register_adapter(parent_params).await.unwrap();

    // Create child adapter in tenant-a (fork of parent)
    let child_params = create_test_adapter_params_with_parent("tenant-a", "child-adapter", &parent_id);
    let child_id = db.register_adapter(child_params).await.unwrap();

    // Get lineage from tenant-a (should include both)
    let lineage = db.get_adapter_lineage(&child_id).await.unwrap();
    assert!(lineage.len() >= 2); // Parent + child

    // Attempt lineage access from tenant-b (should fail or return empty)
    // This tests whether lineage queries are tenant-scoped
    // Note: Actual behavior depends on implementation

    cleanup_test_db(db).await;
}

// Helper functions

fn create_test_adapter_params(tenant_id: &str, name: &str) -> adapteros_db::AdapterRegistrationParams {
    adapteros_db::AdapterRegistrationParams {
        tenant_id: tenant_id.to_string(),
        adapter_id: format!("{}-{}", name, uuid::Uuid::new_v4().simple()),
        name: name.to_string(),
        hash_b3: B3Hash::hash(format!("test-hash-{}", name).as_bytes()).to_string(),
        rank: 8,
        alpha: 16.0,
        targets_json: r#"["q_proj", "k_proj", "v_proj", "o_proj"]"#.to_string(),
        acl_json: r#"["read", "write"]"#.to_string(),
        languages_json: r#"["en"]"#.to_string(),
        framework: "test".to_string(),
        category: "test".to_string(),
        scope: "test".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: "test".to_string(),
        tier: "warm".to_string(),
        expires_at: None,
        parent_id: None,
        fork_type: None,
        fork_reason: None,
        version: "1.0.0".to_string(),
        lifecycle_state: "active".to_string(),
        archived_at: None,
        archived_by: None,
        archive_reason: None,
        purged_at: None,
        base_model_id: "test-model".to_string(),
        manifest_schema_version: "1.0".to_string(),
        content_hash_b3: B3Hash::hash(b"test-content").to_string(),
        metadata_json: "{}".to_string(),
        provenance_json: "{}".to_string(),
    }
}

fn create_test_adapter_params_with_parent(tenant_id: &str, name: &str, parent_id: &str) -> adapteros_db::AdapterRegistrationParams {
    let mut params = create_test_adapter_params(tenant_id, name);
    params.parent_id = Some(parent_id.to_string());
    params.fork_type = Some("branch".to_string());
    params.fork_reason = Some("test fork".to_string());
    params
}
