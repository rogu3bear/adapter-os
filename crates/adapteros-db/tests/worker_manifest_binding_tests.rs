//! Worker Manifest Binding Integration Tests (PRD-01)
//!
//! Tests for worker registration, manifest compatibility, and schema version filtering.
//!
//! Copyright JKCA | 2025 James KC Auchterlonie

mod common;

use adapteros_core::version::API_SCHEMA_VERSION;
use adapteros_db::workers::is_schema_compatible;
use adapteros_db::Db;
use common::db_helpers::create_test_db;

// Constant for the seeded tenant ID (created by seed_dev_data)
const DEFAULT_TENANT_ID: &str = "default";

/// Helper to ensure test infrastructure exists (node, plan)
async fn ensure_test_infrastructure(db: &Db, tenant_id: &str) {
    // Ensure tenant exists (if not using default)
    if tenant_id != DEFAULT_TENANT_ID {
        sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name, created_at)
             VALUES (?, ?, datetime('now'))",
        )
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(&*db.pool())
        .await
        .expect("Failed to ensure tenant exists");
    }

    // Use pre-seeded node from seed_dev_data (node-01)
    // The seed creates nodes: node-01, node-02, node-03

    // Insert manifest if not exists (required FK for plans)
    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json)
         VALUES ('test-manifest-id', ?, 'test-manifest-hash', '{}')",
    )
    .bind(DEFAULT_TENANT_ID) // manifests need valid tenant_id
    .execute(&*db.pool())
    .await
    .expect("Failed to ensure manifest exists");

    // Insert plan if not exists (required FK for workers)
    sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3)
         VALUES ('test-plan', ?, 'test-plan-hash', 'test-manifest-hash', '{}', 'layout-hash')",
    )
    .bind(DEFAULT_TENANT_ID) // plans need valid tenant_id
    .execute(&*db.pool())
    .await
    .expect("Failed to ensure plan exists");
}

/// Helper to insert a test worker directly with SQL
async fn insert_test_worker(
    db: &Db,
    worker_id: &str,
    tenant_id: &str,
    manifest_hash: &str,
    schema_version: &str,
    status: &str,
) {
    // Ensure infrastructure exists first
    ensure_test_infrastructure(db, tenant_id).await;

    // Use node-01 from seed_dev_data
    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, 
         manifest_hash_b3, schema_version, api_version, started_at, registered_at)
         VALUES (?, ?, 'node-01', 'test-plan', '/var/run/test.sock', 9999, ?, ?, ?, '1.0.0', 
         datetime('now'), datetime('now'))",
    )
    .bind(worker_id)
    .bind(tenant_id)
    .bind(status)
    .bind(manifest_hash)
    .bind(schema_version)
    .execute(&*db.pool())
    .await
    .expect("Failed to insert test worker");
}

// =========================================================================
// Worker Registration with Manifest Binding Tests
// =========================================================================

#[tokio::test]
async fn test_worker_with_binding_fields() {
    let db = create_test_db().await.expect("Failed to create test db");

    // Insert worker directly with binding info
    insert_test_worker(
        &db,
        "worker-001",
        DEFAULT_TENANT_ID, // Use seeded tenant
        "manifest-hash-abc",
        "1.0.0",
        "starting",
    )
    .await;

    // Verify worker was registered with binding info
    let worker = db
        .get_worker_with_binding("worker-001")
        .await
        .expect("Failed to get worker")
        .expect("Worker not found");

    assert_eq!(worker.id, "worker-001");
    assert_eq!(
        worker.manifest_hash_b3.as_deref(),
        Some("manifest-hash-abc")
    );
    assert_eq!(worker.schema_version.as_deref(), Some("1.0.0"));
    assert_eq!(worker.status, "starting");
}

#[tokio::test]
async fn test_list_compatible_workers_filters_by_manifest() {
    let db = create_test_db().await.expect("Failed to create test db");

    // Insert worker with correct manifest (serving status)
    insert_test_worker(
        &db,
        "worker-correct",
        DEFAULT_TENANT_ID,
        "correct-manifest",
        API_SCHEMA_VERSION,
        "serving",
    )
    .await;

    // Insert worker with wrong manifest (serving status)
    insert_test_worker(
        &db,
        "worker-wrong",
        DEFAULT_TENANT_ID,
        "wrong-manifest",
        API_SCHEMA_VERSION,
        "serving",
    )
    .await;

    // List compatible workers for correct manifest
    let compatible = db
        .list_compatible_workers("correct-manifest")
        .await
        .expect("Failed to list compatible workers");

    // Should only return the worker with correct manifest
    assert_eq!(compatible.len(), 1);
    assert_eq!(compatible[0].id, "worker-correct");
}

#[tokio::test]
async fn test_list_compatible_workers_filters_by_schema_version() {
    let db = create_test_db().await.expect("Failed to create test db");

    // Insert worker with compatible schema version (same major.minor)
    insert_test_worker(
        &db,
        "worker-compat",
        DEFAULT_TENANT_ID,
        "same-manifest",
        API_SCHEMA_VERSION, // Same as CP
        "serving",
    )
    .await;

    // Insert worker with incompatible schema version
    insert_test_worker(
        &db,
        "worker-incompat",
        DEFAULT_TENANT_ID,
        "same-manifest",
        "99.99.0", // Incompatible
        "serving",
    )
    .await;

    // List compatible workers
    let compatible = db
        .list_compatible_workers("same-manifest")
        .await
        .expect("Failed to list compatible workers");

    // Should only return the worker with compatible schema
    assert_eq!(compatible.len(), 1);
    assert_eq!(compatible[0].id, "worker-compat");
}

#[tokio::test]
async fn test_list_compatible_workers_for_tenant() {
    let db = create_test_db().await.expect("Failed to create test db");

    // Insert worker for default tenant
    insert_test_worker(
        &db,
        "worker-default",
        DEFAULT_TENANT_ID,
        "shared-manifest",
        API_SCHEMA_VERSION,
        "serving",
    )
    .await;

    // Insert worker for another tenant (ensure_test_infrastructure will create it)
    insert_test_worker(
        &db,
        "worker-other",
        "other-tenant",
        "shared-manifest",
        API_SCHEMA_VERSION,
        "serving",
    )
    .await;

    // List compatible workers for default tenant
    let compatible = db
        .list_compatible_workers_for_tenant("shared-manifest", DEFAULT_TENANT_ID)
        .await
        .expect("Failed to list compatible workers");

    // Should only return workers for default tenant
    assert_eq!(compatible.len(), 1);
    assert_eq!(compatible[0].id, "worker-default");
    assert_eq!(compatible[0].tenant_id, DEFAULT_TENANT_ID);
}

#[tokio::test]
async fn test_list_serving_workers_filters_schema() {
    let db = create_test_db().await.expect("Failed to create test db");

    // Insert worker with compatible schema
    insert_test_worker(
        &db,
        "worker-serving-compat",
        DEFAULT_TENANT_ID,
        "test-manifest",
        API_SCHEMA_VERSION,
        "serving",
    )
    .await;

    // Insert worker with incompatible schema
    insert_test_worker(
        &db,
        "worker-serving-incompat",
        DEFAULT_TENANT_ID,
        "test-manifest",
        "999.0.0", // Incompatible
        "serving",
    )
    .await;

    // List all serving workers (should filter by schema)
    let serving = db
        .list_serving_workers()
        .await
        .expect("Failed to list serving workers");

    // Should only return workers with compatible schema
    let compat_ids: Vec<_> = serving.iter().map(|w| w.id.as_str()).collect();
    assert!(compat_ids.contains(&"worker-serving-compat"));
    assert!(!compat_ids.contains(&"worker-serving-incompat"));
}

#[tokio::test]
async fn test_worker_with_null_schema_excluded() {
    let db = create_test_db().await.expect("Failed to create test db");

    // Insert worker with proper schema
    insert_test_worker(
        &db,
        "worker-with-schema",
        DEFAULT_TENANT_ID,
        "test-manifest",
        API_SCHEMA_VERSION,
        "serving",
    )
    .await;

    // Manually insert a worker without schema_version (simulating legacy worker)
    // Use node-01 (from seed) and test-plan (from ensure_test_infrastructure)
    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, manifest_hash_b3, schema_version, started_at)
         VALUES ('worker-null-schema', ?, 'node-01', 'test-plan', '/var/run/w2.sock', 5002, 'serving', 'test-manifest', NULL, datetime('now'))"
    )
    .bind(DEFAULT_TENANT_ID)
    .execute(&*db.pool())
    .await
    .expect("Failed to insert legacy worker");

    // List compatible workers
    let compatible = db
        .list_compatible_workers("test-manifest")
        .await
        .expect("Failed to list compatible workers");

    // Should only return worker with proper schema, not the NULL one
    assert_eq!(compatible.len(), 1);
    assert_eq!(compatible[0].id, "worker-with-schema");
}

// =========================================================================
// Schema Compatibility Function Tests (Unit Tests)
// =========================================================================

#[test]
fn test_is_schema_compatible_basic() {
    // Same version
    assert!(is_schema_compatible("1.0.0", "1.0.0"));

    // Patch difference (should be compatible)
    assert!(is_schema_compatible("1.0.0", "1.0.5"));
    assert!(is_schema_compatible("1.0.10", "1.0.1"));

    // Minor difference (incompatible)
    assert!(!is_schema_compatible("1.0.0", "1.1.0"));

    // Major difference (incompatible)
    assert!(!is_schema_compatible("1.0.0", "2.0.0"));
}

#[test]
fn test_is_schema_compatible_with_api_schema_version() {
    // Worker with same version as control plane should be compatible
    assert!(is_schema_compatible(API_SCHEMA_VERSION, API_SCHEMA_VERSION));

    // Worker with different patch should be compatible
    let parts: Vec<&str> = API_SCHEMA_VERSION.split('.').collect();
    if parts.len() >= 2 {
        let different_patch = format!("{}.{}.99", parts[0], parts[1]);
        assert!(is_schema_compatible(&different_patch, API_SCHEMA_VERSION));
    }
}
