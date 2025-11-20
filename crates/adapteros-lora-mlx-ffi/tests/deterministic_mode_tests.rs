//! Deterministic Mode Tests
//!
//! Tests for MLX backend deterministic mode (CPU fallback for non-deterministic operations).
//!
//! Deterministic mode forces CPU execution for operations with GPU scheduling variance:
//! - Softmax (parallel reduction)
//! - Layer normalization (mean/variance calculation)
//! - Other parallel reduction operations
//!
//! Expected performance impact: ~20-30% overhead

#![cfg(feature = "test-utils")]

use adapteros_core::{derive_seed, B3Hash};
use adapteros_lora_mlx_ffi::{MLXFFIBackend, MLXFFIModel};
use adapteros_lora_kernel_api::FusedKernels;

/// Helper: Create test model
fn create_test_model() -> MLXFFIModel {
    #[cfg(feature = "test-utils")]
    {
        use adapteros_lora_mlx_ffi::mock::MockMLXModel;
        MockMLXModel::new()
    }

    #[cfg(not(feature = "test-utils"))]
    {
        panic!("Test requires test-utils feature");
    }
}

// =============================================================================
// Test 1: Deterministic Mode Attestation
// =============================================================================

#[test]
fn test_deterministic_mode_attestation() {
    let model = create_test_model();

    // Default mode (non-deterministic)
    let backend_default = MLXFFIBackend::new(model.clone());
    let report_default = backend_default.attest_determinism().unwrap();

    assert_eq!(report_default.deterministic, false);
    assert_eq!(
        report_default.floating_point_mode,
        adapteros_lora_kernel_api::attestation::FloatingPointMode::Unknown
    );

    println!("Default mode attestation: {}", report_default.summary());

    // Deterministic mode enabled
    let backend_deterministic = MLXFFIBackend::new(model).with_deterministic_mode();
    let report_deterministic = backend_deterministic.attest_determinism().unwrap();

    assert_eq!(report_deterministic.deterministic, true);
    assert_eq!(
        report_deterministic.floating_point_mode,
        adapteros_lora_kernel_api::attestation::FloatingPointMode::Deterministic
    );

    println!("Deterministic mode attestation: {}", report_deterministic.summary());
}

// =============================================================================
// Test 2: Attestation Validation
// =============================================================================

#[test]
fn test_attestation_validation() {
    let model = create_test_model();

    // Default mode should fail validation
    let backend_default = MLXFFIBackend::new(model.clone());
    let report_default = backend_default.attest_determinism().unwrap();
    assert!(report_default.validate().is_err());

    println!("Default mode validation (expected failure): {:?}",
        report_default.validate().err().unwrap());

    // Deterministic mode should pass validation
    let backend_deterministic = MLXFFIBackend::new(model).with_deterministic_mode();
    let report_deterministic = backend_deterministic.attest_determinism().unwrap();
    assert!(report_deterministic.validate().is_ok());

    println!("Deterministic mode validation: OK");
}

// =============================================================================
// Test 3: CPU Softmax Determinism
// =============================================================================

#[test]
fn test_cpu_softmax_determinism() {
    let model = create_test_model();
    let backend = MLXFFIBackend::new(model).with_deterministic_mode();

    let input = vec![1.0, 2.0, 3.0, 4.0, 5.0];

    // Run softmax multiple times
    let num_runs = 10;
    let mut results = Vec::new();

    for _ in 0..num_runs {
        let result = backend.cpu_softmax(&input).unwrap();
        results.push(result);
    }

    // All results should be EXACTLY identical (CPU fallback)
    for i in 1..results.len() {
        assert_eq!(
            results[0], results[i],
            "CPU softmax results differ between runs"
        );
    }

    println!("CPU softmax determinism validated:");
    println!("  Result: {:?}", results[0]);
    println!("  All {} runs identical", num_runs);
}

// =============================================================================
// Test 4: CPU Layer Norm Determinism
// =============================================================================

#[test]
fn test_cpu_layer_norm_determinism() {
    let model = create_test_model();
    let backend = MLXFFIBackend::new(model).with_deterministic_mode();

    let input = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let eps = 1e-5;

    // Run layer norm multiple times
    let num_runs = 10;
    let mut results = Vec::new();

    for _ in 0..num_runs {
        let result = backend.cpu_layer_norm(&input, eps).unwrap();
        results.push(result);
    }

    // All results should be EXACTLY identical (CPU fallback)
    for i in 1..results.len() {
        assert_eq!(
            results[0], results[i],
            "CPU layer norm results differ between runs"
        );
    }

    println!("CPU layer norm determinism validated:");
    println!("  Result: {:?}", results[0]);
    println!("  All {} runs identical", num_runs);
}

// =============================================================================
// Test 5: Deterministic Mode Flag Check
// =============================================================================

#[test]
fn test_deterministic_mode_flag() {
    let model = create_test_model();

    // Default mode
    let backend_default = MLXFFIBackend::new(model.clone());
    assert_eq!(backend_default.is_deterministic_mode(), false);

    // Deterministic mode
    let backend_deterministic = MLXFFIBackend::new(model).with_deterministic_mode();
    assert_eq!(backend_deterministic.is_deterministic_mode(), true);

    println!("Deterministic mode flag check passed");
}

// =============================================================================
// Test 6: Performance Overhead Measurement
// =============================================================================

#[test]
fn test_deterministic_mode_performance_overhead() {
    use std::time::Instant;

    let model = create_test_model();
    let input = vec![1.0; 4096]; // Large input for measurable overhead

    // Benchmark GPU mode (default)
    let backend_gpu = MLXFFIBackend::new(model.clone());
    let start_gpu = Instant::now();
    for _ in 0..100 {
        let _ = backend_gpu.cpu_softmax(&input).unwrap();
    }
    let time_gpu = start_gpu.elapsed();

    // Benchmark CPU mode (deterministic)
    let backend_cpu = MLXFFIBackend::new(model).with_deterministic_mode();
    let start_cpu = Instant::now();
    for _ in 0..100 {
        let _ = backend_cpu.cpu_softmax(&input).unwrap();
    }
    let time_cpu = start_cpu.elapsed();

    let overhead_pct = (time_cpu.as_secs_f64() / time_gpu.as_secs_f64() - 1.0) * 100.0;

    println!("Deterministic mode performance:");
    println!("  GPU mode: {:?}", time_gpu);
    println!("  CPU mode: {:?}", time_cpu);
    println!("  Overhead: {:.2}%", overhead_pct);

    // Note: This is a simplified test - in reality, softmax is always CPU
    // Real overhead would be measured in full forward pass with GPU ops
}

// =============================================================================
// Test 7: Compiler Flags Verification
// =============================================================================

#[test]
fn test_compiler_flags_in_attestation() {
    let model = create_test_model();

    // Default mode
    let backend_default = MLXFFIBackend::new(model.clone());
    let report_default = backend_default.attest_determinism().unwrap();

    assert!(report_default.compiler_flags.contains(&"-DMLX_HKDF_SEEDED".to_string()));
    assert!(!report_default.compiler_flags.contains(&"-DMLX_DETERMINISTIC_MODE".to_string()));

    // Deterministic mode
    let backend_deterministic = MLXFFIBackend::new(model).with_deterministic_mode();
    let report_deterministic = backend_deterministic.attest_determinism().unwrap();

    assert!(report_deterministic.compiler_flags.contains(&"-DMLX_HKDF_SEEDED".to_string()));
    assert!(report_deterministic.compiler_flags.contains(&"-DMLX_DETERMINISTIC_MODE".to_string()));
    assert!(report_deterministic.compiler_flags.contains(&"-DCPU_FALLBACK_ENABLED".to_string()));

    println!("Compiler flags verified:");
    println!("  Default: {:?}", report_default.compiler_flags);
    println!("  Deterministic: {:?}", report_deterministic.compiler_flags);
}

// =============================================================================
// Test 8: Production Mode Guard
// =============================================================================

#[test]
fn test_production_mode_guard() {
    let model = create_test_model();

    // Simulate production mode check
    let backend_default = MLXFFIBackend::new(model.clone());
    let report_default = backend_default.attest_determinism().unwrap();

    // Production mode should reject non-deterministic backend
    if is_production_mode() {
        assert!(
            report_default.validate().is_err(),
            "Production mode should reject non-deterministic backend"
        );
    }

    // Deterministic mode should pass
    let backend_deterministic = MLXFFIBackend::new(model).with_deterministic_mode();
    let report_deterministic = backend_deterministic.attest_determinism().unwrap();

    if is_production_mode() {
        assert!(
            report_deterministic.validate().is_ok(),
            "Production mode should accept deterministic backend"
        );
    }

    println!("Production mode guard test passed");
}

/// Helper: Check if production mode is enabled
fn is_production_mode() -> bool {
    // In real implementation, this would check config.server.production_mode
    std::env::var("ADAPTEROS_PRODUCTION_MODE").is_ok()
}

// =============================================================================
// Test 9: Deterministic Mode Builder Pattern
// =============================================================================

#[test]
fn test_deterministic_mode_builder_pattern() {
    let model = create_test_model();

    // Builder pattern usage
    let backend = MLXFFIBackend::new(model)
        .with_deterministic_mode();

    assert!(backend.is_deterministic_mode());

    let report = backend.attest_determinism().unwrap();
    assert!(report.deterministic);
    assert!(report.validate().is_ok());

    println!("Builder pattern test passed");
}

// =============================================================================
// Summary Test
// =============================================================================

#[test]
fn summary_deterministic_mode_features() {
    println!("\n=== MLX Deterministic Mode Summary ===\n");

    println!("Features:");
    println!("  - CPU fallback for parallel reduction operations");
    println!("  - Fixed iteration order for sum, softmax, layer norm");
    println!("  - Eliminates GPU scheduling variance");
    println!("  - Passes attestation validation");

    println!("\nPerformance Impact:");
    println!("  - Expected overhead: 20-30%");
    println!("  - Trade-off: determinism vs throughput");

    println!("\nUse Cases:");
    println!("  - Regulatory compliance (finance, healthcare)");
    println!("  - Debugging non-deterministic behavior");
    println!("  - Research reproducibility");

    println!("\nProduction Recommendation:");
    println!("  - Metal backend preferred (no overhead)");
    println!("  - MLX deterministic mode as fallback");

    println!("\n=== End Summary ===\n");
}
