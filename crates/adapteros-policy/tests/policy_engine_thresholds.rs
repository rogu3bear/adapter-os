//! Integration tests for PolicyEngine config-driven thresholds
//!
//! These tests verify that policy enforcement uses manifest config values
//! instead of hard-coded literals, ensuring documentation and enforcement
//! cannot diverge.
//!
//! # References
//! - AGENTS.md: Policy compliance checklist
//! - adapteros-manifest/src/lib.rs: Policy configuration structs

use adapteros_core::AosError;
use adapteros_model_hub::manifest::Policies;
use adapteros_policy::PolicyEngine;

/// Test that max_tokens threshold comes from config
#[test]
fn test_max_tokens_threshold_from_config() {
    // Create policies with custom max_tokens
    let mut policies = Policies::default();
    policies.performance.max_tokens = 500;

    let engine = PolicyEngine::new(policies);

    // Request below threshold should pass
    assert!(engine.check_resource_limits(499).is_ok());

    // Request at threshold should pass
    assert!(engine.check_resource_limits(500).is_ok());

    // Request above threshold should fail
    let result = engine.check_resource_limits(501);
    assert!(result.is_err());
    match result {
        Err(AosError::PolicyViolation(msg)) => {
            assert!(msg.contains("501"));
            assert!(msg.contains("500"));
        }
        _ => panic!("Expected PolicyViolation error"),
    }
}

/// Test that CPU threshold comes from config
#[test]
fn test_cpu_threshold_from_config() {
    // Create policies with custom CPU threshold
    let mut policies = Policies::default();
    policies.performance.cpu_threshold_pct = 80.0;

    let engine = PolicyEngine::new(policies);

    // CPU usage below threshold should pass
    assert!(engine.check_system_thresholds(79.9, 50.0).is_ok());

    // CPU usage at threshold should pass
    assert!(engine.check_system_thresholds(80.0, 50.0).is_ok());

    // CPU usage above threshold should fail
    let result = engine.check_system_thresholds(80.1, 50.0);
    assert!(result.is_err());
    match result {
        Err(AosError::PerformanceViolation(msg)) => {
            assert!(msg.contains("80.1"));
            assert!(msg.contains("80.0"));
        }
        _ => panic!("Expected PerformanceViolation error"),
    }
}

/// Test that memory threshold comes from config
#[test]
fn test_memory_threshold_from_config() {
    // Create policies with custom memory threshold
    let mut policies = Policies::default();
    policies.performance.memory_threshold_pct = 85.0;

    let engine = PolicyEngine::new(policies);

    // Memory usage below threshold should pass
    assert!(engine.check_system_thresholds(50.0, 84.9).is_ok());

    // Memory usage at threshold should pass
    assert!(engine.check_system_thresholds(50.0, 85.0).is_ok());

    // Memory usage above threshold should fail
    let result = engine.check_system_thresholds(50.0, 85.1);
    assert!(result.is_err());
    match result {
        Err(AosError::MemoryPressure(msg)) => {
            assert!(msg.contains("85.1"));
            assert!(msg.contains("85.0"));
        }
        _ => panic!("Expected MemoryPressure error"),
    }
}

/// Test that memory headroom threshold comes from config
#[test]
fn test_memory_headroom_threshold_from_config() {
    // Create policies with custom headroom threshold
    let mut policies = Policies::default();
    policies.memory.min_headroom_pct = 20;

    let engine = PolicyEngine::new(policies);

    // Headroom above threshold should pass
    assert!(engine.check_memory_headroom(20.1).is_ok());

    // Headroom at threshold should pass
    assert!(engine.check_memory_headroom(20.0).is_ok());

    // Headroom below threshold should fail
    let result = engine.check_memory_headroom(19.9);
    assert!(result.is_err());
    match result {
        Err(AosError::MemoryPressure(msg)) => {
            assert!(msg.contains("19.9"));
            assert!(msg.contains("20.0"));
        }
        _ => panic!("Expected MemoryPressure error"),
    }
}

/// Test that circuit breaker threshold comes from config
#[test]
fn test_circuit_breaker_threshold_from_config() {
    // Create policies with custom circuit breaker threshold
    let mut policies = Policies::default();
    policies.performance.circuit_breaker_threshold = 3;

    let engine = PolicyEngine::new(policies);

    // Failure count below threshold should not open breaker
    assert!(!engine.should_open_circuit_breaker(2));

    // Failure count at threshold should open breaker
    assert!(engine.should_open_circuit_breaker(3));

    // Failure count above threshold should open breaker
    assert!(engine.should_open_circuit_breaker(4));
}

/// Test that default values match documented defaults
#[test]
fn test_default_values_match_documentation() {
    let policies = Policies::default();

    // Verify default values from AGENTS.md and manifest documentation
    assert_eq!(policies.performance.max_tokens, 1000);
    assert_eq!(policies.performance.cpu_threshold_pct, 90.0);
    assert_eq!(policies.performance.memory_threshold_pct, 95.0);
    assert_eq!(policies.performance.circuit_breaker_threshold, 5);
    assert_eq!(policies.memory.min_headroom_pct, 15);
}

/// Test that multiple threshold changes work together
#[test]
fn test_multiple_threshold_changes() {
    // Create policies with all custom thresholds
    let mut policies = Policies::default();
    policies.performance.max_tokens = 2000;
    policies.performance.cpu_threshold_pct = 75.0;
    policies.performance.memory_threshold_pct = 80.0;
    policies.performance.circuit_breaker_threshold = 10;
    policies.memory.min_headroom_pct = 25;

    let engine = PolicyEngine::new(policies);

    // Verify all thresholds use custom values
    assert!(engine.check_resource_limits(2000).is_ok());
    assert!(engine.check_resource_limits(2001).is_err());

    assert!(engine.check_system_thresholds(75.0, 80.0).is_ok());
    assert!(engine.check_system_thresholds(75.1, 80.0).is_err());
    assert!(engine.check_system_thresholds(75.0, 80.1).is_err());

    assert!(engine.check_memory_headroom(25.0).is_ok());
    assert!(engine.check_memory_headroom(24.9).is_err());

    assert!(!engine.should_open_circuit_breaker(9));
    assert!(engine.should_open_circuit_breaker(10));
}

/// Test that changing config at runtime affects enforcement
#[test]
fn test_runtime_config_changes() {
    // Start with default config
    let mut policies = Policies::default();
    let engine1 = PolicyEngine::new(policies.clone());

    // Verify default behavior
    assert!(engine1.check_resource_limits(1000).is_ok());
    assert!(engine1.check_resource_limits(1001).is_err());

    // Change config
    policies.performance.max_tokens = 1500;
    let engine2 = PolicyEngine::new(policies);

    // Verify new behavior
    assert!(engine2.check_resource_limits(1500).is_ok());
    assert!(engine2.check_resource_limits(1501).is_err());
}

/// Test edge cases for threshold enforcement
#[test]
fn test_threshold_edge_cases() {
    // Test zero max_tokens
    let mut policies = Policies::default();
    policies.performance.max_tokens = 0;
    let engine = PolicyEngine::new(policies);
    assert!(engine.check_resource_limits(1).is_err());

    // Test zero CPU threshold
    let mut policies = Policies::default();
    policies.performance.cpu_threshold_pct = 0.0;
    let engine = PolicyEngine::new(policies);
    assert!(engine.check_system_thresholds(0.1, 50.0).is_err());

    // Test 100% memory threshold
    let mut policies = Policies::default();
    policies.performance.memory_threshold_pct = 100.0;
    let engine = PolicyEngine::new(policies);
    assert!(engine.check_system_thresholds(50.0, 99.9).is_ok());

    // Test zero headroom threshold
    let mut policies = Policies::default();
    policies.memory.min_headroom_pct = 0;
    let engine = PolicyEngine::new(policies);
    assert!(engine.check_memory_headroom(0.0).is_ok());
}

/// Test that error messages include actual threshold values
#[test]
fn test_error_messages_include_thresholds() {
    let mut policies = Policies::default();
    policies.performance.max_tokens = 750;
    policies.performance.cpu_threshold_pct = 85.0;
    policies.performance.memory_threshold_pct = 92.0;
    policies.memory.min_headroom_pct = 18;

    let engine = PolicyEngine::new(policies);

    // Check that error messages include the configured threshold values
    let result1 = engine.check_resource_limits(800);
    assert!(result1.is_err());
    if let Err(AosError::PolicyViolation(msg)) = result1 {
        assert!(
            msg.contains("750"),
            "Error message should include threshold"
        );
    }

    let result2 = engine.check_system_thresholds(90.0, 50.0);
    assert!(result2.is_err());
    if let Err(AosError::PerformanceViolation(msg)) = result2 {
        assert!(
            msg.contains("85.0"),
            "Error message should include threshold"
        );
    }

    let result3 = engine.check_system_thresholds(50.0, 95.0);
    assert!(result3.is_err());
    if let Err(AosError::MemoryPressure(msg)) = result3 {
        assert!(
            msg.contains("92.0"),
            "Error message should include threshold"
        );
    }

    let result4 = engine.check_memory_headroom(15.0);
    assert!(result4.is_err());
    if let Err(AosError::MemoryPressure(msg)) = result4 {
        assert!(
            msg.contains("18.0"),
            "Error message should include threshold"
        );
    }
}

/// Test that enforcement cannot diverge from documentation
/// This test explicitly validates the contract between manifest defaults
/// and PolicyEngine enforcement, preventing silent divergence.
#[test]
fn test_enforcement_matches_documentation() {
    let policies = Policies::default();
    let engine = PolicyEngine::new(policies.clone());

    // These assertions form a contract between documentation and code.
    // If these fail, either the documentation or code must be updated.

    // Performance thresholds
    assert_eq!(
        policies.performance.max_tokens, 1000,
        "Default max_tokens must match documentation"
    );
    assert_eq!(
        policies.performance.cpu_threshold_pct, 90.0,
        "Default CPU threshold must match documentation"
    );
    assert_eq!(
        policies.performance.memory_threshold_pct, 95.0,
        "Default memory threshold must match documentation"
    );
    assert_eq!(
        policies.performance.circuit_breaker_threshold, 5,
        "Default circuit breaker threshold must match documentation"
    );

    // Memory thresholds
    assert_eq!(
        policies.memory.min_headroom_pct, 15,
        "Default memory headroom must match documentation (Memory Ruleset #12)"
    );

    // Verify enforcement uses these exact values
    assert!(engine.check_resource_limits(1000).is_ok());
    assert!(engine.check_resource_limits(1001).is_err());

    assert!(engine.check_system_thresholds(90.0, 95.0).is_ok());
    assert!(engine.check_system_thresholds(90.1, 95.0).is_err());
    assert!(engine.check_system_thresholds(90.0, 95.1).is_err());

    assert!(engine.check_memory_headroom(15.0).is_ok());
    assert!(engine.check_memory_headroom(14.9).is_err());

    assert!(!engine.should_open_circuit_breaker(4));
    assert!(engine.should_open_circuit_breaker(5));
}
