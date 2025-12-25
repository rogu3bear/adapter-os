//! MLX Bridge Integration Tests
//!
//! Tests that verify the MLX subprocess bridge integrates correctly.
//! These tests require the `mlx-bridge` feature and Python with mlx-lm installed.
//!
//! Run with: cargo test -p adapteros-lora-worker --features mlx-bridge --test mlx_bridge_integration

#![cfg(feature = "mlx-bridge")]

use adapteros_lora_worker::mlx_subprocess_bridge::{MlxBridgeConfig, MLXSubprocessBridge};
use std::path::PathBuf;
use std::process::Command;

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if Python with mlx-lm is available
fn python_mlx_available() -> bool {
    let output = Command::new("python3")
        .args(["-c", "import mlx_lm; print('ok')"])
        .output();

    match output {
        Ok(out) => out.status.success() && String::from_utf8_lossy(&out.stdout).contains("ok"),
        Err(_) => false,
    }
}

/// Check if the bridge script exists
fn bridge_script_exists() -> bool {
    let candidates = vec![
        PathBuf::from("scripts/mlx_bridge_server.py"),
        PathBuf::from("../scripts/mlx_bridge_server.py"),
        PathBuf::from("../../scripts/mlx_bridge_server.py"),
    ];

    candidates.iter().any(|p| p.exists())
}

/// Skip message for tests that require Python/mlx-lm
fn skip_reason() -> Option<String> {
    if !python_mlx_available() {
        return Some("Python with mlx-lm not available (pip install mlx-lm)".to_string());
    }
    if !bridge_script_exists() {
        return Some("Bridge script not found (scripts/mlx_bridge_server.py)".to_string());
    }
    None
}

// ============================================================================
// Unit Tests (no subprocess needed)
// ============================================================================

#[test]
fn test_config_default() {
    let config = MlxBridgeConfig::default();
    assert_eq!(config.python_path, "python3");
    assert_eq!(config.startup_timeout_secs, 120);
    assert_eq!(config.request_timeout_secs, 300);
    assert_eq!(config.max_restarts, 3);
}

#[test]
fn test_config_with_custom_values() {
    let config = MlxBridgeConfig {
        python_path: "/usr/local/bin/python3".to_string(),
        startup_timeout_secs: 60,
        request_timeout_secs: 120,
        max_restarts: 5,
        health_check_interval_secs: 15,
        default_temperature: 0.5,
        default_top_p: 0.8,
    };
    assert_eq!(config.python_path, "/usr/local/bin/python3");
    assert_eq!(config.max_restarts, 5);
}

#[test]
fn test_bridge_creation_fails_with_invalid_path() {
    let result = MLXSubprocessBridge::new(
        PathBuf::from("/nonexistent/model/path"),
        32000,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("does not exist"));
}

// ============================================================================
// Integration Tests (require Python/mlx-lm)
// ============================================================================

/// Test that bridge can be created with valid paths
/// This test is ignored by default as it requires Python/mlx-lm
#[test]
#[ignore = "Requires Python with mlx-lm and a valid model path"]
fn test_bridge_creation_with_real_model() {
    if let Some(reason) = skip_reason() {
        eprintln!("Skipping test: {}", reason);
        return;
    }

    // This would need a real model path to work
    // For CI, we'd need to download a small test model
    let model_path = std::env::var("MLX_TEST_MODEL_PATH")
        .expect("Set MLX_TEST_MODEL_PATH to run this test");

    let result = MLXSubprocessBridge::new(
        PathBuf::from(model_path),
        32000,
    );

    match result {
        Ok(bridge) => {
            println!("Bridge created successfully");
            // Bridge will be dropped, shutting down subprocess
            drop(bridge);
        }
        Err(e) => {
            panic!("Failed to create bridge: {}", e);
        }
    }
}

/// Test bridge health check
#[test]
#[ignore = "Requires Python with mlx-lm and a valid model path"]
fn test_bridge_health_check() {
    if let Some(reason) = skip_reason() {
        eprintln!("Skipping test: {}", reason);
        return;
    }

    let model_path = std::env::var("MLX_TEST_MODEL_PATH")
        .expect("Set MLX_TEST_MODEL_PATH to run this test");

    let bridge = MLXSubprocessBridge::new(
        PathBuf::from(model_path),
        32000,
    ).expect("Failed to create bridge");

    // Health check before loading should work
    let health = bridge.check_bridge_health();
    assert!(health.is_ok(), "Health check failed: {:?}", health);
}

/// Test that FusedKernels trait is implemented correctly
#[test]
fn test_fused_kernels_device_name() {
    // We can test device_name without spawning subprocess
    // by checking the implementation returns expected value
    // This is a compile-time check that the trait is implemented
    fn assert_fused_kernels<T: adapteros_lora_kernel_api::FusedKernels>() {}
    assert_fused_kernels::<MLXSubprocessBridge>();
}

// ============================================================================
// Capability Detection Tests
// ============================================================================

#[test]
fn test_python_detection() {
    // Just log whether Python is available
    let available = python_mlx_available();
    println!("Python with mlx-lm available: {}", available);
    // This test always passes - it's informational
}

#[test]
fn test_bridge_script_detection() {
    // Just log whether bridge script is found
    let exists = bridge_script_exists();
    println!("Bridge script found: {}", exists);
    // This test always passes - it's informational
}
