//! Model Server Performance Benchmarks
//!
//! Measures key performance characteristics of the Model Server architecture:
//! - Forward pass latency (simulated)
//! - KV cache operations (hit/miss, eviction)
//! - Adapter activation tracking overhead
//! - Hot adapter lookup performance
//!
//! Run with: cargo bench -p adapteros-model-server

use adapteros_model_server::{
    activation_tracker::ActivationTracker, adapter_cache::AdapterCache, kv_cache::KvCacheManager,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Benchmark KV cache get_or_create operations
fn bench_kv_cache_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("kv_cache");
    group.measurement_time(Duration::from_secs(5));

    // Parameters: max_bytes, hidden_size, num_layers
    let hidden_size = 4096;
    let num_layers = 32;

    for &max_sessions_mb in &[256u64, 1024u64, 4096u64] {
        let max_bytes = max_sessions_mb * 1024 * 1024;
        let cache = KvCacheManager::new(max_bytes, hidden_size, num_layers);

        // Benchmark cache hit (same session)
        group.bench_with_input(
            BenchmarkId::new("get_or_create_hit", format!("{}mb", max_sessions_mb)),
            &max_sessions_mb,
            |b, _| {
                // Pre-populate cache
                let session_id = "bench-session-hit".to_string();
                cache.get_or_create(&session_id, 2048);

                b.iter(|| {
                    let entry = cache.get_or_create(&session_id, 2048);
                    black_box(entry);
                });
            },
        );

        // Benchmark cache miss (new sessions)
        group.bench_with_input(
            BenchmarkId::new("get_or_create_miss", format!("{}mb", max_sessions_mb)),
            &max_sessions_mb,
            |b, _| {
                let mut counter = 0u64;
                b.iter(|| {
                    counter += 1;
                    let session_id = format!("bench-session-miss-{}", counter);
                    let entry = cache.get_or_create(&session_id, 2048);
                    black_box(entry);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark activation tracker for hot adapter promotion
fn bench_activation_tracker(c: &mut Criterion) {
    let mut group = c.benchmark_group("activation_tracker");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark recording activations
    for &num_adapters in &[8, 32, 128] {
        group.throughput(Throughput::Elements(num_adapters as u64));

        group.bench_with_input(
            BenchmarkId::new("record_request", num_adapters),
            &num_adapters,
            |b, &n| {
                let tracker = ActivationTracker::new(0.10);

                // Register adapters
                for i in 0..n {
                    tracker.register_adapter(i as u32, format!("adapter-{}", i));
                }

                // Create adapter IDs for request
                let adapter_ids: Vec<u32> = (0..4).map(|i| (i % n) as u32).collect();

                b.iter(|| {
                    tracker.record_request(&adapter_ids);
                });
            },
        );

        // Benchmark hot adapter detection
        group.bench_with_input(
            BenchmarkId::new("get_hot_adapters", num_adapters),
            &num_adapters,
            |b, &n| {
                let tracker = ActivationTracker::new(0.10);

                // Register adapters
                for i in 0..n {
                    tracker.register_adapter(i as u32, format!("adapter-{}", i));
                }

                // Make adapter 0 hot (many requests)
                for _ in 0..1000 {
                    tracker.record_request(&[0]);
                }
                // Other adapters get fewer requests
                for i in 1..n {
                    tracker.record_request(&[i as u32]);
                }

                b.iter(|| {
                    let hot = tracker.hot_adapters();
                    black_box(hot);
                });
            },
        );

        // Benchmark activation rate calculation
        group.bench_with_input(
            BenchmarkId::new("activation_rate", num_adapters),
            &num_adapters,
            |b, &n| {
                let tracker = ActivationTracker::new(0.10);

                // Register and populate
                for i in 0..n {
                    tracker.register_adapter(i as u32, format!("adapter-{}", i));
                    for _ in 0..(i * 10) {
                        tracker.record_request(&[i as u32]);
                    }
                }

                b.iter(|| {
                    for i in 0..n {
                        let rate = tracker.activation_rate(i as u32);
                        black_box(rate);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark adapter cache operations
fn bench_adapter_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("adapter_cache");
    group.measurement_time(Duration::from_secs(5));

    // Create dummy adapter weights (simulating LoRA A and B matrices)
    // Typical: rank=8, hidden=4096 -> lora_a = 8*4096 = 32K floats
    fn create_lora_weights(rank: usize, hidden: usize) -> (Vec<f32>, Vec<f32>) {
        let lora_a = vec![0.01f32; rank * hidden];
        let lora_b = vec![0.01f32; hidden * rank];
        (lora_a, lora_b)
    }

    for &max_adapters in &[16, 64, 256] {
        let cache = AdapterCache::new(max_adapters, None);

        // Benchmark loading adapters
        group.bench_with_input(
            BenchmarkId::new("load_adapter", max_adapters),
            &max_adapters,
            |b, _| {
                let (lora_a, lora_b) = create_lora_weights(8, 4096);
                let mut id = 0u32;

                b.iter(|| {
                    id += 1;
                    let _ = cache.load(
                        id,
                        format!("adapter-{}", id),
                        lora_a.clone(),
                        lora_b.clone(),
                        1.0,
                    );
                });
            },
        );

        // Benchmark cache hit (lookup)
        group.bench_with_input(
            BenchmarkId::new("get_adapter_hit", max_adapters),
            &max_adapters,
            |b, _| {
                // Pre-load adapter
                let (lora_a, lora_b) = create_lora_weights(8, 4096);
                let _ = cache.load(9999, "hot-adapter".to_string(), lora_a, lora_b, 1.0);

                b.iter(|| {
                    let entry = cache.get(9999);
                    black_box(entry);
                });
            },
        );

        // Benchmark cache miss
        group.bench_with_input(
            BenchmarkId::new("get_adapter_miss", max_adapters),
            &max_adapters,
            |b, _| {
                b.iter(|| {
                    let entry = cache.get(88888);
                    black_box(entry);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark simulated forward pass latency components
fn bench_forward_pass_components(c: &mut Criterion) {
    let mut group = c.benchmark_group("forward_pass");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark logit allocation for different vocab sizes
    for &vocab_size in &[32_000usize, 128_000usize] {
        group.throughput(Throughput::Elements(vocab_size as u64));

        group.bench_with_input(
            BenchmarkId::new("logit_allocation", vocab_size),
            &vocab_size,
            |b, &v| {
                b.iter(|| {
                    let logits = vec![0.0f32; v];
                    black_box(logits);
                });
            },
        );

        // Benchmark logit copy (simulating response serialization)
        group.bench_with_input(
            BenchmarkId::new("logit_copy", vocab_size),
            &vocab_size,
            |b, &v| {
                let src = vec![0.0f32; v];
                let mut dst = vec![0.0f32; v];

                b.iter(|| {
                    dst.copy_from_slice(&src);
                    black_box(&dst);
                });
            },
        );
    }

    // Benchmark adapter gate application (Q15 scaling)
    for &num_adapters in &[4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("gate_scaling", num_adapters),
            &num_adapters,
            |b, &n| {
                let gates_q15: Vec<i16> = (0..n).map(|i| (i * 1000) as i16).collect();

                b.iter(|| {
                    let mut sum = 0.0f32;
                    for &gate in &gates_q15 {
                        let gate_f32 = gate as f32 / 32767.0;
                        sum += gate_f32;
                    }
                    black_box(sum);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent access patterns
fn bench_concurrent_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent");
    group.measurement_time(Duration::from_secs(10));

    let hidden_size = 4096;
    let num_layers = 32;

    for &num_threads in &[2, 4, 8] {
        // Benchmark concurrent KV cache access
        group.bench_with_input(
            BenchmarkId::new("kv_cache_concurrent", num_threads),
            &num_threads,
            |b, &n| {
                let cache = Arc::new(KvCacheManager::new(
                    4 * 1024 * 1024 * 1024, // 4GB
                    hidden_size,
                    num_layers,
                ));

                b.iter(|| {
                    let handles: Vec<_> = (0..n)
                        .map(|t| {
                            let cache = Arc::clone(&cache);
                            thread::spawn(move || {
                                for i in 0..100 {
                                    let session_id = format!("thread-{}-session-{}", t, i);
                                    cache.get_or_create(&session_id, 2048);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );

        // Benchmark concurrent activation tracking
        group.bench_with_input(
            BenchmarkId::new("activation_concurrent", num_threads),
            &num_threads,
            |b, &n| {
                let tracker = Arc::new(ActivationTracker::new(0.10));

                // Pre-register adapters
                for i in 0..100 {
                    tracker.register_adapter(i, format!("adapter-{}", i));
                }

                b.iter(|| {
                    let handles: Vec<_> = (0..n)
                        .map(|t| {
                            let tracker = Arc::clone(&tracker);
                            thread::spawn(move || {
                                for i in 0..1000 {
                                    let adapter_id = ((t * 10 + i) % 100) as u32;
                                    tracker.record_request(&[adapter_id]);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory overhead estimation
fn bench_memory_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_overhead");

    let hidden_size = 4096;
    let num_layers = 32;

    // Measure per-session memory overhead
    group.bench_function("kv_cache_entry_size", |b| {
        let cache = KvCacheManager::new(16 * 1024 * 1024 * 1024, hidden_size, num_layers);

        b.iter(|| {
            // Create entries and measure
            for i in 0..100 {
                let session_id = format!("mem-test-{}", i);
                cache.get_or_create(&session_id, 2048);
            }
        });
    });

    // Measure tracker memory overhead
    group.bench_function("tracker_entry_size", |b| {
        let tracker = ActivationTracker::new(0.10);

        b.iter(|| {
            for i in 0..1000 {
                tracker.register_adapter(i, format!("adapter-{}", i));
            }
        });
    });

    group.finish();
}

/// Benchmark IPC serialization overhead (simulated)
fn bench_ipc_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("ipc_serialization");
    group.measurement_time(Duration::from_secs(5));

    // Simulate protobuf-like serialization overhead for forward responses
    for &vocab_size in &[32_000usize, 128_000usize] {
        group.throughput(Throughput::Bytes((vocab_size * 4) as u64)); // f32 = 4 bytes

        // Benchmark creating response struct
        group.bench_with_input(
            BenchmarkId::new("create_response", vocab_size),
            &vocab_size,
            |b, &v| {
                b.iter(|| {
                    // Simulate ForwardResponse creation
                    let logits = vec![0.0f32; v];
                    let hidden_states: Option<Vec<f32>> = None;
                    let position = 42u32;
                    let kv_cache_hit = true;
                    let cached_tokens = 128u32;
                    let forward_latency_ms = 15.5f32;

                    black_box((
                        logits,
                        hidden_states,
                        position,
                        kv_cache_hit,
                        cached_tokens,
                        forward_latency_ms,
                    ));
                });
            },
        );

        // Benchmark Vec<f32> -> Vec<u8> conversion (wire format)
        group.bench_with_input(
            BenchmarkId::new("logits_to_bytes", vocab_size),
            &vocab_size,
            |b, &v| {
                let logits = vec![0.0f32; v];

                b.iter(|| {
                    let bytes: Vec<u8> = logits.iter().flat_map(|f| f.to_le_bytes()).collect();
                    black_box(bytes);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_kv_cache_operations,
    bench_activation_tracker,
    bench_adapter_cache,
    bench_forward_pass_components,
    bench_concurrent_access,
    bench_memory_overhead,
    bench_ipc_serialization,
);
criterion_main!(benches);
