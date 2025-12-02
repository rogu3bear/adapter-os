//! PRD-09: Worker Health Monitoring Integration Tests
//!
//! Acceptance tests for:
//! - Hung worker detection (worker marked degraded after consecutive slow responses)
//! - Fatal incident recording (worker crash surfacing)
//! - Worker recovery (degraded -> healthy transition)

use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_server_api::worker_health::{HealthConfig, WorkerHealthMonitor, WorkerHealthStatus};
use std::sync::Arc;

/// Test helper to create an in-memory database with required schema
async fn setup_test_db() -> Result<Db> {
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;
    Ok(db)
}

/// Test helper to create a test worker in the database
async fn create_test_worker(db: &Db, worker_id: &str, tenant_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, status, worker_type, uds_path, created_at)
         VALUES (?, ?, 'test-node', 'serving', 'inference', '/tmp/test.sock', datetime('now'))",
    )
    .bind(worker_id)
    .bind(tenant_id)
    .execute(db.pool())
    .await?;
    Ok(())
}

// =============================================================================
// Acceptance Test 1: Hung Worker Detection
// =============================================================================

#[tokio::test]
async fn test_hung_worker_detection_marks_degraded_after_consecutive_slow() {
    // Setup: Create a health monitor with low thresholds for testing
    let db = setup_test_db().await.expect("Failed to create test DB");
    let config = HealthConfig {
        latency_threshold_ms: 100,      // 100ms threshold for testing
        slow_response_count: 3,         // Only 3 consecutive slow responses to trigger degraded
        recovery_count: 3,
        moving_avg_window: 5,
        polling_interval: std::time::Duration::from_secs(30),
        polling_timeout: std::time::Duration::from_secs(3),
    };

    let monitor = Arc::new(WorkerHealthMonitor::new(db.clone(), config));
    let worker_id = "test-worker-hung";

    // Create test worker
    create_test_worker(&db, worker_id, "test-tenant")
        .await
        .expect("Failed to create test worker");

    // Simulate consecutive slow responses (above threshold)
    for _ in 0..3 {
        monitor.record_response(worker_id, 150).await; // 150ms > 100ms threshold
    }

    // Verify: Worker should be marked as degraded
    let health = monitor.get_worker_health(worker_id);
    assert!(health.is_some(), "Worker health should be tracked");

    let health = health.unwrap();
    assert_eq!(
        health.health_status,
        WorkerHealthStatus::Degraded,
        "Worker should be degraded after {} consecutive slow responses",
        3
    );
    assert!(
        health.avg_latency_ms >= 100.0,
        "Average latency should reflect slow responses"
    );
}

#[tokio::test]
async fn test_worker_stays_healthy_with_fast_responses() {
    // Setup
    let db = setup_test_db().await.expect("Failed to create test DB");
    let config = HealthConfig {
        latency_threshold_ms: 100,
        slow_response_count: 3,
        recovery_count: 3,
        moving_avg_window: 5,
        polling_interval: std::time::Duration::from_secs(30),
        polling_timeout: std::time::Duration::from_secs(3),
    };

    let monitor = Arc::new(WorkerHealthMonitor::new(db.clone(), config));
    let worker_id = "test-worker-healthy";

    create_test_worker(&db, worker_id, "test-tenant")
        .await
        .expect("Failed to create test worker");

    // Simulate fast responses (below threshold)
    for _ in 0..5 {
        monitor.record_response(worker_id, 50).await; // 50ms < 100ms threshold
    }

    // Verify: Worker should stay healthy
    let health = monitor.get_worker_health(worker_id);
    assert!(health.is_some(), "Worker health should be tracked");

    let health = health.unwrap();
    assert_eq!(
        health.health_status,
        WorkerHealthStatus::Healthy,
        "Worker should stay healthy with fast responses"
    );
}

// =============================================================================
// Acceptance Test 2: Fatal Incident Recording
// =============================================================================

#[tokio::test]
async fn test_fatal_incident_recorded_in_database() {
    // Setup
    let db = setup_test_db().await.expect("Failed to create test DB");
    let worker_id = "test-worker-fatal";
    let tenant_id = "test-tenant";

    create_test_worker(&db, worker_id, tenant_id)
        .await
        .expect("Failed to create test worker");

    // Simulate fatal error by inserting incident directly
    let incident_id = uuid::Uuid::now_v7().to_string();
    db.insert_worker_incident(
        &incident_id,
        worker_id,
        tenant_id,
        "fatal",
        "PANIC: Out of memory during inference",
        Some("at src/inference.rs:123\n  in inference_handler"),
        None,
    )
    .await
    .expect("Failed to insert incident");

    // Verify: Incident should be retrievable
    let incidents = db
        .list_worker_incidents(worker_id, Some(10))
        .await
        .expect("Failed to list incidents");

    assert_eq!(incidents.len(), 1, "Should have exactly one incident");
    let incident = &incidents[0];
    assert_eq!(incident.incident_type, "fatal");
    assert!(incident.reason.contains("PANIC"));
    assert!(incident.backtrace_snippet.is_some());
}

#[tokio::test]
async fn test_multiple_incident_types_recorded() {
    let db = setup_test_db().await.expect("Failed to create test DB");
    let worker_id = "test-worker-multi";
    let tenant_id = "test-tenant";

    create_test_worker(&db, worker_id, tenant_id)
        .await
        .expect("Failed to create test worker");

    // Insert various incident types
    for (i, incident_type) in ["fatal", "crash", "hung", "degraded"].iter().enumerate() {
        let incident_id = format!("incident-{}", i);
        db.insert_worker_incident(
            &incident_id,
            worker_id,
            tenant_id,
            incident_type,
            &format!("Test {} incident", incident_type),
            None,
            Some(1000.0 + i as f64 * 100.0),
        )
        .await
        .expect("Failed to insert incident");
    }

    // Verify all incidents are recorded
    let incidents = db
        .list_worker_incidents(worker_id, Some(10))
        .await
        .expect("Failed to list incidents");

    assert_eq!(incidents.len(), 4, "Should have all 4 incidents");

    // Verify incident count method
    let count = db
        .get_worker_incident_count(worker_id)
        .await
        .expect("Failed to count incidents");
    assert_eq!(count, 4, "Incident count should match");
}

// =============================================================================
// Acceptance Test 3: Worker Recovery
// =============================================================================

#[tokio::test]
async fn test_worker_recovers_from_degraded_with_fast_responses() {
    // Setup with lower thresholds for faster testing
    let db = setup_test_db().await.expect("Failed to create test DB");
    let config = HealthConfig {
        latency_threshold_ms: 100,
        slow_response_count: 3,
        recovery_count: 3,    // 3 fast responses to recover
        moving_avg_window: 5,
        polling_interval: std::time::Duration::from_secs(30),
        polling_timeout: std::time::Duration::from_secs(3),
    };

    let monitor = Arc::new(WorkerHealthMonitor::new(db.clone(), config));
    let worker_id = "test-worker-recovery";

    create_test_worker(&db, worker_id, "test-tenant")
        .await
        .expect("Failed to create test worker");

    // First, make worker degraded with slow responses
    for _ in 0..3 {
        monitor.record_response(worker_id, 200).await; // Slow
    }

    // Verify degraded
    let health = monitor.get_worker_health(worker_id).expect("Health should exist");
    assert_eq!(
        health.health_status,
        WorkerHealthStatus::Degraded,
        "Worker should be degraded"
    );

    // Now simulate recovery with fast responses
    for _ in 0..3 {
        monitor.record_response(worker_id, 20).await; // Fast
    }

    // Verify recovery
    let health = monitor.get_worker_health(worker_id).expect("Health should exist");
    assert_eq!(
        health.health_status,
        WorkerHealthStatus::Healthy,
        "Worker should recover to healthy after {} fast responses",
        3
    );
}

#[tokio::test]
async fn test_mixed_responses_dont_trigger_state_change() {
    // Setup
    let db = setup_test_db().await.expect("Failed to create test DB");
    let config = HealthConfig {
        latency_threshold_ms: 100,
        slow_response_count: 3,
        recovery_count: 3,
        moving_avg_window: 5,
        polling_interval: std::time::Duration::from_secs(30),
        polling_timeout: std::time::Duration::from_secs(3),
    };

    let monitor = Arc::new(WorkerHealthMonitor::new(db.clone(), config));
    let worker_id = "test-worker-mixed";

    create_test_worker(&db, worker_id, "test-tenant")
        .await
        .expect("Failed to create test worker");

    // Start healthy
    monitor.record_response(worker_id, 20).await;
    monitor.record_response(worker_id, 20).await;

    // Mix of slow and fast responses (not consecutive)
    monitor.record_response(worker_id, 200).await; // Slow
    monitor.record_response(worker_id, 20).await;  // Fast - breaks consecutive
    monitor.record_response(worker_id, 200).await; // Slow
    monitor.record_response(worker_id, 20).await;  // Fast - breaks consecutive

    // Verify: Worker should stay healthy (no 3 consecutive slow)
    let health = monitor.get_worker_health(worker_id).expect("Health should exist");
    assert_eq!(
        health.health_status,
        WorkerHealthStatus::Healthy,
        "Worker should stay healthy with non-consecutive slow responses"
    );
}

// =============================================================================
// Additional Edge Case Tests
// =============================================================================

#[tokio::test]
async fn test_failure_increments_and_crash_detection() {
    let db = setup_test_db().await.expect("Failed to create test DB");
    let config = HealthConfig::default();
    let monitor = Arc::new(WorkerHealthMonitor::new(db.clone(), config));
    let worker_id = "test-worker-crash";

    create_test_worker(&db, worker_id, "test-tenant")
        .await
        .expect("Failed to create test worker");

    // Record multiple failures
    for _ in 0..5 {
        monitor.record_failure(worker_id, "Connection refused").await;
    }

    // Verify: Worker should be marked as crashed
    let health = monitor.get_worker_health(worker_id).expect("Health should exist");
    assert_eq!(
        health.health_status,
        WorkerHealthStatus::Crashed,
        "Worker should be crashed after multiple failures"
    );
    assert_eq!(health.consecutive_failures, 5);
}

#[tokio::test]
async fn test_health_summary_counts() {
    let db = setup_test_db().await.expect("Failed to create test DB");
    let config = HealthConfig {
        latency_threshold_ms: 100,
        slow_response_count: 2,
        recovery_count: 2,
        moving_avg_window: 5,
        polling_interval: std::time::Duration::from_secs(30),
        polling_timeout: std::time::Duration::from_secs(3),
    };

    let monitor = Arc::new(WorkerHealthMonitor::new(db.clone(), config));

    // Create workers with different health states
    for (id, tenant) in [
        ("worker-healthy-1", "tenant-a"),
        ("worker-healthy-2", "tenant-a"),
        ("worker-degraded", "tenant-b"),
    ] {
        create_test_worker(&db, id, tenant)
            .await
            .expect("Failed to create worker");
    }

    // Make some healthy
    monitor.record_response("worker-healthy-1", 50).await;
    monitor.record_response("worker-healthy-2", 50).await;

    // Make one degraded
    monitor.record_response("worker-degraded", 200).await;
    monitor.record_response("worker-degraded", 200).await;

    // Verify health statuses
    let h1 = monitor.get_worker_health("worker-healthy-1").expect("Should have health");
    let h2 = monitor.get_worker_health("worker-healthy-2").expect("Should have health");
    let hd = monitor.get_worker_health("worker-degraded").expect("Should have health");

    assert_eq!(h1.health_status, WorkerHealthStatus::Healthy);
    assert_eq!(h2.health_status, WorkerHealthStatus::Healthy);
    assert_eq!(hd.health_status, WorkerHealthStatus::Degraded);
}
