//! K-sparse router performance benchmarks
//!
//! This benchmark suite measures routing performance across various dimensions:
//! - K values (number of top adapters selected): K=1, K=3, K=5, K=8
//! - Adapter pool sizes: 10, 50, 100 adapters
//! - Routing overhead relative to inference time (target: <5%)
//!
//! Performance requirements (from Ruleset #11):
//! - Router overhead ≤ 8% of total inference time
//! - Router decision latency target: < 100μs for typical cases
//! - Per-adapter activation latency p95 < 24ms

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::{Duration, Instant};

// Router imports
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use adapteros_policy::packs::router::RouterConfig;

/// Generate test adapter info for benchmarking
fn generate_adapter_info(count: usize) -> Vec<AdapterInfo> {
    (0..count)
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            stable_id: i as u64,
            framework: if i % 3 == 0 {
                Some("django".to_string())
            } else if i % 3 == 1 {
                Some("react".to_string())
            } else {
                None
            },
            languages: vec![i % 8], // Cycle through 8 languages
            tier: if i % 2 == 0 {
                "persistent".to_string()
            } else {
                "ephemeral".to_string()
            },
            scope_path: Some(format!("/src/module_{}", i % 10)),
            lora_tier: Some(format!("tier_{}", i % 3)),
            base_model: Some("qwen-7b".to_string()),
            version_weight: 1.0,
            recommended_for_moe: true,
            reasoning_specialties: Vec::new(),
            adapter_type: Some("standard".to_string()),
            stream_session_id: None,
            base_adapter_id: None,
        })
        .collect()
}

/// Generate test feature vector (22 dimensions)
fn generate_features() -> Vec<f32> {
    vec![
        0.8, 0.1, 0.05, 0.02, 0.01, 0.01, 0.005, 0.005, // Language scores (8)
        0.6, 0.3, 0.1, // Framework scores (3)
        0.5, // Symbol hits (1)
        0.7, // Path tokens (1)
        0.4, 0.3, 0.15, 0.1, 0.03, 0.01, 0.005, 0.005, // Prompt verb scores (8)
        0.45,  // Attention entropy (1)
    ]
}

/// Generate test priors (uniform distribution)
fn generate_priors(count: usize) -> Vec<f32> {
    let uniform_prior = 1.0 / count as f32;
    vec![uniform_prior; count]
}

/// Benchmark routing latency for different K values
///
/// Tests K=1, K=3, K=5, K=8 with a fixed pool of 50 adapters
fn bench_routing_latency_by_k(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_latency_by_k");
    group.sample_size(100);

    let adapter_count = 50;
    let k_values = [1, 3, 5, 8];

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    for k in k_values {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(k), &k, |b, &k_val| {
            let mut router = Router::new_with_weights(RouterWeights::default(), k_val, 1.0, 0.02);

            b.iter(|| {
                let decision = router
                    .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                    .expect("routing decision");
                black_box(decision)
            });
        });
    }

    group.finish();
}

/// Benchmark routing latency for different adapter pool sizes
///
/// Tests 10, 50, 100 adapters with K=3
fn bench_routing_latency_by_adapter_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_latency_by_adapter_count");
    group.sample_size(100);

    let adapter_counts = [10, 50, 100];
    let k = 3;

    for adapter_count in adapter_counts {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(adapter_count),
            &adapter_count,
            |b, &count| {
                let adapter_info = generate_adapter_info(count);
                let features = generate_features();
                let priors = generate_priors(count);
                let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
                let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
                let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);

                b.iter(|| {
                    let decision = router
                        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                        .expect("routing decision");
                    black_box(decision)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark routing overhead as percentage of inference time
///
/// Simulates inference time and measures routing overhead to verify <5% target
fn bench_routing_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_overhead");
    group.sample_size(50);

    let adapter_count = 50;
    let k = 3;

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    // Simulated inference times (typical range: 50-200ms for a forward pass)
    let simulated_inference_times = [
        ("fast_50ms", Duration::from_millis(50)),
        ("medium_100ms", Duration::from_millis(100)),
        ("slow_200ms", Duration::from_millis(200)),
    ];

    for (name, inference_time) in simulated_inference_times {
        group.bench_function(name, |b| {
            let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);

            b.iter_custom(|iters| {
                let mut total_routing_time = Duration::ZERO;
                let mut total_inference_time = Duration::ZERO;

                for _ in 0..iters {
                    // Measure routing time
                    let routing_start = Instant::now();
                    let decision = router
                        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                        .expect("routing decision");
                    black_box(decision);
                    let routing_elapsed = routing_start.elapsed();

                    // Simulate inference time
                    std::thread::sleep(inference_time);

                    total_routing_time += routing_elapsed;
                    total_inference_time += inference_time + routing_elapsed;
                }

                // Calculate overhead percentage
                let overhead_pct =
                    (total_routing_time.as_secs_f64() / total_inference_time.as_secs_f64()) * 100.0;

                eprintln!(
                    "Routing overhead for {}: {:.2}% (target: <5%, ruleset: <8%)",
                    name, overhead_pct
                );

                total_routing_time
            });
        });
    }

    group.finish();
}

/// Benchmark router decision with policy constraints
///
/// Tests routing performance when policy masks filter adapters
fn bench_routing_with_policy_mask(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_with_policy_mask");
    group.sample_size(100);

    let adapter_count = 50;
    let k = 3;

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();

    // Test different policy scenarios
    let scenarios = [
        ("no_restrictions", PolicyMask::allow_all(&adapter_ids, None)),
        ("block_50_percent", {
            let blocked_ids: Vec<String> = (0..adapter_count)
                .step_by(2)
                .map(|i| adapter_ids[i].clone())
                .collect();
            PolicyMask::build(&adapter_ids, None, Some(&blocked_ids), None, None, None)
        }),
        ("allow_only_tier_0", {
            let allowed_ids: Vec<String> = adapter_info
                .iter()
                .filter(|info| info.lora_tier.as_deref() == Some("tier_0"))
                .map(|info| info.id.clone())
                .collect();
            PolicyMask::build(&adapter_ids, Some(&allowed_ids), None, None, None, None)
        }),
    ];

    for (scenario_name, policy_mask) in scenarios {
        group.bench_function(scenario_name, |b| {
            let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);

            b.iter(|| {
                let decision = router
                    .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                    .expect("routing decision");
                black_box(decision)
            });
        });
    }

    group.finish();
}

/// Benchmark deterministic vs adaptive routing modes
fn bench_routing_determinism_modes(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_determinism_modes");
    group.sample_size(100);

    let adapter_count = 50;
    let k = 3;

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    // Deterministic mode (default)
    group.bench_function("deterministic_mode", |b| {
        let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);
        router.set_routing_determinism_mode(false);

        b.iter(|| {
            let decision = router
                .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                .expect("routing decision");
            black_box(decision)
        });
    });

    // Note: Adaptive mode requires DeterminismContext, which we can't easily create in benchmarks
    // without adding more dependencies. We'll skip that for now.

    group.finish();
}

/// Benchmark router performance with different entropy floors
fn bench_routing_entropy_floors(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_entropy_floors");
    group.sample_size(100);

    let adapter_count = 50;
    let k = 3;

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    let entropy_floors = [
        ("eps_0.01", 0.01),
        ("eps_0.02", 0.02),
        ("eps_0.05", 0.05),
        ("eps_0.10", 0.10),
    ];

    for (name, eps) in entropy_floors {
        group.bench_function(name, |b| {
            let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, eps);

            b.iter(|| {
                let decision = router
                    .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                    .expect("routing decision");
                black_box(decision)
            });
        });
    }

    group.finish();
}

/// Benchmark router performance with policy config
fn bench_routing_with_policy_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_with_policy_config");
    group.sample_size(100);

    let adapter_count = 50;
    let k = 3;

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    // Create a router with policy config
    group.bench_function("policy_config_router", |b| {
        let policy_config = RouterConfig::default();
        let mut router =
            Router::new_with_policy_config(RouterWeights::default(), k, 1.0, &policy_config);

        b.iter(|| {
            let decision = router
                .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                .expect("routing decision");
            black_box(decision)
        });
    });

    group.finish();
}

/// End-to-end routing benchmark with full decision pipeline
fn bench_e2e_routing_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_routing_pipeline");
    group.sample_size(50);

    let adapter_count = 50;
    let k = 3;

    group.bench_function("full_pipeline", |b| {
        b.iter_custom(|iters| {
            let mut total_time = Duration::ZERO;

            for _ in 0..iters {
                let start = Instant::now();

                // Setup (typically done once, but included for completeness)
                let adapter_info = generate_adapter_info(adapter_count);
                let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
                let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
                let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);

                // Feature extraction (simulated)
                let features = generate_features();

                // Prior computation (simulated)
                let priors = generate_priors(adapter_count);

                // Routing decision
                let decision = router
                    .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                    .expect("routing decision");

                // Decision processing
                let _indices = decision.indices.clone();
                let _gates = decision.gates_f32();
                let _entropy = decision.entropy;

                black_box(decision);

                total_time += start.elapsed();
            }

            total_time
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_routing_latency_by_k,
    bench_routing_latency_by_adapter_count,
    bench_routing_overhead,
    bench_routing_with_policy_mask,
    bench_routing_determinism_modes,
    bench_routing_entropy_floors,
    bench_routing_with_policy_config,
    bench_e2e_routing_pipeline,
);
criterion_main!(benches);
