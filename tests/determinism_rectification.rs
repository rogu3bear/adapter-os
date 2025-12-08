//! Determinism Rectification Tests
//!
//! These tests verify end-to-end determinism by running the same operations
//! multiple times and asserting identical outputs. Created as part of the
//! P0/P1 rectification effort to ensure claimed determinism actually works.

use adapteros_core::{derive_seed, B3Hash};
use adapteros_lora_router::{Router, RouterDeterminismConfig, RouterWeights};
use adapteros_lora_worker::generation::Generator;

/// Test that Generator produces identical token sequences across multiple runs
/// with the same seed and step-level re-seeding.
#[test]
fn test_generator_determinism_three_runs() {
    let seed = b"rectification-test-seed-32bytes!";
    let logits = vec![1.5, 2.3, 0.8, 3.1, 1.2, 2.0, 0.5, 1.8]; // Varied logits
    let num_steps = 20;
    let num_runs = 3;

    let mut all_sequences: Vec<Vec<u32>> = Vec::new();

    for _run in 0..num_runs {
        let mut gen = Generator::new_deterministic(seed, "inference");
        let mut tokens = Vec::new();

        for step in 0..num_steps {
            gen.reseed_for_step(step);
            let token = gen
                .next_token(&logits)
                .expect("Token generation should succeed");
            tokens.push(token);
        }

        all_sequences.push(tokens);
    }

    // All runs must produce identical sequences
    for i in 1..num_runs {
        assert_eq!(
            all_sequences[0], all_sequences[i],
            "Run {} diverged from baseline run 0:\nBaseline: {:?}\nRun {}: {:?}",
            i, all_sequences[0], i, all_sequences[i]
        );
    }
}

/// Test that Generator with different contexts produces different sequences
/// (domain separation works correctly).
#[test]
fn test_generator_domain_separation() {
    let seed = b"rectification-test-seed-32bytes!";
    let logits = vec![1.0, 1.0, 1.0, 1.0, 1.0]; // Uniform to maximize variance

    let mut gen_inference = Generator::new_deterministic(seed, "inference");
    let mut gen_training = Generator::new_deterministic(seed, "training");

    gen_inference.reseed_for_step(0);
    gen_training.reseed_for_step(0);

    // Collect multiple tokens to ensure statistical difference
    let mut inference_tokens = Vec::new();
    let mut training_tokens = Vec::new();

    for step in 0..10 {
        gen_inference.reseed_for_step(step);
        gen_training.reseed_for_step(step);

        inference_tokens.push(
            gen_inference
                .next_token(&logits)
                .expect("Token generation should succeed"),
        );
        training_tokens.push(
            gen_training
                .next_token(&logits)
                .expect("Token generation should succeed"),
        );
    }

    // Different contexts should produce different sequences
    assert_ne!(
        inference_tokens, training_tokens,
        "Different contexts should produce different sequences (domain separation)"
    );
}

/// Test that HKDF seed derivation is deterministic
#[test]
fn test_hkdf_seed_derivation_determinism() {
    let global_hash = B3Hash::hash(b"manifest-content-for-testing");

    let seed1 = derive_seed(&global_hash, "generation");
    let seed2 = derive_seed(&global_hash, "generation");
    let seed3 = derive_seed(&global_hash, "generation");

    assert_eq!(
        seed1, seed2,
        "HKDF derivation must be deterministic (1 vs 2)"
    );
    assert_eq!(
        seed2, seed3,
        "HKDF derivation must be deterministic (2 vs 3)"
    );

    // Different context should produce different seed
    let seed_router = derive_seed(&global_hash, "router");
    assert_ne!(
        seed1, seed_router,
        "Different contexts should produce different seeds"
    );
}

/// Test that Router produces deterministic decisions with decision hashing
#[test]
fn test_router_determinism_with_decision_hash() {
    let k = 3;
    let tau = 1.0;
    let entropy_floor = 0.02;

    // Create router with determinism enabled
    let mut router1 = Router::new_with_weights(RouterWeights::default(), k, tau, entropy_floor);
    let mut router2 = Router::new_with_weights(RouterWeights::default(), k, tau, entropy_floor);

    // Enable determinism features
    let config = RouterDeterminismConfig {
        ieee754_deterministic: true,
        enable_decision_hashing: true,
    };
    router1.set_determinism_config(config.clone());
    router2.set_determinism_config(config);

    // Same inputs
    let features = vec![0.5, 0.3, 0.8, 0.2, 0.6];
    let priors = vec![0.2, 0.2, 0.2, 0.2, 0.2];
    let adapter_info: Vec<adapteros_lora_router::AdapterInfo> = (0..5)
        .map(|i| adapteros_lora_router::AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![0],
            tier: "persistent".to_string(),
            scope_path: None,
            lora_tier: None,
        })
        .collect();

    // Make routing decisions
    let decision1 = router1.route_with_adapter_info(&features, &priors, &adapter_info);
    let decision2 = router2.route_with_adapter_info(&features, &priors, &adapter_info);

    // Verify identical routing
    assert_eq!(
        decision1.indices.as_slice(),
        decision2.indices.as_slice(),
        "Router indices must be identical"
    );
    assert_eq!(
        decision1.gates_q15.as_slice(),
        decision2.gates_q15.as_slice(),
        "Router Q15 gates must be identical"
    );

    // Verify decision hashes exist and match
    assert!(
        decision1.decision_hash.is_some(),
        "Decision hash should be populated"
    );
    assert!(
        decision2.decision_hash.is_some(),
        "Decision hash should be populated"
    );

    let hash1 = decision1.decision_hash.as_ref().unwrap();
    let hash2 = decision2.decision_hash.as_ref().unwrap();

    assert_eq!(
        hash1.combined_hash, hash2.combined_hash,
        "Decision hashes must be identical for identical inputs"
    );
}

/// Test that the full pipeline (seed derivation → generator → tokens) is deterministic
#[test]
fn test_full_pipeline_determinism() {
    // Simulate manifest hash
    let manifest_hash = B3Hash::hash(b"manifest-v1.0.0-test");

    let num_runs = 3;
    let mut all_results: Vec<Vec<u32>> = Vec::new();

    for _ in 0..num_runs {
        // Derive seed from manifest (as Worker would do)
        let gen_seed = derive_seed(&manifest_hash, "generation");

        // Create generator
        let mut generator = Generator::new(gen_seed);

        // Simulate token generation
        let logits = vec![1.2, 0.8, 2.1, 1.5, 0.9, 1.8, 2.3, 1.1];
        let mut tokens = Vec::new();

        for _ in 0..15 {
            let token = generator
                .next_token(&logits)
                .expect("Token generation should succeed");
            tokens.push(token);
        }

        all_results.push(tokens);
    }

    // Verify all runs are identical
    for i in 1..num_runs {
        assert_eq!(
            all_results[0], all_results[i],
            "Full pipeline run {} diverged from baseline",
            i
        );
    }
}

/// Test that greedy sampling is always deterministic (no randomness)
#[test]
fn test_greedy_always_deterministic() {
    let logits = vec![0.1, 0.5, 0.3, 2.8, 0.2]; // Clear winner at index 3
    let gen = Generator::new([0u8; 32]);

    // Greedy should always pick the highest logit
    for _ in 0..10 {
        let token = gen.greedy(&logits).expect("Greedy should succeed");
        assert_eq!(token, 3, "Greedy must always pick highest logit");
    }
}

/// Test that temperature=0.01 (near-greedy) is effectively deterministic
#[test]
fn test_low_temperature_near_deterministic() {
    let seed = b"low-temp-test-seed-32-bytes!!!!!";
    let logits = vec![1.0, 2.0, 5.0, 1.5, 0.8]; // Clear preference for index 2

    let mut results = Vec::new();

    for _ in 0..5 {
        let mut gen = Generator::new_deterministic(seed, "low_temp").with_temperature(0.01); // Very low temperature
        gen.reseed_for_step(0);

        let token = gen
            .next_token(&logits)
            .expect("Token generation should succeed");
        results.push(token);
    }

    // With very low temperature, should converge to same token
    assert!(
        results.iter().all(|&t| t == results[0]),
        "Low temperature should produce deterministic results: {:?}",
        results
    );
}
