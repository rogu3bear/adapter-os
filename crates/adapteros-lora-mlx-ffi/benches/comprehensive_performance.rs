//! Comprehensive performance benchmarks for MLX FFI backend
//!
//! This suite provides detailed performance profiling for:
//! 1. Forward pass latency (single + batch)
//! 2. Token generation throughput and speed-to-first-token (TTFT)
//! 3. Memory allocation patterns and fragmentation
//! 4. FFI boundary overhead analysis
//! 5. Batched operations efficiency
//! 6. Performance regression testing
//!
//! Run with: `cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance`
//! Generate HTML reports: cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --verbose

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_mlx_ffi::{
    backend::MLXFFIBackend,
    mock::{create_mock_adapter, create_mock_config},
    tensor::MLXFFITensor,
    MLXFFIModel,
};

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Creates a backend suitable for benchmarking
fn create_benchmark_backend() -> MLXFFIBackend {
    let config = create_mock_config();
    let model = MLXFFIModel::new_null(config);
    MLXFFIBackend::new(model)
}

/// Creates deterministic random data
fn create_random_input(size: usize) -> Vec<f32> {
    let seed = B3Hash::hash(b"benchmark-seed");
    let seed_bytes = seed.as_bytes();

    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        let idx = i % 32;
        let byte_val = seed_bytes[idx] as f32;
        let value = ((byte_val + i as f32) % 256.0) / 256.0 - 0.5;
        data.push(value);
    }
    data
}

/// Creates a RouterRing for K adapters
fn create_mock_router_ring(k: usize) -> RouterRing {
    let mut ring = RouterRing::new(k);
    let indices: Vec<u16> = (0..k as u16).collect();
    let gate_value = (32767 / k as i16).max(1);
    let gates: Vec<i16> = vec![gate_value; k];
    ring.set(&indices, &gates);
    ring
}

// =============================================================================
// 1. FORWARD PASS LATENCY BENCHMARKS
// =============================================================================

/// Measures end-to-end inference latency for varying input sizes
fn bench_forward_pass_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("forward_pass_latency");
    group.sample_size(50);
    group.measurement_time(std::time::Duration::from_secs(15));

    // Test different sequence lengths (tokens)
    for &seq_len in &[1usize, 4usize, 8usize, 16usize] {
        // Test different vocabulary sizes
        for &vocab_size in &[8_192usize, 32_000usize, 128_000usize] {
            let param = format!("seq{}_{}", seq_len, vocab_size);

            group.throughput(Throughput::Elements(seq_len as u64));
            group.bench_with_input(
                BenchmarkId::new("inference", &param),
                &(seq_len, vocab_size),
                |b, &(seq, vocab)| {
                    let mut backend = create_benchmark_backend();
                    let mut io = IoBuffers::new(vocab);

                    // Create input tokens
                    io.input_ids = (0..seq).map(|i| (42 + i) as u32).collect();

                    // Register 4 adapters
                    for i in 0..4 {
                        let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
                        let _ = backend.register_adapter(i, adapter);
                    }

                    let ring = create_mock_router_ring(4);

                    b.iter(|| {
                        let _ = backend.run_step(&ring, &mut io);
                    });
                },
            );
        }
    }

    group.finish();
}

/// Measures FFI boundary overhead by comparing direct Rust ops vs FFI calls
fn bench_ffi_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("ffi_overhead");
    group.sample_size(50);
    group.measurement_time(std::time::Duration::from_secs(10));

    // Test different data sizes to see scaling
    for &size in &[64usize, 256usize, 1024usize, 4096usize] {
        let param = format!("{}B", size);

        // Benchmark: Rust-side memory allocation
        group.bench_with_input(
            BenchmarkId::new("rust_vec_alloc", &param),
            &size,
            |b, &s| {
                b.iter(|| {
                    let _vec = vec![0.0f32; s];
                });
            },
        );

        // Benchmark: FFI tensor creation (includes Rust + C++ overhead)
        group.bench_with_input(
            BenchmarkId::new("ffi_tensor_create", &param),
            &size,
            |b, &s| {
                let data = create_random_input(s);
                b.iter(|| {
                    let _tensor = MLXFFITensor::from_data(&data, vec![s]);
                });
            },
        );

        // Benchmark: FFI tensor data extraction
        group.bench_with_input(
            BenchmarkId::new("ffi_tensor_extract", &param),
            &size,
            |b, &s| {
                let data = create_random_input(s);
                let tensor = MLXFFITensor::from_data(&data, vec![s]).unwrap();
                b.iter(|| {
                    let _ = tensor.data();
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// 2. TOKEN GENERATION THROUGHPUT
// =============================================================================

/// Measures tokens generated per second and speed-to-first-token (TTFT)
fn bench_generation_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("generation_throughput");
    group.sample_size(20);
    group.measurement_time(std::time::Duration::from_secs(20));

    // Test generating different token counts
    for &max_tokens in &[10usize, 50usize, 100usize] {
        group.throughput(Throughput::Elements(max_tokens as u64));

        group.bench_with_input(
            BenchmarkId::new("token_generation", format!("{}tokens", max_tokens)),
            &max_tokens,
            |b, &tokens| {
                let mut backend = create_benchmark_backend();
                let ring = create_mock_router_ring(2);
                let mut io = IoBuffers::new(32_000);
                io.input_ids = vec![1, 2, 3]; // Prompt

                b.iter(|| {
                    // Simulate token generation loop
                    for _ in 0..tokens {
                        let mut io_copy = IoBuffers::new(io.output_logits.len());
                        io_copy.input_ids = io.input_ids.clone();
                        io_copy.position = io.position;
                        let _ = backend.run_step(&ring, &mut io_copy);
                    }
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// 3. MEMORY ALLOCATION PATTERNS
// =============================================================================

/// Analyzes memory allocation efficiency and fragmentation
fn bench_memory_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_patterns");
    group.sample_size(50);
    group.measurement_time(std::time::Duration::from_secs(15));

    // Test single allocation
    group.bench_function("single_allocation_1mb", |b| {
        b.iter(|| {
            let data = create_random_input(256 * 1024); // 1MB of f32s
            let _tensor = MLXFFITensor::from_data(&data, vec![256 * 1024]);
        });
    });

    // Test repeated allocations (fragmentation simulation)
    group.bench_function("repeated_alloc_100x", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let data = create_random_input(1024);
                let _tensor = MLXFFITensor::from_data(&data, vec![1024]);
                // tensor dropped here
            }
        });
    });

    // Test mixed-size allocations
    group.bench_function("mixed_size_alloc", |b| {
        let sizes = vec![256, 512, 1024, 2048, 4096];
        b.iter(|| {
            for &size in &sizes {
                let data = create_random_input(size);
                let _tensor = MLXFFITensor::from_data(&data, vec![size]);
            }
        });
    });

    // Test memory pool efficiency with adapter loading
    group.bench_function("memory_pool_adapter_lifecycle", |b| {
        b.iter(|| {
            let mut backend = create_benchmark_backend();

            // Load adapter
            let adapter = create_mock_adapter("adapter", 16);
            let _ = backend.register_adapter(0, adapter);

            // Unload adapter
            let _ = backend.unload_adapter_runtime(0);
        });
    });

    group.finish();
}

/// Measures memory allocation latency under different pressure conditions
fn bench_memory_under_pressure(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_under_pressure");
    group.sample_size(30);
    group.measurement_time(std::time::Duration::from_secs(15));

    // Low memory pressure: free allocations immediately
    group.bench_function("low_pressure_alloc", |b| {
        b.iter(|| {
            for _ in 0..10 {
                let data = create_random_input(4096);
                let _tensor = MLXFFITensor::from_data(&data, vec![4096]);
            }
        });
    });

    // High memory pressure: many simultaneous allocations
    group.bench_function("high_pressure_alloc", |b| {
        b.iter(|| {
            let mut tensors = Vec::new();
            for _ in 0..50 {
                let data = create_random_input(4096);
                if let Ok(tensor) = MLXFFITensor::from_data(&data, vec![4096]) {
                    tensors.push(tensor);
                }
            }
            // All freed at once
            std::hint::black_box(tensors);
        });
    });

    group.finish();
}

// =============================================================================
// 4. BATCHED OPERATIONS EFFICIENCY
// =============================================================================

/// Measures efficiency gains from batch processing
fn bench_batch_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_operations");
    group.sample_size(50);
    group.measurement_time(std::time::Duration::from_secs(15));

    // Compare single operation vs batch
    for &size in &[64usize, 256usize, 1024usize] {
        // Single operation (baseline)
        group.bench_with_input(
            BenchmarkId::new("single_matmul", format!("{}x{}", size, size)),
            &size,
            |b, &s| {
                let data1 = create_random_input(s * s);
                let data2 = create_random_input(s * s);
                let t1 = MLXFFITensor::from_data(&data1, vec![s, s]).unwrap();
                let t2 = MLXFFITensor::from_data(&data2, vec![s, s]).unwrap();

                b.iter(|| {
                    let _ = t1.matmul(&t2);
                });
            },
        );

        // Batch of 4 operations
        group.bench_with_input(
            BenchmarkId::new("batch4_matmul", format!("{}x{}", size, size)),
            &size,
            |b, &s| {
                let tensors: Vec<_> = (0..4)
                    .map(|_i| {
                        let d1 = create_random_input(s * s);
                        let d2 = create_random_input(s * s);
                        (
                            MLXFFITensor::from_data(&d1, vec![s, s]).unwrap(),
                            MLXFFITensor::from_data(&d2, vec![s, s]).unwrap(),
                        )
                    })
                    .collect();

                b.iter(|| {
                    for (t1, t2) in &tensors {
                        let _ = t1.matmul(t2);
                    }
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// 5. PERFORMANCE REGRESSION TESTS
// =============================================================================

/// Regression test: latency baseline
///
/// This benchmark establishes the baseline latency for a standard inference step.
/// If this regresses, it indicates performance degradation in the FFI layer.
fn bench_regression_latency_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("regression_latency_baseline");
    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(20));

    group.bench_function("standard_inference_step", |b| {
        let mut backend = create_benchmark_backend();
        let mut io = IoBuffers::new(32_000);
        io.input_ids = vec![42, 100, 200];

        for i in 0..4 {
            let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
            let _ = backend.register_adapter(i, adapter);
        }

        let ring = create_mock_router_ring(4);

        b.iter(|| {
            let _ = backend.run_step(&ring, &mut io);
        });
    });

    group.finish();
}

/// Regression test: memory usage baseline
fn bench_regression_memory_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("regression_memory_baseline");
    group.sample_size(50);

    group.bench_function("baseline_tensor_allocation", |b| {
        b.iter(|| {
            let data = create_random_input(65536); // 256KB
            let _tensor = MLXFFITensor::from_data(&data, vec![256, 256]);
        });
    });

    group.bench_function("baseline_adapter_load", |b| {
        let mut backend = create_benchmark_backend();

        b.iter(|| {
            let adapter = create_mock_adapter("adapter", 16);
            let _ = backend.register_adapter(0, adapter);
        });
    });

    group.finish();
}

/// Regression test: multi-adapter routing efficiency
fn bench_regression_routing_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("regression_routing_efficiency");
    group.sample_size(50);
    group.measurement_time(std::time::Duration::from_secs(15));

    group.bench_function("k4_routing_32_total_adapters", |b| {
        let mut backend = create_benchmark_backend();

        // Load 32 adapters
        for i in 0..32 {
            let adapter = create_mock_adapter(&format!("adapter-{}", i), 8);
            let _ = backend.register_adapter(i as u16, adapter);
        }

        let mut io = IoBuffers::new(32_000);
        io.input_ids = vec![1, 2, 3];

        let ring = create_mock_router_ring(4);

        b.iter(|| {
            let _ = backend.run_step(&ring, &mut io);
        });
    });

    group.finish();
}

// =============================================================================
// 6. FFI BOUNDARY OVERHEAD ANALYSIS
// =============================================================================

/// Isolates FFI boundary overhead from compute
fn bench_ffi_boundary_isolation(c: &mut Criterion) {
    let mut group = c.benchmark_group("ffi_boundary_isolation");
    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(15));

    // Measure pure Rust overhead
    group.bench_function("rust_only_vector_ops", |b| {
        b.iter(|| {
            let mut v1 = create_random_input(1024);
            let v2 = create_random_input(1024);

            // Rust-only element-wise operation
            for (a, b) in v1.iter_mut().zip(v2.iter()) {
                *a += b;
            }
        });
    });

    // Measure FFI call overhead for array creation
    group.bench_function("ffi_array_creation_overhead", |b| {
        let data = create_random_input(1024);
        b.iter(|| {
            let _tensor = MLXFFITensor::from_data(&data, vec![1024]);
        });
    });

    // Measure FFI call overhead for array copying
    group.bench_function("ffi_array_copy_overhead", |b| {
        let data = create_random_input(1024);
        let tensor = MLXFFITensor::from_data(&data, vec![1024]).unwrap();
        b.iter(|| {
            let _copied = tensor.copy();
        });
    });

    group.finish();
}

// =============================================================================
// CRITERION CONFIGURATION
// =============================================================================

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(15));
    targets =
        bench_forward_pass_latency,
        bench_ffi_overhead,
        bench_generation_throughput,
        bench_memory_patterns,
        bench_memory_under_pressure,
        bench_batch_operations,
        bench_regression_latency_baseline,
        bench_regression_memory_baseline,
        bench_regression_routing_efficiency,
        bench_ffi_boundary_isolation
);

criterion_main!(benches);
