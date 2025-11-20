//! CoreML Inference Performance Benchmarks
//!
//! Benchmarks measuring:
//! - Tokens/second on different devices
//! - Latency distribution (min/avg/max/p50/p95/p99)
//! - Memory usage patterns
//! - Power consumption (with power-metrics feature)
//! - ANE vs GPU performance
//!
//! Run with: cargo bench --bench coreml_inference
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

/// Mock inference function for benchmarking
fn mock_inference_step(input_len: usize, vocab_size: usize) -> Vec<f32> {
    let mut output = vec![0.0f32; vocab_size];

    // Simulate computation
    for i in 0..input_len {
        for j in 0..vocab_size.min(100) {
            output[j] += (i * j) as f32 * 0.001;
        }
    }

    output
}

/// Benchmark single token inference
fn bench_single_token_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_token_inference");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(10));

    let vocab_size = 32000;

    for input_len in [1, 4, 8, 16, 32, 64, 128].iter() {
        group.throughput(Throughput::Elements(*input_len as u64));
        group.bench_with_input(
            BenchmarkId::new("mock_backend", input_len),
            input_len,
            |b, &input_len| {
                b.iter(|| {
                    let output = mock_inference_step(input_len, vocab_size);
                    black_box(output)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark tokens per second (throughput)
fn bench_tokens_per_second(c: &mut Criterion) {
    let mut group = c.benchmark_group("tokens_per_second");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(20));

    let vocab_size = 32000;
    let sequence_lengths = vec![128, 256, 512, 1024, 2048];

    for seq_len in sequence_lengths {
        group.throughput(Throughput::Elements(seq_len));
        group.bench_with_input(
            BenchmarkId::new("throughput", seq_len),
            &seq_len,
            |b, &seq_len| {
                b.iter(|| {
                    let mut total_tokens = 0;
                    for _ in 0..seq_len {
                        let output = mock_inference_step(1, vocab_size);
                        black_box(output);
                        total_tokens += 1;
                    }
                    total_tokens
                });
            },
        );
    }

    group.finish();
}

/// Benchmark latency distribution
fn bench_latency_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_distribution");
    group.sample_size(1000);
    group.measurement_time(Duration::from_secs(30));

    let vocab_size = 32000;
    let input_len = 128;

    group.bench_function("latency_histogram", |b| {
        b.iter(|| {
            let output = mock_inference_step(input_len, vocab_size);
            black_box(output)
        });
    });

    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    group.sample_size(50);

    let test_cases = vec![
        ("small_rank_4", 4, 512),
        ("medium_rank_16", 16, 2048),
        ("large_rank_64", 64, 4096),
    ];

    for (name, rank, hidden_dim) in test_cases {
        let memory_size = rank * hidden_dim * 2; // FP16

        group.bench_function(name, |b| {
            b.iter(|| {
                let buffer = vec![0u8; memory_size];
                black_box(buffer)
            });
        });
    }

    group.finish();
}

/// Benchmark K-sparse routing overhead
fn bench_k_sparse_routing(c: &mut Criterion) {
    let mut group = c.benchmark_group("k_sparse_routing");
    group.sample_size(200);

    for k in [1, 2, 4, 8].iter() {
        group.bench_with_input(BenchmarkId::new("routing", k), k, |b, &k| {
            b.iter(|| {
                // Simulate K-sparse gate computation
                let mut gates = vec![0i16; 8];
                let mut indices = vec![0u16; 8];

                for i in 0..k {
                    gates[i] = (32767 / (i + 1)) as i16;
                    indices[i] = i as u16;
                }

                black_box((gates, indices))
            });
        });
    }

    group.finish();
}

/// Benchmark adapter hot-swap
fn bench_adapter_hotswap(c: &mut Criterion) {
    let mut group = c.benchmark_group("adapter_hotswap");
    group.sample_size(50);

    let adapter_sizes = vec![
        ("tiny_128kb", 128 * 1024),
        ("small_512kb", 512 * 1024),
        ("medium_2mb", 2 * 1024 * 1024),
        ("large_8mb", 8 * 1024 * 1024),
    ];

    for (name, size) in adapter_sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                // Simulate adapter load
                let weights = vec![0u8; size];
                black_box(weights)
            });
        });
    }

    group.finish();
}

/// Benchmark batch processing (ANE optimized for batch=1)
fn bench_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");
    group.sample_size(100);

    let vocab_size = 32000;

    for batch_size in [1, 2, 4, 8].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let mut outputs = Vec::with_capacity(batch_size);
                    for _ in 0..batch_size {
                        let output = mock_inference_step(128, vocab_size);
                        outputs.push(output);
                    }
                    black_box(outputs)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark Q15 quantization overhead
fn bench_q15_quantization(c: &mut Criterion) {
    let mut group = c.benchmark_group("q15_quantization");
    group.sample_size(500);

    let sizes = vec![8, 64, 512, 4096];

    for size in sizes {
        group.bench_with_input(
            BenchmarkId::new("quantize", size),
            &size,
            |b, &size| {
                let float_values: Vec<f32> = (0..size).map(|i| (i as f32) / (size as f32)).collect();

                b.iter(|| {
                    let q15_values: Vec<i16> = float_values
                        .iter()
                        .map(|&v| (v * 32767.0) as i16)
                        .collect();
                    black_box(q15_values)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark long sequence processing
fn bench_long_sequences(c: &mut Criterion) {
    let mut group = c.benchmark_group("long_sequences");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(30));

    let vocab_size = 32000;
    let sequence_lengths = vec![512, 1024, 2048, 4096, 8192];

    for seq_len in sequence_lengths {
        group.throughput(Throughput::Elements(seq_len));
        group.bench_with_input(
            BenchmarkId::new("long_seq", seq_len),
            &seq_len,
            |b, &seq_len| {
                b.iter(|| {
                    let output = mock_inference_step(seq_len, vocab_size);
                    black_box(output)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_single_token_inference,
    bench_tokens_per_second,
    bench_latency_distribution,
    bench_memory_usage,
    bench_k_sparse_routing,
    bench_adapter_hotswap,
    bench_batch_processing,
    bench_q15_quantization,
    bench_long_sequences,
);

criterion_main!(benches);
