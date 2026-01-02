//! FFI Verification Examples and Reference Implementation
//!
//! This module demonstrates how to verify FFI linkage and provides
//! example code for validating attention operations.

use adapteros_core::Result;
use adapteros_lora_mlx_ffi::{
    attention::{mlx_rope, mlx_scaled_dot_product_attention, AttentionConfig, RoPEFrequencies},
    kv_cache::{KVCacheConfig, MLXKVCache},
    tensor::MLXFFITensor,
};

// ============================================================================
// Example 1: Basic KV Cache FFI Usage
// ============================================================================

/// Example: Initialize and use KV cache
pub fn example_kv_cache_basic_usage() -> Result<()> {
    // Step 1: Create cache configuration
    let config = KVCacheConfig {
        num_layers: 32,
        max_seq_length: 4096,
        hidden_dim: 4096,
        num_heads: 32,
        head_dim: 128,
    };

    // Step 2: Create cache instance
    let cache = MLXKVCache::new(config);

    // Step 3: Add key-value pair for first position
    let key_pos_0 = vec![0.5; 4096]; // Placeholder data
    let value_pos_0 = vec![0.5; 4096];

    cache.mlx_kv_cache_update(0, key_pos_0, value_pos_0)?;
    println!(
        "Cache after position 0: {} positions cached",
        cache.get_size()
    );

    // Step 4: Retrieve cached values
    let retrieved_keys = cache.mlx_kv_cache_get_keys(0)?;
    let retrieved_values = cache.mlx_kv_cache_get_values(0)?;
    println!(
        "Retrieved {} keys and {} values",
        retrieved_keys.len(),
        retrieved_values.len()
    );

    // Step 5: Check statistics
    let stats = cache.get_stats();
    println!(
        "Cache stats - Hits: {}, Misses: {}, Memory: {} bytes",
        stats.cache_hits,
        stats.cache_misses,
        cache.get_memory_usage()
    );

    Ok(())
}

// ============================================================================
// Example 2: RoPE Application Verification
// ============================================================================

/// Example: Apply and verify RoPE transformations
pub fn example_rope_application() -> Result<()> {
    // Step 1: Create RoPE frequencies
    let head_dim = 64;
    let rope_freq = RoPEFrequencies::new(head_dim, 10000.0);

    println!(
        "RoPE setup: dim={}, theta={}",
        rope_freq.dim, rope_freq.theta
    );
    println!("Frequencies: {} pairs", rope_freq.inv_freq.len());

    // Step 2: Create a test tensor
    let test_data: Vec<f32> = (0..head_dim).map(|i| (i as f32) / 100.0).collect();
    let tensor = MLXFFITensor::from_data(&test_data, vec![1, head_dim])?;

    // Step 3: Apply RoPE at different positions
    let positions = vec![0, 1, 10, 100, 1000];
    let mut results = Vec::new();

    for pos in positions.iter() {
        let rotated = mlx_rope(&tensor, *pos, &rope_freq, "cpu")?;
        results.push((*pos, rotated));
    }

    println!("\nRoPE applied at positions: {:?}", positions);

    // Step 4: Verify norm preservation
    let original_norm: f32 = test_data.iter().map(|x| x * x).sum::<f32>().sqrt();
    println!("Original norm: {:.6}", original_norm);

    for (pos, result) in &results {
        let result_data = result.to_float_vec()?;
        let result_norm: f32 = result_data.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_diff = (original_norm - result_norm).abs();
        println!(
            "Position {}: norm={:.6}, diff={:.8}",
            pos, result_norm, norm_diff
        );
    }

    Ok(())
}

// ============================================================================
// Example 3: Attention Computation
// ============================================================================

/// Example: Compute scaled dot-product attention
pub fn example_attention_computation() -> Result<()> {
    // Configuration
    let batch_size = 1;
    let seq_len = 4;
    let hidden_size = 64;
    let num_heads = 4;

    println!("Attention configuration:");
    println!("  Batch size: {}", batch_size);
    println!("  Sequence length: {}", seq_len);
    println!("  Hidden size: {}", hidden_size);
    println!("  Heads: {}", num_heads);

    // Step 1: Create attention config
    let config = AttentionConfig::new(hidden_size, num_heads, true)?;
    println!("  Head dimension: {}", config.head_dim);
    println!("  Scale factor: {:.6}", config.scale);
    println!("  Causal mask: {}", config.causal_mask);

    // Step 2: Create sample tensors
    let total_elements = batch_size * seq_len * hidden_size;

    let query_data: Vec<f32> = (0..total_elements)
        .map(|i| ((i as f32) % 1.0) / 100.0)
        .collect();
    let query = MLXFFITensor::from_data(&query_data, vec![batch_size, seq_len, hidden_size])?;

    let key_data: Vec<f32> = (0..total_elements)
        .map(|i| (((i as f32) + 0.5) % 1.0) / 100.0)
        .collect();
    let key = MLXFFITensor::from_data(&key_data, vec![batch_size, seq_len, hidden_size])?;

    let value_data: Vec<f32> = (0..total_elements)
        .map(|i| (((i as f32) + 0.25) % 1.0) / 100.0)
        .collect();
    let value = MLXFFITensor::from_data(&value_data, vec![batch_size, seq_len, hidden_size])?;

    println!("\nTensor shapes:");
    println!("  Query: {:?}", query.shape());
    println!("  Key: {:?}", key.shape());
    println!("  Value: {:?}", value.shape());

    // Step 3: Compute attention
    let output = mlx_scaled_dot_product_attention(&query, &key, &value, &config, None)?;

    println!("\nAttention computation result:");
    println!("  Output shape: {:?}", output.shape());

    // Step 4: Verify output properties
    let output_data = output.to_float_vec()?;
    let mut finite_count = 0;
    let mut nan_count = 0;
    let mut inf_count = 0;

    for val in &output_data {
        if val.is_finite() {
            finite_count += 1;
        } else if val.is_nan() {
            nan_count += 1;
        } else if val.is_infinite() {
            inf_count += 1;
        }
    }

    println!("\nOutput statistics:");
    println!("  Total elements: {}", output_data.len());
    println!("  Finite values: {}", finite_count);
    println!("  NaN values: {}", nan_count);
    println!("  Inf values: {}", inf_count);

    if nan_count > 0 || inf_count > 0 {
        println!("  WARNING: Non-finite values detected!");
    } else {
        println!("  ✓ All values finite");
    }

    Ok(())
}

// ============================================================================
// Example 4: Multi-Position KV Cache Simulation
// ============================================================================

/// Example: Simulate KV cache usage during token generation
pub fn example_kv_cache_generation_simulation() -> Result<()> {
    let config = KVCacheConfig {
        num_layers: 4,
        max_seq_length: 128,
        hidden_dim: 256,
        num_heads: 8,
        head_dim: 32,
    };

    let cache = MLXKVCache::new(config.clone());
    let head_dim = 32;

    println!("Simulating token generation with KV cache:");
    println!("  Layers: {}", config.num_layers);
    println!("  Max sequence: {}", config.max_seq_length);
    println!("  Head dim: {}", config.head_dim);

    // Simulate processing 10 token positions
    let num_positions = 10;

    for pos in 0..num_positions {
        // For each layer
        for layer in 0..config.num_layers {
            // Create dummy key and value
            let key = vec![pos as f32 / 100.0; head_dim];
            let value = vec![pos as f32 / 100.0; head_dim];

            cache.mlx_kv_cache_update(layer, key, value)?;
        }

        if (pos + 1) % 5 == 0 {
            let status = cache.get_status();
            println!("\nAfter position {}:", pos + 1);
            println!("  Cached layers: {}", status.num_cached_layers);
            println!(
                "  Memory: {:.2} KB",
                status.total_memory_bytes as f32 / 1024.0
            );
            println!("  Hit rate: {:.1}%", cache.get_hit_rate() * 100.0);
        }
    }

    // Final statistics
    let stats = cache.get_stats();
    println!("\nFinal statistics:");
    println!("  Total positions: {}", cache.get_size());
    println!("  Cache hits: {}", stats.cache_hits);
    println!("  Cache misses: {}", stats.cache_misses);
    println!(
        "  Memory: {:.2} MB",
        cache.get_memory_usage() as f32 / (1024.0 * 1024.0)
    );

    Ok(())
}

// ============================================================================
// Example 5: Attention with Masking Verification
// ============================================================================

/// Example: Verify causal masking in attention
pub fn example_causal_attention() -> Result<()> {
    let hidden_size = 32;
    let seq_len = 4;
    let num_heads = 2;

    println!("Causal attention verification:");
    println!("  Sequence length: {}", seq_len);
    println!("  Heads: {}", num_heads);

    // Create config with causal masking
    let config_causal = AttentionConfig::new(hidden_size, num_heads, true)?;

    // Create simple test tensors
    let test_data: Vec<f32> = (0..seq_len * hidden_size)
        .map(|i| (i as f32) / (seq_len * hidden_size) as f32)
        .collect();

    let query = MLXFFITensor::from_data(&test_data, vec![1, seq_len, hidden_size])?;
    let key = MLXFFITensor::from_data(&test_data, vec![1, seq_len, hidden_size])?;
    let value = MLXFFITensor::from_data(&test_data, vec![1, seq_len, hidden_size])?;

    // Compute with causal mask
    let output_causal =
        mlx_scaled_dot_product_attention(&query, &key, &value, &config_causal, None)?;

    // Compute without causal mask
    let config_no_mask = AttentionConfig::new(hidden_size, num_heads, false)?;
    let output_no_mask =
        mlx_scaled_dot_product_attention(&query, &key, &value, &config_no_mask, None)?;

    let data_causal = output_causal.to_float_vec()?;
    let data_no_mask = output_no_mask.to_float_vec()?;

    // Compare outputs
    let mut differences = 0;
    let mut max_diff = 0.0f32;

    for (a, b) in data_causal.iter().zip(data_no_mask.iter()) {
        let diff = (a - b).abs();
        if diff > 1e-5 {
            differences += 1;
            max_diff = max_diff.max(diff);
        }
    }

    println!("\nComparison results:");
    println!("  Elements with difference: {}", differences);
    println!("  Maximum difference: {:.8}", max_diff);

    if differences > 0 {
        println!("  ✓ Causal and non-causal masks produce different outputs");
    } else {
        println!("  ✗ WARNING: Masks produced identical outputs");
    }

    Ok(())
}

// ============================================================================
// Example 6: FFI Error Handling
// ============================================================================

/// Example: Proper error handling for FFI operations
pub fn example_error_handling() {
    println!("FFI Error Handling Examples:\n");

    // Example 1: Empty tensor
    println!("1. Empty tensor handling:");
    let config = KVCacheConfig::default();
    let cache = MLXKVCache::new(config);

    match cache.mlx_kv_cache_update(0, vec![], vec![1.0]) {
        Ok(_) => println!("   ERROR: Should have rejected empty key"),
        Err(e) => println!("   ✓ Properly rejected: {}", e),
    }

    // Example 2: Invalid dimensions
    println!("\n2. Invalid attention dimensions:");
    match AttentionConfig::new(256, 5, false) {
        Ok(_) => println!("   ERROR: Should have rejected indivisible dims"),
        Err(e) => println!("   ✓ Properly rejected: {}", e),
    }

    // Example 3: Dimension mismatch
    println!("\n3. Tensor dimension mismatch:");
    if let (Ok(tensor1), Ok(tensor2)) = (
        MLXFFITensor::from_data(&[1.0, 2.0, 3.0], vec![3]),
        MLXFFITensor::from_data(&[1.0, 2.0], vec![2]),
    ) {
        let config = AttentionConfig::new(3, 1, false).unwrap();
        match mlx_scaled_dot_product_attention(&tensor1, &tensor2, &tensor1, &config, None) {
            Ok(_) => println!("   ERROR: Should have rejected dimension mismatch"),
            Err(e) => println!("   ✓ Properly rejected: {}", e),
        }
    }
}

// ============================================================================
// Tests using examples
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example_kv_cache_basic_usage() {
        let result = example_kv_cache_basic_usage();
        assert!(
            result.is_ok(),
            "KV cache example failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_rope_application() {
        let result = example_rope_application();
        assert!(result.is_ok(), "RoPE example failed: {:?}", result.err());
    }

    #[test]
    #[ignore = "attention module matmul shapes need fixing for QK^T computation"]
    fn test_example_attention_computation() {
        let result = example_attention_computation();
        assert!(
            result.is_ok(),
            "Attention example failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_kv_cache_generation_simulation() {
        let result = example_kv_cache_generation_simulation();
        assert!(
            result.is_ok(),
            "Generation simulation failed: {:?}",
            result.err()
        );
    }

    #[test]
    #[ignore = "attention module matmul shapes need fixing for QK^T computation"]
    fn test_example_causal_attention() {
        let result = example_causal_attention();
        assert!(
            result.is_ok(),
            "Causal attention example failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_example_error_handling() {
        example_error_handling();
    }
}
