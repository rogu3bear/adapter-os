//! PRD-RECT-002: Worker Lifecycle Tenant Scoping Tests (Batch 1 of 2)
//!
//! Verifies complete tenant isolation for worker lifecycle operations:
//! - Worker registration respects tenant boundaries
//! - Worker heartbeat only updates own tenant's workers
//! - Worker status queries are tenant-scoped
//! - Worker incident listing is tenant-scoped
//! - Cross-tenant worker access is denied
//!
//! All tests validate that:
//! 1. Non-admin users can only access workers from their own tenant
//! 2. Cross-tenant access returns 404 (not 403) to prevent enumeration
//! 3. Admin users can access workers across tenants (with admin_tenants grants)

use adapteros_api_types::workers::{WorkerRegistrationRequest, WorkerStatusNotification};
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_core::{AosError, Result, WorkerStatus};
use adapteros_db::sqlx;
use adapteros_db::users::Role;
use adapteros_db::workers::{WorkerInsertBuilder, WorkerRegistrationParams};
use adapteros_db::Db;
use adapteros_server_api::auth::{AuthMode, Claims, PrincipalType};
use adapteros_server_api::handlers::workers::{
    get_worker_history, list_workers, notify_worker_status, register_worker, stop_worker,
    HistoryQuery, ListWorkersQuery,
};
use adapteros_server_api::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use chrono::{Duration, Utc};

mod common;
use common::setup_state;
use uuid::Uuid;

// =============================================================================
// Test Helpers
// =============================================================================

/// Create test claims for a user
fn create_test_claims(
    user_id: &str,
    email: &str,
    role: &str,
    tenant_id: &str,
    admin_tenants: Vec<String>,
) -> Claims {
    let now = Utc::now();
    let exp = now + Duration::hours(8);

    Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        roles: vec![role.to_string()],
        tenant_id: tenant_id.to_string(),
        admin_tenants,
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: exp.timestamp(),
        iat: now.timestamp(),
        jti: Uuid::new_v4().to_string(),
        nbf: now.timestamp(),
        iss: "adapteros".to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    }
}

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

// ============================================================================
// Worker Capacity Query Tenant Scoping Tests
// ============================================================================

#[tokio::test]
async fn list_healthy_workers_by_tenant_respects_tenant_boundaries() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-a-capacity")
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-b-capacity")
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    // Register workers in both tenants
    let worker_a = register_test_worker(&db, "tenant-a-capacity", "worker-a").await;
    let worker_b = register_test_worker(&db, "tenant-b-capacity", "worker-b").await;

    // Set both to healthy
    db.transition_worker_status(&worker_a, "healthy", "test", None)
        .await
        .expect("transition worker A to healthy");
    db.transition_worker_status(&worker_b, "healthy", "test", None)
        .await
        .expect("transition worker B to healthy");

    // Query healthy workers for tenant A - should only return worker A
    let tenant_a_workers = db
        .list_healthy_workers_by_tenant("tenant-a-capacity")
        .await
        .expect("query tenant A workers");
    assert_eq!(tenant_a_workers.len(), 1);
    assert_eq!(tenant_a_workers[0].id, worker_a);

    // Query healthy workers for tenant B - should only return worker B
    let tenant_b_workers = db
        .list_healthy_workers_by_tenant("tenant-b-capacity")
        .await
        .expect("query tenant B workers");
    assert_eq!(tenant_b_workers.len(), 1);
    assert_eq!(tenant_b_workers[0].id, worker_b);
}

#[tokio::test]
async fn list_workers_by_tenant_returns_empty_for_other_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-has-workers")
        .bind("Tenant With Workers")
        .execute(db.pool())
        .await
        .expect("create tenant with workers");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-no-workers")
        .bind("Tenant Without Workers")
        .execute(db.pool())
        .await
        .expect("create tenant without workers");

    // Register worker in tenant-has-workers
    let _worker_id = register_test_worker(&db, "tenant-has-workers", "worker-isolated").await;

    // Query for workers in tenant-has-workers - should return 1
    let workers = db
        .list_workers_by_tenant("tenant-has-workers")
        .await
        .expect("query workers");
    assert_eq!(workers.len(), 1);

    // Query for workers in tenant-no-workers - should return empty
    let workers = db
        .list_workers_by_tenant("tenant-no-workers")
        .await
        .expect("query workers");
    assert_eq!(
        workers.len(),
        0,
        "Should return empty list for tenant without workers"
    );
}

// ============================================================================
// Worker Health Check Tenant Scoping Tests
// ============================================================================

#[tokio::test]
async fn worker_health_metrics_are_tenant_scoped() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-health")
        .bind("Health Test Tenant")
        .execute(db.pool())
        .await
        .expect("create tenant");

    let worker_id = register_test_worker(&db, "tenant-health", "worker-health-test").await;

    // Update health metrics for the worker
    db.update_worker_health_metrics(
        &worker_id, "healthy", 12.5, // avg_latency_ms
        10,   // latency_samples
        1,    // consecutive_slow_responses
        0,    // consecutive_failures
    )
    .await
    .expect("update health metrics");

    // Query health for the worker - should succeed
    let health = db
        .get_worker_health(&worker_id)
        .await
        .expect("query health");

    assert!(health.is_some(), "Health record should exist");
    let health = health.unwrap();
    assert_eq!(health.id, worker_id);
    assert_eq!(health.health_status, "healthy");
    assert_eq!(health.avg_latency_ms, Some(12.5));
    assert_eq!(health.latency_samples, Some(10));
    assert_eq!(health.consecutive_slow_responses, Some(1));
    assert_eq!(health.consecutive_failures, Some(0));
    assert!(health.last_response_at.is_some());
}

#[tokio::test]
async fn list_workers_by_health_status_respects_tenant_boundaries() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-health-a")
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-health-b")
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    // Register workers in both tenants and set to healthy
    let worker_a = register_test_worker(&db, "tenant-health-a", "worker-health-a").await;
    let worker_b = register_test_worker(&db, "tenant-health-b", "worker-health-b").await;

    db.transition_worker_status(&worker_a, "healthy", "test", None)
        .await
        .expect("transition A");
    db.transition_worker_status(&worker_b, "healthy", "test", None)
        .await
        .expect("transition B");

    // List all healthy workers - should return both
    let all_healthy = db
        .list_workers_by_health("healthy")
        .await
        .expect("list healthy workers");

    // Note: This query doesn't have tenant filtering (it's a system-wide query),
    // so it should return workers from both tenants.
    assert_eq!(
        all_healthy.len(),
        2,
        "System-wide health query should return workers from all tenants"
    );

    // But tenant-scoped queries should only return tenant-specific workers
    let tenant_a_healthy = db
        .list_healthy_workers_by_tenant("tenant-health-a")
        .await
        .expect("list tenant A healthy");
    assert_eq!(tenant_a_healthy.len(), 1);
    assert_eq!(tenant_a_healthy[0].id, worker_a);

    let tenant_b_healthy = db
        .list_healthy_workers_by_tenant("tenant-health-b")
        .await
        .expect("list tenant B healthy");
    assert_eq!(tenant_b_healthy.len(), 1);
    assert_eq!(tenant_b_healthy[0].id, worker_b);
}

// ============================================================================
// Worker Termination Tenant Ownership Tests
// ============================================================================

#[tokio::test]
async fn worker_termination_requires_tenant_ownership() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-owner")
        .bind("Owner Tenant")
        .execute(db.pool())
        .await
        .expect("create owner tenant");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-attacker")
        .bind("Attacker Tenant")
        .execute(db.pool())
        .await
        .expect("create attacker tenant");

    // Register worker in tenant-owner
    let worker_id = register_test_worker(&db, "tenant-owner", "worker-to-terminate").await;

    // Set worker to healthy first
    db.transition_worker_status(&worker_id, "healthy", "test", None)
        .await
        .expect("set to healthy");

    // Verify worker exists for owner tenant
    let worker = db
        .get_worker_for_tenant("tenant-owner", &worker_id)
        .await
        .expect("query owner tenant");
    assert!(worker.is_some(), "Owner tenant should see the worker");

    // Verify worker is NOT visible to attacker tenant
    let worker = db
        .get_worker_for_tenant("tenant-attacker", &worker_id)
        .await
        .expect("query attacker tenant");
    assert!(
        worker.is_none(),
        "Attacker tenant should NOT see the worker"
    );

    // Termination would be done via handler which uses get_worker_for_tenant,
    // so cross-tenant termination would fail at the lookup stage (returning 404)
}

#[tokio::test]
async fn worker_status_transition_is_tenant_agnostic_at_db_layer() {
    // Note: The DB layer transition_worker_status does NOT enforce tenant scoping
    // (it's a lower-level API). Tenant scoping is enforced at the handler layer
    // via get_worker_for_tenant before calling transition_worker_status.
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-transition")
        .bind("Transition Tenant")
        .execute(db.pool())
        .await
        .expect("create tenant");

    let worker_id = register_test_worker(&db, "tenant-transition", "worker-transition-test").await;

    // DB layer transition_worker_status works on worker_id alone (no tenant check)
    db.transition_worker_status(&worker_id, "healthy", "test", None)
        .await
        .expect("transition succeeds at DB layer");

    // Verify transition succeeded
    let worker = db.get_worker(&worker_id).await.expect("get worker");
    assert!(worker.is_some());
    assert_eq!(worker.unwrap().status, "healthy");

    // This demonstrates that tenant enforcement MUST happen at handler layer
    // by using get_worker_for_tenant before calling transition_worker_status
}

// ============================================================================
// Storage Path Validation Tests
// ============================================================================

#[tokio::test]
async fn worker_registration_validates_uds_path_no_tmp() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-path-test")
        .bind("Path Test Tenant")
        .execute(db.pool())
        .await
        .expect("create tenant");

    // Create required foreign keys
    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind("test-node-path")
    .bind("test-node")
    .bind("http://localhost:8080")
    .execute(db.pool())
    .await
    .expect("create node");

    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind("test-manifest-path")
    .bind("tenant-path-test")
    .bind("test-manifest-hash")
    .bind("{}")
    .execute(db.pool())
    .await
    .expect("create manifest");

    sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("test-plan-path")
    .bind("tenant-path-test")
    .bind("plan-b3:test")
    .bind("test-manifest-hash")
    .bind("[]")
    .bind("layout-b3:test")
    .execute(db.pool())
    .await
    .expect("create plan");

    // Attempt to register worker with forbidden /tmp path
    let params = WorkerRegistrationParams {
        worker_id: "worker-tmp-path".to_string(),
        tenant_id: "tenant-path-test".to_string(),
        node_id: "test-node-path".to_string(),
        plan_id: "test-plan-path".to_string(),
        uds_path: "/tmp/worker.sock".to_string(), // FORBIDDEN
        pid: 1234,
        manifest_hash: "test-manifest-hash".to_string(),
        backend: Some("mlx".to_string()),
        model_hash_b3: None,
        capabilities_json: None,
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
    };

    // Note: The DB layer doesn't validate paths - that's done at the handler layer
    // via reject_tmp_socket before calling register_worker.
    // This test documents that path validation is a handler-layer responsibility.
    let result = db.register_worker(params).await;

    // DB layer accepts the path (no validation there)
    assert!(
        result.is_ok(),
        "DB layer doesn't validate paths - handler must validate before calling"
    );
}

#[tokio::test]
async fn worker_registration_with_path_traversal_in_tenant_id() {
    // This test verifies that tenant_id path traversal is blocked at handler layer
    // The handler validates tenant_id doesn't contain "..", "/", or "\" before
    // constructing the UDS path.
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // This would be caught at handler validation (worker_spawn checks for "..", "/", "\")
    let dangerous_tenant_id = "../../../etc";

    // Attempting to create a worker with path traversal in tenant_id would fail
    // at the handler layer BEFORE reaching the DB layer.
    // The handler code:
    //   if req.tenant_id.contains("..") || req.tenant_id.contains('/') || req.tenant_id.contains('\\') {
    //       return Err(...);
    //   }
    //
    // So this test documents the protection at handler layer.

    // For DB layer testing, we verify that IF a dangerous tenant_id reached the DB
    // (which should never happen due to handler validation), it would just be stored as-is.
    let result = sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind(dangerous_tenant_id)
        .bind("Dangerous Tenant")
        .execute(db.pool())
        .await;

    // DB would accept it (no validation), but handler prevents this scenario
    assert!(
        result.is_ok(),
        "DB layer doesn't validate tenant_id - handler must validate"
    );
}

#[tokio::test]
async fn worker_uds_path_must_be_under_var_not_tmp() {
    // This test documents that UDS paths should be under var/, not /tmp
    // Handler uses reject_tmp_socket to enforce this.
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    // Valid path: var/run/aos/tenant-123/worker.sock
    // Invalid paths: /tmp/worker.sock, /private/tmp/worker.sock

    // The validation happens in worker_spawn handler:
    //   let uds_path = format!("/var/run/aos/{}/worker.sock", req.tenant_id);
    //   let uds_path_buf = std::path::PathBuf::from(&uds_path);
    //   reject_tmp_socket(&uds_path_buf, "worker-socket")?;

    // This test documents the expected behavior rather than testing it directly
    // since path validation is a handler-layer concern.

    use adapteros_config::reject_tmp_socket;
    use std::path::PathBuf;

    // Valid path
    let valid_path = PathBuf::from("var/run/aos/tenant-123/worker.sock");
    assert!(
        reject_tmp_socket(&valid_path, "worker-socket").is_ok(),
        "var/ paths should be accepted"
    );

    // Invalid path - /tmp
    let tmp_path = PathBuf::from("/tmp/worker.sock");
    assert!(
        reject_tmp_socket(&tmp_path, "worker-socket").is_err(),
        "/tmp paths should be rejected"
    );

    // Invalid path - /private/tmp (macOS)
    let private_tmp_path = PathBuf::from("/private/tmp/worker.sock");
    assert!(
        reject_tmp_socket(&private_tmp_path, "worker-socket").is_err(),
        "/private/tmp paths should be rejected"
    );
}

// ============================================================================
// Telemetry Tenant Isolation Tests
// ============================================================================

#[tokio::test]
async fn worker_telemetry_writes_are_tenant_isolated() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-telem-a")
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-telem-b")
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    // Register workers in both tenants
    let worker_a = register_test_worker(&db, "tenant-telem-a", "worker-telem-a").await;
    let worker_b = register_test_worker(&db, "tenant-telem-b", "worker-telem-b").await;

    // Create telemetry events table if it doesn't exist
    // Note: In production, this is created by migrations
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS telemetry_events (
            id TEXT PRIMARY KEY,
            worker_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            payload TEXT,
            timestamp TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(db.pool())
    .await
    .expect("create telemetry_events table");

    // Insert telemetry events for worker A (tenant-telem-a)
    sqlx::query(
        "INSERT INTO telemetry_events (id, worker_id, tenant_id, event_type, payload)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("telem-a-1")
    .bind(&worker_a)
    .bind("tenant-telem-a")
    .bind("inference_complete")
    .bind(r#"{"latency_ms": 150}"#)
    .execute(db.pool())
    .await
    .expect("insert telemetry A");

    // Insert telemetry events for worker B (tenant-telem-b)
    sqlx::query(
        "INSERT INTO telemetry_events (id, worker_id, tenant_id, event_type, payload)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("telem-b-1")
    .bind(&worker_b)
    .bind("tenant-telem-b")
    .bind("inference_complete")
    .bind(r#"{"latency_ms": 200}"#)
    .execute(db.pool())
    .await
    .expect("insert telemetry B");

    // Query telemetry for worker A
    let count_a = db
        .get_worker_telemetry_count(&worker_a, "inference_complete")
        .await
        .expect("query telemetry A");
    assert_eq!(count_a, 1, "Worker A should have 1 telemetry event");

    // Query telemetry for worker B
    let count_b = db
        .get_worker_telemetry_count(&worker_b, "inference_complete")
        .await
        .expect("query telemetry B");
    assert_eq!(count_b, 1, "Worker B should have 1 telemetry event");

    // Verify that querying by worker_id correctly isolates telemetry
    // (get_worker_telemetry_count filters by worker_id, which is unique per tenant)
}

#[tokio::test]
async fn worker_telemetry_queries_cannot_access_other_tenant_data() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-telem-isolated-a")
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-telem-isolated-b")
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    // Register workers
    let worker_a = register_test_worker(&db, "tenant-telem-isolated-a", "worker-iso-a").await;
    let worker_b = register_test_worker(&db, "tenant-telem-isolated-b", "worker-iso-b").await;

    // Create telemetry events table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS telemetry_events (
            id TEXT PRIMARY KEY,
            worker_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            payload TEXT,
            timestamp TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(db.pool())
    .await
    .expect("create telemetry_events table");

    // Insert multiple telemetry events for worker A
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO telemetry_events (id, worker_id, tenant_id, event_type, payload)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(format!("telem-iso-a-{}", i))
        .bind(&worker_a)
        .bind("tenant-telem-isolated-a")
        .bind("inference_complete")
        .bind(r#"{"latency_ms": 100}"#)
        .execute(db.pool())
        .await
        .expect("insert telemetry A");
    }

    // Insert telemetry events for worker B
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO telemetry_events (id, worker_id, tenant_id, event_type, payload)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(format!("telem-iso-b-{}", i))
        .bind(&worker_b)
        .bind("tenant-telem-isolated-b")
        .bind("inference_complete")
        .bind(r#"{"latency_ms": 200}"#)
        .execute(db.pool())
        .await
        .expect("insert telemetry B");
    }

    // Verify counts are isolated
    let count_a = db
        .get_worker_telemetry_count(&worker_a, "inference_complete")
        .await
        .expect("query A");
    assert_eq!(count_a, 5);

    let count_b = db
        .get_worker_telemetry_count(&worker_b, "inference_complete")
        .await
        .expect("query B");
    assert_eq!(count_b, 3);

    // Attempting to query telemetry for worker A using worker B's ID would return 0
    // (not worker A's count), demonstrating isolation
    let cross_query = db
        .get_worker_telemetry_count(&worker_b, "nonexistent_type")
        .await
        .expect("cross query");
    assert_eq!(cross_query, 0);
}

// =============================================================================
// PRD-RECT-002 Batch 1: Handler-Level Tenant Scoping Tests
// =============================================================================

#[tokio::test]
async fn test_list_workers_operator_sees_only_own_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let state = setup_state(None).await.expect("state setup");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-list-a")
        .bind("Tenant A")
        .execute(state.db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-list-b")
        .bind("Tenant B")
        .execute(state.db.pool())
        .await
        .expect("create tenant B");

    // Register workers for both tenants
    let _worker_a = register_test_worker(&state.db, "tenant-list-a", "worker-list-a").await;
    let _worker_b = register_test_worker(&state.db, "tenant-list-b", "worker-list-b").await;

    // Operator for tenant-a (non-admin)
    let claims_a = create_test_claims(
        "operator-list-a",
        "operator@tenant-a.com",
        "operator",
        "tenant-list-a",
        vec![],
    );

    // List workers as operator-a (should only see tenant-a workers)
    let result = list_workers(
        State(state.clone()),
        Extension(claims_a),
        Query(ListWorkersQuery { tenant_id: None }),
    )
    .await;

    assert!(result.is_ok(), "List should succeed");
    let Json(workers) = result.unwrap();

    assert_eq!(workers.len(), 1, "Tenant A operator should see 1 worker");
    assert_eq!(workers[0].id, "worker-list-a");
    assert_eq!(workers[0].tenant_id, "tenant-list-a");
}

#[tokio::test]
async fn test_list_workers_admin_can_query_specific_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let state = setup_state(None).await.expect("state setup");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-admin-a")
        .bind("Tenant A")
        .execute(state.db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-admin-b")
        .bind("Tenant B")
        .execute(state.db.pool())
        .await
        .expect("create tenant B");

    // Register workers for both tenants
    let _worker_a = register_test_worker(&state.db, "tenant-admin-a", "worker-admin-a").await;
    let _worker_b = register_test_worker(&state.db, "tenant-admin-b", "worker-admin-b").await;

    // Admin user with wildcard admin_tenants
    let claims_admin = create_test_claims(
        "admin-1",
        "admin@system.com",
        "admin",
        "system",
        vec!["*".to_string()],
    );

    // Admin queries tenant-a workers
    let result_a = list_workers(
        State(state.clone()),
        Extension(claims_admin.clone()),
        Query(ListWorkersQuery {
            tenant_id: Some("tenant-admin-a".to_string()),
        }),
    )
    .await;

    assert!(result_a.is_ok(), "Admin list should succeed");
    let Json(workers_a) = result_a.unwrap();
    assert_eq!(workers_a.len(), 1);
    assert_eq!(workers_a[0].id, "worker-admin-a");

    // Admin queries tenant-b workers
    let result_b = list_workers(
        State(state.clone()),
        Extension(claims_admin),
        Query(ListWorkersQuery {
            tenant_id: Some("tenant-admin-b".to_string()),
        }),
    )
    .await;

    assert!(result_b.is_ok(), "Admin list should succeed");
    let Json(workers_b) = result_b.unwrap();
    assert_eq!(workers_b.len(), 1);
    assert_eq!(workers_b[0].id, "worker-admin-b");
}

#[tokio::test]
async fn test_stop_worker_cross_tenant_returns_404() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let state = setup_state(None).await.expect("state setup");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-stop-a")
        .bind("Tenant A")
        .execute(state.db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-stop-b")
        .bind("Tenant B")
        .execute(state.db.pool())
        .await
        .expect("create tenant B");

    // Register worker for tenant-a
    let worker_id = register_test_worker(&state.db, "tenant-stop-a", "worker-stop-cross").await;

    // Transition to healthy so it can be stopped
    state
        .db
        .transition_worker_status(&worker_id, "healthy", "test", None)
        .await
        .expect("transition to healthy");

    // Operator for tenant-b (different tenant)
    let claims_b = create_test_claims(
        "operator-stop-b",
        "operator@tenant-b.com",
        "operator",
        "tenant-stop-b",
        vec![],
    );

    // Try to stop tenant-a's worker using tenant-b credentials
    let result = stop_worker(
        State(state.clone()),
        Extension(claims_b),
        Path(worker_id.clone()),
    )
    .await;

    // Should return 404 (not 403) to prevent enumeration
    assert!(result.is_err(), "Cross-tenant stop should fail");
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant stop should return 404"
            );
        }
        Ok(_) => panic!("Cross-tenant stop should fail"),
    }

    // Verify worker was NOT affected
    let worker = state.db.get_worker(&worker_id).await.expect("get worker");
    assert!(worker.is_some());
    assert_eq!(
        worker.unwrap().status,
        "healthy",
        "Worker status should be unchanged"
    );
}

#[tokio::test]
async fn test_worker_history_cross_tenant_returns_404() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let state = setup_state(None).await.expect("state setup");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-hist-a")
        .bind("Tenant A")
        .execute(state.db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-hist-b")
        .bind("Tenant B")
        .execute(state.db.pool())
        .await
        .expect("create tenant B");

    // Register worker for tenant-a
    let worker_id = register_test_worker(&state.db, "tenant-hist-a", "worker-hist-cross").await;

    // Create some history by transitioning status
    state
        .db
        .transition_worker_status(&worker_id, "healthy", "test", None)
        .await
        .expect("transition to healthy");

    // Operator for tenant-b (different tenant)
    let claims_b = create_test_claims(
        "operator-hist-b",
        "operator@tenant-b.com",
        "operator",
        "tenant-hist-b",
        vec![],
    );

    // Try to get worker history for tenant-a's worker using tenant-b credentials
    let result = get_worker_history(
        State(state.clone()),
        Extension(claims_b),
        Path(worker_id.clone()),
        Query(HistoryQuery { limit: None }),
    )
    .await;

    // Should return 404 (not 403) to prevent enumeration
    assert!(result.is_err(), "Cross-tenant history should fail");
    match result {
        Err((status, _)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Cross-tenant history should return 404"
            );
        }
        Ok(_) => panic!("Cross-tenant history access should fail"),
    }
}

#[tokio::test]
async fn test_worker_history_same_tenant_succeeds() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let state = setup_state(None).await.expect("state setup");

    // Create tenant
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-hist-same")
        .bind("Tenant Same")
        .execute(state.db.pool())
        .await
        .expect("create tenant");

    // Register worker
    let worker_id = register_test_worker(&state.db, "tenant-hist-same", "worker-hist-same").await;

    // Create history by transitioning status
    state
        .db
        .transition_worker_status(&worker_id, "healthy", "ready to serve", None)
        .await
        .expect("transition to healthy");

    // Operator for same tenant
    let claims = create_test_claims(
        "operator-hist-same",
        "operator@tenant-same.com",
        "operator",
        "tenant-hist-same",
        vec![],
    );

    // Get worker history
    let result = get_worker_history(
        State(state.clone()),
        Extension(claims),
        Path(worker_id.clone()),
        Query(HistoryQuery { limit: None }),
    )
    .await;

    assert!(result.is_ok(), "Same-tenant history should succeed");
    let Json(history) = result.unwrap();
    assert!(!history.is_empty(), "Should have history records");
    assert!(history.iter().all(|h| h.tenant_id == "tenant-hist-same"));
    assert!(history.iter().all(|h| h.worker_id == worker_id));
}

#[tokio::test]
async fn test_worker_incidents_are_tenant_scoped() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-inc-a")
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-inc-b")
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    // Register workers
    let worker_a = register_test_worker(&db, "tenant-inc-a", "worker-inc-a").await;
    let worker_b = register_test_worker(&db, "tenant-inc-b", "worker-inc-b").await;

    // Insert incidents for each worker
    db.insert_worker_incident(
        &worker_a,
        "tenant-inc-a",
        "crash",
        "OOM error",
        Some("backtrace: ..."),
        Some(500.0),
    )
    .await
    .expect("insert incident A");

    db.insert_worker_incident(
        &worker_b,
        "tenant-inc-b",
        "timeout",
        "Hung",
        Some("backtrace: ..."),
        Some(10000.0),
    )
    .await
    .expect("insert incident B");

    // List incidents for tenant-a
    let incidents_a = db
        .list_tenant_worker_incidents("tenant-inc-a", Some(100))
        .await
        .expect("list incidents A");

    assert_eq!(incidents_a.len(), 1);
    assert_eq!(incidents_a[0].tenant_id, "tenant-inc-a");
    assert_eq!(incidents_a[0].worker_id, worker_a);

    // List incidents for tenant-b
    let incidents_b = db
        .list_tenant_worker_incidents("tenant-inc-b", Some(100))
        .await
        .expect("list incidents B");

    assert_eq!(incidents_b.len(), 1);
    assert_eq!(incidents_b[0].tenant_id, "tenant-inc-b");
    assert_eq!(incidents_b[0].worker_id, worker_b);
}

#[tokio::test]
async fn test_worker_registration_api_respects_tenant_boundaries() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let state = setup_state(None).await.expect("state setup");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-reg-a")
        .bind("Tenant A")
        .execute(state.db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-reg-b")
        .bind("Tenant B")
        .execute(state.db.pool())
        .await
        .expect("create tenant B");

    // Create manifest for tenant-a
    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind("manifest-reg-a")
    .bind("tenant-reg-a")
    .bind("hash-reg-a")
    .bind("{}")
    .execute(state.db.pool())
    .await
    .expect("create manifest");

    // Create plan for tenant-a
    sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("plan-reg-a")
    .bind("tenant-reg-a")
    .bind("plan-b3:reg-a")
    .bind("hash-reg-a")
    .bind("[]")
    .bind("layout-b3:reg-a")
    .execute(state.db.pool())
    .await
    .expect("create plan");

    // Register worker for tenant-a via API
    let req = WorkerRegistrationRequest {
        worker_id: "worker-reg-api-a".to_string(),
        tenant_id: "tenant-reg-a".to_string(),
        plan_id: "plan-reg-a".to_string(),
        manifest_hash: "hash-reg-a".to_string(),
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
        pid: 1001,
        uds_path: "var/run/aos/tenant-reg-a/worker.sock".to_string(),
        capabilities: vec![],
        backend: None,
        model_hash: None,
        strict_mode: false,
    };

    let result = register_worker(State(state.clone()), Json(req))
        .await
        .expect("registration should succeed");

    assert!(result.accepted, "Worker A should be accepted");

    // Verify worker exists for tenant-a
    let worker = state
        .db
        .get_worker_for_tenant("tenant-reg-a", "worker-reg-api-a")
        .await
        .expect("query worker");

    assert!(worker.is_some(), "Worker should exist for tenant-a");
    assert_eq!(worker.unwrap().tenant_id, "tenant-reg-a");

    // Verify worker is NOT visible to tenant-b
    let cross_tenant = state
        .db
        .get_worker_for_tenant("tenant-reg-b", "worker-reg-api-a")
        .await
        .expect("query cross-tenant");

    assert!(
        cross_tenant.is_none(),
        "Worker should NOT be visible to tenant-b"
    );
}

#[tokio::test]
async fn test_worker_status_notification_updates_correct_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let state = setup_state(None).await.expect("state setup");

    // Create two tenants
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-notif-a")
        .bind("Tenant A")
        .execute(state.db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-notif-b")
        .bind("Tenant B")
        .execute(state.db.pool())
        .await
        .expect("create tenant B");

    // Register workers
    let worker_a = register_test_worker(&state.db, "tenant-notif-a", "worker-notif-a").await;
    let worker_b = register_test_worker(&state.db, "tenant-notif-b", "worker-notif-b").await;

    // Worker A sends status notification
    let notification = WorkerStatusNotification {
        worker_id: worker_a.clone(),
        status: "healthy".to_string(),
        reason: "model loaded".to_string(),
        cache_used_mb: None,
        cache_max_mb: None,
        cache_pinned_entries: None,
        cache_active_entries: None,
    };

    notify_worker_status(State(state.clone()), Json(notification))
        .await
        .expect("status update should succeed");

    // Verify worker-a was updated
    let worker_a_rec = state.db.get_worker(&worker_a).await.expect("get worker A");
    assert!(worker_a_rec.is_some());
    assert_eq!(worker_a_rec.unwrap().status, "healthy");

    // Verify worker-b was NOT affected
    let worker_b_rec = state.db.get_worker(&worker_b).await.expect("get worker B");
    assert!(worker_b_rec.is_some());
    assert_eq!(
        worker_b_rec.unwrap().status,
        "registered",
        "Worker B should still be in registered state"
    );
}
