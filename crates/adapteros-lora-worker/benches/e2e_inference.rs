//! End-to-end inference performance benchmarks
//!
//! Benchmarks for complete inference pipelines including cold start,
//! warm inference, and single adapter vs stack comparisons.
//!
//! ## Running Benchmarks
//!
//! Full suite:
//! ```bash
//! cargo bench -p adapteros-lora-worker --bench e2e_inference
//! ```
//!
//! Specific benchmark:
//! ```bash
//! cargo bench -p adapteros-lora-worker --bench e2e_inference -- cold_start
//! cargo bench -p adapteros-lora-worker --bench e2e_inference -- warm_inference
//! cargo bench -p adapteros-lora-worker --bench e2e_inference -- single_vs_stack
//! ```
//!
//! [source: crates/adapteros-lora-worker/benches/e2e_inference.rs]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

// Mock types for inference simulation
#[derive(Clone)]
struct Adapter {
    id: String,
    rank: u16,
    loaded: bool,
    cache_warm: bool,
}

impl Adapter {
    fn new(id: impl Into<String>, rank: u16) -> Self {
        Self {
            id: id.into(),
            rank,
            loaded: false,
            cache_warm: false,
        }
    }

    fn load(&mut self) {
        self.loaded = true;
        self.cache_warm = false;
    }

    fn warm_cache(&mut self) {
        self.cache_warm = true;
    }

    fn run_inference(&self, tokens: &[u32]) -> Vec<f32> {
        // Simulate inference computation
        let output_size = tokens.len() * 4096; // hidden_dim = 4096
        vec![0.5; output_size]
    }
}

struct AdapterStack {
    adapters: Vec<Adapter>,
}

impl AdapterStack {
    fn new(adapters: Vec<Adapter>) -> Self {
        Self { adapters }
    }

    fn run_inference(&self, tokens: &[u32]) -> Vec<f32> {
        // Simulate K-sparse routing + fusion
        let mut outputs: Vec<Vec<f32>> = self
            .adapters
            .iter()
            .map(|adapter| adapter.run_inference(tokens))
            .collect();

        // Fuse outputs (weighted average simulation)
        let output_size = tokens.len() * 4096;
        let mut fused = vec![0.0; output_size];
        let weight = 1.0 / self.adapters.len() as f32;
        for output in &outputs {
            for (i, &val) in output.iter().enumerate() {
                fused[i] += val * weight;
            }
        }
        fused
    }
}

// =============================================================================
// Cold Start Inference Benchmarks
// =============================================================================

/// Benchmark cold start inference (first inference after adapter load)
///
/// Includes:
/// - Adapter loading
/// - Cache initialization
/// - First forward pass
fn bench_cold_start_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold_start_inference");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(10));

    let prompt_lengths = [32, 128, 512, 1024];
    let rank = 16;

    for prompt_len in prompt_lengths {
        group.throughput(Throughput::Elements(prompt_len as u64));

        group.bench_with_input(
            BenchmarkId::new("tokens", prompt_len),
            &prompt_len,
            |b, &prompt_len| {
                b.iter(|| {
                    // Cold start: create and load adapter
                    let mut adapter = Adapter::new("code-review-adapter", rank);
                    adapter.load();

                    // First inference (cold cache)
                    let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                    let output = adapter.run_inference(&tokens);
                    black_box(output);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark cold start with different adapter ranks
///
/// Rank affects model size and loading time
fn bench_cold_start_by_rank(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold_start_by_rank");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(10));

    let ranks = [4, 8, 16, 32, 64];
    let prompt_len = 128;

    for rank in ranks {
        group.throughput(Throughput::Elements(prompt_len as u64));

        group.bench_with_input(BenchmarkId::new("rank", rank), &rank, |b, &rank| {
            b.iter(|| {
                let mut adapter = Adapter::new("test-adapter", rank);
                adapter.load();

                let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                let output = adapter.run_inference(&tokens);
                black_box(output);
            });
        });
    }

    group.finish();
}

/// Benchmark cold start for adapter stack
///
/// Multiple adapters loaded simultaneously
fn bench_cold_start_stack(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold_start_stack");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    let adapter_counts = [1, 2, 4, 8];
    let prompt_len = 128;
    let rank = 16;

    for count in adapter_counts {
        group.throughput(Throughput::Elements(prompt_len as u64 * count as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            b.iter(|| {
                // Create and load stack
                let mut adapters: Vec<Adapter> = (0..count)
                    .map(|i| {
                        let mut adapter = Adapter::new(format!("adapter-{}", i), rank);
                        adapter.load();
                        adapter
                    })
                    .collect();

                let stack = AdapterStack::new(adapters);

                // First inference
                let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                let output = stack.run_inference(&tokens);
                black_box(output);
            });
        });
    }

    group.finish();
}

// =============================================================================
// Warm Inference Benchmarks
// =============================================================================

/// Benchmark warm inference (subsequent inferences with cached state)
///
/// Cache is already initialized, measuring pure inference time
fn bench_warm_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("warm_inference");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let prompt_lengths = [32, 128, 512, 1024, 2048];
    let rank = 16;

    for prompt_len in prompt_lengths {
        group.throughput(Throughput::Elements(prompt_len as u64));

        group.bench_with_input(
            BenchmarkId::new("tokens", prompt_len),
            &prompt_len,
            |b, &prompt_len| {
                // Setup: pre-load and warm cache
                let mut adapter = Adapter::new("code-review-adapter", rank);
                adapter.load();
                adapter.warm_cache();

                b.iter(|| {
                    let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                    let output = adapter.run_inference(&tokens);
                    black_box(output);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark warm inference with batching
///
/// Multiple inference requests in sequence (all warm)
fn bench_warm_inference_batched(c: &mut Criterion) {
    let mut group = c.benchmark_group("warm_inference_batched");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let batch_sizes = [1, 4, 8, 16];
    let prompt_len = 128;
    let rank = 16;

    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(prompt_len as u64 * batch_size as u64));

        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            &batch_size,
            |b, &batch_size| {
                // Setup
                let mut adapter = Adapter::new("code-review-adapter", rank);
                adapter.load();
                adapter.warm_cache();

                b.iter(|| {
                    for _ in 0..batch_size {
                        let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                        let output = adapter.run_inference(&tokens);
                        black_box(output);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark warm inference for adapter stack
///
/// K-sparse routing + fusion with warm cache
fn bench_warm_inference_stack(c: &mut Criterion) {
    let mut group = c.benchmark_group("warm_inference_stack");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let adapter_counts = [1, 2, 4, 8];
    let prompt_len = 128;
    let rank = 16;

    for count in adapter_counts {
        group.throughput(Throughput::Elements(prompt_len as u64));

        group.bench_with_input(BenchmarkId::new("adapters", count), &count, |b, &count| {
            // Setup: pre-load and warm
            let adapters: Vec<Adapter> = (0..count)
                .map(|i| {
                    let mut adapter = Adapter::new(format!("adapter-{}", i), rank);
                    adapter.load();
                    adapter.warm_cache();
                    adapter
                })
                .collect();

            let stack = AdapterStack::new(adapters);

            b.iter(|| {
                let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                let output = stack.run_inference(&tokens);
                black_box(output);
            });
        });
    }

    group.finish();
}

// =============================================================================
// Single Adapter vs Stack Comparison
// =============================================================================

/// Benchmark single adapter inference
///
/// Baseline: single LoRA adapter inference
fn bench_single_adapter(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_adapter");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let prompt_lengths = [32, 128, 512];
    let rank = 16;

    for prompt_len in prompt_lengths {
        group.throughput(Throughput::Elements(prompt_len as u64));

        group.bench_with_input(
            BenchmarkId::new("tokens", prompt_len),
            &prompt_len,
            |b, &prompt_len| {
                let mut adapter = Adapter::new("single-adapter", rank);
                adapter.load();
                adapter.warm_cache();

                b.iter(|| {
                    let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                    let output = adapter.run_inference(&tokens);
                    black_box(output);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark stack of 4 adapters
///
/// K=4 sparse routing with fusion
fn bench_stack_of_4(c: &mut Criterion) {
    let mut group = c.benchmark_group("stack_of_4");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let prompt_lengths = [32, 128, 512];
    let rank = 16;
    let adapter_count = 4;

    for prompt_len in prompt_lengths {
        group.throughput(Throughput::Elements(prompt_len as u64));

        group.bench_with_input(
            BenchmarkId::new("tokens", prompt_len),
            &prompt_len,
            |b, &prompt_len| {
                let adapters: Vec<Adapter> = (0..adapter_count)
                    .map(|i| {
                        let mut adapter = Adapter::new(format!("adapter-{}", i), rank);
                        adapter.load();
                        adapter.warm_cache();
                        adapter
                    })
                    .collect();

                let stack = AdapterStack::new(adapters);

                b.iter(|| {
                    let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                    let output = stack.run_inference(&tokens);
                    black_box(output);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark overhead of K-sparse routing
///
/// Compares single adapter vs stacks of 2, 4, 8 adapters
fn bench_routing_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("routing_overhead");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    let k_values = [1, 2, 4, 8];
    let prompt_len = 128;
    let rank = 16;

    for k in k_values {
        group.throughput(Throughput::Elements(prompt_len as u64));

        group.bench_with_input(BenchmarkId::new("k", k), &k, |b, &k| {
            let adapters: Vec<Adapter> = (0..k)
                .map(|i| {
                    let mut adapter = Adapter::new(format!("adapter-{}", i), rank);
                    adapter.load();
                    adapter.warm_cache();
                    adapter
                })
                .collect();

            let stack = AdapterStack::new(adapters);

            b.iter(|| {
                let tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();
                let output = stack.run_inference(&tokens);
                black_box(output);
            });
        });
    }

    group.finish();
}

// =============================================================================
// Token Generation Benchmarks
// =============================================================================

/// Benchmark autoregressive token generation
///
/// Simulates generating N tokens sequentially
fn bench_token_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("token_generation");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(10));

    let generation_lengths = [16, 64, 128, 256];
    let prompt_len = 32;
    let rank = 16;

    for gen_len in generation_lengths {
        group.throughput(Throughput::Elements(gen_len as u64));

        group.bench_with_input(
            BenchmarkId::new("tokens", gen_len),
            &gen_len,
            |b, &gen_len| {
                let mut adapter = Adapter::new("text-gen-adapter", rank);
                adapter.load();
                adapter.warm_cache();

                b.iter(|| {
                    // Initial prompt
                    let mut tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();

                    // Generate tokens autoregressively
                    for _ in 0..gen_len {
                        let output = adapter.run_inference(&tokens);
                        // Simulate sampling next token (take last logit)
                        let next_token = (output.len() % 32000) as u32;
                        tokens.push(next_token);
                    }

                    black_box(tokens);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark token generation with stack
///
/// Autoregressive generation using adapter stack
fn bench_token_generation_stack(c: &mut Criterion) {
    let mut group = c.benchmark_group("token_generation_stack");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    let generation_lengths = [16, 64, 128];
    let prompt_len = 32;
    let adapter_count = 4;
    let rank = 16;

    for gen_len in generation_lengths {
        group.throughput(Throughput::Elements(gen_len as u64));

        group.bench_with_input(
            BenchmarkId::new("tokens", gen_len),
            &gen_len,
            |b, &gen_len| {
                let adapters: Vec<Adapter> = (0..adapter_count)
                    .map(|i| {
                        let mut adapter = Adapter::new(format!("adapter-{}", i), rank);
                        adapter.load();
                        adapter.warm_cache();
                        adapter
                    })
                    .collect();

                let stack = AdapterStack::new(adapters);

                b.iter(|| {
                    let mut tokens: Vec<u32> = (0..prompt_len).map(|i| i as u32).collect();

                    for _ in 0..gen_len {
                        let output = stack.run_inference(&tokens);
                        let next_token = (output.len() % 32000) as u32;
                        tokens.push(next_token);
                    }

                    black_box(tokens);
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Register Benchmark Groups
// =============================================================================

criterion_group!(
    name = cold_start_benchmarks;
    config = Criterion::default();
    targets = bench_cold_start_inference, bench_cold_start_by_rank, bench_cold_start_stack
);

criterion_group!(
    name = warm_benchmarks;
    config = Criterion::default();
    targets = bench_warm_inference, bench_warm_inference_batched, bench_warm_inference_stack
);

criterion_group!(
    name = comparison_benchmarks;
    config = Criterion::default();
    targets = bench_single_adapter, bench_stack_of_4, bench_routing_overhead
);

criterion_group!(
    name = generation_benchmarks;
    config = Criterion::default();
    targets = bench_token_generation, bench_token_generation_stack
);

criterion_main!(
    cold_start_benchmarks,
    warm_benchmarks,
    comparison_benchmarks,
    generation_benchmarks
);
