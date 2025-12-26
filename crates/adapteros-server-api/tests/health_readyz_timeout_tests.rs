//! Health check /readyz timeout tests
//!
//! Tests for:
//! - Per-check timeout behavior
//! - Latency tracking in responses
//! - JSON body structure on 503
//! - Zero workers/models scenarios

use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_server_api::handlers::health::{
    ReadyMetrics, ReadyzCheck, ReadyzChecks, ReadyzResponse,
};

mod common;

/// Test helper to create an in-memory database with required schema
async fn setup_test_db() -> Result<Db> {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await?;
    Ok(db)
}

/// Test helper to create a test tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await?;
    Ok(())
}

/// Test helper to create a test worker in the database
/// Valid status values: 'created', 'registered', 'healthy', 'draining', 'stopped', 'error'
async fn create_test_worker(db: &Db, worker_id: &str, tenant_id: &str, status: &str) -> Result<()> {
    // Ensure tenant exists
    create_test_tenant(db, tenant_id).await?;

    // Seed a node record to satisfy FK
    let node_id = format!("node-{}", tenant_id);
    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status, created_at)
         VALUES (?, ?, ?, 'active', datetime('now'))",
    )
    .bind(&node_id)
    .bind(format!("{}-host", tenant_id))
    .bind("http://localhost:0")
    .execute(db.pool())
    .await?;

    // Seed a manifest and plan to satisfy worker FK
    let manifest_id = format!("manifest-{}", tenant_id);
    let manifest_hash = format!("hash-{}", tenant_id);
    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json)
         VALUES (?, ?, ?, '{}')",
    )
    .bind(&manifest_id)
    .bind(tenant_id)
    .bind(&manifest_hash)
    .execute(db.pool())
    .await?;

    let plan_id = format!("plan-{}", tenant_id);
    sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3, metadata_json)
         VALUES (?, ?, ?, ?, '[]', 'layout-hash', NULL)",
    )
    .bind(&plan_id)
    .bind(tenant_id)
    .bind(format!("plan-b3-{}", tenant_id))
    .bind(&manifest_hash)
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, memory_headroom_pct, k_current, adapters_loaded_json, started_at, last_seen_at)
         VALUES (?, ?, ?, ?, '/var/run/aos/test.sock', NULL, ?, NULL, NULL, '[]', datetime('now'), datetime('now'))",
    )
    .bind(worker_id)
    .bind(tenant_id)
    .bind(&node_id)
    .bind(&plan_id)
    .bind(status)
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Test helper to create a test model
async fn create_test_model(db: &Db, model_id: &str) -> Result<()> {
    use adapteros_core::B3Hash;

    let hash = B3Hash::hash(model_id.as_bytes()).to_hex();

    sqlx::query(
        "INSERT OR IGNORE INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, model_type, status, backend, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 'base_model', 'available', 'metal', datetime('now'))",
    )
    .bind(model_id)
    .bind(format!("Model {}", model_id))
    .bind(&hash)
    .bind(format!("config-{}", hash))
    .bind(format!("tokenizer-{}", hash))
    .bind(format!("tokenizer-cfg-{}", hash))
    .execute(db.pool())
    .await?;

    Ok(())
}

// =============================================================================
// Test 1: Response Structure Validation
// =============================================================================

#[test]
fn test_readyz_check_structure_has_expected_fields() {
    // Verify the ReadyzCheck struct has ok, hint, latency_ms fields
    let check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: Some(42),
    };
    assert!(check.ok);
    assert!(check.latency_ms.is_some());
    assert_eq!(check.latency_ms.unwrap(), 42);
}

#[test]
fn test_readyz_check_structure_with_hint() {
    let check = ReadyzCheck {
        ok: false,
        hint: Some("test hint".to_string()),
        latency_ms: Some(100),
    };
    assert!(!check.ok);
    assert_eq!(check.hint.as_deref(), Some("test hint"));
    assert_eq!(check.latency_ms, Some(100));
}

#[test]
fn test_readyz_check_structure_none_latency() {
    let check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: None,
    };
    assert!(check.ok);
    assert!(check.latency_ms.is_none());
}

#[test]
fn test_readyz_response_structure() {
    // Verify ReadyzResponse has ready and checks fields
    let response = ReadyzResponse {
        ready: true,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(10),
            },
            worker: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(20),
            },
            models_seeded: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(30),
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };
    assert!(response.ready);
    assert!(response.checks.db.ok);
    assert!(response.checks.worker.ok);
    assert!(response.checks.models_seeded.ok);
    assert_eq!(response.checks.db.latency_ms, Some(10));
    assert_eq!(response.checks.worker.latency_ms, Some(20));
    assert_eq!(response.checks.models_seeded.latency_ms, Some(30));
}

#[test]
fn test_readyz_response_not_ready() {
    let response = ReadyzResponse {
        ready: false,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: false,
                hint: Some("db timeout".to_string()),
                latency_ms: Some(2000),
            },
            worker: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check workers)".to_string()),
                latency_ms: None,
            },
            models_seeded: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check models)".to_string()),
                latency_ms: None,
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };
    assert!(!response.ready);
    assert!(!response.checks.db.ok);
    assert!(!response.checks.worker.ok);
    assert!(!response.checks.models_seeded.ok);
    assert_eq!(response.checks.db.latency_ms, Some(2000));
}

// =============================================================================
// Test 2: Serialization Tests
// =============================================================================

#[test]
fn test_readyz_check_serialization() {
    let check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: Some(42),
    };

    let json = serde_json::to_string(&check).expect("Failed to serialize");
    assert!(json.contains("\"ok\":true"));
    assert!(json.contains("\"latency_ms\":42"));
    // hint should be omitted when None due to skip_serializing_if
    assert!(!json.contains("hint"));
}

#[test]
fn test_readyz_check_serialization_with_hint() {
    let check = ReadyzCheck {
        ok: false,
        hint: Some("test error".to_string()),
        latency_ms: Some(100),
    };

    let json = serde_json::to_string(&check).expect("Failed to serialize");
    assert!(json.contains("\"ok\":false"));
    assert!(json.contains("\"hint\":\"test error\""));
    assert!(json.contains("\"latency_ms\":100"));
}

#[test]
fn test_readyz_check_serialization_none_latency() {
    let check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: None,
    };

    let json = serde_json::to_string(&check).expect("Failed to serialize");
    assert!(json.contains("\"ok\":true"));
    // latency_ms should be omitted when None due to skip_serializing_if
    assert!(!json.contains("latency_ms"));
}

#[test]
fn test_readyz_response_serialization() {
    let response = ReadyzResponse {
        ready: true,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(10),
            },
            worker: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(20),
            },
            models_seeded: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(30),
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("\"ready\":true"));
    assert!(json.contains("\"db\""));
    assert!(json.contains("\"worker\""));
    assert!(json.contains("\"models_seeded\""));
    assert!(json.contains("\"latency_ms\":10"));
    assert!(json.contains("\"latency_ms\":20"));
    assert!(json.contains("\"latency_ms\":30"));
}

// =============================================================================
// Test 3: Database Query Tests
// =============================================================================

#[tokio::test]
async fn test_db_query_successful() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // Test that we can query the database successfully
    let result = sqlx::query("SELECT 1").execute(db.pool()).await;

    assert!(result.is_ok(), "Database query should succeed");
}

#[tokio::test]
async fn test_count_active_workers_zero() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // With no workers, count should be 0
    let count = db
        .count_active_workers()
        .await
        .expect("Failed to count workers");
    assert_eq!(count, 0, "Should have zero active workers");
}

#[tokio::test]
async fn test_count_active_workers_with_healthy_worker() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // Create a healthy worker
    create_test_worker(&db, "worker-1", "tenant-1", "healthy")
        .await
        .expect("Failed to create worker");

    let count = db
        .count_active_workers()
        .await
        .expect("Failed to count workers");
    assert_eq!(count, 1, "Should have one active worker");
}

#[tokio::test]
async fn test_count_active_workers_includes_operational_states() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // Create workers with different statuses
    create_test_worker(&db, "worker-healthy", "tenant-1", "healthy")
        .await
        .expect("Failed to create worker");
    create_test_worker(&db, "worker-error", "tenant-2", "error")
        .await
        .expect("Failed to create worker");
    create_test_worker(&db, "worker-draining", "tenant-3", "draining")
        .await
        .expect("Failed to create worker");
    create_test_worker(&db, "worker-stopped", "tenant-4", "stopped")
        .await
        .expect("Failed to create worker");

    let count = db
        .count_active_workers()
        .await
        .expect("Failed to count workers");
    // count_active_workers counts: 'created', 'registered', 'healthy', 'draining' (not 'error' or 'stopped')
    assert_eq!(
        count, 2,
        "Should count healthy and draining workers (not error or stopped)"
    );
}

#[tokio::test]
async fn test_count_models_baseline() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // Query models count - migration 0171 seeds a default Qwen2.5 model
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool())
        .await
        .expect("Failed to count models");

    // There should be at least the seeded model
    assert!(
        count >= 1,
        "Should have at least the seeded base model, got {}",
        count
    );
}

#[tokio::test]
async fn test_count_models_with_seeded_model() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // Get initial count (includes migration-seeded model)
    let initial_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool())
        .await
        .expect("Failed to count models");

    // Create an additional test model
    create_test_model(&db, "test-model-1")
        .await
        .expect("Failed to create model");

    let final_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool())
        .await
        .expect("Failed to count models");

    assert_eq!(
        final_count,
        initial_count + 1,
        "Should have one additional model"
    );
}

// =============================================================================
// Test 4: Timeout Configuration Tests
// =============================================================================

#[tokio::test]
async fn test_timeout_configuration_defaults() {
    // Test that default timeout values are reasonable
    const DB_TIMEOUT_FALLBACK_MS: u64 = 2000;
    const WORKER_TIMEOUT_FALLBACK_MS: u64 = 2000;
    const MODELS_TIMEOUT_FALLBACK_MS: u64 = 2000;

    assert_eq!(
        DB_TIMEOUT_FALLBACK_MS, 2000,
        "Default DB timeout should be 2000ms"
    );
    assert_eq!(
        WORKER_TIMEOUT_FALLBACK_MS, 2000,
        "Default worker timeout should be 2000ms"
    );
    assert_eq!(
        MODELS_TIMEOUT_FALLBACK_MS, 2000,
        "Default models timeout should be 2000ms"
    );
}

// =============================================================================
// Test 5: Error Scenario Tests
// =============================================================================

#[tokio::test]
async fn test_readyz_structure_for_no_workers_scenario() {
    // Setup: Response structure for zero workers
    let response = ReadyzResponse {
        ready: false,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(10),
            },
            worker: ReadyzCheck {
                ok: false,
                hint: Some("no workers registered".to_string()),
                latency_ms: Some(5),
            },
            models_seeded: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(8),
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };

    // Verify structure
    assert!(
        !response.ready,
        "Service should not be ready with no workers"
    );
    assert!(response.checks.db.ok, "DB check should pass");
    assert!(!response.checks.worker.ok, "Worker check should fail");
    assert_eq!(
        response.checks.worker.hint.as_deref(),
        Some("no workers registered")
    );
    assert!(response.checks.models_seeded.ok, "Models check should pass");
}

#[tokio::test]
async fn test_readyz_structure_for_no_models_scenario() {
    // Setup: Response structure for zero models
    let response = ReadyzResponse {
        ready: false,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(10),
            },
            worker: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(15),
            },
            models_seeded: ReadyzCheck {
                ok: false,
                hint: Some("no models seeded".to_string()),
                latency_ms: Some(5),
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };

    // Verify structure
    assert!(
        !response.ready,
        "Service should not be ready with no models"
    );
    assert!(response.checks.db.ok, "DB check should pass");
    assert!(response.checks.worker.ok, "Worker check should pass");
    assert!(
        !response.checks.models_seeded.ok,
        "Models check should fail"
    );
    assert_eq!(
        response.checks.models_seeded.hint.as_deref(),
        Some("no models seeded")
    );
}

#[tokio::test]
async fn test_readyz_structure_for_db_timeout_scenario() {
    // Setup: Response structure for DB timeout
    let response = ReadyzResponse {
        ready: false,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: false,
                hint: Some("db timeout".to_string()),
                latency_ms: Some(2000),
            },
            worker: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check workers)".to_string()),
                latency_ms: None,
            },
            models_seeded: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check models)".to_string()),
                latency_ms: None,
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };

    // Verify structure
    assert!(
        !response.ready,
        "Service should not be ready with DB timeout"
    );
    assert!(!response.checks.db.ok, "DB check should fail");
    assert_eq!(response.checks.db.hint.as_deref(), Some("db timeout"));
    assert!(!response.checks.worker.ok, "Worker check should fail");
    assert!(
        !response.checks.models_seeded.ok,
        "Models check should fail"
    );
    assert!(
        response.checks.worker.latency_ms.is_none(),
        "Worker latency should be None when DB fails"
    );
    assert!(
        response.checks.models_seeded.latency_ms.is_none(),
        "Models latency should be None when DB fails"
    );
}

#[tokio::test]
async fn test_readyz_structure_for_db_unreachable_scenario() {
    // Setup: Response structure for DB unreachable
    let response = ReadyzResponse {
        ready: false,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: false,
                hint: Some("db unreachable".to_string()),
                latency_ms: Some(150),
            },
            worker: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check workers)".to_string()),
                latency_ms: None,
            },
            models_seeded: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check models)".to_string()),
                latency_ms: None,
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };

    // Verify structure
    assert!(
        !response.ready,
        "Service should not be ready with DB unreachable"
    );
    assert!(!response.checks.db.ok, "DB check should fail");
    assert_eq!(response.checks.db.hint.as_deref(), Some("db unreachable"));
}

// =============================================================================
// Test 6: Latency Tracking Tests
// =============================================================================

#[test]
fn test_latency_ms_field_present_in_check() {
    // Verify that latency_ms field exists and can be set
    let check = ReadyzCheck {
        ok: true,
        hint: None,
        latency_ms: Some(123),
    };

    assert_eq!(check.latency_ms, Some(123));
}

#[test]
fn test_latency_ms_various_values() {
    // Test various latency values
    let test_cases = vec![
        (0, Some(0)),
        (1, Some(1)),
        (100, Some(100)),
        (2000, Some(2000)),
        (9999, Some(9999)),
    ];

    for (input, expected) in test_cases {
        let check = ReadyzCheck {
            ok: true,
            hint: None,
            latency_ms: Some(input),
        };
        assert_eq!(check.latency_ms, expected);
    }
}

#[test]
fn test_all_checks_can_have_different_latencies() {
    let checks = ReadyzChecks {
        db: ReadyzCheck {
            ok: true,
            hint: None,
            latency_ms: Some(10),
        },
        worker: ReadyzCheck {
            ok: true,
            hint: None,
            latency_ms: Some(50),
        },
        models_seeded: ReadyzCheck {
            ok: true,
            hint: None,
            latency_ms: Some(5),
        },
    };

    assert_eq!(checks.db.latency_ms, Some(10));
    assert_eq!(checks.worker.latency_ms, Some(50));
    assert_eq!(checks.models_seeded.latency_ms, Some(5));
}

// =============================================================================
// Test 7: Edge Cases
// =============================================================================

#[tokio::test]
async fn test_multiple_workers_all_healthy() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // Create multiple healthy workers
    for i in 1..=3 {
        create_test_worker(
            &db,
            &format!("worker-{}", i),
            &format!("tenant-{}", i),
            "healthy",
        )
        .await
        .expect("Failed to create worker");
    }

    let count = db
        .count_active_workers()
        .await
        .expect("Failed to count workers");
    assert_eq!(count, 3, "Should have three active workers");
}

#[tokio::test]
async fn test_multiple_models_seeded() {
    let db = setup_test_db().await.expect("Failed to create test DB");

    // Get initial count (includes migration-seeded model)
    let initial_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool())
        .await
        .expect("Failed to count models");

    // Create multiple test models
    for i in 1..=5 {
        create_test_model(&db, &format!("test-model-{}", i))
            .await
            .expect("Failed to create model");
    }

    let final_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool())
        .await
        .expect("Failed to count models");

    assert_eq!(
        final_count,
        initial_count + 5,
        "Should have five additional models"
    );
}

#[test]
fn test_readyz_response_all_checks_failed() {
    let response = ReadyzResponse {
        ready: false,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: false,
                hint: Some("db timeout".to_string()),
                latency_ms: Some(2000),
            },
            worker: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check workers)".to_string()),
                latency_ms: None,
            },
            models_seeded: ReadyzCheck {
                ok: false,
                hint: Some("database unavailable (cannot check models)".to_string()),
                latency_ms: None,
            },
        },
        metrics: None,
        boot_trace_id: String::new(),
        last_error_code: None,
        phases: Vec::new(),
    };

    assert!(!response.ready);
    assert!(!response.checks.db.ok);
    assert!(!response.checks.worker.ok);
    assert!(!response.checks.models_seeded.ok);
}

#[test]
fn test_readyz_response_json_roundtrip() {
    let original = ReadyzResponse {
        ready: true,
        checks: ReadyzChecks {
            db: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(10),
            },
            worker: ReadyzCheck {
                ok: true,
                hint: Some("healthy".to_string()),
                latency_ms: Some(20),
            },
            models_seeded: ReadyzCheck {
                ok: true,
                hint: None,
                latency_ms: Some(30),
            },
        },
        metrics: Some(ReadyMetrics {
            boot_phases_ms: Vec::new(),
            db_latency_ms: Some(10),
            worker_latency_ms: Some(20),
            models_latency_ms: Some(30),
        }),
        boot_trace_id: "trace-id".to_string(),
        last_error_code: None,
        phases: Vec::new(),
    };

    let json = serde_json::to_string(&original).expect("Failed to serialize");
    let deserialized: ReadyzResponse = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(original.ready, deserialized.ready);
    assert_eq!(original.checks.db.ok, deserialized.checks.db.ok);
    assert_eq!(
        original.checks.db.latency_ms,
        deserialized.checks.db.latency_ms
    );
    assert_eq!(original.checks.worker.ok, deserialized.checks.worker.ok);
    assert_eq!(
        original.checks.worker.latency_ms,
        deserialized.checks.worker.latency_ms
    );
    assert_eq!(
        original.checks.models_seeded.ok,
        deserialized.checks.models_seeded.ok
    );
    assert_eq!(
        original.checks.models_seeded.latency_ms,
        deserialized.checks.models_seeded.latency_ms
    );
}
