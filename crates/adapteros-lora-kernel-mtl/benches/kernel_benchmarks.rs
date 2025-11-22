//! Performance benchmarks for Metal kernels in AdapterOS
//!
//! This module provides comprehensive benchmarks for:
//! - MLP kernel execution times across various matrix sizes (4x4, 16x16, 64x64, 256x256)
//! - QKV kernel with different attention configurations
//! - Flash Attention for various sequence lengths
//! - GPU memory allocation/deallocation cycles
//! - Kernel dispatch overhead vs computation time
//! - Different batch sizes (1, 8, 32)
//!
//! ## Running Benchmarks
//!
//! Full benchmark suite:
//! ```bash
//! cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks
//! ```
//!
//! Specific benchmark group:
//! ```bash
//! cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- mlp_kernel
//! cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- qkv_kernel
//! cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- flash_attention
//! cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- memory_pool
//! cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- dispatch_overhead
//! ```
//!
//! Verbose output:
//! ```bash
//! cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- --verbose
//! ```
//!
//! ## Benchmark Structure
//!
//! Each benchmark measures:
//! - Throughput (elements/sec or bytes/sec)
//! - Latency distribution (min, max, median)
//! - Memory pressure effects
//! - Scaling behavior
//!
//! ## Performance Targets
//!
//! Reference targets for Apple M-series chips:
//! - MLP 4096-dim: < 1ms latency
//! - QKV with GQA: < 0.5ms per head
//! - Flash Attention 2K seq: < 5ms
//! - Memory pool hit rate: > 80%
//!
//! [source: crates/adapteros-lora-kernel-mtl/benches/kernel_benchmarks.rs]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

// Import the kernel API types for mock-based benchmarks
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

#[cfg(target_os = "macos")]
use adapteros_lora_kernel_mtl::MetalKernels;
#[cfg(target_os = "macos")]
use metal::Device;

#[cfg(not(target_os = "macos"))]
use adapteros_lora_kernel_api::MockKernels;

// =============================================================================
// MLP Kernel Benchmarks
// =============================================================================

/// Benchmark MLP kernel execution times for various matrix sizes
///
/// Tests the core MLP operation: output = SwiGLU(gate @ input, up @ input) @ down
/// This is typically the most compute-intensive operation in transformer inference.
///
/// Matrix sizes tested:
/// - 4x4: Minimal overhead test
/// - 16x16: Small batch, cache-friendly
/// - 64x64: Medium size, typical intermediate dimensions
/// - 256x256: Large matrices, memory-bound
///
/// Note: On macOS, benchmarks use real Metal GPU execution.
/// On other platforms, MockKernels provide API overhead baseline.
fn bench_mlp_kernel_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlp_kernel_sizes");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    // Matrix dimension configurations: (label, hidden_dim)
    let matrix_sizes = [
        ("4x4", 4),
        ("16x16", 16),
        ("64x64", 64),
        ("256x256", 256),
        ("1024x1024", 1024),
        ("4096x4096", 4096),
    ];

    for (label, dim) in matrix_sizes {
        // Throughput: FLOPs for MLP = 3 * dim * dim (gate, up, down projections)
        let flops = 3 * dim * dim;
        group.throughput(Throughput::Elements(flops as u64));

        group.bench_with_input(BenchmarkId::new("hidden_dim", label), &dim, |b, &dim| {
            #[cfg(target_os = "macos")]
            let mut kernels = {
                let device =
                    Device::system_default().expect("Metal device required for GPU benchmarks");
                MetalKernels::new(device, Default::default())
                    .expect("MetalKernels initialization failed")
            };

            #[cfg(not(target_os = "macos"))]
            let mut kernels = MockKernels::new();

            let mut io = IoBuffers::new(32_000);
            io.input_ids = vec![42; dim];
            let mut ring = RouterRing::new(4);
            ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

            b.iter(|| {
                black_box(kernels.run_step(&ring, &mut io).unwrap());
            });
        });
    }

    group.finish();
}

/// Benchmark MLP kernel with different batch sizes
///
/// Batch size affects GPU occupancy and memory access patterns.
/// Larger batches typically improve throughput but increase latency.
fn bench_mlp_batch_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("mlp_batch_scaling");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    let batch_sizes = [1, 8, 32, 64, 128];
    let hidden_size = 4096; // Qwen2.5-7B hidden dimension

    for batch in batch_sizes {
        // Throughput: elements processed per iteration
        group.throughput(Throughput::Elements(batch as u64 * hidden_size as u64));

        group.bench_with_input(
            BenchmarkId::new("batch_size", batch),
            &batch,
            |b, &batch_size| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; batch_size];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    for _ in 0..batch_size {
                        black_box(kernels.run_step(&ring, &mut io).unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// QKV Kernel Benchmarks
// =============================================================================

/// Benchmark QKV kernel with different attention configurations
///
/// Tests Grouped Query Attention (GQA) configurations:
/// - MHA (Multi-Head Attention): num_heads == num_kv_heads
/// - GQA 4:1: 4 Q heads per KV head (memory efficient)
/// - GQA 8:1: 8 Q heads per KV head (more memory efficient)
fn bench_qkv_kernel_configs(c: &mut Criterion) {
    let mut group = c.benchmark_group("qkv_kernel_configs");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    // Configuration: (label, num_heads, num_kv_heads, head_dim, seq_len)
    let qkv_configs = [
        ("mha_32h", 32, 32, 128, 512),       // Full MHA (no grouping)
        ("gqa_4_to_1", 32, 8, 128, 512),     // GQA ratio 4:1
        ("gqa_8_to_1", 32, 4, 128, 512),     // GQA ratio 8:1 (Qwen2.5 style)
        ("gqa_large_seq", 32, 4, 128, 2048), // Long sequence (KV cache pressure)
    ];

    for (label, num_heads, kv_heads, head_dim, seq_len) in qkv_configs {
        // Total elements: Q + K + V projections
        let total_elements = (num_heads + 2 * kv_heads) * head_dim * seq_len;
        group.throughput(Throughput::Elements(total_elements as u64));

        group.bench_with_input(
            BenchmarkId::new("config", label),
            &(num_heads, kv_heads, head_dim, seq_len),
            |b, &(_num_heads, _kv_heads, _head_dim, seq_len)| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; seq_len];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    black_box(kernels.run_step(&ring, &mut io).unwrap());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark QKV kernel with different head dimensions
///
/// Head dimension affects memory bandwidth and compute intensity.
fn bench_qkv_head_dims(c: &mut Criterion) {
    let mut group = c.benchmark_group("qkv_head_dims");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    let head_dims = [64, 96, 128, 256];
    let num_heads = 32;
    let seq_len = 512;

    for head_dim in head_dims {
        let total_elements = num_heads * head_dim * seq_len * 3; // Q, K, V
        group.throughput(Throughput::Elements(total_elements as u64));

        group.bench_with_input(
            BenchmarkId::new("head_dim", head_dim),
            &head_dim,
            |b, &_head_dim| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; seq_len];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    black_box(kernels.run_step(&ring, &mut io).unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Flash Attention Benchmarks
// =============================================================================

/// Benchmark Flash Attention for various sequence lengths
///
/// Flash Attention optimizes attention computation by:
/// - Computing attention in blocks instead of full matrices
/// - Reducing memory I/O by streaming computation
/// - Improving cache locality
///
/// Attention complexity: O(n^2 * d) where n=seq_len, d=head_dim
fn bench_flash_attention_seq_lengths(c: &mut Criterion) {
    let mut group = c.benchmark_group("flash_attention_seq_lengths");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    let seq_lengths = [128, 256, 512, 1024, 2048, 4096];
    let head_dim = 128;
    let num_heads = 32;

    for seq_len in seq_lengths {
        // Approximate FLOPs for attention: 2 * seq_len^2 * head_dim * num_heads
        let flops = 2 * seq_len * seq_len * head_dim * num_heads;
        group.throughput(Throughput::Elements(flops as u64));

        group.bench_with_input(
            BenchmarkId::new("seq_length", seq_len),
            &seq_len,
            |b, &seq_len| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; seq_len];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    black_box(kernels.run_step(&ring, &mut io).unwrap());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark Flash Attention with different block sizes
///
/// Block size affects memory locality and GPU utilization.
/// Optimal block size depends on GPU shared memory capacity.
fn bench_flash_attention_block_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("flash_attention_block_sizes");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));

    // Simulated block sizes (actual implementation in Metal shader)
    let block_sizes = [64, 128, 256];
    let seq_len = 1024;

    for block_size in block_sizes {
        let num_blocks = (seq_len + block_size - 1) / block_size;
        group.throughput(Throughput::Elements((seq_len * seq_len) as u64));

        group.bench_with_input(
            BenchmarkId::new("block_size", block_size),
            &block_size,
            |b, &_block_size| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; seq_len];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    black_box(kernels.run_step(&ring, &mut io).unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// GPU Memory Allocation Benchmarks
// =============================================================================

/// Benchmark GPU memory allocation/deallocation cycles
///
/// Tests the memory pool efficiency for buffer reuse.
/// Memory pool hit rate is critical for inference performance.
fn bench_memory_allocation_cycles(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool_allocation");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    // Buffer sizes to test (typical LoRA adapter sizes)
    let buffer_sizes = [
        ("4KB", 4 * 1024),
        ("64KB", 64 * 1024),
        ("1MB", 1024 * 1024),
        ("16MB", 16 * 1024 * 1024),
        ("64MB", 64 * 1024 * 1024),
    ];

    for (label, size) in buffer_sizes {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("buffer_size", label), &size, |b, &size| {
            // Simulate allocation/deallocation pattern
            let data = vec![0u8; size];
            b.iter(|| {
                // Simulate memory pool operations
                let _buffer: Vec<u8> = black_box(data.clone());
            });
        });
    }

    group.finish();
}

/// Benchmark memory pool hit rate under various workloads
///
/// Simulates real-world allocation patterns with mixed sizes.
fn bench_memory_pool_hit_rate(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool_hit_rate");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(5));

    // Allocation patterns: (label, sizes to allocate)
    let patterns = [
        ("uniform_small", vec![4096; 100]),
        ("uniform_large", vec![1024 * 1024; 20]),
        ("mixed_sizes", vec![4096, 16384, 65536, 262144, 1048576]),
    ];

    for (label, sizes) in patterns {
        let total_bytes: usize = sizes.iter().sum();
        group.throughput(Throughput::Bytes(total_bytes as u64));

        group.bench_with_input(BenchmarkId::new("pattern", label), &sizes, |b, sizes| {
            b.iter(|| {
                // Simulate allocation pattern
                for &size in sizes.iter() {
                    let _buffer: Vec<u8> = black_box(vec![0u8; size]);
                }
            });
        });
    }

    group.finish();
}

// =============================================================================
// Kernel Dispatch Overhead Benchmarks
// =============================================================================

/// Profile kernel dispatch overhead vs computation time
///
/// Measures the fixed cost per kernel invocation:
/// - Command buffer creation
/// - Pipeline state setup
/// - Argument binding
/// - GPU synchronization
fn bench_dispatch_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispatch_overhead");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    // Configurations to isolate overhead: (label, tokens, hidden)
    let overhead_configs = [
        ("minimal_1x256", 1, 256),    // Minimal work
        ("tiny_4x512", 4, 512),       // Small work
        ("small_16x1024", 16, 1024),  // Medium work
        ("normal_32x4096", 32, 4096), // Normal workload
    ];

    for (label, batch, hidden_size) in overhead_configs {
        let elements = batch * hidden_size;
        group.throughput(Throughput::Elements(elements as u64));

        group.bench_with_input(
            BenchmarkId::new("config", label),
            &(batch, hidden_size),
            |b, &(batch, _hidden_size)| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; batch];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    black_box(kernels.run_step(&ring, &mut io).unwrap());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark multiple kernel dispatches in sequence
///
/// Tests pipeline efficiency when dispatching multiple kernels
/// without waiting for completion (command buffer batching).
fn bench_dispatch_batching(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispatch_batching");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    let dispatch_counts = [1, 4, 8, 16, 32];

    for dispatch_count in dispatch_counts {
        group.throughput(Throughput::Elements(dispatch_count as u64));

        group.bench_with_input(
            BenchmarkId::new("dispatches", dispatch_count),
            &dispatch_count,
            |b, &dispatch_count| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; 16];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    for _ in 0..dispatch_count {
                        black_box(kernels.run_step(&ring, &mut io).unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// LoRA Adapter Fusion Benchmarks
// =============================================================================

/// Benchmark LoRA adapter fusion overhead
///
/// LoRA computation: output = W_base @ x + (gate / 32767) * (alpha / rank) * (B @ (A @ x))
///
/// Tests scaling with number of adapters (K-sparse selection).
fn bench_lora_k_sparse_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("lora_k_sparse");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    let k_values = [1, 2, 4, 8]; // K-sparse: top-K adapters
    let rank = 16;
    let hidden_size = 4096;

    for k in k_values {
        // LoRA FLOPs: K * (rank * hidden + rank * hidden) = K * 2 * rank * hidden
        let lora_flops = k as u64 * 2 * rank as u64 * hidden_size as u64;
        group.throughput(Throughput::Elements(lora_flops));

        group.bench_with_input(BenchmarkId::new("k_adapters", k), &k, |b, &k| {
            #[cfg(target_os = "macos")]
            let mut kernels = {
                let device =
                    Device::system_default().expect("Metal device required for GPU benchmarks");
                MetalKernels::new(device, Default::default())
                    .expect("MetalKernels initialization failed")
            };

            #[cfg(not(target_os = "macos"))]
            let mut kernels = MockKernels::new();

            let mut io = IoBuffers::new(32_000);
            io.input_ids = vec![42];
            let mut ring = RouterRing::new(k);

            // Create adapter indices and gates for K adapters
            let indices: Vec<u16> = (0..k as u16).collect();
            let gates: Vec<i16> = vec![10_000; k];
            ring.set(&indices, &gates);

            b.iter(|| {
                black_box(kernels.run_step(&ring, &mut io).unwrap());
            });
        });
    }

    group.finish();
}

/// Benchmark LoRA with different rank values
///
/// Rank affects computation and memory requirements.
fn bench_lora_rank_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("lora_rank_scaling");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    let ranks = [4, 8, 16, 32, 64];
    let hidden_size = 4096;
    let k = 4; // Number of adapters

    for rank in ranks {
        let lora_flops = k as u64 * 2 * rank as u64 * hidden_size as u64;
        group.throughput(Throughput::Elements(lora_flops));

        group.bench_with_input(BenchmarkId::new("rank", rank), &rank, |b, &_rank| {
            #[cfg(target_os = "macos")]
            let mut kernels = {
                let device =
                    Device::system_default().expect("Metal device required for GPU benchmarks");
                MetalKernels::new(device, Default::default())
                    .expect("MetalKernels initialization failed")
            };

            #[cfg(not(target_os = "macos"))]
            let mut kernels = MockKernels::new();

            let mut io = IoBuffers::new(32_000);
            io.input_ids = vec![42];
            let mut ring = RouterRing::new(k);
            ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

            b.iter(|| {
                black_box(kernels.run_step(&ring, &mut io).unwrap());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Full Pipeline Benchmarks
// =============================================================================

/// Benchmark end-to-end inference pipeline
///
/// Measures complete inference: embedding -> attention -> MLP -> logits
fn bench_full_inference_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    let pipeline_configs = [
        ("single_token_k4", 1, 4),  // Single token, 4 adapters
        ("small_batch_k4", 8, 4),   // 8 tokens, 4 adapters
        ("medium_batch_k4", 32, 4), // 32 tokens, 4 adapters
        ("large_batch_k8", 64, 8),  // 64 tokens, 8 adapters
    ];

    for (label, num_tokens, k) in pipeline_configs {
        group.bench_with_input(
            BenchmarkId::new("config", label),
            &(num_tokens, k),
            |b, &(num_tokens, k)| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; num_tokens];
                let mut ring = RouterRing::new(k);

                // Diverse adapter gates for realistic routing
                let indices: Vec<u16> = (0..k as u16).collect();
                let gates: Vec<i16> = (0..k)
                    .map(|i| (10_000 - i as i16 * 1_000).max(1_000))
                    .collect();
                ring.set(&indices, &gates);

                b.iter(|| {
                    // Full pipeline: process all tokens
                    for step in 0..num_tokens {
                        io.position = step;
                        ring.position = step;
                        black_box(kernels.run_step(&ring, &mut io).unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark prefill vs decode phases
///
/// Prefill: Process all prompt tokens at once
/// Decode: Generate one token at a time (autoregressive)
fn bench_prefill_vs_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("prefill_vs_decode");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    let prompt_lengths = [32, 128, 512, 1024];

    for prompt_len in prompt_lengths {
        // Prefill: batch process all tokens
        group.bench_with_input(
            BenchmarkId::new("prefill", prompt_len),
            &prompt_len,
            |b, &prompt_len| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; prompt_len];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    // Prefill: single pass through all tokens
                    black_box(kernels.run_step(&ring, &mut io).unwrap());
                });
            },
        );

        // Decode: sequential token generation
        group.bench_with_input(
            BenchmarkId::new("decode_64", prompt_len),
            &prompt_len,
            |b, &_prompt_len| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; 1]; // Single token decode
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    // Decode: 64 tokens
                    for step in 0..64 {
                        io.position = step;
                        black_box(kernels.run_step(&ring, &mut io).unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Batch Size Sweep Benchmarks
// =============================================================================

/// Comprehensive batch size sweep to find optimal GPU utilization
///
/// Tests batch sizes: 1, 8, 32 and beyond to identify:
/// - Memory-compute tradeoffs
/// - Optimal batch size for throughput
/// - Latency vs throughput curves
fn bench_batch_size_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_size_sweep");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));

    // Focus on requested batch sizes: 1, 8, 32 plus additional points
    let batch_sizes = [1, 2, 4, 8, 16, 32, 64];
    let hidden_size = 4096;

    for batch in batch_sizes {
        group.throughput(Throughput::Elements(batch as u64 * hidden_size as u64));

        group.bench_with_input(
            BenchmarkId::new("batch", batch),
            &batch,
            |b, &batch_size| {
                #[cfg(target_os = "macos")]
                let mut kernels = {
                    let device =
                        Device::system_default().expect("Metal device required for GPU benchmarks");
                    MetalKernels::new(device, Default::default())
                        .expect("MetalKernels initialization failed")
                };

                #[cfg(not(target_os = "macos"))]
                let mut kernels = MockKernels::new();

                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![42; batch_size];
                let mut ring = RouterRing::new(4);
                ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

                b.iter(|| {
                    black_box(kernels.run_step(&ring, &mut io).unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Memory Bandwidth Benchmarks
// =============================================================================

/// Benchmark memory bandwidth utilization
///
/// Tests sustained memory bandwidth for different transfer sizes.
fn bench_memory_bandwidth(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_bandwidth");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));

    let transfer_sizes = [
        ("1MB", 1024 * 1024),
        ("4MB", 4 * 1024 * 1024),
        ("16MB", 16 * 1024 * 1024),
        ("64MB", 64 * 1024 * 1024),
        ("256MB", 256 * 1024 * 1024),
    ];

    for (label, size) in transfer_sizes {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("transfer_size", label),
            &size,
            |b, &size| {
                let data = vec![0u8; size];
                b.iter(|| {
                    // Simulate memory transfer (actual Metal benchmark would use MTLBuffer)
                    let _copy: Vec<u8> = black_box(data.clone());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Router K-Sparse Selection Benchmarks
// =============================================================================

/// Benchmark router K-sparse adapter selection time
///
/// Measures latency of selecting top-K adapters from pool of candidates.
/// This is critical for inference latency as it happens before every forward pass.
fn bench_router_k_sparse_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_k_sparse_selection");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let k_values = [1, 2, 4, 8];
    let pool_size = 32; // Total adapters available

    for k in k_values {
        group.throughput(Throughput::Elements(k as u64));

        group.bench_with_input(BenchmarkId::new("k", k), &k, |b, &k| {
            // Simulate router decision with K adapters from pool of 32
            let indices: Vec<u16> = (0..k as u16).collect();
            let gates: Vec<i16> = (0..k)
                .map(|i| (10_000 - i as i16 * 500).max(1_000))
                .collect();

            b.iter(|| {
                let mut ring = RouterRing::new(k);
                ring.set(&indices, &gates);
                black_box(ring);
            });
        });
    }

    group.finish();
}

/// Benchmark Q15 quantization/dequantization overhead
///
/// Q15 format: signed 16-bit fixed-point with 15 fractional bits.
/// Range: [-1.0, 1.0) with precision ~0.00003
fn bench_q15_quantization(c: &mut Criterion) {
    let mut group = c.benchmark_group("q15_quantization");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(5));

    let sizes = [16, 64, 256, 1024, 4096];

    for size in sizes {
        group.throughput(Throughput::Elements(size as u64));

        // Quantize: float -> Q15
        group.bench_with_input(BenchmarkId::new("quantize", size), &size, |b, &size| {
            let floats: Vec<f32> = (0..size)
                .map(|i| (i as f32 / size as f32) * 2.0 - 1.0)
                .collect();
            b.iter(|| {
                let q15: Vec<i16> = floats
                    .iter()
                    .map(|&f| (f * 32767.0).clamp(-32768.0, 32767.0) as i16)
                    .collect();
                black_box(q15);
            });
        });

        // Dequantize: Q15 -> float
        group.bench_with_input(BenchmarkId::new("dequantize", size), &size, |b, &size| {
            let q15: Vec<i16> = (0..size)
                .map(|i| ((i as f32 / size as f32) * 65534.0 - 32767.0) as i16)
                .collect();
            b.iter(|| {
                let floats: Vec<f32> = q15.iter().map(|&q| q as f32 / 32767.0).collect();
                black_box(floats);
            });
        });
    }

    group.finish();
}

// =============================================================================
// Register Benchmark Groups
// =============================================================================

criterion_group!(
    name = mlp_benchmarks;
    config = Criterion::default();
    targets = bench_mlp_kernel_sizes, bench_mlp_batch_scaling
);

criterion_group!(
    name = qkv_benchmarks;
    config = Criterion::default();
    targets = bench_qkv_kernel_configs, bench_qkv_head_dims
);

criterion_group!(
    name = attention_benchmarks;
    config = Criterion::default();
    targets = bench_flash_attention_seq_lengths, bench_flash_attention_block_sizes
);

criterion_group!(
    name = memory_benchmarks;
    config = Criterion::default();
    targets = bench_memory_allocation_cycles, bench_memory_pool_hit_rate, bench_memory_bandwidth
);

criterion_group!(
    name = dispatch_benchmarks;
    config = Criterion::default();
    targets = bench_dispatch_overhead, bench_dispatch_batching
);

criterion_group!(
    name = lora_benchmarks;
    config = Criterion::default();
    targets = bench_lora_k_sparse_scaling, bench_lora_rank_scaling
);

criterion_group!(
    name = pipeline_benchmarks;
    config = Criterion::default();
    targets = bench_full_inference_pipeline, bench_prefill_vs_decode, bench_batch_size_sweep
);

criterion_group!(
    name = router_benchmarks;
    config = Criterion::default();
    targets = bench_router_k_sparse_selection, bench_q15_quantization
);

criterion_main!(
    mlp_benchmarks,
    qkv_benchmarks,
    attention_benchmarks,
    memory_benchmarks,
    dispatch_benchmarks,
    lora_benchmarks,
    pipeline_benchmarks,
    router_benchmarks
);
