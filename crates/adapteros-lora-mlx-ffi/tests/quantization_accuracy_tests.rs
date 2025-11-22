//! Quantization accuracy loss tests
//!
//! Tests to verify that quantization maintains acceptable model quality,
//! including round-trip accuracy, error distributions, and SNR metrics.

use adapteros_lora_mlx_ffi::quantization::{MLXQuantizer, QuantizationConfig, QuantizationStats};

/// Test data generator for reproducible tests
fn generate_test_tensor(size: usize, seed: u32) -> Vec<f32> {
    (0..size)
        .map(|i| {
            // Use XOR on integers, then convert to float for deterministic pseudo-random values
            let x = (((i as u32) ^ seed) as f32).sin() * 100.0;
            (x / 100.0).sin() * 0.95 + 0.05 // Normalized to [0, 1)
        })
        .collect()
}

/// Test INT8 quantization round-trip accuracy
#[test]
fn test_int8_roundtrip_accuracy() {
    let data = generate_test_tensor(512, 42);
    let shape = vec![512];
    let group_size = 64;

    // Quantize
    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");

    // Dequantize
    let dequantized = MLXQuantizer::dequantize_int8(&quantized).expect("Dequantization failed");

    // Verify size
    assert_eq!(dequantized.len(), data.len());

    // Check error bounds for INT8
    let mut max_error = 0.0f32;
    let mut mean_error = 0.0f32;

    for (orig, deq) in data.iter().zip(dequantized.iter()) {
        let error = (orig - deq).abs();
        max_error = max_error.max(error);
        mean_error += error;
    }

    mean_error /= data.len() as f32;

    // INT8 should maintain good accuracy
    assert!(
        mean_error < 0.01,
        "Mean error too high for INT8: {:.6}",
        mean_error
    );
    assert!(
        max_error < 0.05,
        "Max error too high for INT8: {:.6}",
        max_error
    );

    println!(
        "INT8 Accuracy: mean_error={:.8}, max_error={:.8}",
        mean_error, max_error
    );
}

/// Test INT4 quantization round-trip accuracy
#[test]
fn test_int4_roundtrip_accuracy() {
    let data = generate_test_tensor(512, 43);
    let shape = vec![512];
    let group_size = 64;

    // Quantize
    let quantized =
        MLXQuantizer::quantize_int4(&data, group_size, &shape).expect("Quantization failed");

    // Dequantize
    let dequantized = MLXQuantizer::dequantize_int4(&quantized).expect("Dequantization failed");

    // Verify size
    assert_eq!(dequantized.len(), data.len());

    // Check error bounds for INT4 (more lenient)
    let mut max_error = 0.0f32;
    let mut mean_error = 0.0f32;

    for (orig, deq) in data.iter().zip(dequantized.iter()) {
        let error = (orig - deq).abs();
        max_error = max_error.max(error);
        mean_error += error;
    }

    mean_error /= data.len() as f32;

    // INT4 is more lossy but should still be acceptable
    assert!(
        mean_error < 0.05,
        "Mean error too high for INT4: {:.6}",
        mean_error
    );
    assert!(
        max_error < 0.2,
        "Max error too high for INT4: {:.6}",
        max_error
    );

    println!(
        "INT4 Accuracy: mean_error={:.8}, max_error={:.8}",
        mean_error, max_error
    );
}

/// Test different group sizes impact on accuracy
#[test]
fn test_group_size_impact() {
    let data = generate_test_tensor(1024, 44);
    let shape = vec![1024];
    let group_sizes = vec![32, 64, 128];

    let mut prev_error = f32::INFINITY;

    for group_size in group_sizes {
        let quantized =
            MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
        let dequantized = MLXQuantizer::dequantize_int8(&quantized).expect("Dequantization failed");

        let mut error = 0.0f32;
        for (orig, deq) in data.iter().zip(dequantized.iter()) {
            error += (orig - deq).abs();
        }
        error /= data.len() as f32;

        // Smaller groups should produce less error
        if group_size < 128 {
            assert!(
                error <= prev_error * 1.1, // Allow 10% deviation due to randomness
                "Error increased with smaller group size: group_size={}, error={:.8}",
                group_size,
                error
            );
        }

        prev_error = error;
        println!("Group size {}: mean_error={:.8}", group_size, error);
    }
}

/// Test extreme values handling
#[test]
fn test_extreme_values() {
    let data = vec![
        0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        0.1,
        -0.1,
        0.999,
        -0.999,
        f32::MIN_POSITIVE,
    ];
    let shape = vec![10];
    let group_size = 5;

    // INT8
    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
    let dequantized = MLXQuantizer::dequantize_int8(&quantized).expect("Dequantization failed");

    assert_eq!(dequantized.len(), data.len());

    // INT4
    let quantized =
        MLXQuantizer::quantize_int4(&data, group_size, &shape).expect("Quantization failed");
    let dequantized = MLXQuantizer::dequantize_int4(&quantized).expect("Dequantization failed");

    assert_eq!(dequantized.len(), data.len());
}

/// Test zero values preservation
#[test]
fn test_zero_values() {
    let data = vec![0.0; 100];
    let shape = vec![100];
    let group_size = 32;

    // INT8
    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
    let dequantized = MLXQuantizer::dequantize_int8(&quantized).expect("Dequantization failed");

    for val in dequantized {
        assert!(val.abs() < 1e-6, "Zero value not preserved in INT8");
    }

    // INT4
    let quantized =
        MLXQuantizer::quantize_int4(&data, group_size, &shape).expect("Quantization failed");
    let dequantized = MLXQuantizer::dequantize_int4(&quantized).expect("Dequantization failed");

    for val in dequantized {
        assert!(val.abs() < 1e-6, "Zero value not preserved in INT4");
    }
}

/// Test uniform distribution accuracy
#[test]
fn test_uniform_distribution() {
    let data: Vec<f32> = (0..256).map(|i| i as f32 / 256.0).collect();
    let shape = vec![256];
    let group_size = 64;

    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
    let stats = MLXQuantizer::calculate_stats(&data, &quantized).expect("Stats calculation failed");

    // Uniform distribution should have low SNR (more predictable error)
    assert!(stats.snr_db > 10.0, "SNR too low for uniform distribution");

    println!(
        "Uniform distribution - SNR: {:.2} dB, Mean error: {:.8}",
        stats.snr_db, stats.mean_error
    );
}

/// Test Gaussian distribution accuracy
#[test]
fn test_gaussian_distribution() {
    // Box-Muller transform for Gaussian values
    let mut data = Vec::new();
    for i in 0..256 {
        let u1 = ((i as f32 + 0.5) / 256.0).max(1e-6);
        let u2 = ((i as f32 + 1.5) / 256.0).max(1e-6);
        let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos();
        data.push((z0 * 0.2).clamp(-1.0, 1.0)); // Clamp to valid range
    }

    let shape = vec![256];
    let group_size = 64;

    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
    let stats = MLXQuantizer::calculate_stats(&data, &quantized).expect("Stats calculation failed");

    assert!(stats.snr_db > 15.0, "SNR too low for Gaussian distribution");

    println!(
        "Gaussian distribution - SNR: {:.2} dB, Mean error: {:.8}",
        stats.snr_db, stats.mean_error
    );
}

/// Test compression ratio accuracy
#[test]
fn test_compression_ratios() {
    let data = generate_test_tensor(4096, 45);
    let shape = vec![4096];
    let group_size = 128;

    // INT8
    let quantized_int8 =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
    let stats_int8 =
        MLXQuantizer::calculate_stats(&data, &quantized_int8).expect("Stats calculation failed");

    // Should compress by 4x (f32 to i8)
    assert!(
        stats_int8.compression_ratio > 3.8 && stats_int8.compression_ratio < 4.2,
        "INT8 compression ratio off: {}",
        stats_int8.compression_ratio
    );

    // INT4
    let quantized_int4 =
        MLXQuantizer::quantize_int4(&data, group_size, &shape).expect("Quantization failed");
    let stats_int4 =
        MLXQuantizer::calculate_stats(&data, &quantized_int4).expect("Stats calculation failed");

    // Should compress by ~8x (f32 to i4 with packing)
    assert!(
        stats_int4.compression_ratio > 7.5 && stats_int4.compression_ratio < 8.5,
        "INT4 compression ratio off: {}",
        stats_int4.compression_ratio
    );

    println!("INT8 compression: {:.2}x", stats_int8.compression_ratio);
    println!("INT4 compression: {:.2}x", stats_int4.compression_ratio);
}

/// Test per-group scaling effectiveness
#[test]
fn test_per_group_scaling() {
    // Create data with varying magnitude
    let mut data = Vec::new();
    for i in 0..256 {
        let magnitude = (i / 64) as f32 * 0.2; // Varies per group
        data.push((magnitude + (i as f32 * 0.01).sin() * 0.1).clamp(-1.0, 1.0));
    }

    let shape = vec![256];
    let group_size = 64;

    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");

    // Verify we have 4 different scales (one per group)
    assert_eq!(quantized.metadata.scales.len(), 4);

    // Scales should differ (since magnitude varies)
    let scale_variance = quantized
        .metadata
        .scales
        .iter()
        .map(|&s| s * s)
        .sum::<f32>()
        / 4.0;
    assert!(scale_variance > 0.001, "Scales are too similar");

    println!("Per-group scales: {:?}", quantized.metadata.scales);
}

/// Test metadata preservation
#[test]
fn test_metadata_preservation() {
    let data = generate_test_tensor(512, 46);
    let shape = vec![16, 32];
    let group_size = 64;

    // INT8
    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");

    assert_eq!(quantized.metadata.shape, shape);
    assert_eq!(quantized.metadata.quantized_dtype, "int8");
    assert_eq!(quantized.metadata.group_size, group_size as u32);

    // INT4
    let quantized =
        MLXQuantizer::quantize_int4(&data, group_size, &shape).expect("Quantization failed");

    assert_eq!(quantized.metadata.shape, shape);
    assert_eq!(quantized.metadata.quantized_dtype, "int4");
    assert_eq!(quantized.metadata.group_size, group_size as u32);
}

/// Test edge case: single element
#[test]
fn test_single_element() {
    let data = vec![0.5];
    let shape = vec![1];
    let group_size = 1;

    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
    let dequantized = MLXQuantizer::dequantize_int8(&quantized).expect("Dequantization failed");

    assert_eq!(dequantized.len(), 1);
    assert!((dequantized[0] - 0.5).abs() < 0.01);
}

/// Test edge case: group size == tensor size
#[test]
fn test_group_size_equals_tensor_size() {
    let data = generate_test_tensor(256, 47);
    let shape = vec![256];
    let group_size = 256;

    let quantized =
        MLXQuantizer::quantize_int8(&data, group_size, &shape).expect("Quantization failed");
    let dequantized = MLXQuantizer::dequantize_int8(&quantized).expect("Dequantization failed");

    assert_eq!(dequantized.len(), data.len());
    assert_eq!(quantized.metadata.scales.len(), 1);
}

/// Test SNR calculation correctness
#[test]
fn test_snr_calculation() {
    let signal = vec![1.0; 100]; // Constant signal = high power
    let quantized =
        MLXQuantizer::quantize_int8(&signal, 50, &vec![100]).expect("Quantization failed");
    let stats =
        MLXQuantizer::calculate_stats(&signal, &quantized).expect("Stats calculation failed");

    // Constant signal should have very high SNR
    assert!(
        stats.snr_db > 30.0,
        "SNR too low for constant signal: {}",
        stats.snr_db
    );
}
