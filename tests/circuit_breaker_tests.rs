//! Circuit breaker integration tests

use adapteros_core::{CircuitBreakerConfig, StandardCircuitBreaker};
use std::sync::Arc;

#[tokio::test]
async fn test_circuit_breaker_integration_with_inference() {
    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout_ms: 1000,
        half_open_max_requests: 5,
    };

    let breaker = Arc::new(StandardCircuitBreaker::new("inference".to_string(), config));

    // Test successful operations
    for i in 0..5 {
        let result = breaker.call(async move {
            // Simulate successful inference
            Ok(format!("response_{}", i))
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), format!("response_{}", i));
    }

    // Verify circuit stays closed
    assert!(matches!(breaker.state(), adapteros_core::CircuitState::Closed));

    // Test failure threshold
    for _ in 0..3 {
        let result: Result<(), _> = breaker.call(async {
            Err(adapteros_core::AosError::Unavailable("inference failed".to_string()))
        }).await;

        assert!(result.is_err());
    }

    // Verify circuit opens
    assert!(matches!(breaker.state(), adapteros_core::CircuitState::Open { .. }));

    // Test circuit breaker rejects requests when open
    let result = breaker.call(async {
        Ok("should be rejected")
    }).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        adapteros_core::AosError::CircuitBreakerOpen { .. } => {},
        _ => panic!("Expected CircuitBreakerOpen error"),
    }

    // Wait for timeout and test half-open recovery
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    // Should transition to half-open
    let state = breaker.state();
    assert!(matches!(state, adapteros_core::CircuitState::HalfOpen));

    // Test successful recovery
    for _ in 0..2 {
        let result = breaker.call(async {
            Ok("recovery_success")
        }).await;

        assert!(result.is_ok());
    }

    // Should close circuit
    assert!(matches!(breaker.state(), adapteros_core::CircuitState::Closed));
}

#[tokio::test]
async fn test_circuit_breaker_timeout_integration() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout_ms: 500,
        half_open_max_requests: 3,
    };

    let breaker = Arc::new(StandardCircuitBreaker::new("timeout_test".to_string(), config));

    // Test timeout protection
    let result = breaker.call(async {
        // Simulate long-running operation that should timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        Ok("completed")
    }).await;

    assert!(result.is_ok());

    // Test timeout triggering circuit breaker
    for _ in 0..2 {
        let result: Result<(), _> = breaker.call(async {
            // This should timeout and fail
            tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
            Ok(())
        }).await;

        // The timeout wrapper should cause failure
        assert!(result.is_err());
    }

    // Circuit should open due to timeouts
    assert!(matches!(breaker.state(), adapteros_core::CircuitState::Open { .. }));
}

#[tokio::test]
async fn test_circuit_breaker_metrics() {
    let config = CircuitBreakerConfig::default();
    let breaker = Arc::new(StandardCircuitBreaker::new("metrics_test".to_string(), config));

    // Perform some operations
    for _ in 0..3 {
        let _ = breaker.call(async { Ok("success") }).await;
    }

    for _ in 0..2 {
        let _: Result<(), _> = breaker.call(async {
            Err(adapteros_core::AosError::Unavailable("fail".to_string()))
        }).await;
    }

    let metrics = breaker.metrics();
    assert_eq!(metrics.requests_total, 5);
    assert_eq!(metrics.successes_total, 3);
    assert_eq!(metrics.failures_total, 2);
    assert_eq!(metrics.opens_total, 1); // Should have opened once
    assert!(matches!(metrics.state, adapteros_core::CircuitState::Open { .. }));
}
