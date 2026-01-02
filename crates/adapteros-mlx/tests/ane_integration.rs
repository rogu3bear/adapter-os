//! ANE Integration Tests
//!
//! These tests verify the AneAccelerator integration with CoreML backend.
//! Tests run regardless of ANE availability - they verify correct behavior
//! in both ANE-available and fallback scenarios.
//!
//! Run with: cargo test -p adapteros-mlx --test ane_integration -- --test-threads=1
//! Run with ANE: cargo test -p adapteros-mlx --test ane_integration --features coreml-ane -- --test-threads=1

use adapteros_mlx::{Array, AneAccelerator, AneConfig, is_ane_available};

/// Helper to compare two f32 vectors for approximate equality
fn assert_approx_equal(a: &[f32], b: &[f32], tolerance: f32, context: &str) {
    assert_eq!(a.len(), b.len(), "{}: length mismatch", context);
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        let diff = (x - y).abs();
        assert!(
            diff <= tolerance || (x.is_nan() && y.is_nan()),
            "{}: mismatch at index {}: {} vs {} (diff: {})",
            context,
            i,
            x,
            y,
            diff
        );
    }
}

/// Helper to compare two f32 vectors for exact equality
fn assert_exact_equal(a: &[f32], b: &[f32], context: &str) {
    assert_eq!(a.len(), b.len(), "{}: length mismatch", context);
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert!(
            x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()),
            "{}: mismatch at index {}: {} vs {} (bits: {:08x} vs {:08x})",
            context,
            i,
            x,
            y,
            x.to_bits(),
            y.to_bits()
        );
    }
}

#[test]
fn test_ane_availability_check() {
    // This should not panic regardless of hardware
    let available = is_ane_available();
    println!("ANE available: {}", available);

    // On non-macOS or without coreml-ane feature, should be false
    #[cfg(not(all(target_os = "macos", feature = "coreml-ane")))]
    {
        assert!(!available, "ANE should not be available without coreml-ane feature");
    }
}

#[test]
fn test_ane_accelerator_creation_disabled() {
    // With ANE explicitly disabled, should return None
    let config = AneConfig {
        enabled: false,
        ..Default::default()
    };
    let ane = AneAccelerator::try_new(config);
    assert!(ane.is_none(), "Disabled config should not create accelerator");
}

#[test]
fn test_ane_config_defaults() {
    let config = AneConfig::default();
    assert_eq!(config.batch_threshold, 32);
    assert!(config.require_determinism);
    assert!(config.enabled);
}

#[test]
fn test_ane_config_production() {
    let config = AneConfig::production();
    assert_eq!(config.batch_threshold, 32);
    assert!(config.require_determinism);
    assert!(config.enabled);
}

#[test]
fn test_ane_config_development() {
    let config = AneConfig::development();
    // Development config disables ANE entirely (uses MLX GPU)
    assert!(!config.enabled);
    assert!(!config.production_mode);
    // Inherits default batch_threshold and require_determinism
    assert_eq!(config.batch_threshold, 32);
}

#[test]
fn test_ane_accelerator_batch_threshold() {
    let config = AneConfig {
        batch_threshold: 16,
        ..Default::default()
    };

    // Try to create accelerator (may return None if ANE not available)
    if let Some(ane) = AneAccelerator::try_new(config) {
        // Below threshold - should not use ANE
        assert!(!ane.should_use_ane(8));
        assert!(!ane.should_use_ane(15));

        // At or above threshold - should use ANE
        assert!(ane.should_use_ane(16));
        assert!(ane.should_use_ane(32));
        assert!(ane.should_use_ane(128));
    }
}

#[test]
fn test_ane_attestation_report() {
    let config = AneConfig::production();

    if let Some(ane) = AneAccelerator::try_new(config) {
        let report = ane.attest();

        // Attestation should always report deterministic (both paths are)
        assert!(report.deterministic, "Both ANE and MLX paths are deterministic");

        if report.ane_enabled {
            assert_eq!(report.compute_units, "CpuAndNeuralEngine");
            assert!(report.notes.contains("fixed-point"));
        } else {
            assert!(report.compute_units.contains("MLX"));
            assert!(report.notes.contains("HKDF"));
        }

        println!("ANE Attestation: {:?}", report);
    }
}

#[test]
fn test_ane_layernorm_fallback() {
    // Test that layernorm works even when ANE is not available (uses MLX fallback)
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();
    let bias = Array::zeros(&[4]).unwrap();

    let config = AneConfig {
        batch_threshold: 1000, // Set high to force MLX fallback
        ..Default::default()
    };

    if let Some(ane) = AneAccelerator::try_new(config) {
        // Should use MLX fallback due to high threshold
        let result = ane.layernorm(&x, &weight, &bias, 1e-5).unwrap();
        let data = result.to_vec_f32().unwrap();

        // Verify output shape matches input
        assert_eq!(data.len(), 8);

        // LayerNorm should normalize each row
        // For [1,2,3,4] with mean=2.5, std~1.118, output should be [-1.34, -0.45, 0.45, 1.34]
        // Just verify reasonable range
        for v in &data {
            assert!(v.abs() < 2.0, "LayerNorm output should be normalized");
        }
    }
}

#[test]
fn test_ane_rms_norm_fallback() {
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();

    let config = AneConfig {
        batch_threshold: 1000, // Force MLX fallback
        ..Default::default()
    };

    if let Some(ane) = AneAccelerator::try_new(config) {
        let result = ane.rms_norm(&x, &weight, 1e-5).unwrap();
        let data = result.to_vec_f32().unwrap();

        assert_eq!(data.len(), 8);

        // RMSNorm output should be reasonable
        for v in &data {
            assert!(v.is_finite(), "RMSNorm output should be finite");
        }
    }
}

#[test]
fn test_ane_softmax_fallback() {
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();

    let config = AneConfig {
        batch_threshold: 1000, // Force MLX fallback
        ..Default::default()
    };

    if let Some(ane) = AneAccelerator::try_new(config) {
        let result = ane.softmax(&x, -1).unwrap();
        let data = result.to_vec_f32().unwrap();

        assert_eq!(data.len(), 8);

        // Softmax outputs should sum to 1 per row and be in [0, 1]
        for row in data.chunks(4) {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5, "Softmax row should sum to 1, got {}", sum);
            for v in row {
                assert!(*v >= 0.0 && *v <= 1.0, "Softmax values should be in [0,1]");
            }
        }
    }
}

#[test]
fn test_ane_layernorm_determinism() {
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();
    let bias = Array::zeros(&[4]).unwrap();

    let config = AneConfig::default();

    if let Some(ane) = AneAccelerator::try_new(config) {
        let result1 = ane.layernorm(&x, &weight, &bias, 1e-5).unwrap();
        let result2 = ane.layernorm(&x, &weight, &bias, 1e-5).unwrap();
        let result3 = ane.layernorm(&x, &weight, &bias, 1e-5).unwrap();

        let data1 = result1.to_vec_f32().unwrap();
        let data2 = result2.to_vec_f32().unwrap();
        let data3 = result3.to_vec_f32().unwrap();

        // Should be bit-exact deterministic
        assert_exact_equal(&data1, &data2, "ANE layernorm run 1 vs 2");
        assert_exact_equal(&data2, &data3, "ANE layernorm run 2 vs 3");
    }
}

#[test]
fn test_ane_matches_mlx() {
    // Verify that ANE path produces same results as MLX path (within tolerance)
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();
    let bias = Array::zeros(&[4]).unwrap();

    // MLX reference (direct Array method)
    let mlx_result = x.layernorm(&weight, &bias, 1e-5).unwrap();
    let mlx_data = mlx_result.to_vec_f32().unwrap();

    let config = AneConfig::default();

    if let Some(ane) = AneAccelerator::try_new(config) {
        // ANE accelerated (if batch meets threshold, otherwise also MLX)
        let ane_result = ane.layernorm(&x, &weight, &bias, 1e-5).unwrap();
        let ane_data = ane_result.to_vec_f32().unwrap();

        // Results should match within floating point tolerance
        // ANE uses fixed-point which may have slight differences
        assert_approx_equal(&mlx_data, &ane_data, 1e-4, "ANE vs MLX layernorm");
    }
}

#[test]
fn test_ane_large_batch() {
    // Test with batch size above default threshold (32)
    let batch_size = 64;
    let hidden_dim = 128;
    let data: Vec<f32> = (0..(batch_size * hidden_dim))
        .map(|i| (i as f32) * 0.01)
        .collect();

    let x = Array::from_f32(&data, &[batch_size as i32, hidden_dim as i32]).unwrap();
    let weight = Array::ones(&[hidden_dim as i32]).unwrap();
    let bias = Array::zeros(&[hidden_dim as i32]).unwrap();

    let config = AneConfig::production();

    if let Some(ane) = AneAccelerator::try_new(config) {
        // Should use ANE path for large batch
        assert!(ane.should_use_ane(batch_size));

        let result = ane.layernorm(&x, &weight, &bias, 1e-5).unwrap();
        let result_data = result.to_vec_f32().unwrap();

        assert_eq!(result_data.len(), batch_size * hidden_dim);

        // Verify normalization
        for v in &result_data {
            assert!(v.is_finite(), "Output should be finite");
        }
    }
}

#[test]
fn test_ane_debug_info() {
    let config = AneConfig::default();

    if let Some(ane) = AneAccelerator::try_new(config) {
        // Debug format should work
        let debug_str = format!("{:?}", ane);
        assert!(debug_str.contains("AneAccelerator"));
        assert!(debug_str.contains("available"));
        assert!(debug_str.contains("batch_threshold"));
    }
}

#[test]
fn test_is_available_method() {
    let config = AneConfig::default();

    if let Some(ane) = AneAccelerator::try_new(config) {
        // is_available should return true if we got an accelerator
        assert!(ane.is_available());
    }
}

#[test]
fn test_batch_threshold_method() {
    let config = AneConfig {
        batch_threshold: 64,
        ..Default::default()
    };

    if let Some(ane) = AneAccelerator::try_new(config) {
        assert_eq!(ane.batch_threshold(), 64);
    }
}

#[test]
#[cfg(all(target_os = "macos", feature = "coreml-ane"))]
fn test_ane_coreml_operations() {
    // This test only runs when ANE feature is enabled
    use adapteros_lora_kernel_coreml::{has_neural_engine, get_mltensor_api_version, MltensorApiVersion};

    println!("Neural Engine available: {}", has_neural_engine());
    println!("MLTensor API version: {:?}", get_mltensor_api_version());

    let config = AneConfig::production();

    match AneAccelerator::try_new(config) {
        Some(ane) => {
            println!("ANE Accelerator created successfully");
            let report = ane.attest();
            println!("Attestation: {:?}", report);

            // Run actual ANE operations
            let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
            let weight = Array::ones(&[4]).unwrap();
            let bias = Array::zeros(&[4]).unwrap();

            let result = ane.layernorm(&x, &weight, &bias, 1e-5).unwrap();
            println!("LayerNorm result: {:?}", result.to_vec_f32().unwrap());
        }
        None => {
            println!("ANE Accelerator not available (likely missing Neural Engine hardware or macOS 15+)");
        }
    }
}
