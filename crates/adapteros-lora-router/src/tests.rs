#![allow(clippy::field_reassign_with_default)]

use super::*;
use crate::policy_mask::PolicyMask;
use adapteros_core::determinism::{DeterminismContext, DeterminismSource};
use adapteros_core::seed::{derive_seed_full, hash_adapter_dir};
use adapteros_core::B3Hash;
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use rand::SeedableRng;
use smallvec::SmallVec;
use std::path::Path;

fn mask_all(adapters: &[AdapterInfo]) -> PolicyMask {
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    PolicyMask::allow_all(&ids, None)
}

fn pivot_from_seed(seed: [u8; 32], len: usize) -> usize {
    let prefix = u32::from_le_bytes([seed[0], seed[1], seed[2], seed[3]]);
    (prefix as usize) % len.max(1)
}

fn seeded_priors(seed: [u8; 32], len: usize) -> Vec<f32> {
    let pivot = pivot_from_seed(seed, len);
    let mut priors = Vec::with_capacity(len);

    for i in 0..len {
        let byte = seed[(i * 3) % seed.len()] as f32;
        priors.push(0.05 + byte / 255.0);
    }

    if len > 0 {
        priors[pivot] += 1.0; // Strong, deterministic bias driven by seed
    }

    priors
}

fn adaptive_order_from_ctx(ctx: &DeterminismContext, adapter_info: &[AdapterInfo]) -> Vec<u16> {
    let mut rng = ChaCha20Rng::from_seed(ctx.router_tiebreak_seed());
    let tie_breakers: Vec<u64> = (0..adapter_info.len()).map(|_| rng.gen()).collect();

    let mut indices: Vec<usize> = (0..adapter_info.len()).collect();
    indices.sort_by(|a, b| {
        tie_breakers[*a]
            .cmp(&tie_breakers[*b])
            .then(adapter_info[*a].stable_id.cmp(&adapter_info[*b].stable_id))
    });

    indices.into_iter().map(|i| i as u16).collect()
}

#[test]
fn test_router_topk() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    let features = vec![0.5; 10];
    let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7, 0.2, 0.8, 0.4, 0.6, 0.0];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    assert_eq!(decision.indices.len(), 3);
    assert_eq!(decision.gates_q15.len(), 3);

    // Gates should sum to approximately 1.0
    let sum: f32 = decision.gates_f32().iter().sum();
    assert!((sum - 1.0).abs() < 0.01);
}

#[test]
fn test_route_on_reasoning_prefers_specialty_match() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    let priors = vec![0.4f32, 0.4f32];
    let adapter_info = vec![
        AdapterInfo {
            id: "creative-writer".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["creative".to_string()],
            ..Default::default()
        },
        AdapterInfo {
            id: "python-coder".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["python".to_string(), "logic".to_string()],
            ..Default::default()
        },
    ];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    let decision = router
        .route_on_reasoning(
            "Let's write the python utility now.",
            &priors,
            &adapter_info,
            &policy_mask,
            None,
        )
        .expect("reasoning route");

    assert_eq!(
        decision.indices.first().copied(),
        Some(1),
        "python-coder should be selected when rationale mentions python"
    );
    assert!(decision
        .decision_hash
        .as_ref()
        .and_then(|h| h.reasoning_hash.as_ref())
        .is_some());
}

#[test]
fn test_route_on_reasoning_coreml_specialty() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    let priors = vec![0.5f32, 0.5f32];
    let adapter_info = vec![
        AdapterInfo {
            id: "coreml-reasoner".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["coreml".to_string(), "reasoning".to_string()],
            ..Default::default()
        },
        AdapterInfo {
            id: "general-writer".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["creative".to_string()],
            ..Default::default()
        },
    ];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    let decision = router
        .route_on_reasoning(
            "Let's use the CoreML reasoning path.",
            &priors,
            &adapter_info,
            &policy_mask,
            None,
        )
        .expect("reasoning route");

    assert_eq!(
        decision.indices.first().copied(),
        Some(0),
        "coreml-reasoner should be selected when rationale mentions coreml reasoning"
    );
}

#[test]
fn test_route_on_reasoning_returns_single_adapter_index() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    let priors = vec![0.55f32, 0.55f32];
    let adapter_info = vec![
        AdapterInfo {
            id: "creative-writer".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["creative".to_string()],
            ..Default::default()
        },
        AdapterInfo {
            id: "python-coder".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["python".to_string()],
            ..Default::default()
        },
    ];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    let decision = router
        .route_on_reasoning(
            "Now implement the python solution.",
            &priors,
            &adapter_info,
            &policy_mask,
            None,
        )
        .expect("reasoning route");

    assert_eq!(decision.indices.len(), 1);
}

#[test]
fn test_reasoning_swap_flow() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    let priors = vec![0.6f32, 0.6f32];
    let adapter_info = vec![
        AdapterInfo {
            id: "creative-writer".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["creative".to_string()],
            ..Default::default()
        },
        AdapterInfo {
            id: "python-coder".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["python".to_string()],
            ..Default::default()
        },
    ];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    let prompt_features = CodeFeatures::from_context("Tell me a creative story").to_vector();
    let initial = router
        .route_with_adapter_info(&prompt_features, &priors, &adapter_info, &policy_mask)
        .expect("initial decision");
    assert_eq!(
        initial.indices.first().copied(),
        Some(0),
        "creative adapter should win initial routing"
    );

    let swap = router
        .route_on_reasoning(
            "<thinking>I should now write python code.</thinking>",
            &priors,
            &adapter_info,
            &policy_mask,
            None,
        )
        .expect("reasoning swap");

    assert_eq!(
        swap.indices.first().copied(),
        Some(1),
        "python adapter should be selected after reasoning swap"
    );
}

#[test]
fn test_entropy_floor() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.1);

    let features = vec![0.0; 5];
    let priors = vec![1.0, 0.0, 0.0, 0.0, 0.0]; // One dominant prior
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");
    let gates = decision.gates_f32();

    // All gates should be >= entropy floor / k
    let min_gate = 0.1 / 3.0;
    for &g in &gates {
        assert!(g >= min_gate - 0.001);
    }
}

#[test]
fn test_route_with_code_features() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    let code_features = CodeFeatures::from_context("Fix this python bug in django app");

    let adapters = vec![
        AdapterInfo {
            id: "python-general".to_string(),
            framework: None,
            languages: vec![0], // Python index
            tier: "persistent".to_string(),
            ..Default::default()
        },
        AdapterInfo {
            id: "django-specific".to_string(),
            framework: Some("django".to_string()),
            languages: vec![0], // Python index
            tier: "persistent".to_string(),
            ..Default::default()
        },
        AdapterInfo {
            id: "rust-general".to_string(),
            framework: None,
            languages: vec![1], // Rust index
            tier: "persistent".to_string(),
            ..Default::default()
        },
    ];

    let decision = router
        .route_with_code_features(&code_features, &adapters)
        .expect("router decision");

    assert_eq!(decision.indices.len(), 3);

    // Django adapter should likely be selected due to framework prior
    // (though exact ordering depends on weights)
    tracing::debug!("Selected indices: {:?}", decision.indices);
    tracing::debug!("Gates: {:?}", decision.gates_f32());
}

#[test]
fn test_weighted_scoring_influences_selection() {
    // Create two different weight configurations
    let heavy_language_weights = RouterWeights::new(
        0.8,  // language - very high
        0.05, // framework
        0.05, // symbols
        0.05, // paths
        0.05, // verb
    );

    let heavy_framework_weights = RouterWeights::new(
        0.05, // language
        0.8,  // framework - very high
        0.05, // symbols
        0.05, // paths
        0.05, // verb
    );

    let router1 = Router::new_with_weights(heavy_language_weights, 3, 1.0, 0.02);
    let router2 = Router::new_with_weights(heavy_framework_weights, 3, 1.0, 0.02);

    // Create code features with strong Python + Django signal
    let features = CodeFeatures::from_context("def main(): # Python Django application");
    let feature_vec = features.to_vector();

    // Get scoring explanations
    let explanation1 = router1.explain_score(&feature_vec);
    let explanation2 = router2.explain_score(&feature_vec);

    // With heavy language weights, language score should dominate
    assert!(
        explanation1.language_score > explanation1.framework_score,
        "Language-weighted router should prioritize language score"
    );

    tracing::debug!("Heavy language weights: {}", explanation1.format());
    tracing::debug!("Heavy framework weights: {}", explanation2.format());

    // Language contribution should be much higher in router1
    assert!(
        explanation1.language_score > explanation2.language_score * 2.0,
        "Language score should be much higher with language-heavy weights: {} vs {}",
        explanation1.language_score,
        explanation2.language_score
    );

    // Framework contribution should be much higher in router2
    assert!(
        explanation2.framework_score > explanation1.framework_score * 2.0,
        "Framework score should be much higher with framework-heavy weights: {} vs {}",
        explanation2.framework_score,
        explanation1.framework_score
    );
}

#[test]
fn test_feature_score_components() {
    let weights = RouterWeights::default();
    let router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    // Create features with known components
    let features = CodeFeatures::from_context("Fix the bug in this Python function");
    let feature_vec = features.to_vector();

    let explanation = router.explain_score(&feature_vec);

    // Total should equal sum of components
    let sum = explanation.language_score
        + explanation.framework_score
        + explanation.symbol_hits_score
        + explanation.path_tokens_score
        + explanation.prompt_verb_score;

    assert!(
        (sum - explanation.total_score).abs() < 0.001,
        "Total score should equal sum of components"
    );
}

#[test]
fn test_default_weights_sum_to_one() {
    let weights = RouterWeights::default();
    let total = weights.total_weight();

    assert!(
        (total - 1.0).abs() < 0.001,
        "Default weights should sum to 1.0, got {}",
        total
    );
}

#[test]
fn test_router_with_policy_config_entropy_floor() {
    // Create a policy config with custom entropy floor
    let policy_config = adapteros_policy::packs::router::RouterConfig {
        entropy_floor: 0.05, // Custom entropy floor
        ..Default::default()
    };

    let router = Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

    // Verify that the entropy floor is read from policy config
    assert_eq!(
        router.entropy_floor(),
        0.05,
        "Entropy floor should match policy config"
    );
}

#[test]
fn test_router_with_policy_config_sample_tokens() {
    // Create a policy config with custom sample tokens
    let policy_config = adapteros_policy::packs::router::RouterConfig {
        sample_tokens_full: 256, // Custom sample tokens
        ..Default::default()
    };

    let router = Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

    // Verify that sample tokens is read from policy config
    assert_eq!(
        router.full_log_tokens(),
        256,
        "Full log tokens should match policy config"
    );
}

#[test]
fn test_router_with_policy_config_k_sparse_clamping() {
    // Create a policy config with k_sparse limit
    let policy_config = adapteros_policy::packs::router::RouterConfig {
        k_sparse: 4, // Limit K to 4
        ..Default::default()
    };

    // Try to create router with k=6 (exceeds policy limit)
    let router = Router::new_with_policy_config(RouterWeights::default(), 6, 1.0, &policy_config);

    // Verify that k is clamped to policy maximum
    assert_eq!(router.top_k(), 4, "K should be clamped to policy maximum");
}

#[test]
fn test_entropy_floor_enforcement_with_policy_config() {
    let policy_config = adapteros_policy::packs::router::RouterConfig {
        entropy_floor: 0.15, // Higher entropy floor
        ..Default::default()
    };

    let mut router =
        Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

    let features = vec![0.0; 5];
    let priors = vec![1.0, 0.0, 0.0, 0.0, 0.0]; // One dominant prior
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");
    let gates = decision.gates_f32();

    // All gates should be >= entropy floor / k
    let min_gate = 0.15 / 3.0;
    for &g in &gates {
        assert!(
            g >= min_gate - 0.001,
            "Gate {} should be >= minimum {} required by policy",
            g,
            min_gate
        );
    }
}

#[test]
fn test_policy_config_different_entropy_floors() {
    // Create two routers with different policy configs
    let mut policy_low = adapteros_policy::packs::router::RouterConfig::default();
    policy_low.entropy_floor = 0.01;

    let mut policy_high = adapteros_policy::packs::router::RouterConfig::default();
    policy_high.entropy_floor = 0.20;

    let mut router_low =
        Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_low);

    let mut router_high =
        Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_high);

    // Same input
    let features = vec![0.1; 5];
    let priors = vec![0.9, 0.05, 0.03, 0.02, 0.0];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision_low = router_low
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision_high = router_high
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    let gates_low = decision_low.gates_f32();
    let gates_high = decision_high.gates_f32();

    // Minimum gate in high entropy floor config should be larger
    let actual_min_low = gates_low.iter().fold(f32::MAX, |a, &b| a.min(b));
    let actual_min_high = gates_high.iter().fold(f32::MAX, |a, &b| a.min(b));

    assert!(
        actual_min_high >= actual_min_low - 0.001,
        "Higher entropy floor should result in higher minimum gate: {} vs {}",
        actual_min_high,
        actual_min_low
    );
}

#[test]
fn test_tau_is_sanitized() {
    let router = Router::new_with_weights(RouterWeights::default(), 2, 0.0, 0.02);
    assert!(
        router.tau() > 0.0,
        "Tau must be sanitized to a positive value"
    );
    let mut router_policy = Router::new_with_policy_config(
        RouterWeights::default(),
        2,
        f32::NAN,
        &adapteros_policy::packs::router::RouterConfig::default(),
    );
    // Route to ensure sanitized tau is used
    let features = vec![0.0f32; 4];
    let priors = vec![0.5f32, 0.5f32];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            ..Default::default()
        })
        .collect();
    let mask = mask_all(&adapter_info);
    let decision = router_policy
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing should sanitize tau");
    assert_eq!(decision.indices.len(), 2);
}

#[test]
fn route_with_adapter_info_rejects_non_finite_priors() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    let features = vec![0.0f32; 3];
    let priors = vec![0.5f32, f32::NAN];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            ..Default::default()
        })
        .collect();
    let mask = mask_all(&adapter_info);
    let err = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect_err("non-finite priors should be rejected");
    assert!(
        format!("{}", err).contains("Non-finite router prior"),
        "Error should mention non-finite router priors, got {}",
        err
    );
}

#[test]
fn test_deterministic_softmax_reproducibility() {
    // Test that deterministic softmax produces identical results across multiple runs
    let scores = vec![(0, 0.9f32), (1, 0.5f32), (2, 0.3f32), (3, 0.1f32)];
    let tau = 1.0f32;

    // Run deterministic softmax multiple times
    let result1 = Router::deterministic_softmax(&scores, tau);
    let result2 = Router::deterministic_softmax(&scores, tau);
    let result3 = Router::deterministic_softmax(&scores, tau);

    // All results should be identical
    assert_eq!(result1.len(), result2.len());
    assert_eq!(result2.len(), result3.len());

    for i in 0..result1.len() {
        assert_eq!(
            result1[i], result2[i],
            "Deterministic softmax should produce identical results (run 1 vs 2)"
        );
        assert_eq!(
            result2[i], result3[i],
            "Deterministic softmax should produce identical results (run 2 vs 3)"
        );
    }

    // Results should sum to approximately 1.0
    let sum: f32 = result1.iter().sum();
    assert!((sum - 1.0).abs() < 0.0001, "Softmax should sum to 1.0");
}

#[test]
fn test_route_gates_follow_deterministic_softmax_path() {
    let priors = vec![0.3f32, 0.2f32, 0.1f32];
    let features = vec![0.0f32; 22];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mut router = Router::new_with_weights(RouterWeights::default(), priors.len(), 1.0, 0.01);
    let mask = mask_all(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    // Recreate the router's top-k ordering
    let mut scores: Vec<(usize, f32)> = priors.iter().enumerate().map(|(i, &p)| (i, p)).collect();
    scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    let top_k: Vec<(usize, f32)> = scores.into_iter().take(priors.len()).collect();

    // Compute expected gates using deterministic softmax + entropy floor + renorm
    let mut expected_gates = Router::deterministic_softmax(&top_k, 1.0);
    let eps = 0.01;
    let min_gate = eps / top_k.len().max(1) as f32;
    for g in expected_gates.iter_mut() {
        *g = g.max(min_gate);
    }
    let sum_expected: f32 = expected_gates.iter().sum();
    for g in expected_gates.iter_mut() {
        *g /= sum_expected;
    }
    let expected_q15: Vec<i16> = expected_gates.iter().map(|&g| quantize_gate(g)).collect();

    assert_eq!(
        decision.gates_q15.as_slice(),
        expected_q15.as_slice(),
        "Route should use deterministic f64 softmax prior to Q15 quantization"
    );
}

#[test]
fn test_seeded_routing_reproducible_for_same_inputs() {
    let global = B3Hash::hash(b"global-router-seed");
    let manifest = B3Hash::hash(b"manifest-router-a");
    let adapter_dir_hash = hash_adapter_dir(Path::new("/adapters/seeded/a"));

    // Same seed context should yield identical priors and routing outcomes
    let seed = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 0);
    let priors = seeded_priors(seed, 4);

    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let features = vec![0.0f32; 3];

    let mut router_run1 = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    let mut router_run2 = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);

    let mask = mask_all(&adapter_info);
    let decision1 = router_run1
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision2 = router_run2
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    assert_eq!(
        decision1.indices, decision2.indices,
        "Same seed-derived priors must yield identical adapter choices"
    );
    assert_eq!(
        decision1.gates_q15, decision2.gates_q15,
        "Same seed-derived priors must yield identical quantized gates"
    );
}

#[test]
fn test_seed_changes_can_shift_routing() {
    let global = B3Hash::hash(b"global-router-seed");
    let manifest = B3Hash::hash(b"manifest-router-a");
    let adapter_dir_hash = hash_adapter_dir(Path::new("/adapters/seeded/a"));
    let adapter_count = 4usize;

    // Different nonces give us different seeds; fall back to a third if the pivot collides
    let seed_a = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 1);
    let mut seed_b = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 2);
    let pivot_a = pivot_from_seed(seed_a, adapter_count);
    let mut pivot_b = pivot_from_seed(seed_b, adapter_count);
    if pivot_a == pivot_b {
        seed_b = derive_seed_full(&global, &manifest, &adapter_dir_hash, 7, "router", 3);
        pivot_b = pivot_from_seed(seed_b, adapter_count);
    }
    assert_ne!(
        pivot_a, pivot_b,
        "Different seed contexts should map to different adapter pivots"
    );

    let priors_a = seeded_priors(seed_a, adapter_count);
    let priors_b = seeded_priors(seed_b, adapter_count);
    assert_ne!(
        priors_a, priors_b,
        "Different seeds must produce different priors for routing"
    );

    let adapter_info: Vec<AdapterInfo> = (0..adapter_count)
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();
    let features = vec![0.0f32; 3];

    let mut router_a = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    let mut router_b = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);

    let mask = mask_all(&adapter_info);
    let decision_a = router_a
        .route_with_adapter_info(&features, &priors_a, &adapter_info, &mask)
        .expect("router decision");
    let decision_b = router_b
        .route_with_adapter_info(&features, &priors_b, &adapter_info, &mask)
        .expect("router decision");

    assert!(
        decision_a.indices != decision_b.indices || decision_a.gates_q15 != decision_b.gates_q15,
        "Different seed contexts should change routing choices or gates when priors differ"
    );
}

#[test]
fn adaptive_routing_uses_context_for_tie_breakers() {
    let mut router_a = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    let mut router_b = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    router_a.set_routing_determinism_mode(true);
    router_b.set_routing_determinism_mode(true);

    let determinism_ctx = DeterminismContext::new(
        [1u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        adapteros_types::adapters::metadata::RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );

    let features = vec![0.0f32; 4];
    let priors = vec![0.5f32; 4];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let policy_mask = mask_all(&adapter_info);

    let decision_a = router_a
        .route_with_adapter_info_with_ctx(
            &features,
            &priors,
            &adapter_info,
            &policy_mask,
            Some(&determinism_ctx),
        )
        .expect("router decision");
    let decision_b = router_b
        .route_with_adapter_info_with_ctx(
            &features,
            &priors,
            &adapter_info,
            &policy_mask,
            Some(&determinism_ctx),
        )
        .expect("router decision");

    assert_eq!(
        decision_a.indices, decision_b.indices,
        "Adaptive routing should be deterministic when provided a determinism context"
    );
    assert_eq!(
        decision_a.gates_q15, decision_b.gates_q15,
        "Gates should also remain deterministic under the same tie-break seed"
    );
}

#[test]
fn adaptive_routing_applies_seeded_order_for_exact_ties() {
    const ADAPTER_COUNT: usize = 8;
    const TOP_K: usize = 3;

    let mut router_a = Router::new_with_weights(RouterWeights::default(), TOP_K, 1.0, 0.02);
    let mut router_b = Router::new_with_weights(RouterWeights::default(), TOP_K, 1.0, 0.02);
    router_a.set_routing_determinism_mode(true);
    router_b.set_routing_determinism_mode(true);

    let priors = vec![0.5f32; ADAPTER_COUNT];
    let adapter_info: Vec<AdapterInfo> = (0..ADAPTER_COUNT)
        .map(|i| AdapterInfo {
            id: format!("adaptive_tie_adapter_{}", i),
            stable_id: (i as u64) + 1,
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();
    let policy_mask = mask_all(&adapter_info);

    let ctx_a = DeterminismContext::new(
        [1u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        adapteros_types::adapters::metadata::RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );
    let ctx_b = DeterminismContext::new(
        [2u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        adapteros_types::adapters::metadata::RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );

    let expected_a = adaptive_order_from_ctx(&ctx_a, &adapter_info);
    let expected_b = adaptive_order_from_ctx(&ctx_b, &adapter_info);
    let mut expected_selected_a = expected_a[..TOP_K].to_vec();
    let mut expected_selected_b = expected_b[..TOP_K].to_vec();
    expected_selected_a.sort_unstable();
    expected_selected_b.sort_unstable();
    assert_ne!(
        expected_selected_a, expected_selected_b,
        "Different adaptive seeds should produce distinct top-k selections on exact ties"
    );

    let decision_a = router_a
        .route_with_adapter_info_with_ctx(&[], &priors, &adapter_info, &policy_mask, Some(&ctx_a))
        .expect("router decision");
    let decision_b = router_b
        .route_with_adapter_info_with_ctx(&[], &priors, &adapter_info, &policy_mask, Some(&ctx_b))
        .expect("router decision");

    let mut selected_a = decision_a.indices.to_vec();
    let mut selected_b = decision_b.indices.to_vec();
    selected_a.sort_unstable();
    selected_b.sort_unstable();

    assert_eq!(
        selected_a.as_slice(),
        expected_selected_a.as_slice(),
        "Adaptive routing should select the seeded top-k subset for exact ties"
    );
    assert_eq!(
        selected_b.as_slice(),
        expected_selected_b.as_slice(),
        "Adaptive routing should select the seeded top-k subset for exact ties"
    );
}

#[test]
fn test_decision_hash_computation() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    // Enable decision hashing
    let mut config = RouterDeterminismConfig::default();
    config.enable_decision_hashing = true;
    router.set_determinism_config(config);

    let features = vec![0.5f32; 10];
    let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    // Decision should have a hash
    assert!(
        decision.decision_hash.is_some(),
        "Decision should have hash when hashing is enabled"
    );

    let hash = decision.decision_hash.unwrap();

    // Hash should have all fields populated
    assert!(
        !hash.input_hash.is_empty(),
        "Input hash should be populated"
    );
    assert!(
        !hash.output_hash.is_empty(),
        "Output hash should be populated"
    );
    assert!(
        !hash.combined_hash.is_empty(),
        "Combined hash should be populated"
    );
    assert_eq!(hash.tau, 1.0, "Tau should match router config");
    assert_eq!(hash.eps, 0.02, "Eps should match router config");
    assert_eq!(hash.k, 3, "K should match router config");
}

#[test]
fn test_decision_hash_reproducibility() {
    // Test that identical inputs produce identical hashes
    let mut router1 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let mut router2 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    // Enable decision hashing on both
    let config = RouterDeterminismConfig::default();
    router1.set_determinism_config(config.clone());
    router2.set_determinism_config(config);

    let features = vec![0.5f32; 10];
    let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision1 = router1
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision2 = router2
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    // Both decisions should have hashes
    assert!(decision1.decision_hash.is_some());
    assert!(decision2.decision_hash.is_some());

    let hash1 = decision1.decision_hash.unwrap();
    let hash2 = decision2.decision_hash.unwrap();

    // Hashes should be identical for identical inputs
    assert_eq!(
        hash1.input_hash, hash2.input_hash,
        "Input hashes should match for identical inputs"
    );
    assert_eq!(
        hash1.output_hash, hash2.output_hash,
        "Output hashes should match for deterministic routing"
    );
    assert_eq!(
        hash1.combined_hash, hash2.combined_hash,
        "Combined hashes should match"
    );
}

#[test]
fn test_ieee754_deterministic_flag() {
    // Test that the IEEE 754 deterministic flag is respected
    let mut router_deterministic = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let mut router_standard = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    // Enable deterministic mode for first router
    let mut config_det = RouterDeterminismConfig::default();
    config_det.ieee754_deterministic = true;
    router_deterministic.set_determinism_config(config_det);

    // Disable deterministic mode for second router
    let mut config_std = RouterDeterminismConfig::default();
    config_std.ieee754_deterministic = false;
    router_standard.set_determinism_config(config_std);

    let features = vec![0.5f32; 10];
    let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision_det = router_deterministic
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision_std = router_standard
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    // Both should produce valid decisions
    assert_eq!(decision_det.indices.len(), 3);
    assert_eq!(decision_std.indices.len(), 3);

    // Gates should sum to approximately 1.0 in both cases
    let sum_det: f32 = decision_det.gates_f32().iter().sum();
    let sum_std: f32 = decision_std.gates_f32().iter().sum();
    assert!((sum_det - 1.0).abs() < 0.01);
    assert!((sum_std - 1.0).abs() < 0.01);

    // For these simple inputs, results should be very close (may differ in last bits)
    // We don't assert exact equality since f32 vs f64 paths may differ slightly
}

#[test]
fn test_decision_hash_disabled() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    // Disable decision hashing
    let mut config = RouterDeterminismConfig::default();
    config.enable_decision_hashing = false;
    router.set_determinism_config(config);

    let features = vec![0.5f32; 10];
    let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            ..Default::default()
        })
        .collect();

    let mask = mask_all(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

    // Decision should NOT have a hash when disabled
    assert!(
        decision.decision_hash.is_none(),
        "Decision should not have hash when hashing is disabled"
    );
}

#[test]
fn test_abstain_thresholds_from_policy_config() {
    // Create a policy config with abstain thresholds
    let mut policy_config = adapteros_policy::packs::router::RouterConfig::default();
    policy_config.abstain_entropy_threshold = Some(0.9);
    policy_config.abstain_confidence_threshold = Some(0.3);

    let router = Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

    // Verify that thresholds are set from policy config
    assert_eq!(
        router.abstain_entropy_threshold(),
        Some(0.9),
        "Entropy threshold should match policy config"
    );
    assert_eq!(
        router.abstain_confidence_threshold(),
        Some(0.3),
        "Confidence threshold should match policy config"
    );
}

#[test]
fn test_set_abstain_thresholds() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    // Initially no thresholds
    assert!(router.abstain_entropy_threshold().is_none());
    assert!(router.abstain_confidence_threshold().is_none());

    // Set thresholds
    router.set_abstain_thresholds(Some(0.85), Some(0.25));

    assert_eq!(router.abstain_entropy_threshold(), Some(0.85));
    assert_eq!(router.abstain_confidence_threshold(), Some(0.25));
}

#[test]
fn test_scope_hint_filters_non_matching_adapters() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    let features = vec![0.0; 4];
    let priors = vec![0.8, 0.8];
    let scope_hint = "domain/group/scope/op";

    let adapter_info = vec![
        AdapterInfo {
            id: "scoped".to_string(),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: Some(scope_hint.to_string()),
            ..Default::default()
        },
        AdapterInfo {
            id: "other".to_string(),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: Some("other/scope".to_string()),
            ..Default::default()
        },
    ];

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
    let decision = router
        .route_with_adapter_info_and_scope(
            &features,
            &priors,
            &adapter_info,
            &policy_mask,
            Some(scope_hint),
        )
        .expect("router decision");

    assert_eq!(decision.indices.len(), 1);
    assert_eq!(decision.indices[0], 0);
}

// ============================================================================
// Q15 QUANTIZATION EDGE CASE TESTS
// ============================================================================

#[test]
fn test_q15_constants_validation() {
    // Verify Q15 constants are correct
    assert_eq!(
        ROUTER_GATE_Q15_DENOM, 32767.0,
        "Q15 denominator MUST be 32767.0, not 32768.0"
    );
    assert_eq!(
        ROUTER_GATE_Q15_MAX, 32767,
        "Q15 max value MUST be 32767 (i16::MAX)"
    );
}

#[test]
fn test_q15_zero_gate_edge_case() {
    // Edge case 1: Gate = 0 → Q15 = 0
    let gate_f32 = 0.0f32;
    let gate_q15 = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;
    let gate_q15_clamped = gate_q15.max(0);

    assert_eq!(gate_q15, 0, "0.0 gate should convert to Q15 = 0");
    assert_eq!(gate_q15_clamped, 0, "Clamped 0 should remain 0");

    // Verify round-trip
    let recovered = gate_q15_clamped as f32 / ROUTER_GATE_Q15_DENOM;
    assert_eq!(recovered, 0.0, "Q15 = 0 should decode to 0.0");
}

#[test]
fn test_q15_max_gate_edge_case() {
    // Edge case 2: Gate = 1.0 → Q15 = 32767
    let gate_f32 = 1.0f32;
    let gate_q15 = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;

    assert_eq!(gate_q15, 32767, "1.0 gate should convert to Q15 = 32767");

    // Verify round-trip
    let recovered = gate_q15 as f32 / ROUTER_GATE_Q15_DENOM;
    assert_eq!(recovered, 1.0, "Q15 = 32767 should decode to exactly 1.0");
}

#[test]
fn test_q15_negative_gate_clamping() {
    // Edge case 3: Negative gates should be clamped to 0
    let negative_gate = -0.5f32;
    let gate_q15_raw = (negative_gate * ROUTER_GATE_Q15_DENOM).round() as i16;
    let gate_q15_clamped = gate_q15_raw.max(0);

    assert!(gate_q15_raw < 0, "Negative gate produces negative Q15");
    assert_eq!(gate_q15_clamped, 0, "Negative Q15 should be clamped to 0");
}

#[test]
fn test_q15_very_small_gates_underflow() {
    // Edge case 4: Very small gates should round to 0 or 1
    let tiny_gates = vec![1e-8, 1e-7, 1e-6, 1e-5, 1e-4];

    for gate in tiny_gates {
        let q = (gate * ROUTER_GATE_Q15_DENOM).round() as i16;
        let q_clamped = q.max(0);

        if gate * ROUTER_GATE_Q15_DENOM < 0.5 {
            assert_eq!(q_clamped, 0, "Gate {} should round to Q15 = 0", gate);
        } else {
            assert!(q_clamped >= 1, "Gate {} should round to Q15 >= 1", gate);
        }
    }
}

#[test]
fn test_q15_sum_normalization() {
    // Edge case 5: Sum of Q15 gates should be ~32767 after normalization
    let normalized_gates = vec![0.25, 0.25, 0.25, 0.25];

    let gates_q15: Vec<i16> = normalized_gates
        .iter()
        .map(|&g| {
            let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
            q.max(0)
        })
        .collect();

    let sum_q15: i32 = gates_q15.iter().map(|&g| g as i32).sum();

    // Sum should be close to 32767 (within rounding error)
    assert!(
        (sum_q15 - ROUTER_GATE_Q15_MAX as i32).abs() <= gates_q15.len() as i32,
        "Sum of Q15 gates ({}) should be within {} of max ({})",
        sum_q15,
        gates_q15.len(),
        ROUTER_GATE_Q15_MAX
    );
}

#[test]
fn test_q15_to_f32_conversion_formula() {
    // Edge case 6: Verify Q15→f32 conversion: gate_q15 / 32767.0
    let test_values = vec![
        (0i16, 0.0f32),
        (1i16, 1.0 / 32767.0),
        (16383i16, 16383.0 / 32767.0),
        (32767i16, 1.0),
    ];

    for (q15, expected_f32) in test_values {
        let converted = q15 as f32 / ROUTER_GATE_Q15_DENOM;
        assert!(
            (converted - expected_f32).abs() < 1e-6,
            "Q15 {} should convert to {}, got {}",
            q15,
            expected_f32,
            converted
        );
    }
}

#[test]
fn test_q15_conversion_determinism() {
    // Edge case 7: Same gates → same Q15 values (determinism)
    let gates = vec![0.2, 0.3, 0.5];

    // Convert 5 times and verify consistency
    let mut results = Vec::new();
    for _ in 0..5 {
        let gates_q15: Vec<i16> = gates
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();
        results.push(gates_q15);
    }

    // All results should be identical
    for i in 1..results.len() {
        assert_eq!(
            results[0], results[i],
            "Q15 conversion should be deterministic"
        );
    }
}

#[test]
fn test_q15_round_trip_precision() {
    // Test f32 → Q15 → f32 round-trip precision
    let test_gates = vec![0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];

    for original in test_gates {
        let q15 = (original * ROUTER_GATE_Q15_DENOM).round() as i16;
        let q15_clamped = q15.max(0);
        let recovered = q15_clamped as f32 / ROUTER_GATE_Q15_DENOM;

        let max_error = 1.0 / ROUTER_GATE_Q15_DENOM;
        let actual_error = (recovered - original).abs();

        assert!(
            actual_error <= max_error,
            "Round-trip error ({}) exceeds max ({}) for gate {}",
            actual_error,
            max_error,
            original
        );
    }
}

#[test]
fn test_q15_not_using_legacy_32768() {
    // Verify we're NOT using incorrect 32768 denominator
    let gate_max = 1.0f32;

    let q15_correct = (gate_max * 32767.0).round() as i32;
    let q15_incorrect = (gate_max * 32768.0).round() as i32;

    assert_eq!(q15_correct, ROUTER_GATE_Q15_MAX as i32);
    assert_ne!(q15_correct, q15_incorrect, "32767 and 32768 must differ");

    let recovered_correct = q15_correct as f32 / 32767.0;
    assert_eq!(recovered_correct, 1.0, "32767 denom gives exact 1.0");
}

#[test]
fn test_q15_decision_gates_f32_method() {
    // Test Decision::gates_f32() conversion
    let decision = Decision {
        indices: SmallVec::from_vec(vec![0, 1, 2]),
        gates_q15: SmallVec::from_vec(vec![32767, 16383, 0]),
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest_b3: None,
        policy_overrides_applied: None,
    };

    let gates_f32 = decision.gates_f32();

    assert_eq!(gates_f32.len(), 3);
    assert_eq!(gates_f32[0], 1.0);
    assert!((gates_f32[1] - 0.5).abs() < 0.001);
    assert_eq!(gates_f32[2], 0.0);
}

#[test]
fn test_router_q15_gates_sum_correctly() {
    // Integration test: router Q15 gates should sum to ~32767
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.5; 22];
    let priors = vec![1.0, 1.0, 1.0];

    let adapter_info: Vec<AdapterInfo> = (0..3)
        .map(|i| AdapterInfo {
            id: format!("adapter-{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&adapter_ids, None);
    let determinism_ctx = DeterminismContext::new(
        [42u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        adapteros_types::adapters::metadata::RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );
    let decision = router
        .route_with_adapter_info_with_ctx(
            &features,
            &priors,
            &adapter_info,
            &mask,
            Some(&determinism_ctx),
        )
        .expect("routing decision");

    let sum_q15: i32 = decision.gates_q15.iter().map(|&g| g as i32).sum();
    let sum_f32: f32 = decision.gates_f32().iter().sum();

    assert!((sum_f32 - 1.0).abs() < 0.01, "Float gates sum to ~1.0");
    assert!(
        (sum_q15 - ROUTER_GATE_Q15_MAX as i32).abs() <= decision.gates_q15.len() as i32,
        "Q15 gates sum to ~32767"
    );
}

#[test]
fn test_router_single_adapter_gets_max_q15() {
    // Single adapter should get gate = 1.0 → Q15 = 32767
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.5; 22];
    let priors = vec![1.0];

    let adapter_info = vec![AdapterInfo {
        id: "adapter-1".to_string(),
        framework: None,
        languages: vec![],
        tier: "default".to_string(),
        scope_path: None,
        lora_tier: None,
        base_model: None,
        ..Default::default()
    }];

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&adapter_ids, None);
    let determinism_ctx = DeterminismContext::new(
        [42u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        adapteros_types::adapters::metadata::RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );
    let decision = router
        .route_with_adapter_info_with_ctx(
            &features,
            &priors,
            &adapter_info,
            &mask,
            Some(&determinism_ctx),
        )
        .expect("routing decision");

    assert_eq!(decision.indices.len(), 1);
    assert_eq!(decision.gates_q15[0], 32767);
    assert_eq!(decision.gates_f32()[0], 1.0);
}

#[test]
fn test_router_q15_determinism_identical_inputs() {
    // Multiple routing calls with identical inputs produce identical Q15
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.5; 22];
    let priors = vec![0.6, 0.3, 0.1];

    let adapter_info: Vec<AdapterInfo> = (0..3)
        .map(|i| AdapterInfo {
            id: format!("adapter-{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&adapter_ids, None);
    let determinism_ctx = DeterminismContext::new(
        [42u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        adapteros_types::adapters::metadata::RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );

    // Make 3 identical routing decisions
    let mut decisions = Vec::new();
    for _ in 0..3 {
        let decision = router
            .route_with_adapter_info_with_ctx(
                &features,
                &priors,
                &adapter_info,
                &mask,
                Some(&determinism_ctx),
            )
            .expect("routing decision");
        decisions.push(decision);
    }

    // All decisions should have identical Q15 gates
    for i in 1..decisions.len() {
        assert_eq!(
            decisions[0].gates_q15, decisions[i].gates_q15,
            "Identical inputs should produce identical Q15 gates"
        );
    }
}

// =============================================================================
// Deterministic Tie-Breaking Sort Tests
// =============================================================================

#[test]
fn test_sort_scores_deterministic_by_score_descending() {
    use crate::sort_scores_deterministic;

    let adapter_info: Vec<AdapterInfo> = (0..3)
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            stable_id: (i as u64 + 1) * 100,
            ..Default::default()
        })
        .collect();

    let mut scores = vec![(0, 0.5f32), (1, 0.9f32), (2, 0.7f32)];

    let ties = sort_scores_deterministic(&mut scores, &adapter_info, true);

    assert!(ties.is_empty(), "No ties expected");
    assert_eq!(scores[0].0, 1); // 0.9
    assert_eq!(scores[1].0, 2); // 0.7
    assert_eq!(scores[2].0, 0); // 0.5
}

#[test]
fn test_sort_scores_deterministic_tie_breaking() {
    use crate::sort_scores_deterministic;

    // Adapters with same score but different stable_ids
    let adapter_info: Vec<AdapterInfo> = vec![
        AdapterInfo {
            id: "a".to_string(),
            stable_id: 300, // Highest stable_id
            ..Default::default()
        },
        AdapterInfo {
            id: "b".to_string(),
            stable_id: 100, // Lowest stable_id - should win ties
            ..Default::default()
        },
        AdapterInfo {
            id: "c".to_string(),
            stable_id: 200,
            ..Default::default()
        },
    ];

    let mut scores = vec![(0, 0.8f32), (1, 0.8f32), (2, 0.8f32)];

    let ties = sort_scores_deterministic(&mut scores, &adapter_info, true);

    // All same score -> sorted by stable_id ascending
    assert_eq!(scores[0].0, 1); // stable_id 100
    assert_eq!(scores[1].0, 2); // stable_id 200
    assert_eq!(scores[2].0, 0); // stable_id 300

    // Should have recorded tie events
    assert!(!ties.is_empty(), "Tie events should be recorded");
}

#[test]
fn test_sort_scores_deterministic_mixed() {
    use crate::sort_scores_deterministic;

    let adapter_info: Vec<AdapterInfo> = vec![
        AdapterInfo {
            id: "a".to_string(),
            stable_id: 200,
            ..Default::default()
        },
        AdapterInfo {
            id: "b".to_string(),
            stable_id: 100, // Lower stable_id wins in tie
            ..Default::default()
        },
        AdapterInfo {
            id: "c".to_string(),
            stable_id: 50,
            ..Default::default()
        },
        AdapterInfo {
            id: "d".to_string(),
            stable_id: 150,
            ..Default::default()
        },
    ];

    // idx 0 and 1 have same score (0.9), idx 2 and 3 are different
    let mut scores = vec![(0, 0.9f32), (1, 0.9f32), (2, 0.5f32), (3, 0.7f32)];

    let ties = sort_scores_deterministic(&mut scores, &adapter_info, true);

    // Expected order: 0.9(stable_id 100), 0.9(stable_id 200), 0.7, 0.5
    assert_eq!(scores[0].0, 1); // 0.9, stable_id 100
    assert_eq!(scores[1].0, 0); // 0.9, stable_id 200
    assert_eq!(scores[2].0, 3); // 0.7
    assert_eq!(scores[3].0, 2); // 0.5

    // One tie event between idx 0 and 1
    assert_eq!(ties.len(), 1);
    assert!(
        (ties[0].idx_a == 0 && ties[0].idx_b == 1) || (ties[0].idx_a == 1 && ties[0].idx_b == 0)
    );
}

#[test]
fn test_sort_scores_deterministic_no_tie_collection() {
    use crate::sort_scores_deterministic;

    let adapter_info: Vec<AdapterInfo> = vec![
        AdapterInfo {
            id: "a".to_string(),
            stable_id: 100,
            ..Default::default()
        },
        AdapterInfo {
            id: "b".to_string(),
            stable_id: 200,
            ..Default::default()
        },
    ];

    let mut scores = vec![(0, 0.8f32), (1, 0.8f32)];

    // Don't collect ties
    let ties = sort_scores_deterministic(&mut scores, &adapter_info, false);

    // Still sorts correctly
    assert_eq!(scores[0].0, 0); // stable_id 100
    assert_eq!(scores[1].0, 1); // stable_id 200

    // But no tie events collected
    assert!(ties.is_empty());
}

#[test]
fn test_sort_scores_deterministic_total_cmp() {
    use crate::sort_scores_deterministic;

    let adapter_info: Vec<AdapterInfo> = vec![
        AdapterInfo {
            id: "a".to_string(),
            stable_id: 100,
            ..Default::default()
        },
        AdapterInfo {
            id: "b".to_string(),
            stable_id: 200,
            ..Default::default()
        },
        AdapterInfo {
            id: "c".to_string(),
            stable_id: 300,
            ..Default::default()
        },
    ];

    // Test with infinity and negative zero
    let mut scores = vec![(0, f32::INFINITY), (1, 0.5f32), (2, f32::NEG_INFINITY)];

    let ties = sort_scores_deterministic(&mut scores, &adapter_info, true);

    assert!(ties.is_empty());
    assert_eq!(scores[0].0, 0); // +inf
    assert_eq!(scores[1].0, 1); // 0.5
    assert_eq!(scores[2].0, 2); // -inf
}

#[test]
fn test_sort_scores_deterministic_consistency() {
    use crate::sort_scores_deterministic;

    let adapter_info: Vec<AdapterInfo> = vec![
        AdapterInfo {
            id: "a".to_string(),
            stable_id: 300,
            ..Default::default()
        },
        AdapterInfo {
            id: "b".to_string(),
            stable_id: 100,
            ..Default::default()
        },
        AdapterInfo {
            id: "c".to_string(),
            stable_id: 200,
            ..Default::default()
        },
        AdapterInfo {
            id: "d".to_string(),
            stable_id: 400,
            ..Default::default()
        },
    ];

    // Run 100 times to verify determinism
    for _ in 0..100 {
        let mut scores = vec![(0, 0.8f32), (1, 0.8f32), (2, 0.8f32), (3, 0.9f32)];
        sort_scores_deterministic(&mut scores, &adapter_info, false);

        // Must always produce same order
        assert_eq!(scores[0].0, 3); // 0.9
        assert_eq!(scores[1].0, 1); // 0.8, stable_id 100
        assert_eq!(scores[2].0, 2); // 0.8, stable_id 200
        assert_eq!(scores[3].0, 0); // 0.8, stable_id 300
    }
}
