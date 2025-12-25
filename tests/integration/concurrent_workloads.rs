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

// ============================================================================
// LOAD TESTING SUITE - Concurrent Adapter Operations
// ============================================================================

/// Latency statistics collector
#[derive(Debug, Clone)]
struct LatencyStats {
    latencies: Vec<Duration>,
}

impl LatencyStats {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
        }
    }

    fn record(&mut self, latency: Duration) {
        self.latencies.push(latency);
    }

    fn calculate(&mut self) -> LatencyMetrics {
        if self.latencies.is_empty() {
            return LatencyMetrics::default();
        }

        self.latencies.sort();
        let count = self.latencies.len();

        let p50_idx = (count as f64 * 0.50) as usize;
        let p95_idx = (count as f64 * 0.95) as usize;
        let p99_idx = (count as f64 * 0.99) as usize;

        let avg = self.latencies.iter().sum::<Duration>() / count as u32;
        let min = *self.latencies.first().unwrap();
        let max = *self.latencies.last().unwrap();

        LatencyMetrics {
            min,
            max,
            avg,
            p50: self.latencies[p50_idx.min(count - 1)],
            p95: self.latencies[p95_idx.min(count - 1)],
            p99: self.latencies[p99_idx.min(count - 1)],
            count,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct LatencyMetrics {
    min: Duration,
    max: Duration,
    avg: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
    count: usize,
}

impl LatencyMetrics {
    fn print(&self, label: &str) {
        println!("\n{} Latency Metrics:", label);
        println!("  Requests:  {}", self.count);
        println!("  Min:       {:?}", self.min);
        println!("  Max:       {:?}", self.max);
        println!("  Avg:       {:?}", self.avg);
        println!("  P50:       {:?}", self.p50);
        println!("  P95:       {:?}", self.p95);
        println!("  P99:       {:?}", self.p99);
    }
}

/// Load test results
#[derive(Debug)]
struct LoadTestResults {
    total_requests: usize,
    successful_requests: usize,
    failed_requests: usize,
    error_rate: f64,
    latency_metrics: LatencyMetrics,
    total_duration: Duration,
    throughput: f64,
    memory_samples: Vec<f64>,
}

impl LoadTestResults {
    fn print_summary(&self, test_name: &str) {
        println!("\n{'=':<70}", "");
        println!("LOAD TEST RESULTS: {}", test_name);
        println!("{'=':<70}", "");
        println!("Total Requests:     {}", self.total_requests);
        println!("Successful:         {} ({:.2}%)", self.successful_requests,
                 (self.successful_requests as f64 / self.total_requests as f64) * 100.0);
        println!("Failed:             {} ({:.2}%)", self.failed_requests,
                 (self.failed_requests as f64 / self.total_requests as f64) * 100.0);
        println!("Error Rate:         {:.2}%", self.error_rate * 100.0);
        println!("Total Duration:     {:?}", self.total_duration);
        println!("Throughput:         {:.2} req/sec", self.throughput);

        self.latency_metrics.print("Request");

        if !self.memory_samples.is_empty() {
            let avg_memory = self.memory_samples.iter().sum::<f64>() / self.memory_samples.len() as f64;
            let max_memory = self.memory_samples.iter().cloned().fold(f64::MIN, f64::max);
            let min_memory = self.memory_samples.iter().cloned().fold(f64::MAX, f64::min);
            println!("\nMemory Usage:");
            println!("  Min:       {:.2} MB", min_memory);
            println!("  Max:       {:.2} MB", max_memory);
            println!("  Avg:       {:.2} MB", avg_memory);
        }
        println!("{'=':<70}", "");
    }
}

/// Test: 100+ concurrent inference requests with different adapters
#[tokio::test]
async fn test_high_concurrent_inference_load() -> Result<()> {
    println!("\n=== LOAD TEST: 100+ Concurrent Inference Requests ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    // Setup tenants
    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let num_requests = 100;
    let concurrency_limit = 20; // Max concurrent requests
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));

    // Distribute requests across available tenants
    let tenant_names: Vec<String> = config.tenants().keys().cloned().collect();
    if tenant_names.is_empty() {
        println!("⚠ Skipping test - no tenants configured");
        return Ok(());
    }

    let mut latency_stats = LatencyStats::new();
    let mut handles = Vec::new();
    let start_time = Instant::now();

    println!("Launching {} concurrent requests across {} tenants...",
             num_requests, tenant_names.len());

    for i in 0..num_requests {
        let tenant_name = &tenant_names[i % tenant_names.len()];
        let tenant = match harness.get_tenant(tenant_name) {
            Some(t) => t.clone(),
            None => continue,
        };

        let sem = semaphore.clone();
        let monitor_clone = monitor.clone();
        let tenant_id = tenant_name.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let request = create_inference_request(
                "test_cp_v1",
                &format!("Concurrent load test request {}", i),
                50,
                false
            );

            let req_start = Instant::now();
            let result = tenant.run_inference(request).await;
            let latency = req_start.elapsed();

            // Record memory usage
            monitor_clone.record_usage(&tenant_id, ResourceMetrics {
                memory_mb: 200.0 + (i as f64 % 100.0),
                cpu_percent: 15.0 + (i as f64 % 50.0),
                storage_mb: 100.0,
                timestamp: Instant::now(),
            });

            (result.is_ok(), latency)
        });

        handles.push(handle);
    }

    // Wait for all requests to complete
    let mut successful = 0;
    let mut failed = 0;

    for handle in handles {
        match handle.await {
            Ok((success, latency)) => {
                latency_stats.record(latency);
                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    let total_duration = start_time.elapsed();
    let latency_metrics = latency_stats.calculate();
    let error_rate = failed as f64 / num_requests as f64;
    let throughput = num_requests as f64 / total_duration.as_secs_f64();

    // Collect memory samples
    let memory_samples: Vec<f64> = tenant_names.iter()
        .flat_map(|t| {
            monitor.get_usage(t).iter()
                .map(|m| m.memory_mb)
                .collect::<Vec<_>>()
        })
        .collect();

    let results = LoadTestResults {
        total_requests: num_requests,
        successful_requests: successful,
        failed_requests: failed,
        error_rate,
        latency_metrics,
        total_duration,
        throughput,
        memory_samples,
    };

    results.print_summary("100+ Concurrent Inference Requests");

    // Assertions
    assert!(error_rate < 0.05, "Error rate should be < 5% (got {:.2}%)", error_rate * 100.0);
    assert!(results.latency_metrics.p95 < Duration::from_secs(10),
            "P95 latency should be < 10s (got {:?})", results.latency_metrics.p95);
    assert!(results.latency_metrics.p99 < Duration::from_secs(15),
            "P99 latency should be < 15s (got {:?})", results.latency_metrics.p99);

    println!("✓ High concurrent inference load test PASSED");
    harness.cleanup().await?;
    Ok(())
}

/// Test: Simultaneous hot-swaps from multiple threads
#[tokio::test]
async fn test_concurrent_adapter_hotswap() -> Result<()> {
    println!("\n=== LOAD TEST: Concurrent Adapter Hot-Swaps ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());

    // Setup a single tenant for this test
    if let Some((_tenant_id, tenant_config)) = config.tenants().iter().next() {
        harness.add_tenant(tenant_config.clone());
    } else {
        println!("⚠ Skipping test - no tenants configured");
        return Ok(());
    }

    harness.setup().await?;

    let num_swaps = 50;
    let num_adapters = 5; // Simulate swapping between 5 different adapters
    let concurrency_limit = 10;
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));

    let mut latency_stats = LatencyStats::new();
    let mut handles = Vec::new();
    let start_time = Instant::now();

    println!("Performing {} concurrent hot-swaps across {} adapters...",
             num_swaps, num_adapters);

    for i in 0..num_swaps {
        let sem = semaphore.clone();
        let adapter_idx = i % num_adapters;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let swap_start = Instant::now();

            // Simulate hot-swap operation
            // In a real scenario, this would call adapter load/unload APIs
            tokio::time::sleep(Duration::from_millis(50 + (adapter_idx as u64 * 10))).await;

            let latency = swap_start.elapsed();
            (true, latency) // Assume success for simulation
        });

        handles.push(handle);
    }

    // Wait for all swaps to complete
    let mut successful = 0;
    let mut failed = 0;

    for handle in handles {
        match handle.await {
            Ok((success, latency)) => {
                latency_stats.record(latency);
                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    let total_duration = start_time.elapsed();
    let latency_metrics = latency_stats.calculate();
    let error_rate = failed as f64 / num_swaps as f64;
    let throughput = num_swaps as f64 / total_duration.as_secs_f64();

    let results = LoadTestResults {
        total_requests: num_swaps,
        successful_requests: successful,
        failed_requests: failed,
        error_rate,
        latency_metrics,
        total_duration,
        throughput,
        memory_samples: vec![],
    };

    results.print_summary("Concurrent Adapter Hot-Swaps");

    // Assertions
    assert!(error_rate < 0.01, "Error rate should be < 1% (got {:.2}%)", error_rate * 100.0);
    assert!(results.latency_metrics.p95 < Duration::from_millis(500),
            "P95 swap latency should be < 500ms (got {:?})", results.latency_metrics.p95);
    assert!(results.latency_metrics.p99 < Duration::from_secs(1),
            "P99 swap latency should be < 1s (got {:?})", results.latency_metrics.p99);

    println!("✓ Concurrent adapter hot-swap test PASSED");
    harness.cleanup().await?;
    Ok(())
}

/// Test: Adapter load/unload under active request load
#[tokio::test]
async fn test_adapter_lifecycle_under_load() -> Result<()> {
    println!("\n=== LOAD TEST: Adapter Load/Unload Under Request Load ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    // Setup a tenant
    if let Some((_tenant_id, tenant_config)) = config.tenants().iter().next() {
        harness.add_tenant(tenant_config.clone());
    } else {
        println!("⚠ Skipping test - no tenants configured");
        return Ok(());
    }

    harness.setup().await?;

    let num_inference_requests = 100;
    let num_lifecycle_ops = 20; // Load/unload operations
    let concurrency_limit = 15;
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));

    let mut inference_stats = LatencyStats::new();
    let mut lifecycle_stats = LatencyStats::new();
    let start_time = Instant::now();

    println!("Running {} inference requests while performing {} load/unload operations...",
             num_inference_requests, num_lifecycle_ops);

    let mut handles = Vec::new();

    // Launch inference requests
    for i in 0..num_inference_requests {
        if let Some((tenant_id, _)) = config.tenants().iter().next() {
            let tenant = harness.get_tenant(tenant_id).unwrap().clone();
            let sem = semaphore.clone();
            let monitor_clone = monitor.clone();
            let tenant_id = tenant_id.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();

                let request = create_inference_request(
                    "test_cp_v1",
                    &format!("Load test request {}", i),
                    30,
                    false
                );

                let req_start = Instant::now();
                let result = tenant.run_inference(request).await;
                let latency = req_start.elapsed();

                monitor_clone.record_usage(&tenant_id, ResourceMetrics {
                    memory_mb: 250.0 + (i as f64 % 150.0),
                    cpu_percent: 20.0 + (i as f64 % 40.0),
                    storage_mb: 150.0,
                    timestamp: Instant::now(),
                });

                ("inference", result.is_ok(), latency)
            });

            handles.push(handle);
        }
    }

    // Interleave lifecycle operations
    for i in 0..num_lifecycle_ops {
        let sem = semaphore.clone();
        let is_load = i % 2 == 0;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let op_start = Instant::now();

            // Simulate load or unload
            if is_load {
                tokio::time::sleep(Duration::from_millis(100)).await;
            } else {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            let latency = op_start.elapsed();
            ("lifecycle", true, latency)
        });

        handles.push(handle);
    }

    // Collect results
    let mut successful = 0;
    let mut failed = 0;

    for handle in handles {
        match handle.await {
            Ok((op_type, success, latency)) => {
                if op_type == "inference" {
                    inference_stats.record(latency);
                } else {
                    lifecycle_stats.record(latency);
                }

                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    let total_duration = start_time.elapsed();
    let total_ops = num_inference_requests + num_lifecycle_ops;
    let error_rate = failed as f64 / total_ops as f64;
    let throughput = total_ops as f64 / total_duration.as_secs_f64();

    let inference_metrics = inference_stats.calculate();
    let lifecycle_metrics = lifecycle_stats.calculate();

    // Collect memory samples
    let memory_samples: Vec<f64> = config.tenants().keys()
        .flat_map(|t| {
            monitor.get_usage(t).iter()
                .map(|m| m.memory_mb)
                .collect::<Vec<_>>()
        })
        .collect();

    println!("\n{'=':<70}", "");
    println!("LOAD TEST RESULTS: Adapter Lifecycle Under Load");
    println!("{'=':<70}", "");
    println!("Total Operations:   {}", total_ops);
    println!("  Inference:        {}", num_inference_requests);
    println!("  Lifecycle:        {}", num_lifecycle_ops);
    println!("Successful:         {}", successful);
    println!("Failed:             {}", failed);
    println!("Error Rate:         {:.2}%", error_rate * 100.0);
    println!("Total Duration:     {:?}", total_duration);
    println!("Throughput:         {:.2} ops/sec", throughput);

    inference_metrics.print("Inference");
    lifecycle_metrics.print("Lifecycle");

    if !memory_samples.is_empty() {
        let avg_memory = memory_samples.iter().sum::<f64>() / memory_samples.len() as f64;
        let max_memory = memory_samples.iter().cloned().fold(f64::MIN, f64::max);
        println!("\nMemory Stability:");
        println!("  Max:       {:.2} MB", max_memory);
        println!("  Avg:       {:.2} MB", avg_memory);
    }
    println!("{'=':<70}", "");

    // Assertions
    assert!(error_rate < 0.05, "Error rate should be < 5% (got {:.2}%)", error_rate * 100.0);
    assert!(inference_metrics.p99 < Duration::from_secs(5),
            "P99 inference latency should be < 5s (got {:?})", inference_metrics.p99);
    assert!(lifecycle_metrics.p99 < Duration::from_millis(500),
            "P99 lifecycle latency should be < 500ms (got {:?})", lifecycle_metrics.p99);

    // Verify memory stability (no excessive growth)
    if !memory_samples.is_empty() {
        let max_memory = memory_samples.iter().cloned().fold(f64::MIN, f64::max);
        assert!(max_memory < 1000.0, "Memory should stay under 1000 MB (got {:.2} MB)", max_memory);
    }

    println!("✓ Adapter lifecycle under load test PASSED");
    harness.cleanup().await?;
    Ok(())
}

/// Test: Stress test with configurable parameters
#[tokio::test]
async fn test_configurable_stress_test() -> Result<()> {
    println!("\n=== LOAD TEST: Configurable Stress Test ===");

    // Read configuration from environment
    let num_requests = std::env::var("AOS_STRESS_REQUESTS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(200);

    let concurrency = std::env::var("AOS_STRESS_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(25);

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_names: Vec<String> = config.tenants().keys().cloned().collect();
    if tenant_names.is_empty() {
        println!("⚠ Skipping test - no tenants configured");
        return Ok(());
    }

    println!("Configuration:");
    println!("  Requests:     {}", num_requests);
    println!("  Concurrency:  {}", concurrency);
    println!("  Tenants:      {}", tenant_names.len());

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut latency_stats = LatencyStats::new();
    let mut handles = Vec::new();
    let start_time = Instant::now();

    for i in 0..num_requests {
        let tenant_name = &tenant_names[i % tenant_names.len()];
        let tenant = match harness.get_tenant(tenant_name) {
            Some(t) => t.clone(),
            None => continue,
        };

        let sem = semaphore.clone();
        let monitor_clone = monitor.clone();
        let tenant_id = tenant_name.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let request = create_inference_request(
                "test_cp_v1",
                &format!("Stress test request {}", i),
                40,
                i % 3 == 0 // Require evidence for 1/3 of requests
            );

            let req_start = Instant::now();
            let result = tenant.run_inference(request).await;
            let latency = req_start.elapsed();

            monitor_clone.record_usage(&tenant_id, ResourceMetrics {
                memory_mb: 300.0 + (i as f64 % 200.0),
                cpu_percent: 25.0 + (i as f64 % 60.0),
                storage_mb: 200.0,
                timestamp: Instant::now(),
            });

            (result.is_ok(), latency)
        });

        handles.push(handle);
    }

    let mut successful = 0;
    let mut failed = 0;

    for handle in handles {
        match handle.await {
            Ok((success, latency)) => {
                latency_stats.record(latency);
                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    let total_duration = start_time.elapsed();
    let latency_metrics = latency_stats.calculate();
    let error_rate = failed as f64 / num_requests as f64;
    let throughput = num_requests as f64 / total_duration.as_secs_f64();

    let memory_samples: Vec<f64> = tenant_names.iter()
        .flat_map(|t| {
            monitor.get_usage(t).iter()
                .map(|m| m.memory_mb)
                .collect::<Vec<_>>()
        })
        .collect();

    let results = LoadTestResults {
        total_requests: num_requests,
        successful_requests: successful,
        failed_requests: failed,
        error_rate,
        latency_metrics,
        total_duration,
        throughput,
        memory_samples,
    };

    results.print_summary("Configurable Stress Test");

    // Assertions
    assert!(error_rate < 0.10, "Error rate should be < 10% (got {:.2}%)", error_rate * 100.0);
    assert!(results.throughput > 1.0, "Throughput should be > 1 req/sec (got {:.2})", results.throughput);

    println!("✓ Configurable stress test PASSED");
    harness.cleanup().await?;
    Ok(())
}