//! Tests for attention entropy extraction
//!
//! Validates entropy computation accuracy and integration with router features.

use adapteros_lora_router::{extract_attn_entropy, CodeFeatures};

#[test]
fn test_entropy_extraction_peaked_distribution() {
    // Create logits with peaked distribution (low entropy)
    let peaked_logits = vec![
        vec![10.0, 0.0, 0.0, 0.0, 0.0], // Very peaked
        vec![9.0, 1.0, 0.0, 0.0, 0.0],  // Still peaked
        vec![10.0, 0.0, 0.0, 0.0, 0.0], // Very peaked
    ];

    let entropy = extract_attn_entropy(&peaked_logits, None);

    // Low entropy expected for peaked distributions
    assert!(
        entropy < 1.0,
        "Peaked distribution should have low entropy, got {}",
        entropy
    );
    assert!(entropy >= 0.0, "Entropy should be non-negative");
}

#[test]
fn test_entropy_extraction_uniform_distribution() {
    // Create logits with uniform distribution (high entropy)
    let uniform_logits = vec![
        vec![1.0, 1.0, 1.0, 1.0, 1.0], // Uniform
        vec![1.0, 1.0, 1.0, 1.0, 1.0], // Uniform
        vec![1.0, 1.0, 1.0, 1.0, 1.0], // Uniform
    ];

    let entropy = extract_attn_entropy(&uniform_logits, None);

    // High entropy expected for uniform distributions
    // log2(5) ≈ 2.32
    assert!(
        entropy > 2.0,
        "Uniform distribution should have high entropy, got {}",
        entropy
    );
    assert!(entropy < 2.5, "Entropy should be bounded");
}

#[test]
fn test_entropy_window_size() {
    let logits = vec![
        vec![10.0, 0.0, 0.0], // Peaked
        vec![10.0, 0.0, 0.0], // Peaked
        vec![1.0, 1.0, 1.0],  // Uniform
        vec![1.0, 1.0, 1.0],  // Uniform
        vec![1.0, 1.0, 1.0],  // Uniform
    ];

    // With window size 2, should only consider last 2 tokens (uniform)
    let entropy_small_window = extract_attn_entropy(&logits, Some(2));

    // With window size 5, should consider all tokens (mix of peaked and uniform)
    let entropy_large_window = extract_attn_entropy(&logits, Some(5));

    // Small window (only uniform tokens) should have higher entropy
    assert!(
        entropy_small_window > entropy_large_window,
        "Small window (uniform only) should have higher entropy than large window (mixed)"
    );
}

#[test]
fn test_entropy_integration_with_features() {
    let mut features = CodeFeatures::from_context("Fix the bug in src/main.rs");

    // Simulate inference with some logits
    let logits = vec![
        vec![5.0, 2.0, 1.0, 1.0], // Moderate entropy
        vec![4.0, 3.0, 2.0, 1.0], // Moderate entropy
        vec![3.0, 3.0, 2.0, 2.0], // Higher entropy
    ];

    let entropy = extract_attn_entropy(&logits, None);
    features.set_attn_entropy(entropy);

    // Convert to vector and verify entropy is included
    let vec = features.to_vector();
    assert_eq!(vec.len(), 22, "Feature vector should have 22 dimensions");

    let extracted_entropy = *vec.last().unwrap();
    assert!(
        (extracted_entropy - entropy).abs() < 1e-6,
        "Extracted entropy should match computed entropy"
    );
}

#[test]
fn test_entropy_empty_logits() {
    let empty_logits: Vec<Vec<f32>> = vec![];
    let entropy = extract_attn_entropy(&empty_logits, None);

    assert_eq!(entropy, 0.0, "Empty logits should have zero entropy");
}

#[test]
fn test_entropy_single_token() {
    let single_token = vec![
        vec![2.0, 1.0, 1.0, 0.5], // One token
    ];

    let entropy = extract_attn_entropy(&single_token, None);
    assert!(entropy > 0.0, "Single token should have non-zero entropy");
}

#[test]
fn test_entropy_numerical_stability() {
    // Test with very large logits (could cause overflow in naive softmax)
    let large_logits = vec![
        vec![1000.0, 0.0, 0.0, 0.0], // Very large values
        vec![999.0, 1.0, 0.0, 0.0],
    ];

    let entropy = extract_attn_entropy(&large_logits, None);

    // Should handle large values without NaN or inf
    assert!(
        entropy.is_finite(),
        "Entropy should be finite for large logits"
    );
    assert!(entropy >= 0.0, "Entropy should be non-negative");
}

#[test]
fn test_entropy_gradual_change() {
    // Gradually changing distributions
    let changing_logits = vec![
        vec![10.0, 1.0, 1.0, 1.0], // Very peaked
        vec![8.0, 2.0, 1.0, 1.0],  // Less peaked
        vec![6.0, 3.0, 2.0, 1.0],  // Even less peaked
        vec![4.0, 4.0, 3.0, 2.0],  // More uniform
    ];

    let entropy = extract_attn_entropy(&changing_logits, None);

    // Entropy should be moderate (between peaked and uniform)
    assert!(
        entropy > 0.5 && entropy < 2.0,
        "Mixed distributions should have moderate entropy, got {}",
        entropy
    );
}
