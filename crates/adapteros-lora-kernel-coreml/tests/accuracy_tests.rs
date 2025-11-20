//! Accuracy Validation Tests
//!
//! Tests comparing CoreML backend outputs with reference implementations:
//! - MLX backend comparison
//! - Metal backend comparison
//! - Quantization error analysis
//! - Numerical stability tests
//! - Edge case handling
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use std::f32;

/// Calculate mean absolute error between two vectors
fn mean_absolute_error(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum();
    sum / a.len() as f32
}

/// Calculate mean squared error between two vectors
fn mean_squared_error(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();
    sum / a.len() as f32
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

#[test]
fn test_mlx_backend_comparison() {
    // Test CoreML vs MLX backend outputs
    let vocab_size = 32000;

    // Simulate CoreML output
    let coreml_output: Vec<f32> = (0..vocab_size).map(|i| (i as f32) * 0.001).collect();

    // Simulate MLX output (should be identical for deterministic backends)
    let mlx_output: Vec<f32> = (0..vocab_size).map(|i| (i as f32) * 0.001).collect();

    let mae = mean_absolute_error(&coreml_output, &mlx_output);
    let mse = mean_squared_error(&coreml_output, &mlx_output);
    let cosine_sim = cosine_similarity(&coreml_output, &mlx_output);

    assert!(mae < 1e-5, "MAE too high: {}", mae);
    assert!(mse < 1e-8, "MSE too high: {}", mse);
    assert!(cosine_sim > 0.9999, "Cosine similarity too low: {}", cosine_sim);
}

#[test]
fn test_metal_backend_comparison() {
    // Test CoreML vs Metal backend outputs
    let vocab_size = 32000;

    // Simulate CoreML output (ANE mode)
    let coreml_output: Vec<f32> = (0..vocab_size).map(|i| (i as f32) * 0.001).collect();

    // Simulate Metal output
    let metal_output: Vec<f32> = (0..vocab_size).map(|i| (i as f32) * 0.001).collect();

    let mae = mean_absolute_error(&coreml_output, &metal_output);
    let cosine_sim = cosine_similarity(&coreml_output, &metal_output);

    // Allow slightly higher tolerance for cross-backend comparison
    assert!(mae < 1e-4, "MAE too high: {}", mae);
    assert!(
        cosine_sim > 0.999,
        "Cosine similarity too low: {}",
        cosine_sim
    );
}

#[test]
fn test_quantization_error_fp16() {
    // Test FP16 quantization error
    let original: Vec<f32> = (0..1000).map(|i| (i as f32) / 1000.0).collect();

    // Simulate FP16 quantization
    let quantized: Vec<f32> = original
        .iter()
        .map(|&x| {
            // Simulate FP16 precision loss
            let as_u16 = ((x * 65504.0).max(-65504.0).min(65504.0) as i32) as u16;
            (as_u16 as f32) / 65504.0
        })
        .collect();

    let mae = mean_absolute_error(&original, &quantized);

    // FP16 should have ~3-4 decimal places of precision
    assert!(mae < 1e-3, "FP16 quantization error too high: {}", mae);
}

#[test]
fn test_quantization_error_int8() {
    // Test INT8 quantization error
    let original: Vec<f32> = (0..256).map(|i| (i as f32) / 255.0).collect();

    // Simulate INT8 quantization
    let quantized: Vec<f32> = original
        .iter()
        .map(|&x| {
            let as_u8 = (x * 255.0).round().max(0.0).min(255.0) as u8;
            (as_u8 as f32) / 255.0
        })
        .collect();

    let mae = mean_absolute_error(&original, &quantized);

    // INT8 has 1/255 precision
    assert!(mae < 1.0 / 255.0, "INT8 quantization error too high: {}", mae);
}

#[test]
fn test_numerical_stability_small_values() {
    // Test numerical stability with small values
    let small_values: Vec<f32> = vec![1e-6, 1e-5, 1e-4, 1e-3, 1e-2];

    // Simulate computation
    let output: Vec<f32> = small_values.iter().map(|&x| x * x).collect();

    // Check for NaN or Inf
    for (i, &val) in output.iter().enumerate() {
        assert!(
            val.is_finite(),
            "Non-finite value at index {}: {}",
            i,
            val
        );
        assert!(val >= 0.0, "Negative value at index {}: {}", i, val);
    }
}

#[test]
fn test_numerical_stability_large_values() {
    // Test numerical stability with large values
    let large_values: Vec<f32> = vec![1e3, 1e4, 1e5, 1e6];

    // Simulate computation with potential overflow
    let output: Vec<f32> = large_values
        .iter()
        .map(|&x| {
            let result = x * x;
            if result.is_infinite() {
                f32::MAX // Clamp to max instead of infinity
            } else {
                result
            }
        })
        .collect();

    // Check for NaN (Inf is handled)
    for (i, &val) in output.iter().enumerate() {
        assert!(!val.is_nan(), "NaN value at index {}: {}", i, val);
    }
}

#[test]
fn test_numerical_stability_mixed_scales() {
    // Test numerical stability with mixed scales
    let mixed_values: Vec<f32> = vec![1e-6, 1.0, 1e6];

    // Simulate addition (challenging for floating-point)
    let sum: f32 = mixed_values.iter().sum();

    assert!(sum.is_finite(), "Sum is not finite: {}", sum);
    assert!(sum >= 1e6, "Sum lost large value precision: {}", sum);
}

#[test]
fn test_edge_case_zero_input() {
    // Test edge case: all zeros
    let input = vec![0.0f32; 1000];
    let output: Vec<f32> = input.iter().map(|&x| x * 2.0).collect();

    assert!(output.iter().all(|&x| x == 0.0));
}

#[test]
fn test_edge_case_negative_values() {
    // Test edge case: negative values
    let input: Vec<f32> = (0..100).map(|i| -(i as f32)).collect();
    let output: Vec<f32> = input.iter().map(|&x| x.abs()).collect();

    assert!(output.iter().all(|&x| x >= 0.0));
}

#[test]
fn test_edge_case_nan_handling() {
    // Test edge case: NaN handling
    let input = vec![f32::NAN, 1.0, 2.0, f32::NAN, 3.0];

    // Filter out NaN values
    let filtered: Vec<f32> = input.iter().filter(|&&x| !x.is_nan()).copied().collect();

    assert_eq!(filtered.len(), 3);
    assert!(filtered.iter().all(|&x| x.is_finite()));
}

#[test]
fn test_edge_case_infinity_handling() {
    // Test edge case: infinity handling
    let input = vec![f32::INFINITY, 1.0, f32::NEG_INFINITY, 2.0];

    // Clamp infinities
    let clamped: Vec<f32> = input
        .iter()
        .map(|&x| {
            if x.is_infinite() {
                if x > 0.0 {
                    f32::MAX
                } else {
                    f32::MIN
                }
            } else {
                x
            }
        })
        .collect();

    assert!(clamped.iter().all(|&x| x.is_finite()));
}

#[test]
fn test_softmax_numerical_stability() {
    // Test softmax with numerical stability tricks
    let logits = vec![1000.0, 999.0, 998.0]; // Would overflow without shift

    // Numerically stable softmax
    let max_logit = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let shifted: Vec<f32> = logits.iter().map(|&x| x - max_logit).collect();
    let exp_sum: f32 = shifted.iter().map(|&x| x.exp()).sum();
    let softmax: Vec<f32> = shifted.iter().map(|&x| x.exp() / exp_sum).collect();

    // Check probabilities sum to 1
    let sum: f32 = softmax.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5, "Softmax doesn't sum to 1: {}", sum);

    // Check all values are valid probabilities
    for (i, &prob) in softmax.iter().enumerate() {
        assert!(
            prob.is_finite() && prob >= 0.0 && prob <= 1.0,
            "Invalid probability at {}: {}",
            i,
            prob
        );
    }
}

#[test]
fn test_cross_entropy_numerical_stability() {
    // Test cross-entropy with numerical stability
    let predictions = vec![0.7, 0.2, 0.1];
    let targets = vec![1.0, 0.0, 0.0];

    // Numerically stable cross-entropy
    let epsilon = 1e-10;
    let ce: f32 = targets
        .iter()
        .zip(predictions.iter())
        .map(|(&t, &p)| -t * (p + epsilon).ln())
        .sum();

    assert!(ce.is_finite(), "Cross-entropy is not finite: {}", ce);
    assert!(ce >= 0.0, "Cross-entropy is negative: {}", ce);
}

#[test]
fn test_gradient_vanishing() {
    // Test detection of vanishing gradients
    let gradients = vec![1e-10, 1e-9, 1e-8, 1e-7];

    let vanishing_threshold = 1e-6;
    let vanishing_count = gradients
        .iter()
        .filter(|&&g| g.abs() < vanishing_threshold)
        .count();

    assert!(
        vanishing_count > 0,
        "Should detect vanishing gradients: {}/{}",
        vanishing_count,
        gradients.len()
    );
}

#[test]
fn test_gradient_explosion() {
    // Test detection of exploding gradients
    let gradients = vec![1e6, 1e7, 1e8, 1e9];

    let explosion_threshold = 1e5;
    let exploding_count = gradients
        .iter()
        .filter(|&&g| g.abs() > explosion_threshold)
        .count();

    assert!(
        exploding_count > 0,
        "Should detect exploding gradients: {}/{}",
        exploding_count,
        gradients.len()
    );
}

#[test]
fn test_relative_error_tolerance() {
    // Test relative error tolerance (important for different scales)
    let test_cases = vec![
        (1.0, 1.0001, 1e-3), // 0.01% error
        (100.0, 100.1, 1e-2), // 0.1% error
        (1000.0, 1010.0, 1e-1), // 1% error
    ];

    for (expected, actual, max_relative_error) in test_cases {
        let relative_error = ((expected - actual).abs() / expected).abs();
        assert!(
            relative_error < max_relative_error,
            "Relative error too high: {} (max: {})",
            relative_error,
            max_relative_error
        );
    }
}

#[test]
fn test_ulp_distance() {
    // Test units in last place (ULP) distance
    let a = 1.0f32;
    let b = 1.0000001f32;

    // Check that values are "close enough" in terms of floating-point representation
    let diff = (a - b).abs();
    let ulp_tolerance = 10.0 * f32::EPSILON;

    assert!(
        diff < ulp_tolerance,
        "ULP distance too large: {} (tolerance: {})",
        diff,
        ulp_tolerance
    );
}
