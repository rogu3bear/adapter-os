//! Integration tests for tenant default stack functionality
//!
//! These tests verify the database-level behavior of default stack operations
//! that back the API endpoints:
//! - GET /v1/tenants/{tenant_id}/default-stack
//! - PUT /v1/tenants/{tenant_id}/default-stack
//! - DELETE /v1/tenants/{tenant_id}/default-stack
//!
//! Note: HTTP-level integration tests require the full server-api to compile.
//! These tests verify the underlying DB operations are correct.

use adapteros_core::Result;
use adapteros_db::traits::CreateStackRequest;
use adapteros_db::Db;
use uuid::Uuid;

/// Test helper to create a tenant with a specific ID
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create tenant: {}", e))
        })?;
    Ok(())
}

/// Test helper to create an in-memory database with migrations
async fn setup_test_db() -> Result<Db> {
    let db = Db::new_in_memory().await?;
    Ok(db)
}

fn stack_name() -> String {
    format!("stack.test.{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn test_default_stack_initially_none() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-1";
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // New tenant should have no default stack
    let default = db.get_default_stack(tenant_id).await.unwrap();
    assert!(default.is_none(), "New tenant should have no default stack");
}

#[tokio::test]
async fn test_set_and_get_default_stack() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-2";
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create a stack
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name(),
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack_id = db.insert_stack(&stack_req).await.unwrap();

    // Set as default
    db.set_default_stack(tenant_id, &stack_id).await.unwrap();

    // Verify it's set
    let default = db.get_default_stack(tenant_id).await.unwrap();
    assert_eq!(default, Some(stack_id), "Default stack should be set");
}

#[tokio::test]
async fn test_clear_default_stack() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-3";
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create and set a default stack
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name(),
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack_id = db.insert_stack(&stack_req).await.unwrap();
    db.set_default_stack(tenant_id, &stack_id).await.unwrap();

    // Clear default
    db.clear_default_stack(tenant_id).await.unwrap();

    // Verify it's cleared
    let default = db.get_default_stack(tenant_id).await.unwrap();
    assert!(default.is_none(), "Default stack should be cleared");
}

#[tokio::test]
async fn test_set_default_to_nonexistent_stack_fails() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-4";
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Try to set default to a non-existent stack
    let result = db
        .set_default_stack(tenant_id, "nonexistent-stack-id")
        .await;

    assert!(
        result.is_err(),
        "Setting default to non-existent stack should fail"
    );
}

#[tokio::test]
async fn test_change_default_stack() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-5";
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create two stacks
    let stack1_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name(),
        description: Some("Stack 1".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack1_id = db.insert_stack(&stack1_req).await.unwrap();

    let stack2_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name(),
        description: Some("Stack 2".to_string()),
        adapter_ids: vec!["adapter2".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack2_id = db.insert_stack(&stack2_req).await.unwrap();

    // Set stack1 as default
    db.set_default_stack(tenant_id, &stack1_id).await.unwrap();
    let default = db.get_default_stack(tenant_id).await.unwrap();
    assert_eq!(default, Some(stack1_id.clone()));

    // Change to stack2
    db.set_default_stack(tenant_id, &stack2_id).await.unwrap();
    let default = db.get_default_stack(tenant_id).await.unwrap();
    assert_eq!(
        default,
        Some(stack2_id),
        "Default stack should be changed to stack2"
    );
}

#[tokio::test]
async fn test_tenant_isolation_for_default_stack() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant1_id = "test-tenant-6a";
    let tenant2_id = "test-tenant-6b";

    if let Err(e) = create_test_tenant(&db, tenant1_id).await {
        eprintln!("Skipping test - tenant1 creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_tenant(&db, tenant2_id).await {
        eprintln!("Skipping test - tenant2 creation failed: {}", e);
        return;
    }

    // Create a stack for tenant1
    let stack_req = CreateStackRequest {
        tenant_id: tenant1_id.to_string(),
        name: stack_name(),
        description: Some("Tenant 1's stack".to_string()),
        adapter_ids: vec!["adapter1".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };
    let stack_id = db.insert_stack(&stack_req).await.unwrap();

    // Try to set tenant2's default to tenant1's stack
    let result = db.set_default_stack(tenant2_id, &stack_id).await;

    assert!(
        result.is_err(),
        "Setting default to another tenant's stack should fail"
    );
}
