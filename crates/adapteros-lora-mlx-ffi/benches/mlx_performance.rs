//! Performance benchmarks for the MLX FFI backend
//!
//! This benchmark suite measures the performance of key operations in the MLX backend:
//! - Inference step latency
//! - LoRA A@B transformation
//! - Multi-adapter routing with K=4
//! - Memory allocation/deallocation
//! - Basic tensor operations (matmul, add)
//!
//! Run with: `cargo bench -p adapteros-lora-mlx-ffi --bench mlx_performance`

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_mlx_ffi::{
    backend::MLXFFIBackend,
    lora::{LoRAAdapter, LoRAConfig},
    mock::{create_mock_adapter, create_mock_config},
    routing::{apply_multi_lora, select_top_k_adapters},
    tensor::MLXFFITensor,
    MLXFFIModel,
};

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Creates a backend suitable for benchmarking
///
/// Returns an MLXFFIBackend with mock configuration for consistent benchmarks.
fn create_benchmark_backend() -> MLXFFIBackend {
    let config = create_mock_config();
    let model = MLXFFIModel::new_null(config);
    MLXFFIBackend::new(model)
}

/// Creates random input data of the specified size
///
/// Uses a deterministic seed for reproducible benchmarks.
fn create_random_input(size: usize) -> Vec<f32> {
    // Use deterministic seeding for reproducible benchmarks
    let seed = B3Hash::hash(b"benchmark-seed");
    let seed_bytes = seed.as_bytes();

    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        // Simple deterministic pseudo-random based on seed and index
        let idx = i % 32;
        let byte_val = seed_bytes[idx] as f32;
        let value = ((byte_val + i as f32) % 256.0) / 256.0 - 0.5;
        data.push(value);
    }
    data
}

/// Creates a RouterRing for K adapters
///
/// Generates a mock router ring with evenly distributed Q15 gates.
fn create_mock_router_ring(k: usize) -> RouterRing {
    let mut ring = RouterRing::new(k);

    // Create indices 0, 1, 2, ..., k-1
    let indices: Vec<u16> = (0..k as u16).collect();

    // Create evenly distributed Q15 gates (sum to ~32767)
    let gate_value = (32767 / k as i16).max(1);
    let gates: Vec<i16> = vec![gate_value; k];

    ring.set(&indices, &gates);
    ring
}

/// Creates mock LoRA adapters for benchmarking
fn create_benchmark_adapters(count: usize, rank: usize) -> Vec<LoRAAdapter> {
    (0..count)
        .map(|i| create_mock_adapter(&format!("bench-adapter-{}", i), rank))
        .collect()
}

// =============================================================================
// BENCHMARKS
// =============================================================================

/// Benchmark a single inference step with RouterRing
fn bench_inference_step(c: &mut Criterion) {
    let mut group = c.benchmark_group("inference_step");

    // Configure for thorough benchmarking
    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(10));

    for &vocab_size in &[8_192usize, 32_000usize] {
        group.bench_with_input(
            BenchmarkId::new("vocab", vocab_size),
            &vocab_size,
            |b, &v| {
                let mut backend = create_benchmark_backend();
                let mut io = IoBuffers::new(v);
                io.input_ids = vec![42, 100, 200, 300]; // Sample token IDs

                // Register some adapters
                for i in 0..4 {
                    let adapter = create_mock_adapter(&format!("adapter-{}", i), 4);
                    let _ = backend.register_adapter(i, adapter);
                }

                let ring = create_mock_router_ring(4);

                b.iter(|| {
                    // Run inference step
                    let _ = backend.run_step(&ring, &mut io);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark a single LoRA A@B transformation
fn bench_lora_transform(c: &mut Criterion) {
    let mut group = c.benchmark_group("lora_transform");

    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(10));

    // Test different ranks
    for &rank in &[4usize, 8usize, 16usize] {
        // Test different hidden dimensions
        for &hidden_dim in &[128usize, 512usize, 1024usize] {
            let param = format!("rank{}_dim{}", rank, hidden_dim);

            group.bench_with_input(
                BenchmarkId::new("transform", &param),
                &(rank, hidden_dim),
                |b, &(r, dim)| {
                    // Create LoRA adapter with proper dimensions
                    let config = LoRAConfig {
                        rank: r,
                        alpha: 16.0,
                        target_modules: vec!["q_proj".to_string()],
                        dropout: 0.0,
                        language_affinities: Vec::new(),
                        framework: None,
                        tier: None,
                    };
                    let mut adapter = LoRAAdapter::new("bench-adapter".to_string(), config);

                    // Create LoRA A matrix: [rank, dim]
                    let lora_a: Vec<Vec<f32>> = (0..r)
                        .map(|_| create_random_input(dim).into_iter().take(dim).collect())
                        .collect();

                    // Create LoRA B matrix: [dim, rank]
                    let lora_b: Vec<Vec<f32>> = (0..dim)
                        .map(|_| create_random_input(r).into_iter().take(r).collect())
                        .collect();

                    adapter.add_module_weights("q_proj", lora_a, lora_b);

                    // Input data
                    let input = create_random_input(dim);
                    let base_output = vec![0.0f32; dim];

                    let adapters = vec![&adapter];
                    let gates = vec![16384u16]; // 0.5 in Q15

                    b.iter(|| {
                        let _ = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output);
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark multi-adapter routing with K=4 adapters
fn bench_multi_adapter_routing(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_adapter_routing");

    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(10));

    // Test different adapter counts (total registered)
    for &total_adapters in &[8usize, 16usize, 32usize] {
        let param = format!("k4_of_{}", total_adapters);

        group.bench_with_input(
            BenchmarkId::new("routing", &param),
            &total_adapters,
            |b, &n| {
                // Create adapters
                let adapters = create_benchmark_adapters(n, 4);
                let adapter_refs: Vec<&LoRAAdapter> = adapters.iter().collect();

                // Create scores for selection (input features not used in this simplified benchmark)
                let scores: Vec<f32> = (0..n)
                    .map(|i| {
                        let base = (i as f32) / (n as f32);
                        base * 0.5 + 0.5 // Normalize to [0.5, 1.0]
                    })
                    .collect();

                b.iter(|| {
                    // Select top-K adapters
                    let selected = select_top_k_adapters(&adapter_refs, &scores, 4);

                    // Apply selected adapters
                    let selected_adapters: Vec<&LoRAAdapter> =
                        selected.iter().map(|&(idx, _)| adapter_refs[idx]).collect();

                    // Create gates from scores
                    let gates: Vec<u16> = selected
                        .iter()
                        .map(|&(_, score)| (score * 32767.0) as u16)
                        .collect();

                    let input = create_random_input(128);
                    let base_output = vec![0.0f32; 128];

                    let _ = apply_multi_lora(
                        &selected_adapters,
                        &gates,
                        "q_proj",
                        &input,
                        &base_output,
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory allocation and deallocation patterns
fn bench_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(10));

    // Test different buffer sizes
    for &size in &[1024usize, 4096usize, 16384usize, 65536usize] {
        let size_kb = size / 1024;
        let param = format!("{}KB", if size_kb > 0 { size_kb } else { 1 });

        group.bench_with_input(BenchmarkId::new("alloc_dealloc", &param), &size, |b, &s| {
            b.iter(|| {
                // Allocate tensor data
                let data = create_random_input(s);

                // Simulate tensor creation (this measures Rust allocation overhead)
                // Real MLX tensor creation would go through FFI
                let tensor_result = MLXFFITensor::from_data(&data, vec![1, s]);

                // Tensor is dropped at end of iteration, measuring deallocation
                let _ = std::hint::black_box(tensor_result);
            });
        });
    }

    // Test repeated allocation/deallocation cycles
    group.bench_function("alloc_cycle_1000", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let data = vec![0.0f32; 1024];
                std::hint::black_box(&data);
                // Data is dropped
            }
        });
    });

    group.finish();
}

/// Benchmark basic tensor operations (matmul, add)
fn bench_tensor_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("tensor_operations");

    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(10));

    // Test matrix sizes
    for &dim in &[64usize, 128usize, 256usize, 512usize] {
        // Benchmark tensor addition
        group.bench_with_input(
            BenchmarkId::new("add", format!("{}x{}", dim, dim)),
            &dim,
            |b, &d| {
                let data1 = create_random_input(d * d);
                let data2 = create_random_input(d * d);

                let tensor1 = MLXFFITensor::from_data(&data1, vec![d, d]).unwrap();
                let tensor2 = MLXFFITensor::from_data(&data2, vec![d, d]).unwrap();

                b.iter(|| {
                    let result = tensor1.add(&tensor2);
                    let _ = std::hint::black_box(result);
                });
            },
        );

        // Benchmark element-wise multiply
        group.bench_with_input(
            BenchmarkId::new("multiply", format!("{}x{}", dim, dim)),
            &dim,
            |b, &d| {
                let data1 = create_random_input(d * d);
                let data2 = create_random_input(d * d);

                let tensor1 = MLXFFITensor::from_data(&data1, vec![d, d]).unwrap();
                let tensor2 = MLXFFITensor::from_data(&data2, vec![d, d]).unwrap();

                b.iter(|| {
                    let result = tensor1.multiply(&tensor2);
                    let _ = std::hint::black_box(result);
                });
            },
        );

        // Benchmark matrix multiplication
        group.bench_with_input(
            BenchmarkId::new("matmul", format!("{}x{}", dim, dim)),
            &dim,
            |b, &d| {
                let data1 = create_random_input(d * d);
                let data2 = create_random_input(d * d);

                let tensor1 = MLXFFITensor::from_data(&data1, vec![d, d]).unwrap();
                let tensor2 = MLXFFITensor::from_data(&data2, vec![d, d]).unwrap();

                b.iter(|| {
                    let result = tensor1.matmul(&tensor2);
                    let _ = std::hint::black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark RouterRing creation and manipulation
fn bench_router_ring(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_ring");

    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(10));

    for &k in &[1usize, 2usize, 4usize, 8usize] {
        group.bench_with_input(BenchmarkId::new("create", k), &k, |b, &k_val| {
            b.iter(|| {
                let ring = create_mock_router_ring(k_val);
                std::hint::black_box(ring);
            });
        });

        group.bench_with_input(BenchmarkId::new("set", k), &k, |b, &k_val| {
            let mut ring = RouterRing::new(k_val);
            let indices: Vec<u16> = (0..k_val as u16).collect();
            let gates: Vec<i16> = vec![4096; k_val];

            b.iter(|| {
                ring.set(&indices, &gates);
            });
        });
    }

    group.finish();
}

/// Benchmark adapter loading and unloading (hot-swap simulation)
fn bench_adapter_hotswap(c: &mut Criterion) {
    let mut group = c.benchmark_group("adapter_hotswap");

    group.sample_size(100);
    group.measurement_time(std::time::Duration::from_secs(10));

    for &rank in &[4usize, 8usize, 16usize] {
        group.bench_with_input(
            BenchmarkId::new("register", format!("rank{}", rank)),
            &rank,
            |b, &r| {
                let backend = create_benchmark_backend();

                b.iter(|| {
                    let adapter = create_mock_adapter("hotswap-adapter", r);
                    let _ = backend.register_adapter(0, adapter);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("unload", format!("rank{}", rank)),
            &rank,
            |b, &r| {
                let backend = create_benchmark_backend();

                b.iter_batched(
                    || {
                        // Setup: register adapter
                        let adapter = create_mock_adapter("hotswap-adapter", r);
                        let _ = backend.register_adapter(0, adapter);
                    },
                    |_| {
                        // Benchmark: unload adapter
                        let _ = backend.unload_adapter_runtime(0);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// =============================================================================
// CRITERION CONFIGURATION
// =============================================================================

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(10));
    targets =
        bench_inference_step,
        bench_lora_transform,
        bench_multi_adapter_routing,
        bench_memory_allocation,
        bench_tensor_operations,
        bench_router_ring,
        bench_adapter_hotswap
);

criterion_main!(benches);
