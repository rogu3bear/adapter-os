//! MLX FFI Integration Benchmarks
//!
//! Measures performance of key MLX FFI operations including:
//! - Runtime initialization
//! - Forward pass with lazy evaluation
//! - KV cache generation
//! - Adapter cache hit/miss
//! - SafeTensors loading strategies

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::{Duration, Instant};

/// Benchmark MLX runtime initialization (should be fast after first call)
fn bench_runtime_init(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_init");
    group.sample_size(10);

    // First init (cold)
    group.bench_function("mlx_runtime_init_first", |b| {
        b.iter(|| {
            // Runtime init is idempotent, so this measures the check path
            let result = adapteros_lora_mlx_ffi::mlx_runtime_init();
            black_box(result)
        });
    });

    // Check if initialized (hot path)
    group.bench_function("mlx_runtime_is_initialized", |b| {
        b.iter(|| {
            let result = adapteros_lora_mlx_ffi::mlx_runtime_is_initialized();
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark adapter cache operations
fn bench_adapter_cache(c: &mut Criterion) {
    use adapteros_lora_mlx_ffi::{MLXAdapterCache, MLXAdapterCacheConfig};

    let mut group = c.benchmark_group("adapter_cache");

    let config = MLXAdapterCacheConfig {
        max_cached_adapters: 16,
        max_total_cache_bytes: 1024 * 1024 * 1024, // 1GB
        ..Default::default()
    };
    let cache = MLXAdapterCache::new(config);

    // Pre-populate cache
    for i in 0..10u16 {
        let weights = vec![i as u8; 1024 * 1024]; // 1MB each
        let _ = cache.cache_adapter(i, weights);
    }

    // Benchmark cache hit
    group.bench_function("cache_hit", |b| {
        b.iter(|| {
            let result = cache.get_cached(black_box(5));
            black_box(result)
        });
    });

    // Benchmark cache miss
    group.bench_function("cache_miss", |b| {
        b.iter(|| {
            let result = cache.get_cached(black_box(999));
            black_box(result)
        });
    });

    // Benchmark cache insert (with potential eviction)
    group.bench_function("cache_insert_1mb", |b| {
        let mut id = 100u16;
        b.iter(|| {
            let weights = vec![0u8; 1024 * 1024];
            let result = cache.cache_adapter(id, weights);
            id = id.wrapping_add(1);
            black_box(result)
        });
    });

    // Benchmark get_stats
    group.bench_function("get_stats", |b| {
        b.iter(|| {
            let stats = cache.get_stats();
            black_box(stats)
        });
    });

    group.finish();
}

/// Benchmark unified SafeTensors loader
fn bench_safetensors_loader(c: &mut Criterion) {
    use adapteros_lora_mlx_ffi::{LoadStrategy, UnifiedSafeTensorsLoader};
    use std::path::PathBuf;

    let mut group = c.benchmark_group("safetensors_loader");
    group.sample_size(10);

    // Find a test safetensors file if available
    let test_paths = [
        PathBuf::from("tests/fixtures/test_weights.safetensors"),
        PathBuf::from("../test_data/small_model.safetensors"),
    ];

    let test_file = test_paths.iter().find(|p| p.exists());

    if let Some(path) = test_file {
        // Benchmark Rust-only loading
        group.bench_function("load_rust_only", |b| {
            b.iter(|| {
                let loader =
                    UnifiedSafeTensorsLoader::load(black_box(path), LoadStrategy::RustOnly);
                black_box(loader)
            });
        });

        // Benchmark MLX-preferred loading
        group.bench_function("load_mlx_preferred", |b| {
            b.iter(|| {
                let loader =
                    UnifiedSafeTensorsLoader::load(black_box(path), LoadStrategy::MlxPreferred);
                black_box(loader)
            });
        });
    } else {
        println!("Skipping safetensors benchmarks: no test file found");
    }

    group.finish();
}

/// Benchmark KV cache operations
fn bench_kv_cache(c: &mut Criterion) {
    use adapteros_lora_mlx_ffi::kv_cache::{KVCacheConfig, MLXKVCache};

    let mut group = c.benchmark_group("kv_cache");

    let config = KVCacheConfig {
        num_layers: 32,
        max_seq_length: 4096,
        ..Default::default()
    };

    // Benchmark cache creation
    group.bench_function("create_cache_32_layers", |b| {
        b.iter(|| {
            let cache = MLXKVCache::new(black_box(config.clone()));
            black_box(cache)
        });
    });

    let cache = MLXKVCache::new(config);

    // Benchmark get_stats
    group.bench_function("get_stats", |b| {
        b.iter(|| {
            let stats = cache.get_stats();
            black_box(stats)
        });
    });

    // Benchmark clear
    group.bench_function("clear_cache", |b| {
        b.iter(|| {
            cache.clear_all();
        });
    });

    group.finish();
}

/// Benchmark memory operations
fn bench_memory_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_ops");
    group.sample_size(10);

    // Benchmark mlx_sync
    group.bench_function("mlx_sync", |b| {
        b.iter(|| {
            adapteros_lora_mlx_ffi::mlx_sync();
        });
    });

    group.finish();
}

/// Manual timing benchmark for operations that need custom measurement
fn bench_manual_timing() -> Vec<(&'static str, Duration)> {
    let mut results = Vec::new();

    // Runtime init timing
    let start = Instant::now();
    let _ = adapteros_lora_mlx_ffi::mlx_runtime_init();
    results.push(("runtime_init", start.elapsed()));

    // Adapter cache timing
    let config = adapteros_lora_mlx_ffi::MLXAdapterCacheConfig::default();
    let cache = adapteros_lora_mlx_ffi::MLXAdapterCache::new(config);

    // Cache insert 10MB
    let weights = vec![0u8; 10 * 1024 * 1024];
    let start = Instant::now();
    let _ = cache.cache_adapter(1, weights);
    results.push(("cache_insert_10mb", start.elapsed()));

    // Cache hit
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = cache.get_cached(1);
    }
    let elapsed = start.elapsed();
    results.push(("cache_hit_x1000", elapsed));
    results.push(("cache_hit_avg", elapsed / 1000));

    // Cache miss
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = cache.get_cached(999);
    }
    let elapsed = start.elapsed();
    results.push(("cache_miss_x1000", elapsed));
    results.push(("cache_miss_avg", elapsed / 1000));

    results
}

criterion_group!(
    benches,
    bench_runtime_init,
    bench_adapter_cache,
    bench_safetensors_loader,
    bench_kv_cache,
    bench_memory_ops,
);

criterion_main!(benches);

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn run_manual_benchmarks() {
        println!("\n=== Manual Benchmark Results ===\n");

        let results = bench_manual_timing();

        for (name, duration) in results {
            println!("{:30} {:>12.3?}", name, duration);
        }

        println!("\n=== End Manual Benchmarks ===\n");
    }
}
