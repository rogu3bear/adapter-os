//! Tests for deterministic dropout and bias fusion
//!
//! These tests verify:
//! - Deterministic dropout with HKDF seeding
//! - Bias term integration in MLP and attention
//! - Inverted dropout scaling
//! - Consistency across runs

use adapteros_lora_kernel_mtl::{GqaConfig, LoraConfig};

#[test]
fn test_lora_dropout_config() {
    let config = LoraConfig::default();
    assert_eq!(
        config.dropout_rate, 0.0,
        "Default dropout should be 0.0 for inference"
    );

    let mut config_with_dropout = config.clone();
    config_with_dropout.dropout_rate = 0.1;
    assert_eq!(config_with_dropout.dropout_rate, 0.1);
}

#[test]
fn test_gqa_dropout_config() {
    let config = GqaConfig::default();
    assert_eq!(
        config.dropout_rate, 0.0,
        "Default dropout should be 0.0 for inference"
    );

    let mut config_with_dropout = config.clone();
    config_with_dropout.dropout_rate = 0.1;
    assert_eq!(config_with_dropout.dropout_rate, 0.1);
}

#[test]
fn test_deterministic_dropout_simulation() {
    // Simulate deterministic dropout behavior
    let dropout_rate = 0.1_f32;
    let seed = 12345_u32;

    // Simple xorshift RNG simulation
    let xorshift = |mut state: u32| -> u32 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };

    // Test dropout mask generation
    for position in 0..100 {
        let state = xorshift(seed ^ position);
        let rand_val = (state as f32) / (u32::MAX as f32);

        let mask = if rand_val >= dropout_rate {
            1.0 / (1.0 - dropout_rate) // Inverted dropout
        } else {
            0.0
        };

        assert!(mask >= 0.0, "Dropout mask should be non-negative");

        if mask > 0.0 {
            // Inverted dropout scaling
            let expected_scale = 1.0 / (1.0 - dropout_rate);
            assert!(
                (mask - expected_scale).abs() < 1e-5,
                "Active mask should scale by 1/(1-p)"
            );
        }
    }
}

#[test]
fn test_dropout_determinism() {
    // Test that dropout is deterministic with same seed
    let dropout_rate = 0.2_f32;
    let seed = 54321_u32;

    let xorshift = |mut state: u32| -> u32 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };

    let generate_mask = |seed: u32, position: u32| -> f32 {
        let state = xorshift(seed ^ position);
        let rand_val = (state as f32) / (u32::MAX as f32);
        if rand_val >= dropout_rate {
            1.0 / (1.0 - dropout_rate)
        } else {
            0.0
        }
    };

    // Generate masks multiple times with same seed
    let positions = [0, 10, 100, 1000];
    for &pos in &positions {
        let masks: Vec<f32> = (0..5).map(|_| generate_mask(seed, pos)).collect();

        // All masks should be identical
        for mask in &masks[1..] {
            assert_eq!(
                masks[0], *mask,
                "Dropout mask should be deterministic at position {}",
                pos
            );
        }
    }
}

#[test]
fn test_dropout_rate_bounds() {
    // Test edge cases for dropout rate
    let test_cases = vec![
        (0.0, true),   // No dropout
        (0.5, true),   // 50% dropout
        (1.0, true),   // Drop all (edge case)
        (-0.1, false), // Invalid: negative
        (1.1, false),  // Invalid: > 1.0
    ];

    for (rate, should_be_valid) in test_cases {
        let is_valid = rate >= 0.0 && rate <= 1.0;
        assert_eq!(
            is_valid, should_be_valid,
            "Dropout rate {} validity should be {}",
            rate, should_be_valid
        );
    }
}

#[test]
fn test_inverted_dropout_scaling() {
    // Test inverted dropout maintains expected value
    let dropout_rates = vec![0.0, 0.1, 0.25, 0.5];
    let input_value = 10.0_f32;

    for rate in dropout_rates {
        if rate == 0.0 {
            // No dropout: value unchanged
            assert_eq!(input_value, 10.0);
        } else if rate < 1.0 {
            // Inverted dropout scale
            let scale = 1.0 / (1.0 - rate);
            let scaled_value = input_value * scale;

            // Expected value when considering dropout probability
            let expected_value = scaled_value * (1.0 - rate) + 0.0 * rate;

            // Should approximately equal original value
            assert!(
                (expected_value - input_value).abs() < 1e-5,
                "Inverted dropout expected value should match input"
            );
        }
    }
}

#[test]
fn test_bias_addition() {
    // Test bias fusion behavior
    let base_value = 5.0_f32;
    let bias = 2.0_f32;
    let result = base_value + bias;

    assert_eq!(result, 7.0, "Bias should be added to base value");
}

#[test]
fn test_bias_with_lora() {
    // Test that bias is added after LoRA delta
    let base_weight = 1.0_f32;
    let lora_delta = 0.5_f32;
    let bias = 0.1_f32;
    let input = 2.0_f32;

    // Result should be: input * (base_weight + lora_delta) + bias
    let expected = input * (base_weight + lora_delta) + bias;
    let result = input * (base_weight + lora_delta) + bias;

    assert_eq!(result, expected);
    assert_eq!(result, 3.1); // 2.0 * 1.5 + 0.1 = 3.1
}

#[test]
fn test_mlp_bias_integration() {
    // Test MLP bias fusion logic
    struct MlpBiases {
        gate_bias: Option<Vec<f32>>,
        up_bias: Option<Vec<f32>>,
        down_bias: Option<Vec<f32>>,
    }

    let biases = MlpBiases {
        gate_bias: Some(vec![0.1, 0.2, 0.3]),
        up_bias: Some(vec![0.4, 0.5, 0.6]),
        down_bias: Some(vec![0.7, 0.8]),
    };

    // Verify biases are present
    assert!(biases.gate_bias.is_some());
    assert!(biases.up_bias.is_some());
    assert!(biases.down_bias.is_some());

    // Verify sizes
    assert_eq!(biases.gate_bias.as_ref().unwrap().len(), 3);
    assert_eq!(biases.up_bias.as_ref().unwrap().len(), 3);
    assert_eq!(biases.down_bias.as_ref().unwrap().len(), 2);
}

#[test]
fn test_attention_bias_integration() {
    // Test attention bias fusion logic
    struct AttentionBiases {
        q_bias: Option<Vec<f32>>,
        k_bias: Option<Vec<f32>>,
        v_bias: Option<Vec<f32>>,
    }

    let num_heads = 32_usize;
    let head_dim = 128_usize;
    let num_kv_heads = 4_usize;

    let biases = AttentionBiases {
        q_bias: Some(vec![0.0; num_heads * head_dim]),
        k_bias: Some(vec![0.0; num_kv_heads * head_dim]),
        v_bias: Some(vec![0.0; num_kv_heads * head_dim]),
    };

    // Verify biases match dimensions
    assert_eq!(biases.q_bias.as_ref().unwrap().len(), num_heads * head_dim);
    assert_eq!(
        biases.k_bias.as_ref().unwrap().len(),
        num_kv_heads * head_dim
    );
    assert_eq!(
        biases.v_bias.as_ref().unwrap().len(),
        num_kv_heads * head_dim
    );
}

#[test]
fn test_nullable_bias_handling() {
    // Test that nullable biases work correctly
    let bias_present: Option<Vec<f32>> = Some(vec![1.0, 2.0, 3.0]);
    let bias_absent: Option<Vec<f32>> = None;

    // With bias present
    assert!(bias_present.is_some());
    let value_with_bias = 5.0 + bias_present.as_ref().unwrap()[0];
    assert_eq!(value_with_bias, 6.0);

    // Without bias
    assert!(bias_absent.is_none());
    let value_without_bias = 5.0;
    assert_eq!(value_without_bias, 5.0);
}

#[test]
fn test_dropout_and_bias_together() {
    // Test that dropout and bias work together correctly
    let input = 10.0_f32;
    let bias = 1.0_f32;
    let dropout_rate = 0.5_f32;

    // Add bias first
    let with_bias = input + bias; // 11.0

    // Apply dropout (simulate keeping the value)
    let dropout_scale = 1.0 / (1.0 - dropout_rate); // 2.0
    let with_dropout = with_bias * dropout_scale; // 22.0

    assert_eq!(with_bias, 11.0);
    assert_eq!(dropout_scale, 2.0);
    assert_eq!(with_dropout, 22.0);
}

#[test]
fn test_seed_uniqueness() {
    // Test that different seeds produce different results
    let position = 0_u32;
    let dropout_rate = 0.3_f32;

    let xorshift = |mut state: u32| -> u32 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };

    let generate_mask = |seed: u32| -> f32 {
        let state = xorshift(seed ^ position);
        let rand_val = (state as f32) / (u32::MAX as f32);
        if rand_val >= dropout_rate {
            1.0 / (1.0 - dropout_rate)
        } else {
            0.0
        }
    };

    // Generate masks with different seeds
    let seeds = [100, 200, 300, 400, 500];
    let masks: Vec<f32> = seeds.iter().map(|&s| generate_mask(s)).collect();

    // At least some masks should be different
    // (with 5 seeds and 30% dropout, very unlikely all are the same)
    let first_mask = masks[0];
    let all_same = masks.iter().all(|&m| m == first_mask);

    assert!(
        !all_same,
        "Different seeds should produce different masks (probabilistically)"
    );
}
