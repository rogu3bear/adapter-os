//! Integration tests for tenant default stack functionality
//!
//! Tests the behavior of set_default_stack(), get_default_stack(), and clear_default_stack()
//! for adapter registration and stack management.

use adapteros_db::traits::CreateStackRequest;
use adapteros_db::Db;
use uuid::Uuid;

fn stack_name() -> String {
    format!("stack.test.{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn test_set_and_get_default_stack() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Create a stack
    let name = stack_name();
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.clone(),
        name,
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack_id = db.insert_stack(&stack_req).await.unwrap();

    // Initially no default stack
    let default_before = db.get_default_stack(&tenant_id).await.unwrap();
    assert!(
        default_before.is_none(),
        "New tenant should have no default stack"
    );

    // Set default stack
    db.set_default_stack(&tenant_id, &stack_id).await.unwrap();

    // Verify default stack is set
    let default_after = db.get_default_stack(&tenant_id).await.unwrap();
    assert_eq!(
        default_after,
        Some(stack_id.clone()),
        "Default stack should be set"
    );
}

#[tokio::test]
async fn test_clear_default_stack() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Create and set a default stack
    let name = stack_name();
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.clone(),
        name,
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack_id = db.insert_stack(&stack_req).await.unwrap();
    db.set_default_stack(&tenant_id, &stack_id).await.unwrap();

    // Verify it's set
    let default_before = db.get_default_stack(&tenant_id).await.unwrap();
    assert!(default_before.is_some());

    // Clear default stack
    db.clear_default_stack(&tenant_id).await.unwrap();

    // Verify it's cleared
    let default_after = db.get_default_stack(&tenant_id).await.unwrap();
    assert!(default_after.is_none(), "Default stack should be cleared");
}

#[tokio::test]
async fn test_set_default_to_nonexistent_stack_fails() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Try to set default to a non-existent stack
    let result = db
        .set_default_stack(&tenant_id, "nonexistent-stack-id")
        .await;

    // Should fail (the implementation validates stack exists for tenant)
    assert!(
        result.is_err(),
        "Setting default to non-existent stack should fail"
    );
}

#[tokio::test]
async fn test_set_default_to_wrong_tenant_stack_fails() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create two tenants
    let tenant1_id = db.create_tenant("tenant-1", false).await.unwrap();
    let tenant2_id = db.create_tenant("tenant-2", false).await.unwrap();

    // Create a stack for tenant 1
    let name = stack_name();
    let stack_req = CreateStackRequest {
        tenant_id: tenant1_id.clone(),
        name,
        description: Some("Tenant 1's stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack_id = db.insert_stack(&stack_req).await.unwrap();

    // Try to set tenant 2's default to tenant 1's stack
    let result = db.set_default_stack(&tenant2_id, &stack_id).await;

    // Should fail due to tenant isolation
    assert!(
        result.is_err(),
        "Setting default to another tenant's stack should fail"
    );
}

#[tokio::test]
async fn test_changing_default_stack() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Create two stacks
    let stack1_name = stack_name();
    let stack1_req = CreateStackRequest {
        tenant_id: tenant_id.clone(),
        name: stack1_name.clone(),
        description: Some("Stack 1".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack1_id = db.insert_stack(&stack1_req).await.unwrap();

    let stack2_req = CreateStackRequest {
        tenant_id: tenant_id.clone(),
        name: stack_name(),
        description: Some("Stack 2".to_string()),
        adapter_ids: vec!["adapter2".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack2_id = db.insert_stack(&stack2_req).await.unwrap();

    // Set first stack as default
    db.set_default_stack(&tenant_id, &stack1_id).await.unwrap();

    let default = db.get_default_stack(&tenant_id).await.unwrap();
    assert_eq!(default, Some(stack1_id.clone()));

    // Change to second stack
    db.set_default_stack(&tenant_id, &stack2_id).await.unwrap();

    let default = db.get_default_stack(&tenant_id).await.unwrap();
    assert_eq!(
        default,
        Some(stack2_id.clone()),
        "Default stack should be updated to stack 2"
    );
}

#[tokio::test]
async fn test_activate_stack_sets_lifecycle_active() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = db
        .create_tenant("test-tenant-activate", false)
        .await
        .unwrap();

    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name(),
        description: Some("Activation test stack".to_string()),
        adapter_ids: vec!["adapter-activate".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack_id = db.insert_stack(&stack_req).await.unwrap();

    // Force lifecycle_state to draft to verify activation flips it back
    sqlx::query("UPDATE adapter_stacks SET lifecycle_state = 'draft' WHERE id = ?")
        .bind(&stack_id)
        .execute(db.pool())
        .await
        .unwrap();

    db.activate_stack(&tenant_id, &stack_id)
        .await
        .expect("Stack activation should succeed");

    let stack = db
        .get_stack(&tenant_id, &stack_id)
        .await
        .unwrap()
        .expect("Stack should exist");
    assert_eq!(stack.lifecycle_state, "active");
}
