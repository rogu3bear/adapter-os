#![cfg(all(test, feature = "extended-tests"))]

//! Operational Resilience Tests
//!
//! Comprehensive testing for operational resilience including:
//! - Circuit breaker integration with failure scenarios
//! - Retry policy testing with mock failures and budget management
//! - Operational failover tests for high availability
//! - Chaos engineering tests for resilience validation

use adapteros_core::circuit_breaker::{CircuitBreakerConfig, CircuitState, StandardCircuitBreaker};
use adapteros_core::{AosError, Result};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};

/// Mock service that can be configured to fail in various ways
#[derive(Clone)]
struct MockService {
    name: String,
    failure_rate: Arc<AtomicUsize>, // 0-100, percentage of requests that fail
    failure_type: Arc<Mutex<MockFailureType>>,
    request_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    failure_count: Arc<AtomicU64>,
    recovery_time: Arc<AtomicU64>, // milliseconds until service recovers
}

#[derive(Clone, Debug)]
enum MockFailureType {
    Network,
    Timeout,
    ServiceUnavailable,
    InternalError,
    Intermittent, // Alternate success/failure
}

impl MockService {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            failure_rate: Arc::new(AtomicUsize::new(0)),
            failure_type: Arc::new(Mutex::new(MockFailureType::Network)),
            request_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            failure_count: Arc::new(AtomicU64::new(0)),
            recovery_time: Arc::new(AtomicU64::new(0)),
        }
    }

    fn set_failure_rate(&self, rate: usize) {
        self.failure_rate.store(rate.min(100), Ordering::Relaxed);
    }

    async fn set_failure_type(&self, failure_type: MockFailureType) {
        *self.failure_type.lock().await = failure_type;
    }

    fn set_recovery_time(&self, ms: u64) {
        self.recovery_time.store(ms, Ordering::Relaxed);
    }

    async fn execute(&self) -> Result<String> {
        let request_num = self.request_count.fetch_add(1, Ordering::Relaxed);
        let failure_rate = self.failure_rate.load(Ordering::Relaxed);
        let recovery_time = self.recovery_time.load(Ordering::Relaxed);

        // Check if we're in recovery mode
        if recovery_time > 0 {
            sleep(Duration::from_millis(recovery_time)).await;
            self.recovery_time.store(0, Ordering::Relaxed);
            self.success_count.fetch_add(1, Ordering::Relaxed);
            return Ok(format!(
                "{}: recovered on request {}",
                self.name, request_num
            ));
        }

        // Determine if this request should fail
        let should_fail = match *self.failure_type.lock().await {
            MockFailureType::Intermittent => request_num % 2 == 0, // Fail every other request
            _ => (request_num % 100) < failure_rate as u64,
        };

        if should_fail {
            self.failure_count.fetch_add(1, Ordering::Relaxed);
            let failure_type = self.failure_type.lock().await.clone();
            return Err(match failure_type {
                MockFailureType::Network => {
                    AosError::Network(format!("{} network failure", self.name))
                }
                MockFailureType::Timeout => AosError::Timeout {
                    duration: Duration::from_secs(5),
                },
                MockFailureType::ServiceUnavailable => {
                    AosError::Unavailable(format!("{} service unavailable", self.name))
                }
                MockFailureType::InternalError => {
                    AosError::Internal(format!("{} internal error", self.name))
                }
                MockFailureType::Intermittent => {
                    AosError::Network(format!("{} intermittent failure", self.name))
                }
            });
        }

        self.success_count.fetch_add(1, Ordering::Relaxed);
        Ok(format!("{}: success on request {}", self.name, request_num))
    }

    fn metrics(&self) -> ServiceMetrics {
        ServiceMetrics {
            requests: self.request_count.load(Ordering::Relaxed),
            successes: self.success_count.load(Ordering::Relaxed),
            failures: self.failure_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
struct ServiceMetrics {
    requests: u64,
    successes: u64,
    failures: u64,
}

/// Test circuit breaker activation under sustained failures and load
#[tokio::test]
async fn test_circuit_breaker_under_load() {
    println!("\n=== Test: Circuit Breaker Under Load ===");

    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout_ms: 1000, // 1 second for faster testing
        half_open_max_requests: 5,
    };

    let breaker = StandardCircuitBreaker::new("test_service".to_string(), config);
    let service = MockService::new("test_service");

    // Phase 1: Normal operation
    println!("Phase 1: Normal operation (0% failure rate)");
    service.set_failure_rate(0);
    for _ in 0..10 {
        let result = breaker.call(async { service.execute().await }).await;
        assert!(result.is_ok(), "Should succeed in normal operation");
    }
    assert_eq!(breaker.state(), CircuitState::Closed);
    println!("✓ Circuit remains closed during normal operation");

    // Phase 2: Introduce failures to trigger circuit breaker
    println!("Phase 2: Sustained failures (100% failure rate)");
    service.set_failure_rate(100);

    // Should open after failure_threshold consecutive failures
    for i in 0..5 {
        let result = breaker.call(async { service.execute().await }).await;
        assert!(result.is_err(), "Should fail with high failure rate");

        if i >= 2 {
            match breaker.state() {
                CircuitState::Open { .. } => {
                    println!("✓ Circuit opened after {} failures", i + 1);
                    break;
                }
                _ => continue,
            }
        }
    }

    // Phase 3: Circuit should remain open
    println!("Phase 3: Circuit remains open under continued failures");
    for _ in 0..5 {
        let result = breaker.call(async { service.execute().await }).await;
        assert!(result.is_err(), "Should fail when circuit is open");
        assert!(matches!(breaker.state(), CircuitState::Open { .. }));
    }

    // Phase 4: Wait for timeout and test half-open state
    println!("Phase 4: Testing half-open recovery");
    sleep(Duration::from_millis(1100)).await; // Wait for timeout

    // First request in half-open should be allowed
    let result = breaker.call(async { service.execute().await }).await;
    // This might fail or succeed depending on timing, but circuit should handle it

    // Phase 5: Recovery - reduce failure rate
    println!("Phase 5: Service recovery");
    service.set_failure_rate(0);

    // Should eventually close after success_threshold successes
    for i in 0..10 {
        let result = breaker.call(async { service.execute().await }).await;

        if i >= 1 {
            // After a couple attempts
            if matches!(breaker.state(), CircuitState::Closed) {
                println!("✓ Circuit closed after recovery");
                break;
            }
        }
    }

    let final_metrics = breaker.metrics();
    println!(
        "Final metrics: requests={}, successes={}, failures={}, opens={}",
        final_metrics.requests_total,
        final_metrics.successes_total,
        final_metrics.failures_total,
        final_metrics.opens_total
    );

    assert!(
        final_metrics.requests_total > 0,
        "Should have processed requests"
    );
    assert!(
        final_metrics.opens_total >= 1,
        "Should have opened at least once"
    );
    assert!(
        matches!(breaker.state(), CircuitState::Closed),
        "Should end in closed state"
    );

    println!("✓ Circuit breaker under load test passed");
}

/// Test retry logic with exponential backoff and circuit breaker integration
#[tokio::test]
async fn test_retry_policy_exhaustion() {
    println!("\n=== Test: Retry Policy Exhaustion ===");

    let breaker = StandardCircuitBreaker::new(
        "retry_test".to_string(),
        CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout_ms: 500,
            half_open_max_requests: 3,
        },
    );

    let service = MockService::new("retry_test");
    service.set_failure_rate(100); // Always fail

    // Phase 1: Test retry attempts exhaustion
    println!("Phase 1: Testing retry exhaustion with exponential backoff");

    async fn execute_with_retry(
        service: &MockService,
        breaker: &StandardCircuitBreaker,
        max_attempts: u32,
        base_delay: Duration,
        backoff_factor: f64,
    ) -> Result<String> {
        let mut attempt = 0;
        let mut delay = base_delay;

        loop {
            attempt += 1;

            let result = breaker.call(async { service.execute().await }).await;

            match result {
                Ok(success) => return Ok(success),
                Err(_) if attempt < max_attempts => {
                    sleep(delay).await;
                    delay =
                        Duration::from_millis((delay.as_millis() as f64 * backoff_factor) as u64);
                }
                Err(e) => return Err(e),
            }
        }
    }

    let start_time = Instant::now();
    let result = execute_with_retry(
        &service,
        &breaker,
        3,                         // max attempts
        Duration::from_millis(10), // base delay
        2.0,                       // backoff factor
    )
    .await;

    let duration = start_time.elapsed();
    assert!(
        result.is_err(),
        "Should eventually fail after retries exhausted"
    );
    // Should have waited for: 10ms + 20ms + 40ms = 70ms minimum
    assert!(
        duration >= Duration::from_millis(60),
        "Should have waited for retry delays"
    );

    let metrics = service.metrics();
    assert_eq!(metrics.requests, 3, "Should have made 3 attempts");
    assert_eq!(metrics.failures, 3, "All attempts should have failed");
    println!(
        "✓ Retry policy exhausted after {} attempts",
        metrics.requests
    );

    // Phase 2: Test circuit breaker activation from repeated failures
    println!("Phase 2: Testing circuit breaker activation");

    // Continue making requests to trigger circuit breaker
    for i in 0..10 {
        let result = breaker.call(async { service.execute().await }).await;
        assert!(result.is_err(), "Should fail due to service failure rate");

        // Check if circuit breaker opened
        if i >= 4 {
            // After failure_threshold
            if matches!(breaker.state(), CircuitState::Open { .. }) {
                println!("✓ Circuit breaker opened after {} failed requests", i + 1);
                break;
            }
        }
    }

    // Phase 3: Test circuit breaker prevents requests when open
    println!("Phase 3: Testing circuit breaker prevents requests when open");

    for _ in 0..5 {
        let result = breaker.call(async { service.execute().await }).await;
        assert!(result.is_err(), "Should be rejected by circuit breaker");
    }

    let final_metrics = breaker.metrics();
    assert!(
        final_metrics.requests_total >= 10,
        "Should have processed many requests"
    );
    assert!(
        final_metrics.opens_total >= 1,
        "Should have opened circuit breaker"
    );

    println!("✓ Retry policy exhaustion test passed");
}

/// Test operational failover and service restoration
#[tokio::test]
async fn test_operational_failover() {
    println!("\n=== Test: Operational Failover ===");

    // Create primary and backup services
    let primary = MockService::new("primary");
    let backup = MockService::new("backup");
    let circuit_breaker = StandardCircuitBreaker::new(
        "failover_test".to_string(),
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 500,
            half_open_max_requests: 3,
        },
    );

    // Simple failover logic: try primary first, then backup
    async fn execute_with_failover(
        primary: &MockService,
        backup: &MockService,
        breaker: &StandardCircuitBreaker,
    ) -> Result<String> {
        // Try primary first
        let primary_result = breaker.call(async { primary.execute().await }).await;

        match primary_result {
            Ok(response) => Ok(format!("PRIMARY: {}", response)),
            Err(_) => {
                // Try backup
                breaker
                    .call(async { backup.execute().await })
                    .await
                    .map(|response| format!("BACKUP: {}", response))
            }
        }
    }

    // Phase 1: Normal operation with primary
    println!("Phase 1: Normal operation with primary service");
    primary.set_failure_rate(0);
    backup.set_failure_rate(0);

    for _ in 0..5 {
        let result = execute_with_failover(&primary, &backup, &circuit_breaker).await;
        assert!(result.is_ok(), "Should succeed with primary");
        assert!(
            result.unwrap().contains("PRIMARY"),
            "Should use primary service"
        );
    }

    let primary_metrics = primary.metrics();
    assert_eq!(
        primary_metrics.requests, 5,
        "Primary should handle all requests"
    );
    println!(
        "✓ Primary service handled {} requests successfully",
        primary_metrics.requests
    );

    // Phase 2: Primary failure triggers failover
    println!("Phase 2: Primary failure triggers failover");
    primary.set_failure_rate(100); // Primary always fails
    backup.set_failure_rate(0); // Backup works

    // Should failover to backup
    for i in 0..5 {
        let result = execute_with_failover(&primary, &backup, &circuit_breaker).await;
        if result.is_ok() {
            let response = result.unwrap();
            if response.contains("BACKUP") {
                println!("✓ Failover to backup occurred on attempt {}", i + 1);
                break;
            }
        }
    }

    let backup_metrics = backup.metrics();
    assert!(
        backup_metrics.requests >= 5,
        "Backup should have handled failed requests"
    );

    // Phase 3: Primary recovery
    println!("Phase 3: Primary recovery");
    primary.set_failure_rate(0); // Restore primary
    primary.set_recovery_time(50); // Brief recovery time

    // Should use primary again
    for _ in 0..5 {
        let result = execute_with_failover(&primary, &backup, &circuit_breaker).await;
        assert!(result.is_ok(), "Should succeed after recovery");
    }

    let final_primary_metrics = primary.metrics();
    assert!(
        final_primary_metrics.requests > primary_metrics.requests,
        "Primary should be used after recovery"
    );

    println!(
        "✓ Primary recovered and handled additional {} requests",
        final_primary_metrics.requests - primary_metrics.requests
    );

    // Phase 4: Circuit breaker protection during failures
    println!("Phase 4: Circuit breaker protection");

    // Force both services to fail
    primary.set_failure_rate(100);
    backup.set_failure_rate(100);

    // Make many requests to trigger circuit breaker
    let mut failure_count = 0;
    for _ in 0..10 {
        let result = execute_with_failover(&primary, &backup, &circuit_breaker).await;
        if result.is_err() {
            failure_count += 1;
        }
    }

    // Circuit breaker should eventually open
    match circuit_breaker.state() {
        CircuitState::Open { .. } => {
            println!("✓ Circuit breaker opened after {} failures", failure_count)
        }
        state => println!("Circuit breaker state: {:?}", state),
    }

    let cb_metrics = circuit_breaker.metrics();
    assert!(
        cb_metrics.opens_total >= 1,
        "Circuit breaker should have opened"
    );

    println!("✓ Operational failover test passed");
}

/// Chaos engineering test for resilience validation
#[tokio::test]
async fn test_chaos_engineering_resilience() {
    println!("\n=== Test: Chaos Engineering Resilience ===");

    let service = MockService::new("chaos_service");
    let circuit_breaker = StandardCircuitBreaker::new(
        "chaos_test".to_string(),
        CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_ms: 200,
            half_open_max_requests: 5,
        },
    );

    // Phase 1: Intermittent failures
    println!("Phase 1: Intermittent failure injection");
    service
        .set_failure_type(MockFailureType::Intermittent)
        .await;
    service.set_failure_rate(50); // 50% failure rate

    let mut intermittent_failures = 0;
    let mut intermittent_successes = 0;

    for _ in 0..20 {
        let result = circuit_breaker
            .call(async { service.execute().await })
            .await;

        match result {
            Ok(_) => intermittent_successes += 1,
            Err(_) => intermittent_failures += 1,
        }
    }

    println!(
        "Intermittent chaos: {} successes, {} failures",
        intermittent_successes, intermittent_failures
    );
    assert!(
        intermittent_failures > 5 && intermittent_successes > 5,
        "Should have both successes and failures"
    );

    // Phase 2: Sustained failures (cascading scenario)
    println!("Phase 2: Sustained failure simulation");
    service.set_failure_rate(100); // Always fail
    service.set_failure_type(MockFailureType::Network).await;

    let mut sustained_failures = 0;
    for _ in 0..15 {
        let result = circuit_breaker
            .call(async { service.execute().await })
            .await;

        if result.is_err() {
            sustained_failures += 1;
        }

        // Check if circuit breaker opened
        if matches!(circuit_breaker.state(), CircuitState::Open { .. }) {
            println!("✓ Circuit breaker opened during sustained failures");
            break;
        }
    }

    assert!(
        sustained_failures >= 5,
        "Should have multiple sustained failures"
    );

    // Phase 3: Recovery under load
    println!("Phase 3: Recovery under load");
    service.set_failure_rate(0); // Stop failures
    service.set_recovery_time(50);

    // Wait for circuit breaker timeout
    sleep(Duration::from_millis(250)).await;

    let mut recovery_successes = 0;
    for _ in 0..20 {
        let result = circuit_breaker
            .call(async { service.execute().await })
            .await;

        if result.is_ok() {
            recovery_successes += 1;
        }
    }

    assert!(
        recovery_successes >= 15,
        "Should recover successfully under load"
    );
    assert!(
        matches!(circuit_breaker.state(), CircuitState::Closed),
        "Circuit breaker should close after recovery"
    );

    println!(
        "Recovery: {} successes out of 20 requests",
        recovery_successes
    );

    // Phase 4: Concurrent chaos
    println!("Phase 4: Concurrent request chaos");
    service.set_failure_rate(100); // Always fail

    // Make many concurrent requests
    let mut handles = vec![];
    for _ in 0..30 {
        let cb = circuit_breaker.clone();
        let svc = service.clone();
        let handle = tokio::spawn(async move { cb.call(async { svc.execute().await }).await });
        handles.push(handle);
    }

    let mut concurrent_failures = 0;
    for handle in handles {
        if handle.await.unwrap().is_err() {
            concurrent_failures += 1;
        }
    }

    assert!(
        concurrent_failures >= 20,
        "Should have many concurrent failures"
    );
    assert!(
        matches!(circuit_breaker.state(), CircuitState::Open { .. }),
        "Circuit breaker should open under concurrent load"
    );

    let final_metrics = circuit_breaker.metrics();
    println!(
        "Final chaos metrics: requests={}, successes={}, failures={}, opens={}",
        final_metrics.requests_total,
        final_metrics.successes_total,
        final_metrics.failures_total,
        final_metrics.opens_total
    );

    assert!(
        final_metrics.opens_total >= 2,
        "Should have opened circuit breaker multiple times"
    );

    println!("✓ Chaos engineering resilience test passed");
}
