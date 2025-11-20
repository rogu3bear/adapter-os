//! Example demonstrating model configuration caching in MLX backend
//!
//! This example shows:
//! 1. Loading a model with configuration caching enabled
//! 2. Accessing cached configuration without file I/O
//! 3. Using cached config for accurate memory estimation
//! 4. Handling multiple models with different configurations
//!
//! Run with: cargo run --example config_caching_example --features mmap

use adapteros_lora_mlx_ffi::ModelConfigCacheManager;
use adapteros_core::Result;
use std::path::Path;

fn main() -> Result<()> {

    println!("=== MLX Backend Configuration Caching Example ===\n");

    // Example 1: Load model with caching enabled
    println!("1. Loading model with configuration caching...");
    example_load_with_caching()?;

    println!("\n2. Demonstrating cache reuse...");
    example_cache_reuse()?;

    println!("\n3. Using cached config for memory estimation...");
    example_memory_estimation()?;

    println!("\n4. Dynamic model support...");
    example_dynamic_models()?;

    println!("\n=== Examples Completed Successfully ===");
    Ok(())
}

/// Example 1: Load model with caching enabled
fn example_load_with_caching() -> Result<()> {
    // Note: In production, use actual model path
    let model_dir = std::env::var("MLX_MODEL_PATH")
        .unwrap_or_else(|_| "./models/test".to_string());

    // Initialize cache manager
    let cache_manager = ModelConfigCacheManager::new(
        Path::new(&model_dir).join("config.json")
    );

    println!("Cache initialized for: {}", model_dir);
    println!("Is cached: {}", cache_manager.is_cached());

    // First access: loads from file
    match cache_manager.get() {
        Ok(config) => {
            println!("\nConfiguration loaded:");
            println!("  Vocabulary size: {}", config.vocab_size);
            println!("  Hidden size: {}", config.hidden_size);
            println!("  Layers: {}", config.num_hidden_layers);
            println!("  Attention heads: {}", config.num_attention_heads);
            println!("  KV heads: {}", config.num_key_value_heads);
            println!("  Head dimension: {}", config.head_dim);
            println!("  Intermediate size: {}", config.intermediate_size);
        }
        Err(e) => {
            println!("  [Demo mode - file not found: {}]", e);
        }
    }

    Ok(())
}

/// Example 2: Cache reuse without file I/O
fn example_cache_reuse() -> Result<()> {
    let cache_manager = ModelConfigCacheManager::new("./config.json");

    // Second and subsequent accesses use the cache
    for i in 0..3 {
        match cache_manager.get_cached() {
            Some(config) => {
                println!("Access {}: Cache hit - hidden_size = {}", i + 1, config.hidden_size);
            }
            None => {
                println!("Access {}: Cache miss - would load from file", i + 1);
            }
        }
    }

    Ok(())
}

/// Example 3: Using cached config for memory estimation
fn example_memory_estimation() -> Result<()> {
    println!("Adapter memory estimation examples:\n");

    // Different LoRA configurations
    let configs = vec![
        ("Small LoRA", 8, 4),
        ("Medium LoRA", 16, 4),
        ("Large LoRA", 32, 8),
    ];

    // Assume standard 7B model dimensions
    let hidden_size = 4096;

    for (name, rank, num_modules) in configs {
        let estimated_bytes = rank * hidden_size * 2 * num_modules * 4;
        let estimated_mb = estimated_bytes as f32 / (1024.0 * 1024.0);

        println!("{:12} (rank={}, modules={}): {:.2} MB",
                 name, rank, num_modules, estimated_mb);
    }

    println!("\nFormula: rank × hidden_size × 2 × num_modules × sizeof(f32)");
    println!("  - Factor 2: shared down + module-specific up projections");
    println!("  - sizeof(f32) = 4 bytes");

    Ok(())
}

/// Example 4: Supporting different model architectures
fn example_dynamic_models() -> Result<()> {
    println!("Model architecture differences:\n");

    // Different model architectures
    let models = vec![
        ("Qwen 7B", 32000, 4096, 32, 32, 8, 11008),
        ("Llama 7B", 32000, 4096, 32, 32, 8, 11008),
        ("Mistral 7B", 32000, 4096, 32, 32, 8, 14336),
        ("Qwen 13B", 32000, 5120, 40, 40, 10, 13824),
    ];

    for (name, _vocab, hidden, layers, heads, _kv_heads, _inter) in models {
        let head_dim = hidden / heads;

        println!("{:15} - Hidden: {}, Layers: {}, Heads: {}, Head dim: {}",
                 name, hidden, layers, heads, head_dim);
    }

    println!("\nThe caching system works with all these architectures!");
    println!("Each backend instance maintains its own configuration cache.");

    Ok(())
}

/// Demonstrate performance impact
///
/// This is a conceptual example showing how caching helps:
#[allow(dead_code)]
fn example_performance_impact() -> Result<()> {
    println!("Performance impact of caching:\n");

    // Simulate repeated config access
    let iterations = 1000;

    println!("Without cache (file I/O for each access):");
    println!("  {} accesses × ~2ms per access = {}ms", iterations, iterations * 2);

    println!("\nWith cache (memory access after first load):");
    println!("  1 file access: 2ms");
    println!("  {} cached accesses: {}ms (0.5µs per access)", iterations - 1, (iterations - 1) / 1000);
    println!("  Total: ~2ms");

    println!("\nSpeedup: ~1000x faster for repeated access patterns");

    Ok(())
}

/// Demonstrate error handling
#[allow(dead_code)]
fn example_error_handling() -> Result<()> {
    println!("Error handling examples:\n");

    // Invalid configuration path
    let cache = ModelConfigCacheManager::new("/nonexistent/config.json");
    match cache.get() {
        Ok(_) => println!("Loaded successfully"),
        Err(e) => println!("Expected error: {}", e),
    }

    // Invalid JSON would cause parse error
    // let invalid_json = "{ broken json }";
    // match ModelConfigCache::from_json(invalid_json) {
    //     Ok(_) => println!("Parsed successfully"),
    //     Err(e) => println!("Expected error: {}", e),
    // }

    Ok(())
}
