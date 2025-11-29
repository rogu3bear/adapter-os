//! Backend Capability Tests
//!
//! These tests verify that all backend implementations:
//! 1. Don't use deceptive trait defaults that hide broken functionality
//! 2. Implement required methods properly
//! 3. Report honest health status
//!
//! Created as part of the Codebase Rectification Plan (P4) to catch
//! future regressions where a backend silently uses broken defaults.

use adapteros_core::Result;
use adapteros_lora_kernel_api::{BackendHealth, FusedKernels};

/// Test that MetalKernels implements health_check properly (not using default)
#[cfg(target_os = "macos")]
#[test]
fn test_metal_kernels_has_real_health_check() -> Result<()> {
    use adapteros_lora_kernel_mtl::MetalKernels;

    let kernels = MetalKernels::new()?;
    let health = kernels.health_check()?;

    // Should not be the default "Degraded: not implemented" response
    // MetalKernels should return Healthy or a real degraded reason
    match &health {
        BackendHealth::Degraded { reason } => {
            assert!(
                !reason.contains("not implemented"),
                "MetalKernels should not use default health_check: got {:?}",
                health
            );
        }
        BackendHealth::Healthy => {
            // This is good - real implementation
        }
        BackendHealth::Failed { reason, .. } => {
            // This is also valid - real implementation reporting failure
            assert!(
                !reason.contains("not implemented"),
                "MetalKernels should not use default health_check"
            );
        }
    }

    Ok(())
}

/// Test that MetalKernels implements get_metrics properly (not using default)
#[cfg(target_os = "macos")]
#[test]
fn test_metal_kernels_has_real_metrics() -> Result<()> {
    use adapteros_lora_kernel_mtl::MetalKernels;

    let kernels = MetalKernels::new()?;
    let metrics = kernels.get_metrics();

    // The metrics struct should have a valid memory_usage_bytes from vram_tracker
    // A real implementation will report actual values (even if 0)
    // This is more of a smoke test that get_metrics() doesn't panic

    // With no adapters loaded, memory should be 0 or minimal
    assert!(
        metrics.memory_usage_bytes < 1_000_000_000, // Less than 1GB with no adapters
        "MetalKernels should report reasonable memory usage, got {} bytes",
        metrics.memory_usage_bytes
    );

    Ok(())
}

/// Test that MetalKernels implements load_adapter properly (not using default error)
#[cfg(target_os = "macos")]
#[test]
fn test_metal_kernels_has_real_load_adapter() -> Result<()> {
    use adapteros_lora_kernel_mtl::MetalKernels;
    use safetensors::{serialize, tensor::TensorView, Dtype};
    use std::collections::HashMap;

    let mut kernels = MetalKernels::new()?;

    // Create minimal SafeTensors for testing
    let rank = 4usize;
    let features = 8usize;

    let lora_a_data: Vec<f32> = (0..(rank * features)).map(|i| (i as f32) * 0.01).collect();
    let lora_a_bytes: Vec<u8> = lora_a_data.iter().flat_map(|f| f.to_le_bytes()).collect();

    let lora_b_data: Vec<f32> = (0..(features * rank)).map(|i| (i as f32) * 0.02).collect();
    let lora_b_bytes: Vec<u8> = lora_b_data.iter().flat_map(|f| f.to_le_bytes()).collect();

    let tensors: Vec<(&str, TensorView<'_>)> = vec![
        (
            "base_model.model.layers.0.self_attn.q_proj.lora_A.weight",
            TensorView::new(Dtype::F32, vec![rank, features], &lora_a_bytes).unwrap(),
        ),
        (
            "base_model.model.layers.0.self_attn.q_proj.lora_B.weight",
            TensorView::new(Dtype::F32, vec![features, rank], &lora_b_bytes).unwrap(),
        ),
    ];

    let metadata: Option<HashMap<String, String>> = None;
    let weights = serialize(tensors, &metadata).expect("Failed to serialize");

    // Should succeed with real implementation (not return "Hot-swap not supported")
    let result = kernels.load_adapter(1, &weights);

    match &result {
        Ok(()) => {
            // Good - real implementation
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                !msg.contains("not supported"),
                "MetalKernels should not use default load_adapter: got {}",
                msg
            );
        }
    }

    Ok(())
}

/// Test that GPU fingerprinting methods return proper errors (not deceptive Ok(true))
#[cfg(target_os = "macos")]
#[test]
fn test_metal_kernels_gpu_fingerprint_not_deceptive() -> Result<()> {
    use adapteros_lora_kernel_mtl::MetalKernels;

    let kernels = MetalKernels::new()?;

    // Verify fingerprint for non-existent adapter should return Ok(false), not Ok(true)
    let result = kernels.verify_gpu_fingerprint(999, 0, "nonexistent");

    match result {
        Ok(verified) => {
            assert!(
                !verified,
                "verify_gpu_fingerprint should return false for non-existent adapter baseline, not deceptively true"
            );
        }
        Err(_) => {
            // Error is also acceptable (explicit failure)
        }
    }

    Ok(())
}

/// Test that default trait implementation returns error (not deceptive success)
#[test]
fn test_default_verify_gpu_fingerprint_returns_error() {
    use adapteros_lora_kernel_api::MockKernels;

    let kernels = MockKernels::new();

    // MockKernels uses the default trait implementation
    // Default should return error since it's not implemented
    let result = kernels.verify_gpu_fingerprint(1, 1000, "abc123");

    assert!(
        result.is_err(),
        "Default verify_gpu_fingerprint should return error, not deceptive Ok(true)"
    );
}

/// Test that default trait implementation returns error for store_gpu_fingerprint
#[test]
fn test_default_store_gpu_fingerprint_returns_error() {
    use adapteros_lora_kernel_api::MockKernels;

    let mut kernels = MockKernels::new();

    // MockKernels uses the default trait implementation
    let result = kernels.store_gpu_fingerprint(1, 1000, "abc123");

    assert!(
        result.is_err(),
        "Default store_gpu_fingerprint should return error, not silently succeed"
    );
}

/// Test that default health_check returns Degraded (not Healthy)
#[test]
fn test_default_health_check_returns_degraded() {
    use adapteros_lora_kernel_api::MockKernels;

    let kernels = MockKernels::new();
    let health = kernels.health_check().expect("health_check should not error");

    match health {
        BackendHealth::Healthy => {
            panic!("Default health_check should not return Healthy without actual checking");
        }
        BackendHealth::Degraded { reason } => {
            assert!(
                reason.contains("not implemented"),
                "Default should indicate health check is not implemented"
            );
        }
        BackendHealth::Failed { .. } => {
            // Also acceptable
        }
    }
}

/// Non-macOS stub to ensure test file compiles everywhere
#[cfg(not(target_os = "macos"))]
#[test]
fn test_backend_capability_stub() {
    // On non-macOS, just verify the test infrastructure works
    assert!(true, "Backend capability tests require macOS for Metal");
}
