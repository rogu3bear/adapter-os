//! Chat inference pipeline benchmarks
//!
//! Measures the key CPU-bound stages of the chat inference pipeline:
//! - Router scoring (K-sparse selection with policy masks)
//! - Policy mask construction
//! - Chat prompt assembly at representative sizes
//!
//! These benchmarks exercise real code from adapteros-lora-router and
//! adapteros-policy. They do NOT require a running server or GPU.
//!
//! Run:
//! ```bash
//! cargo bench -p adapteros-benchmarks --bench chat_inference
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a deterministic adapter pool of `count` adapters.
fn make_adapters(count: usize) -> Vec<AdapterInfo> {
    (0..count)
        .map(|i| AdapterInfo {
            id: format!("adapter_{i}"),
            stable_id: i as u64,
            framework: match i % 4 {
                0 => Some("django".into()),
                1 => Some("react".into()),
                2 => Some("fastapi".into()),
                _ => None,
            },
            languages: vec![i % 8],
            tier: if i % 2 == 0 {
                "persistent".into()
            } else {
                "ephemeral".into()
            },
            scope_path: Some(format!("/src/module_{}", i % 10)),
            lora_tier: Some(format!("tier_{}", i % 3)),
            base_model: Some("qwen-7b".into()),
            recommended_for_moe: true,
            reasoning_specialties: Vec::new(),
            adapter_type: Some("standard".into()),
            stream_session_id: None,
            base_adapter_id: None,
        })
        .collect()
}

/// 22-dimension feature vector matching the router's expected input shape.
fn make_features() -> Vec<f32> {
    vec![
        0.8, 0.1, 0.05, 0.02, 0.01, 0.01, 0.005, 0.005, // language (8)
        0.6, 0.3, 0.1, // framework (3)
        0.5, // symbol hits
        0.7, // path tokens
        0.4, 0.3, 0.15, 0.1, 0.03, 0.01, 0.005, 0.005, // prompt verb (8)
        0.45,  // attention entropy
    ]
}

fn make_priors(count: usize) -> Vec<f32> {
    vec![1.0 / count as f32; count]
}

/// Build a synthetic chat prompt at approximately `target_chars` length.
fn make_chat_prompt(target_chars: usize) -> String {
    let system = "You are a helpful coding assistant.\n\n";
    let user_prefix = "User: ";
    let assistant_prefix = "\nAssistant: ";
    let turn = "Please explain how LoRA adapters work in the context of fine-tuning large language models for domain-specific tasks. ";

    let mut prompt = String::with_capacity(target_chars + 256);
    prompt.push_str(system);

    while prompt.len() < target_chars {
        prompt.push_str(user_prefix);
        prompt.push_str(turn);
        prompt.push_str(assistant_prefix);
        prompt.push_str("LoRA (Low-Rank Adaptation) works by freezing the pretrained model weights and injecting trainable rank decomposition matrices. ");
    }
    prompt.truncate(target_chars);
    prompt
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Router scoring with varying K (top-K adapter selection).
fn bench_router_scoring_by_k(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat/router_scoring_by_k");
    group.sample_size(200);

    let adapter_count = 50;
    let adapters = make_adapters(adapter_count);
    let features = make_features();
    let priors = make_priors(adapter_count);
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&ids, None);

    for k in [1, 3, 5, 8] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(k), &k, |b, &k_val| {
            let mut router = Router::new_with_weights(RouterWeights::default(), k_val, 1.0, 0.02);
            b.iter(|| {
                black_box(
                    router
                        .route_with_adapter_info(&features, &priors, &adapters, &mask)
                        .expect("route"),
                )
            });
        });
    }
    group.finish();
}

/// Router scoring with varying adapter pool size (K=3 fixed).
fn bench_router_scoring_by_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat/router_scoring_by_pool");
    group.sample_size(200);

    let k = 3;

    for pool_size in [10, 25, 50, 100] {
        let adapters = make_adapters(pool_size);
        let features = make_features();
        let priors = make_priors(pool_size);
        let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
        let mask = PolicyMask::allow_all(&ids, None);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(pool_size),
            &pool_size,
            |b, _| {
                let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);
                b.iter(|| {
                    black_box(
                        router
                            .route_with_adapter_info(&features, &priors, &adapters, &mask)
                            .expect("route"),
                    )
                });
            },
        );
    }
    group.finish();
}

/// Policy mask construction with varying constraint sets.
fn bench_policy_mask_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat/policy_mask_construction");
    group.sample_size(200);

    let pool_size = 50;
    let adapters = make_adapters(pool_size);
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();

    // No restrictions
    group.bench_function("allow_all", |b| {
        b.iter(|| black_box(PolicyMask::allow_all(&ids, None)));
    });

    // Block 50% of adapters
    let blocked: Vec<String> = ids.iter().step_by(2).cloned().collect();
    group.bench_function("block_50pct", |b| {
        b.iter(|| {
            black_box(PolicyMask::build(
                &ids,
                None,
                Some(&blocked),
                None,
                None,
                None,
            ))
        });
    });

    // Allow only tier_0 adapters (~33%)
    let allowed: Vec<String> = adapters
        .iter()
        .filter(|a| a.lora_tier.as_deref() == Some("tier_0"))
        .map(|a| a.id.clone())
        .collect();
    group.bench_function("allow_tier0_only", |b| {
        b.iter(|| {
            black_box(PolicyMask::build(
                &ids,
                Some(&allowed),
                None,
                None,
                None,
                None,
            ))
        });
    });

    group.finish();
}

/// Chat prompt assembly at representative sizes.
fn bench_prompt_assembly(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat/prompt_assembly");
    group.sample_size(100);

    // Approximate char counts for typical prompt sizes
    for (label, chars) in [
        ("512_tokens", 2048),
        ("2k_tokens", 8192),
        ("8k_tokens", 32768),
        ("32k_tokens", 131072),
    ] {
        group.throughput(Throughput::Bytes(chars as u64));
        group.bench_function(label, |b| {
            b.iter(|| black_box(make_chat_prompt(chars)));
        });
    }

    group.finish();
}

/// Combined router + mask: realistic chat request flow.
fn bench_chat_routing_e2e(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat/e2e_routing");
    group.sample_size(100);

    let pool_size = 50;
    let k = 3;

    group.bench_function("route_with_mask", |b| {
        b.iter(|| {
            // Construct adapters and mask (would be cached in real server)
            let adapters = make_adapters(pool_size);
            let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
            let blocked: Vec<String> = ids.iter().step_by(3).cloned().collect();
            let mask = PolicyMask::build(&ids, None, Some(&blocked), None, None, None);
            let features = make_features();
            let priors = make_priors(pool_size);

            let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);
            let decision = router
                .route_with_adapter_info(&features, &priors, &adapters, &mask)
                .expect("route");

            // Access decision fields (simulates response assembly)
            let _indices = decision.indices.clone();
            let _gates = decision.gates_f32();
            let _entropy = decision.entropy;
            black_box(decision)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(2));
    targets =
        bench_router_scoring_by_k,
        bench_router_scoring_by_pool,
        bench_policy_mask_construction,
        bench_prompt_assembly,
        bench_chat_routing_e2e,
);
criterion_main!(benches);
