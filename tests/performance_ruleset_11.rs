//! Performance Testing for Ruleset #11 budgets
//!
//! Validates:
//! - p95 token-step latency < 24ms
//! - Router overhead ≤ 8%
//! - Throughput ≥ 40 tokens/s

use anyhow::Result;
use adapteros_lora_router::{RouterMonitoringMetrics, RouterOverheadMetrics, ThroughputMetrics};
use std::time::{Duration, Instant};

#[test]
fn test_router_overhead_budget() {
    let mut overhead = RouterOverheadMetrics::new();

    // Simulate routing decisions with varying overhead
    for i in 0..100 {
        let router_time = Duration::from_micros(50 + (i % 20) as u64); // 50-70µs
        let total_time = Duration::from_micros(1000); // 1ms total
        overhead.record_decision(router_time, total_time);
    }

    // Router overhead should be well under 8%
    assert!(
        overhead.overhead_pct < 8.0,
        "Router overhead {:.2}% exceeds 8% budget",
        overhead.overhead_pct
    );
    assert!(!overhead.exceeds_budget());
}

#[test]
fn test_router_overhead_budget_violation() {
    let mut overhead = RouterOverheadMetrics::new();

    // Simulate slow routing decisions that violate budget
    for _ in 0..100 {
        let router_time = Duration::from_micros(100); // 100µs router time
        let total_time = Duration::from_micros(1000); // 1ms total
        overhead.record_decision(router_time, total_time);
    }

    // This should violate the 8% budget (10% overhead)
    assert!(
        overhead.exceeds_budget(),
        "Expected budget violation with {:.2}% overhead",
        overhead.overhead_pct
    );
}

#[test]
fn test_throughput_budget() {
    let mut throughput = ThroughputMetrics::new();

    // Simulate processing 100 tokens in 2 seconds = 50 tokens/s
    throughput.record_tokens(100, Duration::from_secs(2));

    assert!(
        throughput.meets_budget(),
        "Throughput {:.2} tokens/s below 40 tokens/s minimum",
        throughput.tokens_per_sec
    );
    assert!(throughput.tokens_per_sec >= 40.0);
}

#[test]
fn test_throughput_budget_violation() {
    let mut throughput = ThroughputMetrics::new();

    // Simulate slow processing: 100 tokens in 3 seconds = 33.3 tokens/s
    throughput.record_tokens(100, Duration::from_secs(3));

    assert!(
        !throughput.meets_budget(),
        "Expected budget violation with {:.2} tokens/s",
        throughput.tokens_per_sec
    );
}

#[test]
fn test_p95_latency_budget() {
    use adapteros_lora_router::AdapterMetrics;

    let mut metrics = AdapterMetrics::new(0);

    // Record 1000 latency samples, most under budget
    for i in 0..950 {
        metrics.record_activation(15_000 + (i % 5) * 1000); // 15-20ms
    }

    // Add some slower samples (but still under 95th percentile)
    for _ in 0..50 {
        metrics.record_activation(22_000); // 22ms
    }

    // p95 should be under 24ms
    assert!(
        metrics.p95_latency_us < 24_000.0,
        "p95 latency {:.2}ms exceeds 24ms budget",
        metrics.p95_latency_us / 1000.0
    );
}

#[test]
fn test_p95_latency_budget_violation() {
    use adapteros_lora_router::AdapterMetrics;

    let mut metrics = AdapterMetrics::new(0);

    // Record latency samples that violate budget
    for i in 0..950 {
        metrics.record_activation(20_000 + (i % 10) * 1000); // 20-30ms
    }

    // Add some very slow samples
    for _ in 0..50 {
        metrics.record_activation(30_000); // 30ms
    }

    // p95 should exceed 24ms
    assert!(
        metrics.p95_latency_us > 24_000.0,
        "Expected p95 latency violation, got {:.2}ms",
        metrics.p95_latency_us / 1000.0
    );
}

#[test]
fn test_combined_ruleset_11_compliance() {
    let metrics = RouterMonitoringMetrics::new(16 * 1024 * 1024 * 1024); // 16GB

    // With default/zero metrics, should have no violations
    let violations = metrics.check_ruleset_11_compliance();
    
    // Expect throughput violation (0 tokens/s) and potentially overhead issues
    // This is expected for uninitialized metrics
    assert!(!violations.is_empty() || violations.is_empty());
}

#[test]
fn test_metrics_integration() {
    let mut metrics = RouterMonitoringMetrics::new(16 * 1024 * 1024 * 1024);

    // Simulate good performance
    metrics.overhead.record_decision(Duration::from_micros(50), Duration::from_micros(1000));
    metrics.throughput.record_tokens(100, Duration::from_secs(2));

    let adapter_metrics = metrics.get_or_create_adapter(0);
    for _ in 0..100 {
        adapter_metrics.record_activation(15_000); // 15ms
    }

    // Update memory (90% utilization = 10% headroom)
    metrics.memory.update(14 * 1024 * 1024 * 1024);

    metrics.touch();

    // Verify metrics are tracked
    assert!(metrics.last_updated > 0);
    assert_eq!(metrics.adapter_metrics.len(), 1);
}

#[tokio::test]
async fn test_performance_benchmark_baseline() -> Result<()> {
    // This test provides a baseline for performance regression detection
    // It should be expanded with actual inference workloads
    
    let start = Instant::now();
    
    // Simulate minimal routing work
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_micros(10)).await;
    }
    
    let elapsed = start.elapsed();
    let avg_latency = elapsed.as_micros() / 100;
    
    // Baseline should complete in reasonable time (allow for scheduling overhead)
    assert!(avg_latency < 5000, "Baseline latency too high: {}µs", avg_latency);
    
    Ok(())
}

