//! PRD-RECT-002: Worker Lifecycle Tenant Scoping Tests
//!
//! These tests validate that worker lifecycle operations are properly
//! tenant-scoped, returning 404 for cross-tenant access.

use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_db::Db;

mod common;
use common::{setup_state, test_admin_claims, test_viewer_claims};

/// Helper to register a test worker
async fn register_test_worker(db: &Db, tenant_id: &str, worker_id: &str) -> String {
    // First ensure we have required foreign keys
    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind("test-node-worker-tenant")
    .bind("test-node")
    .bind("http://localhost:8080")
    .execute(db.pool())
    .await
    .expect("create node");

    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind("test-manifest-worker-tenant")
    .bind(tenant_id)
    .bind("test-manifest-hash")
    .bind("{}")
    .execute(db.pool())
    .await
    .expect("create manifest");

    sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("test-plan-worker-tenant")
    .bind(tenant_id)
    .bind("plan-b3:test-plan")
    .bind("test-manifest-hash")
    .bind("[]")
    .bind("layout-b3:test")
    .execute(db.pool())
    .await
    .expect("create plan");

    let params = WorkerRegistrationParams {
        worker_id: worker_id.to_string(),
        tenant_id: tenant_id.to_string(),
        node_id: "test-node-worker-tenant".to_string(),
        plan_id: "test-plan-worker-tenant".to_string(),
        uds_path: format!("var/run/{}/worker.sock", worker_id),
        pid: 1234,
        manifest_hash: "test-manifest-hash".to_string(),
        backend: Some("mlx".to_string()),
        model_hash_b3: None,
        capabilities_json: None,
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
    };

    db.register_worker(params).await.expect("register worker");
    worker_id.to_string()
}

// ============================================================================
// PRD-RECT-002: Tenant-Scoped Worker Query Tests
// ============================================================================

#[tokio::test]
async fn get_worker_for_tenant_returns_worker_for_same_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create test tenant
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-worker-test")
        .bind("Worker Test Tenant")
        .execute(db.pool())
        .await
        .expect("create tenant");

    let worker_id = register_test_worker(&db, "tenant-worker-test", "worker-same-tenant").await;

    // Query for worker with matching tenant - should succeed
    let worker = db
        .get_worker_for_tenant("tenant-worker-test", &worker_id)
        .await
        .expect("query worker");

    assert!(worker.is_some(), "Worker should be found for same tenant");
    assert_eq!(worker.unwrap().id, worker_id);
}

#[tokio::test]
async fn get_worker_for_tenant_returns_none_for_different_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-a-worker")
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-b-worker")
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    // Register worker in tenant A
    let worker_id = register_test_worker(&db, "tenant-a-worker", "worker-tenant-a").await;

    // Query for worker from tenant B - should return None (not error)
    let worker = db
        .get_worker_for_tenant("tenant-b-worker", &worker_id)
        .await
        .expect("query should succeed without error");

    assert!(
        worker.is_none(),
        "Worker should NOT be found for different tenant"
    );
}

#[tokio::test]
async fn get_worker_for_tenant_returns_none_for_nonexistent_worker() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-no-worker")
        .bind("Tenant No Worker")
        .execute(db.pool())
        .await
        .expect("create tenant");

    // Query for nonexistent worker
    let worker = db
        .get_worker_for_tenant("tenant-no-worker", "nonexistent-worker-id")
        .await
        .expect("query should succeed without error");

    assert!(worker.is_none(), "Nonexistent worker should return None");
}

#[tokio::test]
async fn cross_tenant_worker_access_indistinguishable_from_not_found() {
    // This test ensures that cross-tenant access and "not found" responses
    // are indistinguishable, preventing tenant enumeration attacks
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-enum-test-a")
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-enum-test-b")
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    // Register worker in tenant A
    let existing_worker_id =
        register_test_worker(&db, "tenant-enum-test-a", "worker-enum-test").await;

    // Both of these should return None:
    // 1. Cross-tenant access (worker exists but wrong tenant)
    let cross_tenant_result = db
        .get_worker_for_tenant("tenant-enum-test-b", &existing_worker_id)
        .await
        .expect("query 1");

    // 2. Nonexistent worker
    let not_found_result = db
        .get_worker_for_tenant("tenant-enum-test-b", "completely-fake-worker-id")
        .await
        .expect("query 2");

    // Both should be None - indistinguishable
    assert!(cross_tenant_result.is_none());
    assert!(not_found_result.is_none());
}

// ============================================================================
// Worker Status Transition Validation Tests
// ============================================================================

#[tokio::test]
async fn worker_status_can_transition_to_healthy() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-status-test")
        .bind("Status Test Tenant")
        .execute(db.pool())
        .await
        .expect("create tenant");

    let worker_id = register_test_worker(&db, "tenant-status-test", "worker-status-test").await;

    // Transition to healthy
    db.transition_worker_status(&worker_id, "healthy", "test", None)
        .await
        .expect("transition to healthy");

    // Verify status
    let worker = db.get_worker(&worker_id).await.expect("get worker");
    assert!(worker.is_some());
    assert_eq!(worker.unwrap().status, "healthy");
}

#[tokio::test]
async fn worker_status_can_transition_to_draining() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-draining-test")
        .bind("Draining Test Tenant")
        .execute(db.pool())
        .await
        .expect("create tenant");

    let worker_id = register_test_worker(&db, "tenant-draining-test", "worker-draining-test").await;

    // First transition to healthy
    db.transition_worker_status(&worker_id, "healthy", "test", None)
        .await
        .expect("transition to healthy");

    // Then transition to draining (valid status)
    db.transition_worker_status(&worker_id, "draining", "test", None)
        .await
        .expect("transition to draining");

    // Verify status
    let worker = db.get_worker(&worker_id).await.expect("get worker");
    assert!(worker.is_some());
    assert_eq!(worker.unwrap().status, "draining");
}

#[tokio::test]
async fn worker_status_can_transition_to_error() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-error-test")
        .bind("Error Test Tenant")
        .execute(db.pool())
        .await
        .expect("create tenant");

    let worker_id = register_test_worker(&db, "tenant-error-test", "worker-error-test").await;

    // Transition to error
    db.transition_worker_status(&worker_id, "error", "test", None)
        .await
        .expect("transition to error");

    // Verify status
    let worker = db.get_worker(&worker_id).await.expect("get worker");
    assert!(worker.is_some());
    assert_eq!(worker.unwrap().status, "error");
}
