<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use adapteros_benchmarks::*;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_kernel_mtl::MetalKernels;
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::time::{Duration, Instant};
use futures::future::join_all;

/// Benchmark inference throughput
fn bench_inference_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Initialize Metal kernels
        let mut kernels = MetalKernels::new().unwrap();
        kernels.load(b"dummy_plan").unwrap();

        // Create test data
        let vocab_size = 152064;
        let batch_sizes = [1, 4, 8, 16, 32];

        for &batch_size in &batch_sizes {
            let mut group = c.benchmark_group(format!("inference_throughput_batch_{}", batch_size));
            group.throughput(Throughput::Elements(batch_size as u64));
            group.sample_size(50);

            group.bench_function("metal_inference", |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();

                    for _ in 0..iters {
                        let input_ids = vec![1u32; batch_size as usize];
                        let mut io_buffers = IoBuffers {
                            input_ids: input_ids.clone(),
                            output_logits: vec![0.0f32; vocab_size],
                        };

                        let router_ring = RouterRing::from_slices(&[0, 1], &[16384, 8192]);

                        // For batched inference, we'd need to modify the kernel API
                        // For now, simulate sequential processing
                        for _ in 0..batch_size {
                            let mut single_io = IoBuffers {
                                input_ids: vec![1u32],
                                output_logits: vec![0.0f32; vocab_size],
                            };
                            black_box(kernels.run_step(&router_ring, &mut single_io).unwrap());
                        }
                    }

                    start.elapsed()
                })
            });

            group.finish();
        }
    });
}

/// Benchmark concurrent request processing
fn bench_concurrent_requests(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let concurrent_counts = [1, 4, 8, 16, 32];

        for &concurrency in &concurrent_counts {
            let mut group = c.benchmark_group(format!("concurrent_requests_{}_workers", concurrency));
            group.throughput(Throughput::Elements(concurrency as u64));
            group.sample_size(30);

            group.bench_function("async_request_processing", |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();

                    for _ in 0..iters {
                        let futures = (0..concurrency).map(|_| {
                            async {
                                // Simulate async request processing
                                tokio::time::sleep(Duration::from_micros(100)).await;
                                // Simulate some computation
                                let mut sum = 0u64;
                                for i in 0..1000 {
                                    sum = sum.wrapping_add(i);
                                }
                                black_box(sum)
                            }
                        });

                        black_box(join_all(futures).await);
                    }

                    start.elapsed()
                })
            });

            group.finish();
        }
    });
}

/// Benchmark request queue throughput
fn bench_request_queue(c: &mut Criterion) {
    use tokio::sync::mpsc;

    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let queue_sizes = [1, 10, 100, 1000];

        for &queue_size in &queue_sizes {
            let mut group = c.benchmark_group(format!("request_queue_size_{}", queue_size));
            group.throughput(Throughput::Elements(queue_size as u64));
            group.sample_size(50);

            group.bench_function("async_queue_processing", |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();

                    for _ in 0..iters {
                        async fn process_queue(queue_size: usize) -> u64 {
                            let (tx, mut rx) = mpsc::channel(queue_size);

                            // Producer task
                            let producer = tokio::spawn(async move {
                                for i in 0..queue_size {
                                    tx.send(i as u64).await.unwrap();
                                }
                            });

                            // Consumer task
                            let consumer = tokio::spawn(async move {
                                let mut sum = 0u64;
                                while let Some(val) = rx.recv().await {
                                    sum = sum.wrapping_add(val);
                                    // Simulate processing time
                                    tokio::time::sleep(Duration::from_micros(10)).await;
                                }
                                sum
                            });

                            let (producer_result, consumer_result) = tokio::join!(producer, consumer);
                            producer_result.unwrap();
                            consumer_result.unwrap()
                        }

                        black_box(process_queue(queue_size).await);
                    }

                    start.elapsed()
                })
            });

            group.finish();
        }
    });
}

/// Benchmark adapter routing throughput
fn bench_adapter_routing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let adapter_counts = [1, 4, 8, 16, 32];

        for &num_adapters in &adapter_counts {
            let mut group = c.benchmark_group(format!("adapter_routing_{}_adapters", num_adapters));
            group.throughput(Throughput::Elements(num_adapters as u64));
            group.sample_size(100);

            group.bench_function("router_ring_construction", |b| {
                b.iter(|| {
                    let mut indices = Vec::with_capacity(num_adapters);
                    let mut gates_q15 = Vec::with_capacity(num_adapters);

                    for i in 0..num_adapters {
                        indices.push(i as u16);
                        gates_q15.push((32767 / num_adapters * (i + 1)) as i16); // Distribute gates
                    }

                    let router_ring = RouterRing::from_slices(&indices, &gates_q15);

                    black_box(router_ring);
                })
            });

            group.bench_function("adapter_fusion_computation", |b| {
                b.iter(|| {
                    let mut total_gate_weight = 0.0f32;
                    let mut adapter_contributions = Vec::with_capacity(num_adapters);

                    for i in 0..num_adapters {
                        let gate = (32767 / num_adapters * (i + 1)) as f32 / 32767.0; // Q15 to float
                        total_gate_weight += gate;
                        adapter_contributions.push(gate);
                    }

                    // Simulate fusion computation
                    let base_output = 1.0f32;
                    let fused_output = base_output * total_gate_weight;

                    // Simulate per-adapter contribution
                    for contribution in adapter_contributions {
                        black_box(contribution * base_output);
                    }

                    black_box(fused_output);
                })
            });

            group.finish();
        }
    });
}

/// Benchmark evidence processing throughput
fn bench_evidence_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let evidence_counts = [10, 100, 1000, 10000];

        for &num_evidence in &evidence_counts {
            let mut group = c.benchmark_group(format!("evidence_processing_{}_items", num_evidence));
            group.throughput(Throughput::Elements(num_evidence as u64));
            group.sample_size(50);

            group.bench_function("evidence_validation", |b| {
                b.iter(|| {
                    let mut rng = utils::DeterministicRng::new(42);
                    let mut valid_count = 0;
                    let threshold = 0.5f32;

                    for _ in 0..num_evidence {
                        let score = rng.next_f32();
                        if score > threshold {
                            valid_count += 1;
                        }
                    }

                    black_box(valid_count);
                })
            });

            group.bench_function("evidence_aggregation", |b| {
                b.iter(|| {
                    let mut rng = utils::DeterministicRng::new(42);
                    let mut total_score = 0.0f32;
                    let mut max_score = 0.0f32;
                    let mut evidence_list = Vec::with_capacity(num_evidence);

                    for _ in 0..num_evidence {
                        let score = rng.next_f32();
                        total_score += score;
                        max_score = max_score.max(score);
                        evidence_list.push(score);
                    }

                    let avg_score = total_score / num_evidence as f32;

                    // Simulate confidence computation
                    let confidence = if max_score > 0.8 {
                        0.9
                    } else if avg_score > 0.6 {
                        0.7
                    } else {
                        0.5
                    };

                    black_box((avg_score, max_score, confidence));
                })
            });

            group.finish();
        }
    });
}

/// Benchmark end-to-end request latency
fn bench_end_to_end_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Simulate different request complexities
        let complexities = ["simple", "medium", "complex"];

        for &complexity in &complexities {
            let mut group = c.benchmark_group(format!("end_to_end_latency_{}", complexity));
            group.sample_size(100);

            group.bench_function("request_processing", |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();

                    for _ in 0..iters {
                        async fn process_request(complexity: &str) -> u64 {
                            // Simulate request parsing
                            tokio::time::sleep(Duration::from_micros(50)).await;

                            // Simulate model inference based on complexity
                            let inference_time = match complexity {
                                "simple" => Duration::from_micros(100),
                                "medium" => Duration::from_micros(500),
                                "complex" => Duration::from_micros(2000),
                                _ => Duration::from_micros(100),
                            };
                            tokio::time::sleep(inference_time).await;

                            // Simulate response formatting
                            tokio::time::sleep(Duration::from_micros(25)).await;

                            // Return some result
                            42u64
                        }

                        black_box(process_request(complexity).await);
                    }

                    start.elapsed()
                })
            });

            group.finish();
        }
    });
}

criterion_group!(
    name = throughput_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(20))
        .noise_threshold(0.05);
    targets = bench_inference_throughput, bench_concurrent_requests, bench_request_queue,
             bench_adapter_routing, bench_evidence_processing, bench_end_to_end_latency
);

<<<<<<< HEAD
criterion_main!(throughput_benches);
=======
criterion_main!(throughput_benches);
>>>>>>> integration-branch
