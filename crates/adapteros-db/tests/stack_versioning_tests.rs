//! Integration tests for stack versioning

use adapteros_db::sqlite_backend::SqliteBackend;
use adapteros_db::traits::{CreateStackRequest, DatabaseBackend};
use uuid::Uuid;

fn stack_name() -> String {
    format!("stack.test.{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn test_stack_version_starts_at_one() {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
        .execute(backend.pool())
        .await
        .unwrap();

    let name = stack_name();
    let req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name: name.clone(),
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string(), "adapter2".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    let stack_id = backend.insert_stack(&req).await.unwrap();
    let stack = backend
        .get_stack("test-tenant", &stack_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(stack.version, 1, "New stack should start at version 1");
}

#[tokio::test]
async fn test_stack_version_increments_on_adapter_change() {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
        .execute(backend.pool())
        .await
        .unwrap();

    let name = stack_name();
    // Create initial stack
    let req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name: name.clone(),
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string(), "adapter2".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    let stack_id = backend.insert_stack(&req).await.unwrap();

    // Verify initial version
    let stack_v1 = backend
        .get_stack("test-tenant", &stack_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stack_v1.version, 1);

    // Update with different adapter_ids
    let update_req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name,
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string(), "adapter3".to_string()], // Changed!
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    backend
        .update_stack("test-tenant", &stack_id, &update_req)
        .await
        .unwrap();

    // Verify version incremented
    let stack_v2 = backend
        .get_stack("test-tenant", &stack_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stack_v2.version, 2,
        "Version should increment when adapter_ids change"
    );
}

#[tokio::test]
async fn test_stack_version_increments_on_workflow_change() {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
        .execute(backend.pool())
        .await
        .unwrap();

    let name = stack_name();
    let req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name: name.clone(),
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    let stack_id = backend.insert_stack(&req).await.unwrap();

    // Update with different workflow_type
    let update_req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name,
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()], // Same
        workflow_type: Some("Sequential".to_string()), // Changed!
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    backend
        .update_stack("test-tenant", &stack_id, &update_req)
        .await
        .unwrap();

    let stack_v2 = backend
        .get_stack("test-tenant", &stack_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stack_v2.version, 2,
        "Version should increment when workflow_type changes"
    );
}

#[tokio::test]
async fn test_stack_version_no_increment_on_metadata_change() {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
        .execute(backend.pool())
        .await
        .unwrap();

    let original_name = stack_name();
    let req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name: original_name.clone(),
        description: Some("Original description".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    let stack_id = backend.insert_stack(&req).await.unwrap();

    // Update only description (metadata change, not config change)
    let renamed = stack_name();
    let update_req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name: renamed.clone(),                            // Changed
        description: Some("New description".to_string()), // Changed
        adapter_ids: vec!["adapter1".to_string()],        // Same
        workflow_type: Some("Parallel".to_string()),      // Same
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    backend
        .update_stack("test-tenant", &stack_id, &update_req)
        .await
        .unwrap();

    let stack = backend
        .get_stack("test-tenant", &stack_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stack.version, 1,
        "Version should NOT increment for metadata-only changes"
    );
    assert_eq!(stack.name, renamed, "Name should be updated");
    assert_eq!(
        stack.description.as_deref(),
        Some("New description"),
        "Description should be updated"
    );
}

#[tokio::test]
async fn test_stack_version_multiple_increments() {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
        .execute(backend.pool())
        .await
        .unwrap();

    let base_name = stack_name();
    let req = CreateStackRequest {
        tenant_id: "test-tenant".to_string(),
        name: base_name.clone(),
        description: None,
        adapter_ids: vec!["a1".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };

    let stack_id = backend.insert_stack(&req).await.unwrap();

    // Make 5 configuration changes
    for i in 2..=6 {
        let update_req = CreateStackRequest {
            tenant_id: "test-tenant".to_string(),
            name: base_name.clone(),
            description: None,
            adapter_ids: vec![format!("a{}", i)],
            workflow_type: Some("Parallel".to_string()),
            determinism_mode: None,
            routing_determinism_mode: None,
        };

        backend
            .update_stack("test-tenant", &stack_id, &update_req)
            .await
            .unwrap();

        let stack = backend
            .get_stack("test-tenant", &stack_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stack.version,
            i,
            "Version should be {} after {} updates",
            i,
            i - 1
        );
    }

    let final_stack = backend
        .get_stack("test-tenant", &stack_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_stack.version, 6, "Final version should be 6");
}

#[tokio::test]
async fn test_list_stacks_includes_version() {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
        .execute(backend.pool())
        .await
        .unwrap();

    // Create multiple stacks
    for i in 1..=3 {
        let req = CreateStackRequest {
            tenant_id: "test-tenant".to_string(),
            name: format!("stack.test.{}", i),
            description: None,
            adapter_ids: vec![format!("adapter{}", i)],
            workflow_type: Some("Parallel".to_string()),
            determinism_mode: None,
            routing_determinism_mode: None,
        };
        backend.insert_stack(&req).await.unwrap();
    }

    let stacks = backend.list_stacks_for_tenant("test-tenant").await.unwrap();
    assert_eq!(stacks.len(), 3);

    for stack in stacks {
        assert_eq!(stack.version, 1, "All new stacks should have version 1");
    }
}
