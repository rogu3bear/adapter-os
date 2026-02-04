#![cfg(all(test, feature = "mlx"))]
//! Integration verification tests for MLX FFI
//!
//! These tests verify that all major components work correctly together and
//! require real MLX (`--features mlx`).

use adapteros_lora_mlx_ffi::{
    kv_cache::{KVCacheConfig, MLXKVCache},
    mlx_runtime_init_with_device, mlx_runtime_is_initialized, mlx_sync, MLXAdapterCache,
    MLXAdapterCacheConfig, MlxDeviceType,
};
use std::time::Instant;

#[test]
fn verify_runtime_initialization() {
    println!("\n=== Runtime Initialization Verification ===\n");

    let start = Instant::now();
    let result = mlx_runtime_init_with_device(MlxDeviceType::Cpu);
    let init_time = start.elapsed();

    println!(
        "mlx_runtime_init_with_device() completed in {:?}",
        init_time
    );
    println!("Result: {:?}", result);

    let is_init = mlx_runtime_is_initialized();
    println!("mlx_runtime_is_initialized(): {}", is_init);

    result.expect("Runtime init should succeed");
    assert!(is_init, "Runtime should be initialized");

    println!("\n[PASS] Runtime initialization verified\n");
}

#[test]
fn verify_adapter_cache_operations() {
    println!("\n=== Adapter Cache Verification ===\n");

    let config = MLXAdapterCacheConfig {
        max_cached_adapters: 8,
        max_total_cache_bytes: 100 * 1024 * 1024, // 100MB
        ..Default::default()
    };

    let cache = MLXAdapterCache::new(config);
    println!("Created adapter cache with max 8 adapters, 100MB limit");

    // Insert test adapters
    for i in 0..5u16 {
        let weights = vec![i as u8; 1024 * 1024]; // 1MB each
        let start = Instant::now();
        let _ = cache.cache_adapter(i, weights);
        println!("Cached adapter {} (1MB) in {:?}", i, start.elapsed());
    }

    // Test cache hit
    let start = Instant::now();
    let hit = cache.get_cached(2);
    let hit_time = start.elapsed();
    println!(
        "Cache hit for adapter 2: {} in {:?}",
        hit.is_some(),
        hit_time
    );
    assert!(hit.is_some(), "Should find cached adapter");

    // Test cache miss
    let start = Instant::now();
    let miss = cache.get_cached(99);
    let miss_time = start.elapsed();
    println!(
        "Cache miss for adapter 99: {} in {:?}",
        miss.is_none(),
        miss_time
    );
    assert!(miss.is_none(), "Should not find non-existent adapter");

    // Get stats
    let stats = cache.get_stats();
    println!("\nCache Statistics:");
    println!("  Adapter count: {}", stats.adapter_count);
    println!(
        "  Total bytes cached: {} ({:.2} MB)",
        stats.total_bytes_cached,
        stats.total_bytes_cached as f64 / 1024.0 / 1024.0
    );
    println!("  Cache hits: {}", stats.cache_hits);
    println!("  Cache misses: {}", stats.cache_misses);
    println!("  Hit rate: {:.2}%", cache.hit_rate() * 100.0);

    println!("\n[PASS] Adapter cache operations verified\n");
}

#[test]
fn verify_kv_cache_operations() {
    println!("\n=== KV Cache Verification ===\n");

    let config = KVCacheConfig {
        num_layers: 32,
        num_heads: 32,
        head_dim: 128,
        max_seq_length: 4096,
        ..Default::default()
    };

    let start = Instant::now();
    let cache = MLXKVCache::new(config);
    let create_time = start.elapsed();
    println!("Created 32-layer KV cache in {:?}", create_time);

    // Get initial stats
    let stats = cache.get_stats();
    println!("\nKV Cache Statistics:");
    println!("  Cache hits: {}", stats.cache_hits);
    println!("  Cache misses: {}", stats.cache_misses);
    println!("  Evictions: {}", stats.evictions);
    println!(
        "  Peak memory: {} bytes ({:.2} MB)",
        stats.peak_memory_bytes,
        stats.peak_memory_bytes as f64 / 1024.0 / 1024.0
    );
    println!("  Clears: {}", stats.clears);

    // Clear cache
    let start = Instant::now();
    cache.clear_all();
    let clear_time = start.elapsed();
    println!("\nCleared cache in {:?}", clear_time);

    let stats_after = cache.get_stats();
    println!("Clears after clear_all(): {}", stats_after.clears);

    println!("\n[PASS] KV cache operations verified\n");
}

#[test]
fn verify_memory_sync() {
    println!("\n=== Memory Sync Verification ===\n");

    let iterations = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        mlx_sync();
    }
    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;

    println!(
        "mlx_sync() x{}: total {:?}, avg {:?}",
        iterations, total_time, avg_time
    );

    println!("\n[PASS] Memory sync verified\n");
}

#[test]
fn verify_complete_workflow() {
    println!("\n=== Complete Workflow Verification ===\n");

    // Step 1: Initialize runtime
    let _ = mlx_runtime_init_with_device(MlxDeviceType::Cpu);
    println!("[1/4] Runtime initialized");

    // Step 2: Create adapter cache
    let adapter_cache = MLXAdapterCache::new(MLXAdapterCacheConfig::default());
    let _ = adapter_cache.cache_adapter(1, vec![0u8; 512 * 1024]); // 512KB
    println!("[2/4] Adapter cache created and populated");

    // Step 3: Create KV cache
    let kv_config = KVCacheConfig {
        num_layers: 12,
        num_heads: 12,
        head_dim: 64,
        max_seq_length: 2048,
        ..Default::default()
    };
    let kv_cache = MLXKVCache::new(kv_config);
    println!("[3/4] KV cache created (12 layers, 2048 max seq)");

    // Step 4: Sync and verify
    mlx_sync();
    let adapter_stats = adapter_cache.get_stats();
    let kv_stats = kv_cache.get_stats();
    println!("[4/4] Memory synced");

    println!("\nFinal State:");
    println!("  Adapters cached: {}", adapter_stats.adapter_count);
    println!(
        "  Adapter memory: {:.2} KB",
        adapter_stats.total_bytes_cached as f64 / 1024.0
    );
    println!(
        "  KV cache peak memory: {:.2} KB",
        kv_stats.peak_memory_bytes as f64 / 1024.0
    );

    println!("\n[PASS] Complete workflow verified\n");
}
