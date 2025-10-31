use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MockKernels, RouterRing};
use adapteros_lora_router::Router;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_kernel_steps(c: &mut Criterion) {
    let mut group = c.benchmark_group("kernel_steps");

    for &vocab in &[8_192usize, 32_000usize] {
        group.bench_with_input(BenchmarkId::from_parameter(vocab), &vocab, |b, &v| {
            // Mock kernels and IO buffers
            let mut kernels = MockKernels::new();
            let mut io = IoBuffers::new(v);
            // Seed input with one token
            io.input_ids.push(42);
            let mut ring = RouterRing::new(4);
            ring.set(&[0, 1, 2, 3], &[10_000, 8_000, 6_000, 4_000]);

            b.iter(|| {
                // Simulate 32 autoregressive steps
                for step in 0..32 {
                    io.position = step;
                    ring.position = step;
                    kernels.run_step(&ring, &mut io).unwrap();
                }
            });
        });
    }

    group.finish();
}

fn bench_router_decision(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_decision");
    // Router with 8 adapters
    let seed = [0u8; 32];
    let mut router = Router::new(vec![1.0; 8], 4, 1.0, 0.01, seed);
    // Simple feature vector and uniform priors
    let features = vec![0.1f32; 64];
    let priors = vec![1.0f32; 8];

    group.bench_function("route_8x64", |b| {
        b.iter(|| {
            let _ = router.route(&features, &priors);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_kernel_steps, bench_router_decision);
criterion_main!(benches);
