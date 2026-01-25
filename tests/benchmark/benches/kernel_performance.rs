use adapteros_benchmarks::utils;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_kernel_mtl::MetalKernels;
use adapteros_memory::unified_memory::{AllocationRequest, MemoryType, UnifiedMemoryManager};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

/// Benchmark Metal kernel operations
fn bench_metal_kernels(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Initialize Metal kernels
        let mut kernels = MetalKernels::new().unwrap();

        // Create dummy plan bytes for initialization
        let plan_bytes = b"dummy_plan_for_benchmarking";

        // Load kernels with dummy plan
        kernels.load(plan_bytes).unwrap();

        // Create benchmark data
        let mut data_gen = utils::DataGenerator::new(42);
        let input_ids = vec![1u32, 2, 3, 4]; // Sample token IDs
        let vocab_size = 152064; // Qwen2.5-7B vocab size

        let _io_buffers = IoBuffers {
            input_ids: input_ids.clone(),
            output_logits: vec![0.0f32; vocab_size],
            position: 0,
            attention_entropy: None,
            activations: None,
        };

        // Create router ring with sample adapters
        let mut router_ring = RouterRing::new(3);
        router_ring.set(&[0, 1, 2], &[16384, 8192, 4096]); // Q15 format gates

        // Benchmark single inference step
        c.bench_function("metal_kernel_inference_step", |b| {
            b.iter(|| {
                let mut io_copy = IoBuffers {
                    input_ids: input_ids.clone(),
                    output_logits: vec![0.0f32; vocab_size],
                    position: 0,
                    attention_entropy: None,
                    activations: None,
                };
                kernels.run_step(&router_ring, &mut io_copy).unwrap();
                black_box(());
            })
        });

        // Benchmark matrix multiplication operations
        let matrix_a = data_gen.generate_matrix(1024, 1024);
        let matrix_b = data_gen.generate_matrix(1024, 1024);

        c.bench_function("matrix_multiplication_1024x1024", |b| {
            b.iter(|| {
                // Simulate matrix multiplication workload
                let mut result = vec![0.0f32; 1024 * 1024];
                for i in 0..1024 {
                    for j in 0..1024 {
                        for k in 0..1024 {
                            result[i * 1024 + j] += matrix_a[i * 1024 + k] * matrix_b[k * 1024 + j];
                        }
                    }
                }
                black_box(result);
            })
        });

        // Benchmark attention mechanism simulation
        let seq_len = 512;
        let hidden_size = 1024;
        let num_heads = 8;
        let head_dim = hidden_size / num_heads;

        let q = data_gen.generate_matrix(seq_len, hidden_size);
        let k = data_gen.generate_matrix(seq_len, hidden_size);
        let v = data_gen.generate_matrix(seq_len, hidden_size);

        c.bench_function("attention_mechanism_512_seq", |b| {
            b.iter(|| {
                let mut attention_output = vec![0.0f32; seq_len * hidden_size];

                // Simulate multi-head attention
                for head in 0..num_heads {
                    let head_offset = head * head_dim;

                    // QK^T attention scores
                    let mut scores = vec![0.0f32; seq_len * seq_len];
                    for i in 0..seq_len {
                        for j in 0..seq_len {
                            let mut score = 0.0f32;
                            for d in 0..head_dim {
                                let q_idx = i * hidden_size + head_offset + d;
                                let k_idx = j * hidden_size + head_offset + d;
                                score += q[q_idx] * k[k_idx];
                            }
                            scores[i * seq_len + j] = score / (head_dim as f32).sqrt();
                        }
                    }

                    // Softmax and weighted sum
                    for i in 0..seq_len {
                        // Simple softmax approximation
                        let mut max_score = scores[i * seq_len];
                        for j in 1..seq_len {
                            max_score = max_score.max(scores[i * seq_len + j]);
                        }

                        let mut sum_exp = 0.0f32;
                        for j in 0..seq_len {
                            scores[i * seq_len + j] = (scores[i * seq_len + j] - max_score).exp();
                            sum_exp += scores[i * seq_len + j];
                        }

                        for j in 0..seq_len {
                            scores[i * seq_len + j] /= sum_exp;
                        }

                        // Weighted sum with V
                        for d in 0..head_dim {
                            let mut output = 0.0f32;
                            for j in 0..seq_len {
                                let v_idx = j * hidden_size + head_offset + d;
                                output += scores[i * seq_len + j] * v[v_idx];
                            }
                            attention_output[i * hidden_size + head_offset + d] = output;
                        }
                    }
                }

                black_box(attention_output);
            })
        });

        // Benchmark LoRA adapter fusion
        let num_adapters = 8;
        let adapter_dim = 64;
        let mut adapters = Vec::new();

        for i in 0..num_adapters {
            let lora_a = data_gen.generate_matrix(hidden_size, adapter_dim);
            let lora_b = data_gen.generate_matrix(adapter_dim, hidden_size);
            adapters.push((lora_a, lora_b, i as f32 * 0.1)); // gate weight
        }

        c.bench_function("lora_adapter_fusion_8_adapters", |b| {
            b.iter(|| {
                let mut fused_output = vec![0.0f32; hidden_size];

                for (lora_a, lora_b, gate) in &adapters {
                    // Simulate LoRA: output += gate * (input @ lora_a @ lora_b)
                    let mut temp = vec![0.0f32; adapter_dim];
                    for i in 0..adapter_dim {
                        for j in 0..hidden_size {
                            temp[i] += 1.0 * lora_a[j * adapter_dim + i]; // input[j] assumed 1.0
                        }
                    }

                    for i in 0..hidden_size {
                        for j in 0..adapter_dim {
                            fused_output[i] += gate * temp[j] * lora_b[j * hidden_size + i];
                        }
                    }
                }

                black_box(fused_output);
            })
        });
    });
}

/// Benchmark memory operations
fn bench_memory_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Initialize memory manager and pool
        let mut manager = UnifiedMemoryManager::new(1024 * 1024 * 100);
        manager.init_pool("bench", 1024 * 1024 * 100).unwrap();
        let memory_manager = std::rc::Rc::new(manager);

        // Benchmark buffer allocation
        c.bench_function("memory_pool_allocation_1mb", |b| {
            b.iter(|| {
                let request = AllocationRequest {
                    size: 1024 * 1024,
                    backend: "bench".to_string(),
                    alignment: 16,
                    memory_type: MemoryType::GPU,
                };
                let buffer = memory_manager.allocate(request).unwrap();
                black_box(buffer);
            })
        });

        // Benchmark buffer deallocation
        c.bench_function("memory_pool_deallocation_1mb", |b| {
            b.iter(|| {
                let request = AllocationRequest {
                    size: 1024 * 1024,
                    backend: "bench".to_string(),
                    alignment: 16,
                    memory_type: MemoryType::GPU,
                };
                let buffer = memory_manager.allocate(request).unwrap();
                memory_manager.deallocate(&buffer).unwrap();
            })
        });

        // Benchmark memory copying
        let src_data = vec![1.0f32; 1024 * 1024]; // 4MB of floats
        let mut dst_data = vec![0.0f32; 1024 * 1024];

        c.bench_function("memory_copy_4mb", |b| {
            b.iter(|| {
                dst_data.copy_from_slice(&src_data);
                black_box(&dst_data);
            })
        });

        // Benchmark memory zeroing
        c.bench_function("memory_zero_4mb", |b| {
            b.iter(|| {
                dst_data.fill(0.0);
                black_box(&dst_data);
            })
        });
    });
}

/// Benchmark deterministic operations
fn bench_deterministic_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark hash computation for determinism verification
        let data = vec![1u8; 1024 * 1024]; // 1MB of data

        c.bench_function("determinism_hash_1mb", |b| {
            b.iter(|| {
                let hash = adapteros_core::B3Hash::hash(&data);
                black_box(hash);
            })
        });

        // Benchmark deterministic RNG
        let mut rng = utils::DeterministicRng::new(42);

        c.bench_function("deterministic_rng_1m_samples", |b| {
            b.iter(|| {
                let mut sum = 0u32;
                for _ in 0..1_000_000 {
                    sum = sum.wrapping_add(rng.next_u32());
                }
                black_box(sum);
            })
        });

        // Benchmark evidence validation simulation
        let evidence_data = vec![0.5f32; 1000]; // Simulated evidence scores

        c.bench_function("evidence_validation_1000_scores", |b| {
            b.iter(|| {
                let mut valid_count = 0;
                let threshold = 0.3f32;

                for &score in &evidence_data {
                    if score > threshold {
                        valid_count += 1;
                    }
                }

                black_box(valid_count);
            })
        });
    });
}

/// Benchmark CoreML vs Metal comparison
#[cfg(target_os = "macos")]
fn bench_backend_comparison(c: &mut Criterion) {
    use adapteros_lora_kernel_api::MockKernels;
    use adapteros_lora_kernel_mtl::ane_acceleration::{
        ANEAccelerator, ANECalibrationMethod, ANEDataType, ANELoRAConfig, ANEModelConfig,
        ANEQuantization,
    };

    // Metal backend benchmark
    c.bench_function("backend_metal_inference", |b| {
        let rt = Runtime::new().unwrap();

        b.iter(|| {
            rt.block_on(async {
                // Mock Metal inference
                let mut kernels = MockKernels::new();
                kernels.load(b"metal_plan").unwrap();

                let mut ring = RouterRing::new(2);
                ring.set(&[0, 1], &[16384, 16384]);
                let mut io = IoBuffers::new(1024);
                io.input_ids = vec![1, 2, 3, 4];
                io.position = 0;

                kernels.run_step(&ring, &mut io).unwrap();
                black_box(());
            })
        })
    });

    // CoreML/ANE backend benchmark
    c.bench_function("backend_coreml_inference", |b| {
        let accelerator_result = ANEAccelerator::new();

        if let Ok(mut accelerator) = accelerator_result {
            if accelerator.capabilities().available {
                let config = ANEModelConfig {
                    model_id: "bench_model".to_string(),
                    input_dimensions: vec![1, 256],
                    output_dimensions: vec![1, 256],
                    data_type: ANEDataType::Float16,
                    lora_config: ANELoRAConfig {
                        rank: 8,
                        alpha: 16.0,
                        target_modules: vec!["proj".to_string()],
                        quantization: ANEQuantization {
                            enabled: false,
                            bits: 16,
                            calibration_method: ANECalibrationMethod::Static,
                        },
                    },
                };

                let _session_id = accelerator.create_session(config).unwrap();

                b.iter(|| {
                    // Benchmark would execute ANE inference here
                    // For now, just measure overhead
                    black_box(accelerator.performance_metrics());
                })
            } else {
                b.iter(|| {
                    // ANE not available, skip
                    black_box(0);
                })
            }
        }
    });

    // Compare latencies
    c.bench_function("backend_comparison_overhead", |b| {
        let backends = vec!["Metal", "CoreML", "MLX", "Mock"];

        b.iter(|| {
            for backend in &backends {
                // Simulate backend selection overhead
                let _selected = black_box(*backend);
                std::hint::black_box(backend.len());
            }
        })
    });
}

/// Benchmark training operations
#[cfg(target_os = "macos")]
fn bench_training_operations(c: &mut Criterion) {
    // Benchmark gradient computation
    c.bench_function("training_gradient_computation", |b| {
        let weights = vec![0.5f32; 1024];
        let gradients = vec![0.01f32; 1024];
        let learning_rate = 0.001f32;

        b.iter(|| {
            let mut updated_weights = weights.clone();
            for (w, g) in updated_weights.iter_mut().zip(&gradients) {
                *w -= learning_rate * g;
            }
            black_box(updated_weights);
        })
    });

    // Benchmark LoRA weight update
    c.bench_function("training_lora_weight_update", |b| {
        let rank = 16;
        let hidden = 1024;
        let lora_a = vec![0.1f32; rank * hidden];
        let lora_b = vec![0.1f32; hidden * rank];
        let grads_a = vec![0.01f32; rank * hidden];
        let grads_b = vec![0.01f32; hidden * rank];
        let lr = 0.001f32;

        b.iter(|| {
            let mut a_updated = lora_a.clone();
            let mut b_updated = lora_b.clone();

            for (w, g) in a_updated.iter_mut().zip(&grads_a) {
                *w -= lr * g;
            }
            for (w, g) in b_updated.iter_mut().zip(&grads_b) {
                *w -= lr * g;
            }

            black_box((a_updated, b_updated));
        })
    });

    // Benchmark quantization
    c.bench_function("training_int8_quantization", |b| {
        let weights = vec![0.5f32; 4096];

        b.iter(|| {
            let max_val = weights.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            let scale = max_val / 127.0;

            let quantized: Vec<i8> = weights
                .iter()
                .map(|&w| (w / scale).round().clamp(-128.0, 127.0) as i8)
                .collect();

            black_box((quantized, scale));
        })
    });
}

/// Benchmark memory efficiency
#[cfg(target_os = "macos")]
fn bench_memory_efficiency(c: &mut Criterion) {
    use adapteros_memory::unified_memory::{AllocationRequest, MemoryType, UnifiedMemoryManager};

    c.bench_function("memory_allocation_throughput", |b| {
        let mut manager = UnifiedMemoryManager::new(100 * 1024 * 1024);
        manager.init_pool("bench", 80 * 1024 * 1024).unwrap();

        b.iter(|| {
            let request = AllocationRequest {
                size: 1024 * 1024, // 1MB
                backend: "bench".to_string(),
                alignment: 16,
                memory_type: MemoryType::GPU,
            };

            if let Ok(block) = manager.allocate(request) {
                manager.deallocate(&block).unwrap();
            }
        })
    });

    c.bench_function("memory_fragmentation_test", |b| {
        let mut manager = UnifiedMemoryManager::new(100 * 1024 * 1024);
        manager.init_pool("bench", 80 * 1024 * 1024).unwrap();

        b.iter(|| {
            let mut blocks = Vec::new();

            // Allocate 10 blocks
            for _ in 0..10 {
                let request = AllocationRequest {
                    size: 2 * 1024 * 1024,
                    backend: "bench".to_string(),
                    alignment: 16,
                    memory_type: MemoryType::GPU,
                };

                if let Ok(block) = manager.allocate(request) {
                    blocks.push(block);
                }
            }

            // Free alternating blocks
            for (i, block) in blocks.iter().enumerate() {
                if i % 2 == 0 {
                    manager.deallocate(block).unwrap();
                }
            }

            // Cleanup remaining
            for (i, block) in blocks.iter().enumerate() {
                if i % 2 != 0 {
                    manager.deallocate(block).unwrap();
                }
            }

            black_box(());
        })
    });
}

#[cfg(target_os = "macos")]
criterion_group!(
    name = kernel_benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(10))
        .noise_threshold(0.05);
    targets = bench_metal_kernels,
              bench_memory_operations,
              bench_deterministic_operations,
              bench_backend_comparison,
              bench_training_operations,
              bench_memory_efficiency
);

#[cfg(not(target_os = "macos"))]
criterion_group!(
    name = kernel_benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(10))
        .noise_threshold(0.05);
    targets = bench_memory_operations, bench_deterministic_operations
);

criterion_main!(kernel_benches);
