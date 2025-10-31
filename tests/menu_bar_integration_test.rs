#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for MenuBar Status Monitor functionality
//!
//! Tests the complete integration between Rust status writer and Swift menu bar app.

use adapteros_db::Db;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_orchestrator::TrainingService;
use adapteros_server::status_writer::{write_status, AdapterOSStatus};
use adapteros_server_api::state::ApiConfig;
use adapteros_server_api::AppState;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

/// Test the complete status writer integration
#[tokio::test]
async fn test_status_writer_integration() {
    // Setup test database
    let db = Db::connect("sqlite::memory:").await.unwrap();

    // Create sample data
    setup_test_data(&db).await;

    // Create AppState
    let app_state = create_test_app_state(db).await;

    // Initialize uptime tracking
    adapteros_server::status_writer::init_start_time();

    // Write status
    let result = write_status(&app_state).await;
    assert!(result.is_ok(), "Status write should succeed");

    // Verify file was created
    let status_path = Path::new("var/adapteros_status.json");
    assert!(status_path.exists(), "Status file should exist");

    // Read and validate content
    let content = fs::read_to_string(status_path).unwrap();
    let status: AdapterOSStatus = serde_json::from_str(&content).unwrap();

    // Validate schema version
    assert_eq!(status.schema_version, "1.0");

    // Validate status fields
    assert_eq!(status.status, "ok");
    assert!(status.adapters_loaded >= 0);
    assert!(status.worker_count >= 0);

    // Validate base model fields
    assert!(status.base_model_loaded);
    assert_eq!(status.base_model_status, "ready");
    assert!(status.base_model_id.is_some());
    assert!(status.base_model_name.is_some());

    // Cleanup
    let _ = fs::remove_file(status_path);
}

/// Test status transitions based on system state
#[tokio::test]
async fn test_status_transitions() {
    let db = Db::connect("sqlite::memory:").await.unwrap();

    // Test 1: No adapters, no workers = "error"
    let app_state = create_test_app_state(db.clone()).await;
    let status = adapteros_server::status_writer::collect_status(&app_state)
        .await
        .unwrap();
    assert_eq!(status.status, "error");

    // Test 2: Add adapters but no workers = "degraded"
    setup_test_adapters(&db, 1).await;
    let status = adapteros_server::status_writer::collect_status(&app_state)
        .await
        .unwrap();
    assert_eq!(status.status, "degraded");

    // Test 3: Add workers = "ok"
    setup_test_workers(&db, 1).await;
    let status = adapteros_server::status_writer::collect_status(&app_state)
        .await
        .unwrap();
    assert_eq!(status.status, "ok");
}

/// Test error handling in status collection
#[tokio::test]
async fn test_error_handling() {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    let app_state = create_test_app_state(db).await;

    // Test with invalid database state - should not panic
    let status = adapteros_server::status_writer::collect_status(&app_state)
        .await
        .unwrap();

    // Should return valid status even with errors
    assert!(!status.status.is_empty());
    assert!(status.adapters_loaded >= 0); // Should default to 0 on error
    assert!(status.worker_count >= 0); // Should default to 0 on error
}

/// Test schema backward compatibility
#[tokio::test]
async fn test_schema_compatibility() {
    // Create status without new fields (simulating older version)
    let legacy_json = r#"{
        "status": "ok",
        "uptime_secs": 100,
        "adapters_loaded": 2,
        "deterministic": true,
        "kernel_hash": "abcd1234",
        "telemetry_mode": "local",
        "worker_count": 1
    }"#;

    // Should deserialize successfully (unknown fields ignored)
    let result: Result<AdapterOSStatus, _> = serde_json::from_str(legacy_json);
    assert!(result.is_ok(), "Should handle missing fields gracefully");
}

/// Test concurrent status writes
#[tokio::test]
async fn test_concurrent_writes() {
    use tokio::task;

    let db = Db::connect("sqlite::memory:").await.unwrap();
    setup_test_data(&db).await;
    let app_state = create_test_app_state(db).await;

    // Spawn multiple concurrent writes
    let mut handles = vec![];
    for _ in 0..5 {
        let state_clone = app_state.clone();
        let handle = task::spawn(async move { write_status(&state_clone).await });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent writes should succeed");
    }

    // Verify file still exists and is valid
    let status_path = Path::new("var/adapteros_status.json");
    assert!(status_path.exists());

    let content = fs::read_to_string(status_path).unwrap();
    let _: AdapterOSStatus = serde_json::from_str(&content).unwrap();

    // Cleanup
    let _ = fs::remove_file(status_path);
}

// Helper functions

async fn setup_test_data(db: &Db) {
    setup_test_adapters(db, 2).await;
    setup_test_workers(db, 1).await;
}

async fn setup_test_adapters(db: &Db, count: usize) {
    for i in 0..count {
        let _ = sqlx::query(
            "INSERT INTO adapters (adapter_id, status, created_at) VALUES (?, 'active', datetime('now'))"
        )
        .bind(format!("test-adapter-{}", i))
        .execute(db.pool())
        .await;
    }
}

async fn setup_test_workers(db: &Db, count: usize) {
    for i in 0..count {
        let _ = sqlx::query(
            "INSERT INTO workers (worker_id, status, created_at) VALUES (?, 'active', datetime('now'))"
        )
        .bind(format!("test-worker-{}", i))
        .execute(db.pool())
        .await;
    }
}

async fn create_test_app_state(db: Db) -> AppState {
    let api_config = Arc::new(RwLock::new(ApiConfig {
        metrics: adapteros_server_api::state::MetricsConfig {
            enabled: false,
            bearer_token: String::new(),
            system_metrics_interval_secs: 0,
        },
        golden_gate: None,
        bundles_root: "var/bundles".to_string(),
        rate_limits: None,
    }));

    let metrics_exporter = Arc::new(MetricsExporter::new(Default::default()).unwrap());
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new().unwrap());
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
        metrics_collector.clone(),
    ));
    for name in [
        "inference_latency_p95_ms",
        "queue_depth",
        "tokens_per_second",
        "memory_usage_mb",
    ] {
        metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
    }
    let training_service = Arc::new(TrainingService::new());

    AppState::with_sqlite(
        db,
        vec![], // empty JWT secret for testing
        api_config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        training_service,
    )
}
