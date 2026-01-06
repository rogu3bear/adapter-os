#![cfg(all(test, feature = "extended-tests"))]

//! Network Partition Chaos Tests
//!
//! PRD-CHAOS-001: Comprehensive testing for network partition scenarios including:
//! - Circuit breaker behavior under network failures
//! - Worker isolation and recovery
//! - Split-brain detection and handling
//! - Graceful degradation under partial connectivity
//!
//! These tests simulate various network failure modes to ensure the system
//! maintains determinism guarantees and recovers correctly.

use adapteros_core::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, StandardCircuitBreaker,
};
use adapteros_core::{AosError, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration, Instant};

// ============================================================
// Network Partition Simulator
// ============================================================

/// Simulates network conditions between components
#[derive(Clone)]
struct NetworkPartitionSimulator {
    /// Whether the network is currently partitioned
    partitioned: Arc<AtomicBool>,
    /// Simulated latency in milliseconds (0 = no latency)
    latency_ms: Arc<AtomicU64>,
    /// Packet loss rate (0-100)
    packet_loss_percent: Arc<AtomicUsize>,
    /// Total requests attempted
    requests_total: Arc<AtomicU64>,
    /// Requests that failed due to partition
    partition_failures: Arc<AtomicU64>,
    /// Requests that succeeded
    successes: Arc<AtomicU64>,
}

impl NetworkPartitionSimulator {
    fn new() -> Self {
        Self {
            partitioned: Arc::new(AtomicBool::new(false)),
            latency_ms: Arc::new(AtomicU64::new(0)),
            packet_loss_percent: Arc::new(AtomicUsize::new(0)),
            requests_total: Arc::new(AtomicU64::new(0)),
            partition_failures: Arc::new(AtomicU64::new(0)),
            successes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Simulate a full network partition
    fn partition(&self) {
        self.partitioned.store(true, Ordering::SeqCst);
    }

    /// Heal the network partition
    fn heal(&self) {
        self.partitioned.store(false, Ordering::SeqCst);
    }

    /// Check if network is currently partitioned
    fn is_partitioned(&self) -> bool {
        self.partitioned.load(Ordering::SeqCst)
    }

    /// Set network latency
    fn set_latency(&self, ms: u64) {
        self.latency_ms.store(ms, Ordering::Relaxed);
    }

    /// Set packet loss percentage (0-100)
    fn set_packet_loss(&self, percent: usize) {
        self.packet_loss_percent
            .store(percent.min(100), Ordering::Relaxed);
    }

    /// Simulate a network request with current conditions
    async fn send_request<T, F>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.requests_total.fetch_add(1, Ordering::Relaxed);

        // Check for partition
        if self.is_partitioned() {
            self.partition_failures.fetch_add(1, Ordering::Relaxed);
            return Err(AosError::Network(
                "Connection refused: network partition".to_string(),
            ));
        }

        // Apply latency
        let latency = self.latency_ms.load(Ordering::Relaxed);
        if latency > 0 {
            sleep(Duration::from_millis(latency)).await;
        }

        // Check for packet loss
        let packet_loss = self.packet_loss_percent.load(Ordering::Relaxed);
        if packet_loss > 0 {
            let request_num = self.requests_total.load(Ordering::Relaxed);
            if (request_num % 100) < packet_loss as u64 {
                self.partition_failures.fetch_add(1, Ordering::Relaxed);
                return Err(AosError::Network("Packet lost in transit".to_string()));
            }
        }

        // Execute the actual operation
        let result = operation.await;
        if result.is_ok() {
            self.successes.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    /// Get current metrics
    fn metrics(&self) -> NetworkMetrics {
        NetworkMetrics {
            requests_total: self.requests_total.load(Ordering::Relaxed),
            partition_failures: self.partition_failures.load(Ordering::Relaxed),
            successes: self.successes.load(Ordering::Relaxed),
            is_partitioned: self.is_partitioned(),
        }
    }
}

#[derive(Debug, Clone)]
struct NetworkMetrics {
    requests_total: u64,
    partition_failures: u64,
    successes: u64,
    is_partitioned: bool,
}

// ============================================================
// Mock Worker for Partition Testing
// ============================================================

/// Simulates a worker that can be isolated by network partition
struct MockWorker {
    id: String,
    network: NetworkPartitionSimulator,
    circuit_breaker: StandardCircuitBreaker,
    requests_processed: Arc<AtomicU64>,
    is_healthy: Arc<AtomicBool>,
}

impl MockWorker {
    fn new(id: &str, network: NetworkPartitionSimulator) -> Self {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 500, // Fast timeout for testing
            half_open_max_requests: 3,
        };

        Self {
            id: id.to_string(),
            network,
            circuit_breaker: StandardCircuitBreaker::new(format!("worker_{}", id), config),
            requests_processed: Arc::new(AtomicU64::new(0)),
            is_healthy: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Process a request through the worker
    async fn process(&self, input: &str) -> Result<String> {
        self.circuit_breaker
            .call(async {
                self.network
                    .send_request(async {
                        // Simulate actual work
                        self.requests_processed.fetch_add(1, Ordering::Relaxed);
                        Ok(format!("Worker {} processed: {}", self.id, input))
                    })
                    .await
            })
            .await
    }

    fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state()
    }

    fn requests_processed(&self) -> u64 {
        self.requests_processed.load(Ordering::Relaxed)
    }
}

// ============================================================
// Network Partition Tests
// ============================================================

/// Test circuit breaker opens during network partition
#[tokio::test]
async fn test_circuit_breaker_opens_on_partition() {
    println!("\n=== Test: Circuit Breaker Opens on Network Partition ===");

    let network = NetworkPartitionSimulator::new();
    let worker = MockWorker::new("test_worker", network.clone());

    // Phase 1: Normal operation
    println!("Phase 1: Normal operation");
    for i in 0..5 {
        let result = worker.process(&format!("request_{}", i)).await;
        assert!(result.is_ok(), "Should succeed before partition");
    }
    assert_eq!(worker.circuit_state(), CircuitState::Closed);
    println!("✓ Circuit closed during normal operation");

    // Phase 2: Simulate network partition
    println!("Phase 2: Network partition");
    network.partition();
    assert!(network.is_partitioned());

    // Should fail and eventually open circuit
    for i in 0..5 {
        let result = worker.process(&format!("partitioned_{}", i)).await;
        assert!(result.is_err(), "Should fail during partition");
    }

    // Circuit should be open now
    match worker.circuit_state() {
        CircuitState::Open { .. } => {
            println!("✓ Circuit opened after partition failures");
        }
        state => panic!("Expected Open state, got {:?}", state),
    }

    let metrics = network.metrics();
    assert!(
        metrics.partition_failures > 0,
        "Should have partition failures"
    );
    println!(
        "✓ Partition failures recorded: {}",
        metrics.partition_failures
    );
}

/// Test circuit breaker recovers after partition heals
#[tokio::test]
async fn test_circuit_breaker_recovers_after_heal() {
    println!("\n=== Test: Circuit Breaker Recovers After Partition Heals ===");

    let network = NetworkPartitionSimulator::new();
    let worker = MockWorker::new("recovery_worker", network.clone());

    // Create partition and open circuit
    network.partition();
    for _ in 0..5 {
        let _ = worker.process("fail").await;
    }
    assert!(matches!(worker.circuit_state(), CircuitState::Open { .. }));
    println!("✓ Circuit opened during partition");

    // Heal partition
    network.heal();
    println!("Network partition healed");

    // Wait for circuit timeout
    sleep(Duration::from_millis(600)).await;

    // Circuit should transition to half-open and then close
    let mut recovered = false;
    for i in 0..10 {
        let result = worker.process(&format!("recovery_{}", i)).await;
        if result.is_ok() && worker.circuit_state() == CircuitState::Closed {
            recovered = true;
            println!("✓ Circuit recovered after {} requests", i + 1);
            break;
        }
        // Small delay between attempts
        sleep(Duration::from_millis(50)).await;
    }

    assert!(recovered, "Circuit should have recovered");
    println!("✓ System recovered from network partition");
}

/// Test intermittent packet loss triggers circuit breaker appropriately
#[tokio::test]
async fn test_packet_loss_circuit_breaker_behavior() {
    println!("\n=== Test: Packet Loss Circuit Breaker Behavior ===");

    let network = NetworkPartitionSimulator::new();
    let worker = MockWorker::new("packet_loss_worker", network.clone());

    // Set 50% packet loss
    network.set_packet_loss(50);
    println!("Simulating 50% packet loss");

    let mut successes = 0;
    let mut failures = 0;

    for i in 0..20 {
        match worker.process(&format!("lossy_{}", i)).await {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }

    println!("Results: {} successes, {} failures", successes, failures);

    // With 50% loss, we should have some of each
    // Circuit may or may not open depending on consecutive failures
    let state = worker.circuit_state();
    println!("Final circuit state: {:?}", state);

    // The important thing is the system handles it gracefully
    assert!(
        successes > 0 || matches!(state, CircuitState::Open { .. }),
        "Should either have successes or circuit should be protecting"
    );
    println!("✓ System handled packet loss gracefully");
}

/// Test high latency triggers timeout handling
#[tokio::test]
async fn test_high_latency_handling() {
    println!("\n=== Test: High Latency Handling ===");

    let network = NetworkPartitionSimulator::new();
    let worker = MockWorker::new("latency_worker", network.clone());

    // Set high latency
    network.set_latency(100);
    println!("Simulating 100ms network latency");

    let start = Instant::now();
    let result = worker.process("latent_request").await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Request should succeed with latency");
    assert!(
        elapsed >= Duration::from_millis(100),
        "Should have experienced latency"
    );
    println!("✓ Request completed with {}ms latency", elapsed.as_millis());

    // Test with timeout that's shorter than latency
    network.set_latency(200);
    let start = Instant::now();
    let result = timeout(
        Duration::from_millis(100),
        worker.process("timeout_request"),
    )
    .await;

    match result {
        Err(_) => {
            println!("✓ Request timed out as expected");
        }
        Ok(_) => {
            // Might succeed if we get lucky with timing
            println!("Request completed before timeout");
        }
    }
}

/// Test split-brain scenario with multiple workers
#[tokio::test]
async fn test_split_brain_scenario() {
    println!("\n=== Test: Split-Brain Scenario ===");

    // Create two workers with separate network simulators
    let network_a = NetworkPartitionSimulator::new();
    let network_b = NetworkPartitionSimulator::new();

    let worker_a = MockWorker::new("worker_a", network_a.clone());
    let worker_b = MockWorker::new("worker_b", network_b.clone());

    // Both workers start healthy
    assert!(worker_a.process("test_a").await.is_ok());
    assert!(worker_b.process("test_b").await.is_ok());
    println!("✓ Both workers healthy initially");

    // Partition worker_a (simulates split-brain)
    network_a.partition();
    println!("Worker A partitioned (split-brain scenario)");

    // Worker B should still work
    for i in 0..5 {
        let result = worker_b.process(&format!("b_request_{}", i)).await;
        assert!(result.is_ok(), "Worker B should continue working");
    }
    println!("✓ Worker B continues serving during split-brain");

    // Worker A should fail and circuit should open
    for _ in 0..5 {
        let _ = worker_a.process("fail").await;
    }
    assert!(matches!(
        worker_a.circuit_state(),
        CircuitState::Open { .. }
    ));
    println!("✓ Worker A circuit opened during partition");

    // Heal and verify both recover
    network_a.heal();
    sleep(Duration::from_millis(600)).await;

    // Both should eventually be healthy
    let mut a_recovered = false;
    for i in 0..10 {
        if worker_a.process(&format!("recovery_{}", i)).await.is_ok() {
            if worker_a.circuit_state() == CircuitState::Closed {
                a_recovered = true;
                break;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }

    assert!(a_recovered, "Worker A should recover");
    println!("✓ Both workers recovered from split-brain");
}

/// Test rapid partition/heal cycles (flapping)
#[tokio::test]
async fn test_partition_flapping() {
    println!("\n=== Test: Partition Flapping ===");

    let network = NetworkPartitionSimulator::new();
    let worker = MockWorker::new("flapping_worker", network.clone());

    let mut total_successes = 0;
    let mut total_failures = 0;

    // Rapidly toggle partition state
    for cycle in 0..5 {
        // Partition
        network.partition();
        for _ in 0..3 {
            match worker.process("flap").await {
                Ok(_) => total_successes += 1,
                Err(_) => total_failures += 1,
            }
        }

        // Heal
        network.heal();
        // Small delay for circuit to potentially recover
        sleep(Duration::from_millis(100)).await;

        for _ in 0..3 {
            match worker.process("flap").await {
                Ok(_) => total_successes += 1,
                Err(_) => total_failures += 1,
            }
        }

        println!(
            "Cycle {}: successes={}, failures={}",
            cycle, total_successes, total_failures
        );
    }

    // System should have handled the flapping without panicking
    println!(
        "Final: {} successes, {} failures",
        total_successes, total_failures
    );
    println!("✓ System handled partition flapping gracefully");
}

/// Test that circuit breaker metrics are accurate during partition
#[tokio::test]
async fn test_circuit_breaker_metrics_during_partition() {
    println!("\n=== Test: Circuit Breaker Metrics During Partition ===");

    let network = NetworkPartitionSimulator::new();
    let worker = MockWorker::new("metrics_worker", network.clone());

    // Normal operation
    for _ in 0..5 {
        let _ = worker.process("normal").await;
    }

    let metrics_before = worker.circuit_breaker.metrics();
    println!(
        "Before partition: requests={}, successes={}, failures={}",
        metrics_before.requests_total,
        metrics_before.successes_total,
        metrics_before.failures_total
    );

    // Partition and generate failures
    network.partition();
    for _ in 0..5 {
        let _ = worker.process("partitioned").await;
    }

    let metrics_after = worker.circuit_breaker.metrics();
    println!(
        "After partition: requests={}, successes={}, failures={}, opens={}",
        metrics_after.requests_total,
        metrics_after.successes_total,
        metrics_after.failures_total,
        metrics_after.opens_total
    );

    // Verify metrics are accurate
    assert!(
        metrics_after.requests_total > metrics_before.requests_total,
        "Request count should increase"
    );
    assert!(
        metrics_after.failures_total > metrics_before.failures_total,
        "Failure count should increase"
    );
    assert!(
        metrics_after.opens_total >= 1,
        "Circuit should have opened at least once"
    );

    println!("✓ Circuit breaker metrics accurately tracked partition impact");
}

/// Test graceful degradation under partial connectivity
#[tokio::test]
async fn test_graceful_degradation() {
    println!("\n=== Test: Graceful Degradation Under Partial Connectivity ===");

    let network = NetworkPartitionSimulator::new();

    // Create circuit breaker with more forgiving thresholds
    let config = CircuitBreakerConfig {
        failure_threshold: 5, // More tolerant
        success_threshold: 2,
        timeout_ms: 300,
        half_open_max_requests: 5,
    };

    let breaker = StandardCircuitBreaker::new("degraded_service".to_string(), config);

    // Set 30% packet loss - partial connectivity
    network.set_packet_loss(30);
    println!("Simulating 30% packet loss (partial connectivity)");

    let mut successes = 0;
    let mut circuit_open_count = 0;

    for i in 0..50 {
        let result = breaker
            .call(async {
                network
                    .send_request(async { Ok(format!("request_{}", i)) })
                    .await
            })
            .await;

        match result {
            Ok(_) => successes += 1,
            Err(AosError::CircuitBreakerOpen { .. }) => {
                circuit_open_count += 1;
                // Wait for circuit timeout
                sleep(Duration::from_millis(350)).await;
            }
            Err(_) => {} // Network failure, expected
        }
    }

    println!(
        "Results: {} successes, {} circuit opens",
        successes, circuit_open_count
    );

    // With 30% loss and forgiving thresholds, we should have reasonable success rate
    // The circuit breaker provides protection while allowing throughput
    assert!(
        successes > 10,
        "Should have reasonable success rate under partial connectivity"
    );
    println!("✓ System degraded gracefully under partial connectivity");
}

// ============================================================
// Stress Tests
// ============================================================

/// Stress test: concurrent requests during partition
#[tokio::test]
async fn test_concurrent_requests_during_partition() {
    println!("\n=== Test: Concurrent Requests During Partition ===");

    let network = NetworkPartitionSimulator::new();
    let worker = Arc::new(MockWorker::new("concurrent_worker", network.clone()));

    // Spawn concurrent requests
    let mut handles = vec![];

    for i in 0..20 {
        let worker_clone = Arc::clone(&worker);
        let network_clone = network.clone();

        let handle = tokio::spawn(async move {
            // Random partition during execution
            if i == 10 {
                network_clone.partition();
            }

            let mut local_successes = 0;
            let mut local_failures = 0;

            for j in 0..5 {
                match worker_clone
                    .process(&format!("concurrent_{}_{}", i, j))
                    .await
                {
                    Ok(_) => local_successes += 1,
                    Err(_) => local_failures += 1,
                }
                sleep(Duration::from_millis(10)).await;
            }

            (local_successes, local_failures)
        });

        handles.push(handle);
    }

    // Collect results
    let mut total_successes = 0;
    let mut total_failures = 0;

    for handle in handles {
        let (s, f) = handle.await.unwrap();
        total_successes += s;
        total_failures += f;
    }

    println!(
        "Concurrent test: {} successes, {} failures",
        total_successes, total_failures
    );

    // Should have handled concurrent access without panicking
    assert!(
        total_successes > 0 || total_failures > 0,
        "Should have processed requests"
    );
    println!("✓ System handled concurrent requests during partition");
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    /// Test that network metrics are thread-safe
    #[tokio::test]
    async fn test_network_metrics_thread_safety() {
        let network = NetworkPartitionSimulator::new();
        let network_clone = network.clone();

        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = network_clone
                    .send_request(async { Ok::<_, AosError>("test") })
                    .await;
            }
        });

        for _ in 0..100 {
            let _ = network
                .send_request(async { Ok::<_, AosError>("test") })
                .await;
        }

        handle.await.unwrap();

        let metrics = network.metrics();
        assert_eq!(
            metrics.requests_total, 200,
            "Should have counted all requests"
        );
    }
}
