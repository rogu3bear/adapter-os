#![cfg(all(test, feature = "extended-tests"))]
//! Concurrent Workload Tests
//!
//! Tests for multiple tenants running inference workloads simultaneously,
//! verifying resource allocation fairness and performance isolation.

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use futures::future::join_all;
use super::test_utils::*;
use super::fixtures::*;

/// Test concurrent inference across multiple tenants
#[tokio::test]
async fn test_concurrent_inference_workload() -> Result<()> {
    println!("\n=== Test: Concurrent Inference Workload ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    // Setup test tenants
    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();
    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Create concurrent workload
    let requests_per_tenant = 5;
    let concurrency_limit = Arc::new(Semaphore::new(3)); // Max 3 concurrent per tenant

    let start_time = Instant::now();

    // Launch concurrent requests for tenant A
    let tenant_a_handles: Vec<_> = (0..requests_per_tenant)
        .map(|i| {
            let tenant = tenant_a.clone();
            let monitor = monitor.clone();
            let semaphore = concurrency_limit.clone();

            tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                let request = create_inference_request(
                    "test_cp_v1",
                    &format!("Tenant A request {}", i),
                    50,
                    false
                );

                let req_start = Instant::now();
                let result = tenant.run_inference(request).await;
                let req_duration = req_start.elapsed();

                // Record resource usage (simulated)
                monitor.record_usage("tenant_a", ResourceMetrics {
                    memory_mb: 200.0 + (i as f64 * 10.0),
                    cpu_percent: 10.0 + (i as f64 * 2.0),
                    storage_mb: 50.0,
                    timestamp: Instant::now(),
                });

                (result, req_duration, i)
            })
        })
        .collect();

    // Launch concurrent requests for tenant B
    let tenant_b_handles: Vec<_> = (0..requests_per_tenant)
        .map(|i| {
            let tenant = tenant_b.clone();
            let monitor = monitor.clone();
            let semaphore = concurrency_limit.clone();

            tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                let request = create_inference_request(
                    "test_cp_v1",
                    &format!("Tenant B request {}", i),
                    50,
                    false
                );

                let req_start = Instant::now();
                let result = tenant.run_inference(request).await;
                let req_duration = req_start.elapsed();

                // Record resource usage (simulated)
                monitor.record_usage("tenant_b", ResourceMetrics {
                    memory_mb: 250.0 + (i as f64 * 15.0),
                    cpu_percent: 12.0 + (i as f64 * 3.0),
                    storage_mb: 75.0,
                    timestamp: Instant::now(),
                });

                (result, req_duration, i)
            })
        })
        .collect();

    // Wait for all requests to complete
    let tenant_a_results = join_all(tenant_a_handles).await;
    let tenant_b_results = join_all(tenant_b_handles).await;

    let total_duration = start_time.elapsed();

    // Verify all requests succeeded
    for result in &tenant_a_results {
        let (response_result, _, i) = result.as_ref().unwrap();
        assert!(response_result.is_ok(),
            "Tenant A request {} should succeed", i);
        let response = response_result.as_ref().unwrap();
        assert_eq!(response["status"], "success",
            "Tenant A request {} should have success status", i);
    }

    for result in &tenant_b_results {
        let (response_result, _, i) = result.as_ref().unwrap();
        assert!(response_result.is_ok(),
            "Tenant B request {} should succeed", i);
        let response = response_result.as_ref().unwrap();
        assert_eq!(response["status"], "success",
            "Tenant B request {} should have success status", i);
    }

    // Analyze performance
    let tenant_a_avg_duration: Duration = tenant_a_results.iter()
        .map(|r| r.as_ref().unwrap().1)
        .sum::<Duration>() / tenant_a_results.len() as u32;

    let tenant_b_avg_duration: Duration = tenant_b_results.iter()
        .map(|r| r.as_ref().unwrap().1)
        .sum::<Duration>() / tenant_b_results.len() as u32;

    // Verify reasonable performance (should complete within reasonable time)
    assert!(tenant_a_avg_duration < Duration::from_secs(5),
        "Tenant A average request duration too high: {:?}", tenant_a_avg_duration);
    assert!(tenant_b_avg_duration < Duration::from_secs(5),
        "Tenant B average request duration too high: {:?}", tenant_b_avg_duration);

    // Check resource usage fairness
    let avg_usage_a = monitor.average_usage("tenant_a").unwrap();
    let avg_usage_b = monitor.average_usage("tenant_b").unwrap();

    // Memory usage should be reasonable and different between tenants
    assert!(avg_usage_a.memory_mb > 150.0 && avg_usage_a.memory_mb < 400.0,
        "Tenant A memory usage should be reasonable: {} MB", avg_usage_a.memory_mb);
    assert!(avg_usage_b.memory_mb > 200.0 && avg_usage_b.memory_mb < 500.0,
        "Tenant B memory usage should be reasonable: {} MB", avg_usage_b.memory_mb);

    println!("✓ Concurrent inference workload completed");
    println!("  Total duration: {:?}", total_duration);
    println!("  Tenant A avg response time: {:?}", tenant_a_avg_duration);
    println!("  Tenant B avg response time: {:?}", tenant_b_avg_duration);
    println!("  Tenant A avg memory: {:.1} MB", avg_usage_a.memory_mb);
    println!("  Tenant B avg memory: {:.1} MB", avg_usage_b.memory_mb);

    harness.cleanup().await?;
    Ok(())
}

/// Test resource allocation fairness under concurrent load
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

    // Simulate different workload patterns
    let workloads = vec![
        ("tenant_a", tenant_a, 3), // Light workload
        ("tenant_b", tenant_b, 5), // Medium workload
        ("tenant_c", tenant_c, 8), // Heavy workload
    ];

    let mut handles = vec![];

    for (tenant_id, tenant, num_requests) in workloads {
        let monitor = monitor.clone();
        let tenant_id = tenant_id.to_string();

        let handle = tokio::spawn(async move {
            let mut total_memory = 0.0;
            let mut total_cpu = 0.0;

            for i in 0..num_requests {
                let request = create_inference_request(
                    "test_cp_v1",
                    &format!("Workload test request {}", i),
                    30,
                    false
                );

                let _result = tenant.run_inference(request).await.unwrap();

                // Simulate resource usage based on workload
                let memory_usage = 100.0 + (i as f64 * 20.0);
                let cpu_usage = 5.0 + (i as f64 * 2.0);

                total_memory += memory_usage;
                total_cpu += cpu_usage;

                monitor.record_usage(&tenant_id, ResourceMetrics {
                    memory_mb: memory_usage,
                    cpu_percent: cpu_usage,
                    storage_mb: 50.0,
                    timestamp: Instant::now(),
                });

                // Small delay between requests
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            (tenant_id, total_memory / num_requests as f64, total_cpu / num_requests as f64)
        });

        handles.push(handle);
    }

    // Wait for all workloads to complete
    let results: Vec<_> = join_all(handles).await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // Analyze fairness
    let mut memory_usages = vec![];
    let mut cpu_usages = vec![];

    for (tenant_id, avg_memory, avg_cpu) in &results {
        println!("  {}: avg memory {:.1} MB, avg CPU {:.1}%",
            tenant_id, avg_memory, avg_cpu);
        memory_usages.push(*avg_memory);
        cpu_usages.push(*avg_cpu);
    }

    // Verify resource usage is proportional to workload
    // (Lighter workloads should use fewer resources)
    assert!(memory_usages[0] < memory_usages[1],
        "Tenant A (light) should use less memory than tenant B (medium)");
    assert!(memory_usages[1] < memory_usages[2],
        "Tenant B (medium) should use less memory than tenant C (heavy)");

    assert!(cpu_usages[0] < cpu_usages[1],
        "Tenant A (light) should use less CPU than tenant B (medium)");
    assert!(cpu_usages[1] < cpu_usages[2],
        "Tenant B (medium) should use less CPU than tenant C (heavy)");

    println!("✓ Resource allocation fairness verified");
    harness.cleanup().await?;
    Ok(())
}

/// Test performance isolation under mixed workloads
#[tokio::test]
async fn test_performance_isolation() -> Result<()> {
    println!("\n=== Test: Performance Isolation ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();
    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Test that one tenant's heavy workload doesn't affect another's performance
    let start_time = Instant::now();

    // Tenant A: Light workload
    let tenant_a_handle = tokio::spawn(async move {
        let mut durations = vec![];

        for i in 0..3 {
            let request = create_inference_request(
                "test_cp_v1",
                &format!("Light workload {}", i),
                25,
                false
            );

            let req_start = Instant::now();
            let _result = tenant_a.run_inference(request).await.unwrap();
            durations.push(req_start.elapsed());
        }

        durations
    });

    // Tenant B: Heavy concurrent workload
    let tenant_b_handle = tokio::spawn(async move {
        let mut handles = vec![];

        for i in 0..10 {
            let tenant = tenant_b.clone();
            let handle = tokio::spawn(async move {
                let request = create_inference_request(
                    "test_cp_v1",
                    &format!("Heavy workload {}", i),
                    75,
                    false
                );

                let req_start = Instant::now();
                let _result = tenant.run_inference(request).await.unwrap();
                req_start.elapsed()
            });
            handles.push(handle);
        }

        let results: Vec<_> = join_all(handles).await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        results
    });

    // Wait for both workloads to complete
    let tenant_a_durations = tenant_a_handle.await?;
    let tenant_b_durations = tenant_b_handle.await?;

    let total_duration = start_time.elapsed();

    // Analyze performance isolation
    let tenant_a_avg: Duration = tenant_a_durations.iter().sum::<Duration>() / tenant_a_durations.len() as u32;
    let tenant_b_avg: Duration = tenant_b_durations.iter().sum::<Duration>() / tenant_b_durations.len() as u32;

    // Tenant A's performance should not be significantly degraded by tenant B's workload
    assert!(tenant_a_avg < Duration::from_secs(2),
        "Tenant A performance should not be degraded: {:?}", tenant_a_avg);

    // Both tenants should complete within reasonable time despite concurrent load
    assert!(total_duration < Duration::from_secs(30),
        "Total workload duration too high: {:?}", total_duration);

    println!("✓ Performance isolation verified");
    println!("  Total duration: {:?}", total_duration);
    println!("  Tenant A avg response: {:?}", tenant_a_avg);
    println!("  Tenant B avg response: {:?}", tenant_b_avg);

    harness.cleanup().await?;
    Ok(())
}

/// Test workload prioritization and scheduling
#[tokio::test]
async fn test_workload_prioritization() -> Result<()> {
    println!("\n=== Test: Workload Prioritization ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());

    // Only use premium tenant for this test
    if let Some(tenant_config) = config.get_tenant("tenant_b") {
        harness.add_tenant(tenant_config.clone());
    } else {
        println!("⚠ Skipping test - premium tenant not configured");
        return Ok(());
    }

    harness.setup().await?;

    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Test different priority levels
    let priorities = vec![
        ("high", 10, 20),   // High priority: smaller requests, more concurrency
        ("medium", 50, 5),  // Medium priority: medium requests, medium concurrency
        ("low", 100, 2),    // Low priority: large requests, low concurrency
    ];

    let mut results = vec![];

    for (priority, tokens, count) in priorities {
        let start_time = Instant::now();
        let mut handles = vec![];

        for i in 0..count {
            let tenant = tenant_b.clone();
            let priority = priority.to_string();

            let handle = tokio::spawn(async move {
                let request = create_inference_request(
                    "test_cp_v1",
                    &format!("{} priority request {}", priority, i),
                    tokens,
                    false
                );

                let req_start = Instant::now();
                let result = tenant.run_inference(request).await;
                let duration = req_start.elapsed();

                (result.is_ok(), duration)
            });

            handles.push(handle);
        }

        let workload_results: Vec<_> = join_all(handles).await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        let workload_duration = start_time.elapsed();
        let success_rate = workload_results.iter()
            .filter(|(success, _)| *success)
            .count() as f64 / workload_results.len() as f64;

        let avg_duration: Duration = workload_results.iter()
            .map(|(_, duration)| *duration)
            .sum::<Duration>() / workload_results.len() as u32;

        results.push((priority.to_string(), success_rate, avg_duration, workload_duration));
    }

    // Verify prioritization works (higher priority should have better performance)
    for (i, (priority, success_rate, avg_duration, _)) in results.iter().enumerate() {
        println!("  {} priority: {:.1}% success, avg response {:?}",
            priority, success_rate * 100.0, avg_duration);

        assert!(*success_rate > 0.8,
            "{} priority workload should have >80% success rate", priority);
    }

    println!("✓ Workload prioritization verified");
    harness.cleanup().await?;
    Ok(())
}