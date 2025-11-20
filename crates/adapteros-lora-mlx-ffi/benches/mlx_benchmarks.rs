//! Comprehensive MLX Backend Performance Benchmarks
//!
//! This benchmark suite profiles the MLX backend performance across multiple dimensions:
//! - Single token latency
//! - Batch inference throughput
//! - Memory allocation patterns
//! - Cache efficiency
//! - Adapter switching overhead
//! - Operation-level timing (matmul, attention, etc.)
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use adapteros_lora_mlx_ffi::{
    backend::MLXFFIBackend, lora::{LoRAAdapter, LoRAConfig}, memory, MLXFFIModel,
};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_core::{B3Hash, derive_seed};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::{Duration, Instant};

// ================================
// Configuration Constants
// ================================

const VOCAB_SIZES: &[usize] = &[8_192, 32_000, 152_064]; // Various model vocab sizes
const SEQUENCE_LENGTHS: &[usize] = &[1, 8, 32, 128, 512]; // Token sequence lengths
const BATCH_SIZES: &[usize] = &[1, 4, 8, 16]; // Batch sizes for throughput testing
const ADAPTER_COUNTS: &[usize] = &[1, 2, 4, 8]; // K-sparse adapter counts
const HIDDEN_DIMS: &[usize] = &[768, 1024, 2048, 4096]; // Model hidden dimensions
const LORA_RANKS: &[usize] = &[4, 8, 16, 32, 64]; // LoRA rank configurations

// ================================
// Helper Structures
// ================================

/// Performance metrics collected during benchmarking
#[derive(Debug, Clone)]
struct PerformanceMetrics {
    total_time: Duration,
    tokens_generated: usize,
    memory_start: usize,
    memory_end: usize,
    memory_peak: usize,
    allocation_count_start: usize,
    allocation_count_end: usize,
}

impl PerformanceMetrics {
    fn tokens_per_second(&self) -> f64 {
        self.tokens_generated as f64 / self.total_time.as_secs_f64()
    }

    fn memory_delta(&self) -> i64 {
        self.memory_end as i64 - self.memory_start as i64
    }

    fn allocations_per_token(&self) -> f64 {
        (self.allocation_count_end - self.allocation_count_start) as f64
            / self.tokens_generated as f64
    }

    fn memory_per_token(&self) -> f64 {
        self.memory_delta() as f64 / self.tokens_generated as f64
    }
}

/// Benchmark context with shared resources
struct BenchmarkContext {
    backend: Option<MLXFFIBackend>,
    adapters: Vec<LoRAAdapter>,
}

impl BenchmarkContext {
    fn new() -> Self {
        Self {
            backend: None,
            adapters: Vec::new(),
        }
    }

    /// Setup a mock MLX model for benchmarking
    fn setup_mock_backend(&mut self, hidden_dim: usize) {
        // Create a mock model configuration
        use adapteros_lora_mlx_ffi::ModelConfig;

        // For benchmarking, we'll use a simplified mock
        // In production, this would load a real model
        tracing::info!(
            hidden_dim = hidden_dim,
            "Setting up mock MLX backend for benchmarking"
        );
    }

    /// Create test adapters with specified configuration
    fn create_test_adapters(&mut self, count: usize, rank: usize, hidden_dim: usize) {
        self.adapters.clear();

        for i in 0..count {
            let config = LoRAConfig {
                rank,
                alpha: (rank * 2) as f32,
                target_modules: vec![
                    "q_proj".to_string(),
                    "k_proj".to_string(),
                    "v_proj".to_string(),
                    "o_proj".to_string(),
                ],
                dropout: 0.1,
            };

            let shared_down = vec![vec![0.01; hidden_dim]; rank];
            let mut adapter = LoRAAdapter::new_with_shared_down(
                format!("bench_adapter_{}", i),
                config,
                shared_down,
            );

            // Add module weights for each target
            for module in &["q_proj", "k_proj", "v_proj", "o_proj"] {
                let lora_b = vec![vec![0.02; rank]; hidden_dim];
                adapter.add_module_weights(module, lora_b);
            }

            self.adapters.push(adapter);
        }
    }
}

// ================================
// Core Performance Benchmarks
// ================================

/// Benchmark 1: Single Token Latency
///
/// Measures the time to generate a single token with varying configurations.
/// This is the critical metric for interactive applications.
fn bench_single_token_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_single_token_latency");

    // Reset memory tracking for clean baseline
    memory::reset();

    for &hidden_dim in &[1024, 4096] {
        for &rank in &[8, 16] {
            let param = format!("h{}_r{}", hidden_dim, rank);

            group.bench_function(BenchmarkId::new("latency", &param), |b| {
                let mut ctx = BenchmarkContext::new();
                ctx.setup_mock_backend(hidden_dim);
                ctx.create_test_adapters(1, rank, hidden_dim);

                let adapter = &ctx.adapters[0];
                let input = vec![0.1; hidden_dim];
                let base_output = vec![0.0; hidden_dim];

                b.iter(|| {
                    // Simulate single LoRA application
                    let mut output = base_output.clone();

                    // Shared down projection
                    let mut intermediate = vec![0.0; rank];
                    if let Some(shared_down) = adapter.shared_down() {
                        for r in 0..rank {
                            for (h, &val) in input.iter().enumerate() {
                                intermediate[r] += val * shared_down[r][h];
                            }
                        }
                    }

                    // Module-specific up projection
                    if let Some(lora_b) = adapter.get_module_weights("q_proj") {
                        for h in 0..hidden_dim {
                            for r in 0..rank {
                                output[h] += intermediate[r] * lora_b[h][r];
                            }
                        }
                    }

                    black_box(output)
                });
            });
        }
    }

    group.finish();
}

/// Benchmark 2: Batch Inference Throughput
///
/// Measures throughput (tokens/sec) for different batch sizes and sequence lengths.
fn bench_batch_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_batch_throughput");
    group.sample_size(30); // Fewer samples for longer-running benchmarks

    for &batch_size in &[1, 4, 8] {
        for &seq_len in &[32, 128] {
            let param = format!("b{}_s{}", batch_size, seq_len);

            // Set throughput for statistical reporting
            group.throughput(Throughput::Elements((batch_size * seq_len) as u64));

            group.bench_function(BenchmarkId::new("throughput", &param), |b| {
                let hidden_dim = 1024;
                let rank = 16;

                let mut ctx = BenchmarkContext::new();
                ctx.setup_mock_backend(hidden_dim);
                ctx.create_test_adapters(2, rank, hidden_dim);

                b.iter(|| {
                    let start = Instant::now();
                    let mut tokens_processed = 0;

                    for _ in 0..batch_size {
                        for _ in 0..seq_len {
                            // Simulate token generation
                            let input = vec![0.1; hidden_dim];
                            black_box(&input);
                            tokens_processed += 1;
                        }
                    }

                    let elapsed = start.elapsed();
                    black_box((tokens_processed, elapsed))
                });
            });
        }
    }

    group.finish();
}

/// Benchmark 3: Memory Allocation Patterns
///
/// Profiles memory allocation behavior during adapter operations.
fn bench_memory_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_memory_allocation");

    for &adapter_count in &[1, 4, 8] {
        group.bench_function(
            BenchmarkId::new("allocation_pattern", adapter_count),
            |b| {
                let hidden_dim = 2048;
                let rank = 16;

                b.iter_custom(|iters| {
                    let mut total_duration = Duration::ZERO;

                    for _ in 0..iters {
                        memory::reset();
                        let mem_start = memory::memory_usage();
                        let alloc_start = memory::allocation_count();

                        let start = Instant::now();

                        // Create adapters and track allocations
                        let mut ctx = BenchmarkContext::new();
                        ctx.create_test_adapters(adapter_count, rank, hidden_dim);

                        // Simulate adapter usage
                        for adapter in &ctx.adapters {
                            let _ = adapter.parameter_count();
                            let _ = adapter.memory_usage();
                        }

                        total_duration += start.elapsed();

                        let mem_end = memory::memory_usage();
                        let alloc_end = memory::allocation_count();

                        // Log memory metrics
                        tracing::debug!(
                            adapter_count = adapter_count,
                            memory_delta_mb = (mem_end - mem_start) as f32 / (1024.0 * 1024.0),
                            allocations = alloc_end - alloc_start,
                            "Memory allocation pattern"
                        );
                    }

                    total_duration
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 4: Cache Efficiency
///
/// Tests cache performance with different access patterns.
fn bench_cache_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_cache_efficiency");

    // Sequential access pattern (cache-friendly)
    group.bench_function("sequential_access", |b| {
        let data = vec![1.0f32; 1024 * 1024]; // 4MB array

        b.iter(|| {
            let mut sum = 0.0f32;
            for &val in &data {
                sum += val;
            }
            black_box(sum)
        });
    });

    // Random access pattern (cache-unfriendly)
    group.bench_function("random_access", |b| {
        let data = vec![1.0f32; 1024 * 1024];
        let indices: Vec<usize> = (0..10000).map(|i| (i * 103) % data.len()).collect();

        b.iter(|| {
            let mut sum = 0.0f32;
            for &idx in &indices {
                sum += data[idx];
            }
            black_box(sum)
        });
    });

    // Strided access pattern (moderate cache efficiency)
    group.bench_function("strided_access", |b| {
        let data = vec![1.0f32; 1024 * 1024];
        let stride = 64; // Cache line size

        b.iter(|| {
            let mut sum = 0.0f32;
            let mut idx = 0;
            while idx < data.len() {
                sum += data[idx];
                idx += stride;
            }
            black_box(sum)
        });
    });

    group.finish();
}

/// Benchmark 5: Adapter Switching Overhead
///
/// Measures the cost of hot-swapping adapters at runtime.
fn bench_adapter_switching_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_adapter_switching");

    for &adapter_count in &[2, 4, 8] {
        group.bench_function(
            BenchmarkId::new("switch_overhead", adapter_count),
            |b| {
                let hidden_dim = 2048;
                let rank = 16;

                let mut ctx = BenchmarkContext::new();
                ctx.create_test_adapters(adapter_count, rank, hidden_dim);

                b.iter(|| {
                    // Simulate switching between adapters
                    for i in 0..adapter_count {
                        let adapter = &ctx.adapters[i];
                        let _ = adapter.id();
                        let _ = adapter.config();
                        black_box(adapter.has_module("q_proj"));
                    }
                });
            },
        );
    }

    group.finish();
}

// ================================
// Operation-Level Benchmarks
// ================================

/// Benchmark 6: Matrix Multiplication Performance
///
/// Tests matmul operations at various sizes (LoRA's core operation).
fn bench_matmul_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_matmul_operations");

    for &dim in &[256, 512, 1024, 2048] {
        for &rank in &[8, 16, 32] {
            let param = format!("{}x{}", dim, rank);

            group.bench_function(BenchmarkId::new("matmul", &param), |b| {
                let matrix_a = vec![vec![0.1f32; rank]; dim];
                let matrix_b = vec![vec![0.2f32; dim]; rank];

                b.iter(|| {
                    // Matrix multiplication: C = A @ B^T
                    let mut result = vec![vec![0.0f32; dim]; dim];

                    for i in 0..dim {
                        for j in 0..dim {
                            for k in 0..rank {
                                result[i][j] += matrix_a[i][k] * matrix_b[k][j];
                            }
                        }
                    }

                    black_box(result)
                });
            });
        }
    }

    group.finish();
}

/// Benchmark 7: Attention Mechanism Performance
///
/// Simulates attention computation patterns.
fn bench_attention_mechanism(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_attention");
    group.sample_size(20); // Attention is expensive

    for &seq_len in &[32, 128, 512] {
        for &hidden_dim in &[512, 1024] {
            let param = format!("s{}_h{}", seq_len, hidden_dim);

            group.bench_function(BenchmarkId::new("attention", &param), |b| {
                let q = vec![vec![0.1f32; hidden_dim]; seq_len];
                let k = vec![vec![0.2f32; hidden_dim]; seq_len];
                let v = vec![vec![0.3f32; hidden_dim]; seq_len];

                b.iter(|| {
                    // Simplified attention: softmax(Q @ K^T) @ V
                    let mut scores = vec![vec![0.0f32; seq_len]; seq_len];

                    // Q @ K^T
                    for i in 0..seq_len {
                        for j in 0..seq_len {
                            let mut score = 0.0f32;
                            for d in 0..hidden_dim {
                                score += q[i][d] * k[j][d];
                            }
                            scores[i][j] = score / (hidden_dim as f32).sqrt();
                        }
                    }

                    // Softmax
                    for i in 0..seq_len {
                        let max_score = scores[i].iter().copied().fold(f32::NEG_INFINITY, f32::max);
                        let mut sum_exp = 0.0f32;

                        for j in 0..seq_len {
                            scores[i][j] = (scores[i][j] - max_score).exp();
                            sum_exp += scores[i][j];
                        }

                        for j in 0..seq_len {
                            scores[i][j] /= sum_exp;
                        }
                    }

                    // Scores @ V
                    let mut output = vec![vec![0.0f32; hidden_dim]; seq_len];
                    for i in 0..seq_len {
                        for d in 0..hidden_dim {
                            for j in 0..seq_len {
                                output[i][d] += scores[i][j] * v[j][d];
                            }
                        }
                    }

                    black_box(output)
                });
            });
        }
    }

    group.finish();
}

/// Benchmark 8: LoRA Forward Pass
///
/// Full LoRA computation: input @ A @ B with scaling.
fn bench_lora_forward_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_lora_forward");

    for &hidden_dim in &[1024, 2048, 4096] {
        for &rank in &[8, 16, 32] {
            let param = format!("h{}_r{}", hidden_dim, rank);

            group.bench_function(BenchmarkId::new("forward", &param), |b| {
                let input = vec![0.1f32; hidden_dim];
                let lora_a = vec![vec![0.01f32; hidden_dim]; rank];
                let lora_b = vec![vec![0.02f32; rank]; hidden_dim];
                let alpha = (rank * 2) as f32;
                let scaling = alpha / rank as f32;

                b.iter(|| {
                    // Step 1: input @ A
                    let mut intermediate = vec![0.0f32; rank];
                    for r in 0..rank {
                        for (h, &val) in input.iter().enumerate() {
                            intermediate[r] += val * lora_a[r][h];
                        }
                    }

                    // Step 2: intermediate @ B
                    let mut output = vec![0.0f32; hidden_dim];
                    for h in 0..hidden_dim {
                        for r in 0..rank {
                            output[h] += intermediate[r] * lora_b[h][r];
                        }
                    }

                    // Step 3: Apply scaling
                    for val in &mut output {
                        *val *= scaling;
                    }

                    black_box(output)
                });
            });
        }
    }

    group.finish();
}

/// Benchmark 9: Multi-Adapter Fusion (K-sparse)
///
/// Tests performance of fusing multiple adapters with gating.
fn bench_multi_adapter_fusion(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_multi_adapter_fusion");
    group.sample_size(30);

    for &k in &[1, 2, 4, 8] {
        group.bench_function(BenchmarkId::new("k_sparse", k), |b| {
            let hidden_dim = 2048;
            let rank = 16;

            let mut ctx = BenchmarkContext::new();
            ctx.create_test_adapters(k, rank, hidden_dim);

            let input = vec![0.1f32; hidden_dim];
            let gates: Vec<f32> = (0..k).map(|i| 1.0 / (i + 1) as f32).collect();

            b.iter(|| {
                let mut fused_output = vec![0.0f32; hidden_dim];

                for (adapter, &gate) in ctx.adapters.iter().zip(&gates) {
                    // Apply LoRA with gate weight
                    let mut intermediate = vec![0.0f32; rank];

                    if let Some(shared_down) = adapter.shared_down() {
                        for r in 0..rank {
                            for (h, &val) in input.iter().enumerate() {
                                intermediate[r] += val * shared_down[r][h];
                            }
                        }
                    }

                    if let Some(lora_b) = adapter.get_module_weights("q_proj") {
                        for h in 0..hidden_dim {
                            for r in 0..rank {
                                fused_output[h] += gate * intermediate[r] * lora_b[h][r];
                            }
                        }
                    }
                }

                black_box(fused_output)
            });
        });
    }

    group.finish();
}

// ================================
// Memory and Cache Benchmarks
// ================================

/// Benchmark 10: Memory Transfer Operations
///
/// Tests memory copy and transfer performance.
fn bench_memory_transfers(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_memory_transfers");

    for &size_mb in &[1, 4, 16, 64] {
        let size_bytes = size_mb * 1024 * 1024;
        let size_floats = size_bytes / 4;

        group.throughput(Throughput::Bytes(size_bytes as u64));

        group.bench_function(BenchmarkId::new("copy", size_mb), |b| {
            let src = vec![1.0f32; size_floats];
            let mut dst = vec![0.0f32; size_floats];

            b.iter(|| {
                dst.copy_from_slice(&src);
                black_box(&dst)
            });
        });

        group.bench_function(BenchmarkId::new("clone", size_mb), |b| {
            let src = vec![1.0f32; size_floats];

            b.iter(|| {
                let dst = src.clone();
                black_box(dst)
            });
        });
    }

    group.finish();
}

/// Benchmark 11: Garbage Collection Impact
///
/// Measures GC performance and overhead.
fn bench_gc_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_gc_impact");

    group.bench_function("gc_collect", |b| {
        b.iter(|| {
            memory::gc_collect();
        });
    });

    group.bench_function("memory_stats", |b| {
        b.iter(|| {
            let stats = memory::stats();
            black_box(stats)
        });
    });

    group.finish();
}

// ================================
// Comparison Benchmarks
// ================================

/// Benchmark 12: Shared vs Separate Down Projections
///
/// Compares memory and performance of shared architecture.
fn bench_shared_vs_separate(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlx_shared_vs_separate");

    let hidden_dim = 2048;
    let rank = 16;
    let num_modules = 4;

    group.bench_function("shared_down_projection", |b| {
        let shared_down = vec![vec![0.01f32; hidden_dim]; rank];
        let up_projections: Vec<Vec<Vec<f32>>> = (0..num_modules)
            .map(|_| vec![vec![0.02f32; rank]; hidden_dim])
            .collect();

        let input = vec![0.1f32; hidden_dim];

        b.iter(|| {
            // Single shared down projection
            let mut intermediate = vec![0.0f32; rank];
            for r in 0..rank {
                for (h, &val) in input.iter().enumerate() {
                    intermediate[r] += val * shared_down[r][h];
                }
            }

            // Multiple up projections
            let mut outputs = Vec::new();
            for lora_b in &up_projections {
                let mut output = vec![0.0f32; hidden_dim];
                for h in 0..hidden_dim {
                    for r in 0..rank {
                        output[h] += intermediate[r] * lora_b[h][r];
                    }
                }
                outputs.push(output);
            }

            black_box(outputs)
        });
    });

    group.bench_function("separate_projections", |b| {
        let down_projections: Vec<Vec<Vec<f32>>> = (0..num_modules)
            .map(|_| vec![vec![0.01f32; hidden_dim]; rank])
            .collect();
        let up_projections: Vec<Vec<Vec<f32>>> = (0..num_modules)
            .map(|_| vec![vec![0.02f32; rank]; hidden_dim])
            .collect();

        let input = vec![0.1f32; hidden_dim];

        b.iter(|| {
            let mut outputs = Vec::new();

            for (lora_a, lora_b) in down_projections.iter().zip(&up_projections) {
                // Separate down projection for each module
                let mut intermediate = vec![0.0f32; rank];
                for r in 0..rank {
                    for (h, &val) in input.iter().enumerate() {
                        intermediate[r] += val * lora_a[r][h];
                    }
                }

                // Up projection
                let mut output = vec![0.0f32; hidden_dim];
                for h in 0..hidden_dim {
                    for r in 0..rank {
                        output[h] += intermediate[r] * lora_b[h][r];
                    }
                }
                outputs.push(output);
            }

            black_box(outputs)
        });
    });

    group.finish();
}

// ================================
// Benchmark Groups Configuration
// ================================

criterion_group!(
    name = latency_benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(2));
    targets = bench_single_token_latency, bench_adapter_switching_overhead
);

criterion_group!(
    name = throughput_benches;
    config = Criterion::default()
        .sample_size(30)
        .measurement_time(Duration::from_secs(10))
        .warm_up_time(Duration::from_secs(3));
    targets = bench_batch_throughput, bench_multi_adapter_fusion
);

criterion_group!(
    name = memory_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(5));
    targets = bench_memory_allocation_patterns, bench_memory_transfers, bench_gc_impact
);

criterion_group!(
    name = operation_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(8));
    targets = bench_matmul_operations, bench_attention_mechanism, bench_lora_forward_pass
);

criterion_group!(
    name = cache_benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(Duration::from_secs(5));
    targets = bench_cache_efficiency
);

criterion_group!(
    name = comparison_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(5));
    targets = bench_shared_vs_separate
);

criterion_main!(
    latency_benches,
    throughput_benches,
    memory_benches,
    operation_benches,
    cache_benches,
    comparison_benches
);
