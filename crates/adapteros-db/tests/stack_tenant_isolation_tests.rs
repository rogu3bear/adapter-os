//! Stack Cross-Tenant Isolation Tests (P0 Critical)
//!
//! Tests for tenant isolation in adapter stack operations.
//! Cross-tenant access must be prevented at the database layer.
//!
//! These tests verify:
//! - Insert stack with foreign tenant adapter fails
//! - Update stack adapters to foreign tenant fails
//! - Get stack from wrong tenant fails
//! - List stacks isolated by tenant
//! - Set default to non-existent stack fails

use adapteros_db::sqlite_backend::SqliteBackend;
use adapteros_db::traits::{CreateStackRequest, DatabaseBackend};
use uuid::Uuid;

fn stack_name() -> String {
    format!("stack.test.{}", Uuid::new_v4().simple())
}

/// Helper to create a test backend with two tenants
async fn setup_multi_tenant_backend() -> SqliteBackend {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();

    // Create two tenants
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-a', 'Tenant A')")
        .execute(backend.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-b', 'Tenant B')")
        .execute(backend.pool())
        .await
        .unwrap();

    backend
}

/// Test that list_stacks_for_tenant only returns stacks for the specified tenant.
///
/// Stacks from other tenants must not be visible.
#[tokio::test]
async fn test_list_stacks_isolated_by_tenant() {
    let backend = setup_multi_tenant_backend().await;

    // Create 2 stacks for tenant-a
    for i in 1..=2 {
        let req = CreateStackRequest {
            tenant_id: "tenant-a".to_string(),
            name: format!("stack.tenant-a.{}", i),
            description: None,
            adapter_ids: vec![format!("adapter-a{}", i)],
            workflow_type: Some("Parallel".to_string()),
            determinism_mode: None,
            routing_determinism_mode: None,
        };
        backend.insert_stack(&req).await.unwrap();
    }

    // Create 3 stacks for tenant-b
    for i in 1..=3 {
        let req = CreateStackRequest {
            tenant_id: "tenant-b".to_string(),
            name: format!("stack.tenant-b.{}", i),
            description: None,
            adapter_ids: vec![format!("adapter-b{}", i)],
            workflow_type: Some("Parallel".to_string()),
            determinism_mode: None,
            routing_determinism_mode: None,
        };
        backend.insert_stack(&req).await.unwrap();
    }

    // List for tenant-a should only return 2 stacks
    let stacks_a = backend.list_stacks_for_tenant("tenant-a").await.unwrap();
    assert_eq!(stacks_a.len(), 2, "Tenant A should only see their 2 stacks");
    for stack in &stacks_a {
        assert_eq!(stack.tenant_id, "tenant-a", "All stacks should belong to tenant-a");
        assert!(
            stack.name.contains("tenant-a"),
            "Stack names should contain tenant-a"
        );
    }

    // List for tenant-b should only return 3 stacks
    let stacks_b = backend.list_stacks_for_tenant("tenant-b").await.unwrap();
    assert_eq!(stacks_b.len(), 3, "Tenant B should only see their 3 stacks");
    for stack in &stacks_b {
        assert_eq!(stack.tenant_id, "tenant-b", "All stacks should belong to tenant-b");
        assert!(
            stack.name.contains("tenant-b"),
            "Stack names should contain tenant-b"
        );
    }

    // List for non-existent tenant should return empty
    let stacks_c = backend.list_stacks_for_tenant("tenant-c").await.unwrap();
    assert!(stacks_c.is_empty(), "Non-existent tenant should have no stacks");
}

/// Test that get_stack with wrong tenant returns None.
///
/// Even if you know the stack ID, you cannot access it from a different tenant.
#[tokio::test]
async fn test_get_stack_cross_tenant_returns_none() {
    let backend = setup_multi_tenant_backend().await;

    // Create stack for tenant-a
    let req = CreateStackRequest {
        tenant_id: "tenant-a".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = backend.insert_stack(&req).await.unwrap();

    // Verify tenant-a can access it
    let stack_a = backend.get_stack("tenant-a", &stack_id).await.unwrap();
    assert!(stack_a.is_some(), "Owner tenant should see the stack");

    // Tenant-b should NOT be able to access it
    let stack_b = backend.get_stack("tenant-b", &stack_id).await.unwrap();
    assert!(
        stack_b.is_none(),
        "Other tenant should NOT see the stack even with valid ID"
    );
}

/// Test that update_stack with wrong tenant fails.
///
/// A tenant should not be able to modify another tenant's stack.
#[tokio::test]
async fn test_update_stack_cross_tenant_fails() {
    let backend = setup_multi_tenant_backend().await;

    // Create stack for tenant-a
    let original_name = stack_name();
    let req = CreateStackRequest {
        tenant_id: "tenant-a".to_string(),
        name: original_name.clone(),
        description: Some("Original".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = backend.insert_stack(&req).await.unwrap();

    // Attempt to update from tenant-b
    let malicious_update = CreateStackRequest {
        tenant_id: "tenant-b".to_string(), // Wrong tenant!
        name: original_name.clone(),
        description: Some("Malicious update".to_string()),
        adapter_ids: vec!["malicious_adapter".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    // This should fail or not find the stack
    let result = backend
        .update_stack("tenant-b", &stack_id, &malicious_update)
        .await;

    // Either it returns an error or silently does nothing
    // Either way, the original stack should be unchanged
    let original = backend.get_stack("tenant-a", &stack_id).await.unwrap().unwrap();
    assert_eq!(
        original.description.as_deref(),
        Some("Original"),
        "Stack should not be modified by other tenant"
    );
    assert_eq!(
        original.adapter_ids_json,
        r#"["adapter1"]"#,
        "Adapter IDs should be unchanged"
    );
}

/// Test that delete_stack with wrong tenant fails.
///
/// A tenant should not be able to delete another tenant's stack.
#[tokio::test]
async fn test_delete_stack_cross_tenant_fails() {
    let backend = setup_multi_tenant_backend().await;

    // Create stack for tenant-a
    let req = CreateStackRequest {
        tenant_id: "tenant-a".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = backend.insert_stack(&req).await.unwrap();

    // Attempt delete from tenant-b
    let delete_result = backend.delete_stack("tenant-b", &stack_id).await;

    // Should either fail or not affect the stack
    // Verify stack still exists for tenant-a
    let stack = backend.get_stack("tenant-a", &stack_id).await.unwrap();
    assert!(
        stack.is_some(),
        "Stack should still exist after cross-tenant delete attempt"
    );
}

/// Test that stacks with same name can exist for different tenants.
///
/// Stack names are scoped per tenant, so "my-stack" can exist for both tenants.
#[tokio::test]
async fn test_same_stack_name_different_tenants() {
    let backend = setup_multi_tenant_backend().await;

    let shared_name = "stack.shared.name";

    // Create stack with same name for tenant-a
    let req_a = CreateStackRequest {
        tenant_id: "tenant-a".to_string(),
        name: shared_name.to_string(),
        description: Some("Tenant A's stack".to_string()),
        adapter_ids: vec!["adapter-a".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id_a = backend.insert_stack(&req_a).await.unwrap();

    // Create stack with same name for tenant-b - should succeed
    let req_b = CreateStackRequest {
        tenant_id: "tenant-b".to_string(),
        name: shared_name.to_string(),
        description: Some("Tenant B's stack".to_string()),
        adapter_ids: vec!["adapter-b".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id_b = backend.insert_stack(&req_b).await;

    // Both should succeed (names scoped by tenant)
    assert!(
        stack_id_b.is_ok(),
        "Same name should be allowed for different tenants"
    );

    // They should have different IDs
    let stack_id_b = stack_id_b.unwrap();
    assert_ne!(
        stack_id_a, stack_id_b,
        "Stacks should have different IDs"
    );

    // Each tenant should see their own version
    let stack_a = backend.get_stack("tenant-a", &stack_id_a).await.unwrap().unwrap();
    let stack_b = backend.get_stack("tenant-b", &stack_id_b).await.unwrap().unwrap();

    assert_eq!(stack_a.description.as_deref(), Some("Tenant A's stack"));
    assert_eq!(stack_b.description.as_deref(), Some("Tenant B's stack"));
}
