//! Integration tests for AdapterOS determinism guards
//!
//! These tests verify that the determinism guards work correctly in the worker
//! and catch nondeterministic operations.

use adapteros_lint::{runtime_guards, strict_mode};
use adapteros_lora_worker::{
    determinism_guards_enabled, determinism_violation_count, init_determinism_guards,
};

#[tokio::test]
async fn test_determinism_guards_initialization() {
    // Test that guards can be initialized
    let result = init_determinism_guards();
    assert!(result.is_ok());

    // Verify guards are enabled
    assert!(determinism_guards_enabled());
}

#[tokio::test]
async fn test_strict_mode_environment_variable() {
    // Set environment variable
    std::env::set_var("ADAPTEROS_STRICT_MODE", "1");

    // Reinitialize guards
    let result = init_determinism_guards();
    assert!(result.is_ok());

    // Verify strict mode is enabled
    assert!(strict_mode::is_strict_mode());

    // Clean up
    std::env::remove_var("ADAPTEROS_STRICT_MODE");
}

#[tokio::test]
async fn test_violation_count_tracking() {
    // Initialize guards
    let result = init_determinism_guards();
    assert!(result.is_ok());

    let initial_count = determinism_violation_count();

    // Report some violations
    runtime_guards::guard_spawn_blocking();
    runtime_guards::guard_wall_clock_time("test");
    runtime_guards::guard_random_generation("test");

    let final_count = determinism_violation_count();
    assert_eq!(final_count, initial_count + 3);
}

#[tokio::test]
async fn test_guarded_functions() {
    // Initialize guards
    let result = init_determinism_guards();
    assert!(result.is_ok());

    // Test guarded functions - these should trigger violations
    let _now = runtime_guards::guarded_system_time_now();
    let _instant = runtime_guards::guarded_instant_now();
    let _value: u32 = runtime_guards::guarded_random();
    let _rng = runtime_guards::guarded_thread_rng();

    // Verify violations were recorded
    assert!(determinism_violation_count() > 0);
}

#[tokio::test]
async fn test_strict_mode_panic() {
    // Enable strict mode
    strict_mode::enable_strict_mode();

    // Initialize guards
    let result = init_determinism_guards();
    assert!(result.is_ok());

    // This should panic in strict mode
    let result = std::panic::catch_unwind(|| {
        runtime_guards::guard_spawn_blocking();
    });

    assert!(result.is_err());

    // Clean up
    strict_mode::disable_strict_mode();
}

#[tokio::test]
async fn test_determinism_guards_integration() {
    // Test that the guards work with the worker initialization
    let result = init_determinism_guards();
    assert!(result.is_ok());

    // Verify guards are active
    assert!(determinism_guards_enabled());

    // Test that violations are tracked
    let initial_count = determinism_violation_count();

    // Simulate some nondeterministic operations
    runtime_guards::guard_file_io("std::fs::read_to_string");
    runtime_guards::guard_syscall("std::process::Command");

    let final_count = determinism_violation_count();
    assert_eq!(final_count, initial_count + 2);
}
