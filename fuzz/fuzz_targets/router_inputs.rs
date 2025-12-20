#![no_main]

use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use blake3::Hasher;
use libfuzzer_sys::fuzz_target;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

fn seed_rng(data: &[u8]) -> ChaCha20Rng {
    let mut hasher = Hasher::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut seed = [0u8; 32];
    seed.copy_from_slice(digest.as_bytes());
    ChaCha20Rng::from_seed(seed)
}

fuzz_target!(|data: &[u8]| {
    let mut rng = seed_rng(data);

    // Keep adapter count small to avoid blowing up allocations
    let adapter_count = rng.gen_range(1..=8usize);
    let k = rng.gen_range(1..=adapter_count.min(8));

    let feature_len_choices = [21usize, 22, 25, rng.gen_range(5..30)];
    let feature_len = *feature_len_choices
        .get(rng.gen_range(0..feature_len_choices.len()))
        .unwrap_or(&21);

    let mut features: Vec<f32> = (0..feature_len)
        .map(|_| rng.gen_range(-5.0f32..5.0f32))
        .collect();

    // Ensure a few entries are non-zero to exercise branches
    if !features.is_empty() {
        features[0] = 1.0;
    }

    let priors: Vec<f32> = (0..adapter_count)
        .map(|_| rng.gen_range(-2.0f32..2.0f32))
        .collect();

    let adapter_info: Vec<AdapterInfo> = (0..adapter_count)
        .map(|idx| AdapterInfo {
            id: format!("adapter-{idx}"),
            framework: Some(if rng.gen_bool(0.5) { "coreml" } else { "mlx" }.to_string()),
            languages: vec![rng.gen_range(0..8)],
            tier: if rng.gen_bool(0.5) {
                "prod".to_string()
            } else {
                "exp".to_string()
            },
            scope_path: if rng.gen_bool(0.3) {
                Some("scope/a".to_string())
            } else {
                None
            },
            lora_tier: None,
            base_model: None,
        })
        .collect();

    let ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = if rng.gen_bool(0.5) {
        PolicyMask::allow_all(&ids, None)
    } else {
        PolicyMask::deny_all(&ids, None)
    };

    let mut router_a = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.05);
    let mut router_b = Router::new_with_weights(RouterWeights::default(), k, 1.0, 0.05);

    let decision_a = router_a.route_with_adapter_info(&features, &priors, &adapter_info, &mask);
    let decision_b = router_b.route_with_adapter_info(&features, &priors, &adapter_info, &mask);

    // Determinism check: same inputs should yield identical indices and gates
    assert_eq!(decision_a.indices, decision_b.indices);
    assert_eq!(decision_a.gates_q15, decision_b.gates_q15);

    // Exercise conversion to router ring (assert within function guards k <= 8)
    let _ring = decision_a.to_router_ring();

    // Slightly perturb features to ensure router handles varied input sizes gracefully
    if feature_len > 3 {
        features[feature_len - 1] *= -1.0;
        let _ = router_a.route_with_adapter_info(&features, &priors, &adapter_info, &mask);
    }
});
