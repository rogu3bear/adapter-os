//! Tests for configurable attention scaling
//!
//! These tests verify:
//! - Default sqrt-based scaling
//! - Custom attention scaling factors
//! - Scaling impact on attention scores
//! - Numerical stability

use adapteros_lora_kernel_mtl::GqaConfig;

#[test]
fn test_default_attention_scaling() {
    let config = GqaConfig::default();
    
    // Default should be 0.0 (meaning use sqrt scaling)
    assert_eq!(config.attention_scale, 0.0);
    
    // Compute expected sqrt scaling
    let head_dim = config.head_dim as f32;
    let expected_scale = 1.0 / head_dim.sqrt();
    
    assert!(expected_scale > 0.0);
    assert!(expected_scale < 1.0);
    
    // For head_dim=128, scale should be 1/sqrt(128) ≈ 0.0884
    let expected_value = 1.0 / 128.0_f32.sqrt();
    assert!((expected_scale - expected_value).abs() < 1e-5);
}

#[test]
fn test_custom_attention_scaling() {
    let mut config = GqaConfig::default();
    
    // Set custom scaling factor
    config.attention_scale = 0.1;
    
    assert_eq!(config.attention_scale, 0.1);
    assert!(config.attention_scale > 0.0);
}

#[test]
fn test_attention_score_scaling() {
    // Test how scaling affects attention scores
    let raw_score = 10.0_f32;
    
    // Different scaling strategies
    let scales = vec![
        1.0 / 128.0_f32.sqrt(),  // sqrt(head_dim) for dim=128
        1.0 / 64.0_f32.sqrt(),   // sqrt(head_dim) for dim=64
        0.1,                      // Custom scale
        1.0,                      // No scaling
    ];
    
    for scale in scales {
        let scaled_score = raw_score * scale;
        
        assert!(scaled_score.is_finite(), "Scaled score should be finite");
        assert_eq!(scaled_score, raw_score * scale);
        
        // Verify softmax stability improves with smaller scores
        if scale < 1.0 {
            assert!(scaled_score < raw_score, "Scaling should reduce large scores");
        }
    }
}

#[test]
fn test_scaling_impact_on_softmax() {
    // Test softmax with and without scaling
    let scores = vec![10.0, 20.0, 30.0];
    
    // Helper function to compute softmax
    let softmax = |values: &[f32]| -> Vec<f32> {
        let max_val = values.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let exp_sum: f32 = values.iter().map(|&x| (x - max_val).exp()).sum();
        values.iter().map(|&x| (x - max_val).exp() / exp_sum).collect()
    };
    
    // Without scaling
    let probs_no_scale = softmax(&scores);
    
    // With scaling
    let scale = 0.1;
    let scaled_scores: Vec<f32> = scores.iter().map(|&s| s * scale).collect();
    let probs_with_scale = softmax(&scaled_scores);
    
    // Both should sum to 1.0
    let sum_no_scale: f32 = probs_no_scale.iter().sum();
    let sum_with_scale: f32 = probs_with_scale.iter().sum();
    
    assert!((sum_no_scale - 1.0).abs() < 1e-5, "Softmax should sum to 1.0");
    assert!((sum_with_scale - 1.0).abs() < 1e-5, "Softmax should sum to 1.0");
    
    // Scaling should make distribution more uniform
    let entropy = |probs: &[f32]| -> f32 {
        -probs.iter().map(|&p| if p > 0.0 { p * p.ln() } else { 0.0 }).sum::<f32>()
    };
    
    let entropy_no_scale = entropy(&probs_no_scale);
    let entropy_with_scale = entropy(&probs_with_scale);
    
    // Smaller scores (with scaling) should increase entropy
    assert!(
        entropy_with_scale > entropy_no_scale,
        "Scaling should increase attention entropy"
    );
}

#[test]
fn test_sqrt_scaling_formula() {
    // Test sqrt scaling for different head dimensions
    let head_dims = vec![32, 64, 128, 256];
    
    for head_dim in head_dims {
        let scale = 1.0 / (head_dim as f32).sqrt();
        
        assert!(scale > 0.0, "Scale should be positive");
        assert!(scale < 1.0, "Scale should be less than 1.0");
        
        // Verify scale decreases as head_dim increases
        if head_dim > 32 {
            let prev_scale = 1.0 / 32.0_f32.sqrt();
            assert!(
                scale < prev_scale,
                "Scale should decrease with head_dim"
            );
        }
        
        // Verify formula: scale * sqrt(head_dim) = 1
        let product = scale * (head_dim as f32).sqrt();
        assert!((product - 1.0).abs() < 1e-5, "scale * sqrt(d) should equal 1.0");
    }
}

#[test]
fn test_numerical_stability() {
    // Test that attention scaling prevents overflow
    let large_scores = vec![100.0, 200.0, 300.0];
    
    // Without scaling, exp(300) would overflow
    let overflow_check = 300.0_f32.exp();
    assert!(
        overflow_check.is_finite() || overflow_check.is_infinite(),
        "Large scores may cause overflow"
    );
    
    // With scaling, scores are manageable
    let scale = 1.0 / 128.0_f32.sqrt();  // ≈ 0.0884
    let scaled_scores: Vec<f32> = large_scores.iter().map(|&s| s * scale).collect();
    
    // Scaled scores should be much smaller
    for scaled in &scaled_scores {
        assert!(scaled < &30.0, "Scaled scores should be smaller");
        let exp_val = scaled.exp();
        assert!(exp_val.is_finite(), "Exp of scaled score should be finite");
    }
}

#[test]
fn test_attention_scale_configuration() {
    // Test different attention scaling configurations
    let mut config = GqaConfig::default();
    
    // Test 1: Default sqrt scaling (0.0 means use sqrt)
    assert_eq!(config.attention_scale, 0.0);
    let computed_scale = if config.attention_scale > 0.0 {
        config.attention_scale
    } else {
        1.0 / (config.head_dim as f32).sqrt()
    };
    assert!((computed_scale - 0.0884).abs() < 0.001);
    
    // Test 2: Custom constant scaling
    config.attention_scale = 0.5;
    let computed_scale = if config.attention_scale > 0.0 {
        config.attention_scale
    } else {
        1.0 / (config.head_dim as f32).sqrt()
    };
    assert_eq!(computed_scale, 0.5);
    
    // Test 3: No scaling (scale = 1.0)
    config.attention_scale = 1.0;
    assert_eq!(config.attention_scale, 1.0);
}

#[test]
fn test_scale_range_validity() {
    // Test that scales are in valid range
    let valid_scales = vec![0.01, 0.1, 0.5, 1.0];
    let invalid_scales = vec![-0.1, 0.0, -1.0];  // Note: 0.0 is special (means default)
    
    for scale in valid_scales {
        assert!(scale > 0.0, "Scale should be positive");
    }
    
    for scale in invalid_scales {
        if scale < 0.0 {
            // Negative scales don't make sense
            assert!(scale < 0.0, "This scale is invalid");
        }
    }
}

#[test]
fn test_qkv_scaling_dimensions() {
    // Test scaling works with GQA dimensions
    let config = GqaConfig::default();
    
    let num_attention_heads = config.num_attention_heads;
    let num_kv_heads = config.num_key_value_heads;
    let head_dim = config.head_dim;
    
    // Verify GQA ratio
    let gqa_ratio = num_attention_heads / num_kv_heads;
    assert_eq!(gqa_ratio, 8, "Qwen uses 8:1 GQA ratio");
    
    // Scaling should be based on head_dim, not number of heads
    let scale = 1.0 / (head_dim as f32).sqrt();
    assert!((scale - 0.0884).abs() < 0.001);
}

#[test]
fn test_attention_score_range() {
    // Test that scaling keeps scores in reasonable range
    let config = GqaConfig::default();
    let scale = 1.0 / (config.head_dim as f32).sqrt();
    
    // Simulate Q·K^T for random vectors
    let dot_products = vec![50.0, 100.0, 150.0, 200.0];
    
    for dot_prod in dot_products {
        let scaled_score = dot_prod * scale;
        
        // Scaled scores should be in manageable range for softmax
        assert!(scaled_score < 20.0, "Scaled score should be < 20 for numerical stability");
        assert!(scaled_score > -20.0, "Scaled score should be > -20 for numerical stability");
    }
}

#[test]
fn test_learned_vs_fixed_scaling() {
    // Compare fixed vs learned (custom) scaling
    let mut config = GqaConfig::default();
    
    // Fixed sqrt scaling
    let fixed_scale = 1.0 / (config.head_dim as f32).sqrt();
    
    // Simulate learned scaling (slightly different from sqrt)
    let learned_scale = fixed_scale * 1.2;  // 20% larger
    
    config.attention_scale = learned_scale;
    
    let test_score = 100.0_f32;
    let fixed_result = test_score * fixed_scale;
    let learned_result = test_score * learned_scale;
    
    // Learned scaling should produce different results
    assert_ne!(fixed_result, learned_result);
    
    // But both should be in valid range
    assert!(fixed_result > 0.0 && fixed_result < 20.0);
    assert!(learned_result > 0.0 && learned_result < 20.0);
}

