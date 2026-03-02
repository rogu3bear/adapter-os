//! K-sparse router performance test
//!
//! A simple test harness to measure router performance without full benchmarking framework

use std::time::Instant;

// Router imports
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};

/// Generate test adapter info
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
            languages: vec![i % 8],
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
            adapter_type: None,
            stream_session_id: None,
            base_adapter_id: None,
        })
        .collect()
}

/// Generate test feature vector (22 dimensions)
fn generate_features() -> Vec<f32> {
    vec![
        0.8, 0.1, 0.05, 0.02, 0.01, 0.01, 0.005, 0.005, // Language scores
        0.6, 0.3, 0.1, // Framework scores
        0.5, // Symbol hits
        0.7, // Path tokens
        0.4, 0.3, 0.15, 0.1, 0.03, 0.01, 0.005, 0.005, // Prompt verb scores
        0.45,  // Attention entropy
    ]
}

/// Generate test priors
fn generate_priors(count: usize) -> Vec<f32> {
    let uniform_prior = 1.0 / count as f32;
    vec![uniform_prior; count]
}

fn main() {
    println!("=== K-sparse Router Performance Test ===\n");

    // Test 1: Routing latency by K value
    println!("Test 1: Routing latency by K value (50 adapters)");
    println!("{:-<60}", "");
    let adapter_count = 50;
    let k_values = [1, 3, 5, 8];
    let iterations: u128 = 1000;

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    for k in k_values {
        let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = router
                .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                .expect("routing decision");
        }

        let elapsed = start.elapsed();
        let avg_us = elapsed.as_micros() / iterations;
        println!("K={}: {:.2}μs per routing decision", k, avg_us);
    }

    // Test 2: Routing latency by adapter count
    println!("\nTest 2: Routing latency by adapter count (K=3)");
    println!("{:-<60}", "");
    let adapter_counts = [10, 50, 100];
    let k = 3;

    for adapter_count in adapter_counts {
        let adapter_info = generate_adapter_info(adapter_count);
        let features = generate_features();
        let priors = generate_priors(adapter_count);
        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
        let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
        let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);

        let start = Instant::now();
        for _ in 0..iterations {
            let _ = router
                .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
                .expect("routing decision");
        }

        let elapsed = start.elapsed();
        let avg_us = elapsed.as_micros() / iterations;
        println!(
            "{} adapters: {:.2}μs per routing decision",
            adapter_count, avg_us
        );
    }

    // Test 3: Routing overhead
    println!("\nTest 3: Routing overhead vs inference time");
    println!("{:-<60}", "");
    let adapter_count = 50;
    let k = 3;

    let adapter_info = generate_adapter_info(adapter_count);
    let features = generate_features();
    let priors = generate_priors(adapter_count);
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
    let mut router = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.02);

    // Measure routing time
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
            .expect("routing decision");
    }
    let routing_time = start.elapsed();

    // Simulated inference times
    let simulated_inference_times = [
        ("Fast (50ms)", 50_000u64),
        ("Medium (100ms)", 100_000u64),
        ("Slow (200ms)", 200_000u64),
    ];

    for (name, inference_us) in simulated_inference_times {
        let total_time_us = inference_us + (routing_time.as_micros() / iterations) as u64;
        let overhead_pct = ((routing_time.as_micros() as f64 / iterations as f64)
            / (total_time_us as f64))
            * 100.0;
        println!(
            "{}: {:.3}% overhead (target: <5%, ruleset: <8%)",
            name, overhead_pct
        );
    }

    // Summary
    println!("\n=== Performance Summary ===");
    println!("{:-<60}", "");
    let avg_routing_us = routing_time.as_micros() / iterations;
    println!("Average routing latency: {:.2}μs", avg_routing_us);
    println!(
        "Target: <100μs for typical cases ({})",
        if avg_routing_us < 100 {
            "✓ PASS"
        } else {
            "✗ FAIL"
        }
    );

    // Verify overhead for 100ms inference
    let typical_inference_us = 100_000u64;
    let total_us = typical_inference_us + avg_routing_us as u64;
    let overhead_pct = (avg_routing_us as f64 / total_us as f64) * 100.0;
    println!(
        "Routing overhead (100ms inference): {:.3}% (target: <5%, {})",
        overhead_pct,
        if overhead_pct < 5.0 {
            "✓ PASS"
        } else if overhead_pct < 8.0 {
            "⚠ ACCEPTABLE (ruleset: <8%)"
        } else {
            "✗ FAIL"
        }
    );

    println!("\n=== Detailed Results ===");
    println!("K values tested: {:?}", k_values);
    println!("Adapter counts tested: {:?}", adapter_counts);
    println!("Iterations per test: {}", iterations);
}
