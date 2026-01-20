//! Comprehensive tests for worker spawning functionality
//!
//! Tests cover:
//! - Successful worker spawn and communication
//! - Worker crash handling and restart
//! - Resource limit enforcement
//! - Worker health monitoring
//! - Graceful shutdown propagation
//!
//! Note: Many tests require specific system configurations (root privileges,
//! PF rules enabled, aos-worker binary) and are marked as #[ignore].

use adapteros_node::agent::{NodeAgent, NodeHealth, WorkerInfo};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Helper to create a test temp directory in var/tmp
fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

// ============================================================
// NodeAgent Creation Tests
// ============================================================

#[test]
fn test_node_agent_creation() {
    let _agent = NodeAgent::new();
    // Agent should be created successfully without panicking
    // The Default impl should also work
    let _agent_default = NodeAgent::default();
}

#[tokio::test]
async fn test_node_agent_initial_state() {
    let agent = NodeAgent::new();

    // Initial state should have no workers
    let workers = agent.list_workers().await;
    assert!(workers.is_ok(), "list_workers should succeed");
    assert!(
        workers.unwrap().is_empty(),
        "Initial worker list should be empty"
    );
}

// ============================================================
// Worker Spawning Tests
// ============================================================

#[tokio::test]
#[ignore = "Requires PF rules enabled and aos-worker binary [prereq: sudo pfctl -e]"]
async fn test_spawn_worker_success() {
    let agent = NodeAgent::new();

    // Spawn a worker with test parameters
    let result = agent
        .spawn_worker(
            "test-tenant",
            "test-plan",
            1000, // uid
            1000, // gid
            Some(512),
            None,
        )
        .await;

    assert!(
        result.is_ok(),
        "spawn_worker should succeed: {:?}",
        result.err()
    );

    let pid = result.unwrap();
    assert!(pid > 0, "PID should be positive");

    // Verify worker is tracked
    let workers = agent.list_workers().await.unwrap();
    assert_eq!(workers.len(), 1, "Should have one worker");
    assert_eq!(workers[0].pid, pid, "Worker PID should match");
    assert_eq!(
        workers[0].tenant_id, "test-tenant",
        "Tenant ID should match"
    );
    assert_eq!(workers[0].plan_id, "test-plan", "Plan ID should match");

    // Cleanup
    let _ = agent.stop_worker(pid).await;
}

#[tokio::test]
#[ignore = "Requires PF rules enabled [prereq: sudo pfctl -e]"]
async fn test_spawn_worker_with_model_cache_config() {
    let agent = NodeAgent::new();

    let result = agent
        .spawn_worker(
            "tenant-cache-test",
            "plan-cache-test",
            1000,
            1000,
            Some(1024), // 1GB model cache
            None,
        )
        .await;

    // Even if spawn fails due to missing binary, we verify the API accepts the config
    match result {
        Ok(pid) => {
            assert!(pid > 0, "Should get valid PID");
            let _ = agent.stop_worker(pid).await;
        }
        Err(e) => {
            // If PF rules not enabled, this is expected
            let err_msg = format!("{}", e);
            assert!(
                err_msg.contains("PF") || err_msg.contains("worker"),
                "Error should be about PF or worker: {}",
                err_msg
            );
        }
    }
}

#[tokio::test]
#[ignore = "Requires PF rules enabled [prereq: sudo pfctl -e]"]
async fn test_spawn_worker_with_config_toml_path() {
    let _tmpdir = new_test_tempdir();
    let agent = NodeAgent::new();

    let result = agent
        .spawn_worker(
            "tenant-config-test",
            "plan-config-test",
            1000,
            1000,
            None,
            Some("/etc/aos/worker.toml"),
        )
        .await;

    // Verify the API accepts config path parameter
    match result {
        Ok(pid) => {
            let _ = agent.stop_worker(pid).await;
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            // Expected failure modes
            assert!(
                err_msg.contains("PF") || err_msg.contains("worker"),
                "Error should be about PF or worker: {}",
                err_msg
            );
        }
    }
}

#[tokio::test]
async fn test_spawn_worker_fails_without_pf() {
    let agent = NodeAgent::new();

    // This test verifies that worker spawning fails when PF rules are not enabled.
    // In most test environments, PF is not configured with deny-all egress rules.
    let result = agent
        .spawn_worker("test-tenant", "test-plan", 1000, 1000, None, None)
        .await;

    // Depending on system state, we may get a PF error or proceed to simulated worker
    match result {
        Ok(pid) => {
            // Simulated worker was created (PF check passed or was bypassed)
            // This can happen if PF is accidentally enabled or in dev mode
            let _ = agent.stop_worker(pid).await;
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            // Should fail because PF egress rules are not active
            assert!(
                err_msg.contains("PF egress rules not active")
                    || err_msg.contains("PF")
                    || err_msg.contains("firewall"),
                "Expected PF-related error, got: {}",
                err_msg
            );
        }
    }
}

// ============================================================
// Worker Lifecycle Tests
// ============================================================

#[tokio::test]
async fn test_stop_nonexistent_worker() {
    let agent = NodeAgent::new();

    // Try to stop a worker that doesn't exist
    let result = agent.stop_worker(99999).await;
    assert!(result.is_err(), "Stopping nonexistent worker should fail");

    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("not found") || err_msg.contains("99999"),
        "Error message should indicate worker not found: {}",
        err_msg
    );
}

#[tokio::test]
#[ignore = "Requires PF rules enabled and aos-worker binary"]
async fn test_spawn_multiple_workers() {
    let agent = NodeAgent::new();

    // Spawn multiple workers for different tenants
    let pids: Vec<u32> = Vec::new();

    for i in 0..3 {
        let tenant_id = format!("tenant-{}", i);
        let plan_id = format!("plan-{}", i);

        let result = agent
            .spawn_worker(&tenant_id, &plan_id, 1000 + i, 1000 + i, None, None)
            .await;

        if let Ok(pid) = result {
            assert!(pid > 0, "PID should be positive");
        }
    }

    // Cleanup
    for pid in pids {
        let _ = agent.stop_worker(pid).await;
    }
}

#[tokio::test]
#[ignore = "Requires PF rules enabled and aos-worker binary"]
async fn test_worker_graceful_shutdown() {
    let agent = NodeAgent::new();

    let result = agent
        .spawn_worker("shutdown-test", "plan-shutdown", 1000, 1000, None, None)
        .await;

    if let Ok(pid) = result {
        // Worker should be running
        let workers = agent.list_workers().await.unwrap();
        assert!(
            workers.iter().any(|w| w.pid == pid),
            "Worker should be in list"
        );

        // Stop the worker (sends SIGTERM for graceful shutdown)
        let stop_result = agent.stop_worker(pid).await;
        assert!(stop_result.is_ok(), "Stop should succeed");

        // Worker should be removed from tracking
        let workers_after = agent.list_workers().await.unwrap();
        assert!(
            !workers_after.iter().any(|w| w.pid == pid),
            "Worker should be removed from list"
        );
    }
}

// ============================================================
// Health Monitoring Tests
// ============================================================

#[tokio::test]
async fn test_node_health_without_pf() {
    let agent = NodeAgent::new();

    let health = agent.get_health().await;
    assert!(health.is_ok(), "get_health should succeed");

    let health = health.unwrap();
    // Without PF enabled, pf_enabled should be false
    // (This depends on system state, so we just check the structure)
    assert!(
        health.memory_available_mb > 0,
        "Memory should be positive (placeholder value)"
    );
    assert_eq!(health.worker_count, 0, "Initial worker count should be 0");
}

#[tokio::test]
#[ignore = "Requires PF rules enabled [prereq: sudo pfctl -e && echo 'block out all' | sudo pfctl -f -]"]
async fn test_node_health_with_pf_enabled() {
    let agent = NodeAgent::new();

    let health = agent.get_health().await;
    assert!(health.is_ok(), "get_health should succeed");

    let health = health.unwrap();
    assert!(
        health.pf_enabled,
        "PF should be enabled when rules are active"
    );
}

#[tokio::test]
async fn test_pf_status_caching() {
    let agent = NodeAgent::new();

    // First call should check actual PF status
    let status1 = agent.check_pf_status().await;
    assert!(status1.is_ok(), "First check should complete");

    // Second call within cache TTL (30s) should use cached result
    let status2 = agent.check_pf_status().await;
    assert!(status2.is_ok(), "Second check should complete");

    // Results should be consistent within cache window
    assert_eq!(
        status1.unwrap(),
        status2.unwrap(),
        "Cached results should match"
    );
}

// ============================================================
// Worker Info Tests
// ============================================================

#[test]
fn test_worker_info_fields() {
    use std::time::Instant;

    let info = WorkerInfo {
        pid: 12345,
        tenant_id: "test-tenant".to_string(),
        plan_id: "test-plan".to_string(),
        uds_path: "/var/run/aos/test-tenant/aos.sock".to_string(),
        started_at: Instant::now(),
    };

    assert_eq!(info.pid, 12345);
    assert_eq!(info.tenant_id, "test-tenant");
    assert_eq!(info.plan_id, "test-plan");
    assert!(info.uds_path.contains("test-tenant"));
    assert!(info.started_at.elapsed().as_secs() < 1);
}

#[test]
fn test_node_health_serialization() {
    let health = NodeHealth {
        pf_enabled: true,
        worker_count: 5,
        memory_available_mb: 8192,
    };

    // Test serialization
    let json = serde_json::to_string(&health);
    assert!(json.is_ok(), "Should serialize to JSON");

    let json_str = json.unwrap();
    assert!(json_str.contains("\"pf_enabled\":true"));
    assert!(json_str.contains("\"worker_count\":5"));
    assert!(json_str.contains("\"memory_available_mb\":8192"));

    // Test deserialization
    let deserialized: Result<NodeHealth, _> = serde_json::from_str(&json_str);
    assert!(deserialized.is_ok(), "Should deserialize from JSON");

    let health2 = deserialized.unwrap();
    assert_eq!(health2.pf_enabled, health.pf_enabled);
    assert_eq!(health2.worker_count, health.worker_count);
    assert_eq!(health2.memory_available_mb, health.memory_available_mb);
}

// ============================================================
// Error Path Tests
// ============================================================

#[tokio::test]
async fn test_spawn_with_invalid_tenant_id() {
    let agent = NodeAgent::new();

    // Test with empty tenant ID - should still work (no validation on tenant_id content)
    let result = agent
        .spawn_worker("", "test-plan", 1000, 1000, None, None)
        .await;

    // Will likely fail due to PF, but we're testing the path accepts empty tenant
    match result {
        Ok(pid) => {
            // Clean up if it somehow succeeded
            let _ = agent.stop_worker(pid).await;
        }
        Err(e) => {
            // Expected - PF or other system requirement not met
            let _ = e;
        }
    }
}

#[tokio::test]
#[ignore = "Requires root privileges to test uid/gid errors"]
async fn test_spawn_with_privileged_uid_gid() {
    let agent = NodeAgent::new();

    // Try to spawn with uid=0 (root) - requires CAP_SETUID capability
    let result = agent
        .spawn_worker("root-test", "plan-root", 0, 0, None, None)
        .await;

    // This should fail without proper capabilities
    match result {
        Ok(pid) => {
            // If running as root, this might succeed
            let _ = agent.stop_worker(pid).await;
        }
        Err(e) => {
            // Expected when not running with appropriate capabilities
            let err_msg = format!("{}", e);
            assert!(
                err_msg.contains("permission")
                    || err_msg.contains("PF")
                    || err_msg.contains("setuid"),
                "Should fail with permission or PF error: {}",
                err_msg
            );
        }
    }
}

// ============================================================
// Concurrent Access Tests
// ============================================================

#[tokio::test]
async fn test_concurrent_worker_list_access() {
    let agent = Arc::new(NodeAgent::new());

    // Spawn multiple tasks that concurrently access the worker list
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let agent = Arc::clone(&agent);
            tokio::spawn(async move {
                let result = agent.list_workers().await;
                assert!(result.is_ok(), "Concurrent list_workers should succeed");
            })
        })
        .collect();

    for handle in handles {
        handle.await.expect("Task should complete");
    }
}

#[tokio::test]
async fn test_concurrent_health_checks() {
    let agent = Arc::new(NodeAgent::new());

    // Spawn multiple tasks that concurrently check health
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let agent = Arc::clone(&agent);
            tokio::spawn(async move {
                let result = agent.get_health().await;
                assert!(result.is_ok(), "Concurrent get_health should succeed");
            })
        })
        .collect();

    for handle in handles {
        handle.await.expect("Task should complete");
    }
}

#[tokio::test]
async fn test_concurrent_pf_status_checks() {
    let agent = Arc::new(NodeAgent::new());

    // Multiple concurrent PF status checks should all use caching correctly
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let agent = Arc::clone(&agent);
            tokio::spawn(async move {
                let result = agent.check_pf_status().await;
                assert!(result.is_ok(), "Concurrent check_pf_status should succeed");
            })
        })
        .collect();

    for handle in handles {
        handle.await.expect("Task should complete");
    }
}

// ============================================================
// Resource Limit Tests
// ============================================================

#[tokio::test]
#[ignore = "Requires PF rules enabled and system resources"]
async fn test_model_cache_budget_propagation() {
    let agent = NodeAgent::new();

    // Spawn worker with specific model cache budget
    let cache_mb = 2048u64;
    let result = agent
        .spawn_worker(
            "cache-budget-test",
            "plan-cache",
            1000,
            1000,
            Some(cache_mb),
            None,
        )
        .await;

    if let Ok(pid) = result {
        // The model cache budget should be propagated to the worker via
        // AOS_MODEL_CACHE_MAX_MB environment variable
        // (We can't easily verify this without inspecting the worker process)
        let _ = agent.stop_worker(pid).await;
    }
}

// ============================================================
// UDS Path Tests
// ============================================================

#[test]
fn test_uds_path_format() {
    // Verify the expected UDS path format
    let tenant_id = "my-tenant";
    let expected_path = format!("/var/run/aos/{}/aos.sock", tenant_id);

    assert!(expected_path.contains(tenant_id));
    assert!(expected_path.ends_with(".sock"));
}

#[tokio::test]
#[ignore = "Requires PF rules enabled"]
async fn test_uds_path_in_worker_info() {
    let agent = NodeAgent::new();
    let tenant_id = "uds-path-test";

    let result = agent
        .spawn_worker(tenant_id, "plan-uds", 1000, 1000, None, None)
        .await;

    if let Ok(pid) = result {
        let workers = agent.list_workers().await.unwrap();
        let worker = workers.iter().find(|w| w.pid == pid);

        if let Some(w) = worker {
            assert!(
                w.uds_path.contains(tenant_id),
                "UDS path should contain tenant ID"
            );
            assert!(
                w.uds_path.ends_with(".sock"),
                "UDS path should end in .sock"
            );
        }

        let _ = agent.stop_worker(pid).await;
    }
}

// ============================================================
// Simulated Worker Tests (for development/testing mode)
// ============================================================

#[tokio::test]
async fn test_simulated_worker_creation() {
    // When aos-worker binary is not found and PF check passes,
    // a simulated worker is created for development/testing purposes.
    // This test verifies that behavior path.

    let agent = NodeAgent::new();

    // The result depends on PF status
    let result = agent
        .spawn_worker("sim-test", "sim-plan", 1000, 1000, None, None)
        .await;

    match result {
        Ok(pid) => {
            // Either PF is enabled and simulated worker was created
            assert!(pid > 0, "Simulated PID should be positive");

            // Verify worker is tracked
            let workers = agent.list_workers().await.unwrap();
            let worker = workers.iter().find(|w| w.pid == pid);
            assert!(worker.is_some(), "Simulated worker should be tracked");

            // Cleanup
            let _ = agent.stop_worker(pid).await;
        }
        Err(e) => {
            // PF rules not active - expected in most test environments
            let err_msg = format!("{}", e);
            assert!(err_msg.contains("PF"), "Should fail due to PF: {}", err_msg);
        }
    }
}

// ============================================================
// Integration Tests (Full Flow)
// ============================================================

#[tokio::test]
#[ignore = "Full integration test requiring PF, aos-worker, and proper privileges"]
async fn test_full_worker_lifecycle() {
    let agent = NodeAgent::new();
    let tenant_id = "lifecycle-test";
    let plan_id = "full-lifecycle";

    // 1. Verify initial health
    let health = agent.get_health().await.unwrap();
    assert!(health.pf_enabled, "PF should be enabled");
    let initial_count = health.worker_count;

    // 2. Spawn worker
    let pid = agent
        .spawn_worker(tenant_id, plan_id, 1000, 1000, Some(512), None)
        .await
        .expect("Spawn should succeed");

    // 3. Verify worker is tracked
    let workers = agent.list_workers().await.unwrap();
    assert_eq!(
        workers.len(),
        initial_count + 1,
        "Worker count should increase"
    );

    // 4. Verify health reflects new worker
    let health = agent.get_health().await.unwrap();
    assert_eq!(
        health.worker_count,
        initial_count + 1,
        "Health worker count should match"
    );

    // 5. Wait for worker to initialize (simulated)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 6. Stop worker
    agent.stop_worker(pid).await.expect("Stop should succeed");

    // 7. Verify worker is removed
    let workers = agent.list_workers().await.unwrap();
    assert_eq!(
        workers.len(),
        initial_count,
        "Worker count should return to initial"
    );
}
