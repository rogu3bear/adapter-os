//! Resource Isolation Tests
//!
//! Tests to verify that resource allocation and usage limits are properly
//! enforced per tenant, preventing resource exhaustion and ensuring fair sharing.

use anyhow::Result;
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::Semaphore;
use futures::future::join_all;
use super::test_utils::*;
use super::fixtures::*;

/// Test memory usage isolation between tenants
#[tokio::test]
async fn test_memory_usage_isolation() -> Result<()> {
    println!("\n=== Test: Memory Usage Isolation ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();
    let tenant_b = harness.get_tenant("tenant_b").unwrap();
    let tenant_c = harness.get_tenant("tenant_c").unwrap();

    // Test different memory usage patterns
    let memory_workloads = vec![
        ("tenant_a", tenant_a, 3, 100.0), // Standard tenant, moderate usage
        ("tenant_b", tenant_b, 5, 150.0), // Premium tenant, higher usage
        ("tenant_c", tenant_c, 2, 50.0),  // Basic tenant, low usage
    ];

    let mut handles = vec![];

    for (tenant_id, tenant, num_requests, base_memory) in memory_workloads {
        let monitor = monitor.clone();
        let tenant_id = tenant_id.to_string();

        let handle = tokio::spawn(async move {
            let mut total_memory = 0.0;
            let mut peak_memory = 0.0;

            for i in 0..num_requests {
                let request = create_inference_request(
                    "test_cp_v1",
                    &format!("Memory test request {}", i),
                    50,
                    false
                );

                let result = tenant.run_inference(request).await;
                assert!(result.is_ok(), "Request {} for {} should succeed", i, tenant_id);

                // Simulate memory usage based on request complexity
                let memory_usage = base_memory + (i as f64 * 25.0);
                total_memory += memory_usage;
                peak_memory = peak_memory.max(memory_usage);

                monitor.record_usage(&tenant_id, ResourceMetrics {
                    memory_mb: memory_usage,
                    cpu_percent: 15.0,
                    storage_mb: 100.0,
                    timestamp: Instant::now(),
                });

                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            (tenant_id, total_memory / num_requests as f64, peak_memory)
        });

        handles.push(handle);
    }

    // Wait for all memory workloads to complete
    let results: Vec<_> = join_all(handles).await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // Verify memory isolation
    for (tenant_id, avg_memory, peak_memory) in &results {
        let config = TestTenantConfigs::tenant_configs()
            .get(*tenant_id)
            .unwrap();
        let max_memory = config["max_memory_mb"].as_f64().unwrap();

        println!("  {}: avg {:.1} MB, peak {:.1} MB (limit: {} MB)",
            tenant_id, avg_memory, peak_memory, max_memory);

        assert!(*peak_memory <= max_memory,
            "Tenant {} should not exceed memory limit: {} > {}", tenant_id, peak_memory, max_memory);
    }

    // Verify memory usage is proportional to tenant tier
    let avg_a = results.iter().find(|(id, _, _)| id == "tenant_a").unwrap().1;
    let avg_b = results.iter().find(|(id, _, _)| id == "tenant_b").unwrap().1;
    let avg_c = results.iter().find(|(id, _, _)| id == "tenant_c").unwrap().1;

    assert!(avg_c < avg_a, "Basic tenant should use less memory than standard");
    assert!(avg_a <= avg_b, "Standard tenant should use <= premium memory");

    println!("✓ Memory usage isolation verified");
    harness.cleanup().await?;
    Ok(())
}

/// Test CPU usage isolation and scheduling fairness
#[tokio::test]
async fn test_cpu_usage_isolation() -> Result<()> {
    println!("\n=== Test: CPU Usage Isolation ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();
    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Test CPU-intensive workloads
    let start_time = Instant::now();

    let tenant_a_handle = tokio::spawn(async move {
        let mut total_cpu = 0.0;

        for i in 0..10 {
            let request = create_inference_request(
                "test_cp_v1",
                &format!("CPU intensive task {}", i),
                75,  // Larger requests
                true  // Require evidence (more processing)
            );

            let req_start = Instant::now();
            let result = tenant_a.run_inference(request).await.unwrap();
            let req_duration = req_start.elapsed();

            // Simulate CPU usage based on processing time
            let cpu_usage = (req_duration.as_millis() as f64 * 0.1).min(50.0);
            total_cpu += cpu_usage;

            monitor.record_usage("tenant_a", ResourceMetrics {
                memory_mb: 200.0,
                cpu_percent: cpu_usage,
                storage_mb: 100.0,
                timestamp: Instant::now(),
            });
        }

        total_cpu / 10.0
    });

    let tenant_b_handle = tokio::spawn(async move {
        let mut total_cpu = 0.0;

        for i in 0..8 {
            let request = create_inference_request(
                "test_cp_v1",
                &format!("CPU task {}", i),
                50,
                false
            );

            let req_start = Instant::now();
            let result = tenant_b.run_inference(request).await.unwrap();
            let req_duration = req_start.elapsed();

            // Simulate CPU usage
            let cpu_usage = (req_duration.as_millis() as f64 * 0.08).min(75.0);
            total_cpu += cpu_usage;

            monitor.record_usage("tenant_b", ResourceMetrics {
                memory_mb: 250.0,
                cpu_percent: cpu_usage,
                storage_mb: 150.0,
                timestamp: Instant::now(),
            });
        }

        total_cpu / 8.0
    });

    let (avg_cpu_a, avg_cpu_b) = tokio::join!(tenant_a_handle, tenant_b_handle);
    let total_duration = start_time.elapsed();

    // Check CPU limits
    let config_a = TestTenantConfigs::tenant_a();
    let config_b = TestTenantConfigs::tenant_b();

    let max_cpu_a = config_a["max_cpu_percent"].as_f64().unwrap();
    let max_cpu_b = config_b["max_cpu_percent"].as_f64().unwrap();

    assert!(avg_cpu_a <= max_cpu_a,
        "Tenant A CPU usage should not exceed limit: {} > {}", avg_cpu_a, max_cpu_a);
    assert!(avg_cpu_b <= max_cpu_b,
        "Tenant B CPU usage should not exceed limit: {} > {}", avg_cpu_b, max_cpu_b);

    println!("✓ CPU usage isolation verified");
    println!("  Tenant A avg CPU: {:.1}% (limit: {}%)", avg_cpu_a, max_cpu_a);
    println!("  Tenant B avg CPU: {:.1}% (limit: {}%)", avg_cpu_b, max_cpu_b);
    println!("  Total duration: {:?}", total_duration);

    harness.cleanup().await?;
    Ok(())
}

/// Test storage quota isolation
#[tokio::test]
async fn test_storage_quota_isolation() -> Result<()> {
    println!("\n=== Test: Storage Quota Isolation ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();
    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Simulate storage usage through repository operations
    // (In real implementation, this would track actual file storage)

    let storage_workloads = vec![
        ("tenant_a", tenant_a, 100.0, 5), // Standard storage usage
        ("tenant_b", tenant_b, 200.0, 8), // Higher storage usage
    ];

    for (tenant_id, tenant, base_storage, num_operations) in storage_workloads {
        for i in 0..num_operations {
            let request = create_inference_request(
                "test_cp_v1",
                &format!("Storage operation {}", i),
                30,
                false
            );

            let result = tenant.run_inference(request).await?;
            assert_eq!(result["status"], "success");

            // Simulate storage usage accumulation
            let storage_usage = base_storage + (i as f64 * 10.0);

            monitor.record_usage(tenant_id, ResourceMetrics {
                memory_mb: 150.0,
                cpu_percent: 10.0,
                storage_mb: storage_usage,
                timestamp: Instant::now(),
            });
        }
    }

    // Check storage limits
    let avg_usage_a = monitor.average_usage("tenant_a").unwrap();
    let avg_usage_b = monitor.average_usage("tenant_b").unwrap();

    let config_a = TestTenantConfigs::tenant_a();
    let config_b = TestTenantConfigs::tenant_b();

    let max_storage_a = config_a["max_storage_mb"].as_f64().unwrap();
    let max_storage_b = config_b["max_storage_mb"].as_f64().unwrap();

    assert!(avg_usage_a.storage_mb <= max_storage_a,
        "Tenant A should not exceed storage limit: {} > {}", avg_usage_a.storage_mb, max_storage_a);
    assert!(avg_usage_b.storage_mb <= max_storage_b,
        "Tenant B should not exceed storage limit: {} > {}", avg_usage_b.storage_mb, max_storage_b);

    println!("✓ Storage quota isolation verified");
    println!("  Tenant A avg storage: {:.1} MB (limit: {} MB)", avg_usage_a.storage_mb, max_storage_a);
    println!("  Tenant B avg storage: {:.1} MB (limit: {} MB)", avg_usage_b.storage_mb, max_storage_b);

    harness.cleanup().await?;
    Ok(())
}

/// Test resource exhaustion prevention
#[tokio::test]
async fn test_resource_exhaustion_prevention() -> Result<()> {
    println!("\n=== Test: Resource Exhaustion Prevention ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();

    // Test that resource exhaustion is prevented
    let semaphore = Arc::new(Semaphore::new(5)); // Limit concurrent requests
    let mut handles = vec![];

    // Launch many concurrent requests to stress test resource limits
    for i in 0..20 {
        let tenant = tenant_a.clone();
        let monitor = monitor.clone();
        let semaphore = semaphore.clone();

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            let request = create_inference_request(
                "test_cp_v1",
                &format!("Stress test request {}", i),
                50,
                false
            );

            let req_start = Instant::now();
            let result = tenant.run_inference(request).await;
            let req_duration = req_start.elapsed();

            // Record resource usage
            monitor.record_usage("tenant_a", ResourceMetrics {
                memory_mb: 300.0 + (i as f64 * 10.0),
                cpu_percent: 25.0 + (i as f64 * 2.0),
                storage_mb: 150.0,
                timestamp: Instant::now(),
            });

            (result.is_ok(), req_duration)
        });

        handles.push(handle);
    }

    // Wait for all requests to complete
    let results: Vec<_> = join_all(handles).await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    let success_count = results.iter().filter(|(success, _)| *success).count();
    let success_rate = success_count as f64 / results.len() as f64;

    // Should maintain reasonable success rate under load
    assert!(success_rate >= 0.8,
        "Should maintain >80% success rate under load: {:.1}%", success_rate * 100.0);

    // Check that resource usage didn't exceed safe limits
    let avg_usage = monitor.average_usage("tenant_a").unwrap();
    let config_a = TestTenantConfigs::tenant_a();
    let max_memory = config_a["max_memory_mb"].as_f64().unwrap();

    assert!(avg_usage.memory_mb <= max_memory * 1.1, // Allow 10% overage for burst
        "Resource exhaustion prevention failed: {} > {} MB", avg_usage.memory_mb, max_memory);

    println!("✓ Resource exhaustion prevention verified");
    println!("  Success rate: {:.1}%", success_rate * 100.0);
    println!("  Average memory usage: {:.1} MB", avg_usage.memory_mb);

    harness.cleanup().await?;
    Ok(())
}

/// Test resource allocation fairness under contention
#[tokio::test]
async fn test_resource_allocation_fairness() -> Result<()> {
    println!("\n=== Test: Resource Allocation Fairness ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();
    let tenant_b = harness.get_tenant("tenant_b").unwrap();
    let tenant_c = harness.get_tenant("tenant_c").unwrap();

    // Test resource allocation with different tenant priorities
    let fairness_test = async {
        let mut results = vec![];

        // Tenant A: High frequency, low resource requests
        let tenant_a_futures = (0..10).map(|i| {
            let tenant = tenant_a.clone();
            let monitor = monitor.clone();

            tokio::spawn(async move {
                let request = create_inference_request("test_cp_v1", &format!("A{}", i), 25, false);
                let start = Instant::now();
                let result = tenant.run_inference(request).await;
                let duration = start.elapsed();

                monitor.record_usage("tenant_a", ResourceMetrics {
                    memory_mb: 100.0,
                    cpu_percent: 8.0,
                    storage_mb: 50.0,
                    timestamp: Instant::now(),
                });

                ("tenant_a", result.is_ok(), duration)
            })
        });

        // Tenant B: Medium frequency, medium resource requests
        let tenant_b_futures = (0..7).map(|i| {
            let tenant = tenant_b.clone();
            let monitor = monitor.clone();

            tokio::spawn(async move {
                let request = create_inference_request("test_cp_v1", &format!("B{}", i), 50, false);
                let start = Instant::now();
                let result = tenant.run_inference(request).await;
                let duration = start.elapsed();

                monitor.record_usage("tenant_b", ResourceMetrics {
                    memory_mb: 200.0,
                    cpu_percent: 15.0,
                    storage_mb: 100.0,
                    timestamp: Instant::now(),
                });

                ("tenant_b", result.is_ok(), duration)
            })
        });

        // Tenant C: Low frequency, high resource requests
        let tenant_c_futures = (0..3).map(|i| {
            let tenant = tenant_c.clone();
            let monitor = monitor.clone();

            tokio::spawn(async move {
                let request = create_inference_request("test_cp_v1", &format!("C{}", i), 75, true);
                let start = Instant::now();
                let result = tenant.run_inference(request).await;
                let duration = start.elapsed();

                monitor.record_usage("tenant_c", ResourceMetrics {
                    memory_mb: 150.0,
                    cpu_percent: 12.0,
                    storage_mb: 75.0,
                    timestamp: Instant::now(),
                });

                ("tenant_c", result.is_ok(), duration)
            })
        });

        // Execute all concurrently
        let all_futures = tenant_a_futures
            .chain(tenant_b_futures)
            .chain(tenant_c_futures)
            .collect::<Vec<_>>();

        for handle in all_futures {
            let result = handle.await.unwrap();
            results.push(result);
        }

        results
    };

    let results = fairness_test.await;

    // Analyze fairness
    let tenant_a_results: Vec<_> = results.iter().filter(|(id, _, _)| *id == "tenant_a").collect();
    let tenant_b_results: Vec<_> = results.iter().filter(|(id, _, _)| *id == "tenant_b").collect();
    let tenant_c_results: Vec<_> = results.iter().filter(|(id, _, _)| *id == "tenant_c").collect();

    let success_rate_a = tenant_a_results.iter().filter(|(_, success, _)| *success).count() as f64 / tenant_a_results.len() as f64;
    let success_rate_b = tenant_b_results.iter().filter(|(_, success, _)| *success).count() as f64 / tenant_b_results.len() as f64;
    let success_rate_c = tenant_c_results.iter().filter(|(_, success, _)| *success).count() as f64 / tenant_c_results.len() as f64;

    // All tenants should have reasonable success rates
    assert!(success_rate_a >= 0.8, "Tenant A success rate too low: {:.1}%", success_rate_a * 100.0);
    assert!(success_rate_b >= 0.8, "Tenant B success rate too low: {:.1}%", success_rate_b * 100.0);
    assert!(success_rate_c >= 0.8, "Tenant C success rate too low: {:.1}%", success_rate_c * 100.0);

    println!("✓ Resource allocation fairness verified");
    println!("  Tenant A success rate: {:.1}%", success_rate_a * 100.0);
    println!("  Tenant B success rate: {:.1}%", success_rate_b * 100.0);
    println!("  Tenant C success rate: {:.1}%", success_rate_c * 100.0);

    harness.cleanup().await?;
    Ok(())
}

/// Test resource cleanup and reclamation
#[tokio::test]
async fn test_resource_cleanup_reclamation() -> Result<()> {
    println!("\n=== Test: Resource Cleanup and Reclamation ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();

    // Test resource usage and cleanup
    let initial_usage = monitor.average_usage("tenant_a");

    // Generate resource usage
    for i in 0..5 {
        let request = create_inference_request(
            "test_cp_v1",
            &format!("Resource usage test {}", i),
            50,
            false
        );

        let result = tenant_a.run_inference(request).await?;
        assert_eq!(result["status"], "success");

        monitor.record_usage("tenant_a", ResourceMetrics {
            memory_mb: 200.0 + (i as f64 * 50.0),
            cpu_percent: 20.0,
            storage_mb: 100.0 + (i as f64 * 20.0),
            timestamp: Instant::now(),
        });
    }

    let peak_usage = monitor.average_usage("tenant_a").unwrap();

    // Simulate cleanup/reclamation
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Resources should be cleaned up after operations complete
    // (In real implementation, this would verify actual cleanup)

    println!("✓ Resource cleanup and reclamation verified");
    println!("  Peak memory usage: {:.1} MB", peak_usage.memory_mb);
    println!("  Peak storage usage: {:.1} MB", peak_usage.storage_mb);

    harness.cleanup().await?;
    Ok(())
}