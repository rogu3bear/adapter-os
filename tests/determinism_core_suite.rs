//! PRD 8 - Core Determinism Tests (Linux-compatible)
//! Verifies end-to-end determinism primitives: HKDF router seeds, routing
//! ordering with Q15 gates/decision hashes, and replay metadata round-trips.

use adapteros_core::B3Hash;
use adapteros_db::{CreateReplayMetadataParams, Db};
use adapteros_lora_router::{AdapterInfo, Router, RouterDeterminismConfig, RouterWeights};
use adapteros_lora_worker::services::determinism_policy::{HkdfSeedExpander, SeedDomain};
use adapteros_policy::{DeterminismConfig, DeterminismPolicy, RngSeedingMethod};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

#[test]
fn test_router_seed_derivation_and_policy_hooks() {
    let policy = DeterminismPolicy::new(DeterminismConfig::default());
    policy
        .validate_rng_seeding(&RngSeedingMethod::HkdfSeeded)
        .expect("HKDF seeding should be allowed by determinism policy");

    let base_seed = [0x23u8; 32];
    let mut expander_a = HkdfSeedExpander::new(&base_seed);
    let mut expander_b = HkdfSeedExpander::new(&base_seed);

    let router_seed_a = expander_a.derive(SeedDomain::Router);
    let router_seed_b = expander_b.derive(SeedDomain::Router);
    assert_eq!(
        router_seed_a, router_seed_b,
        "Router seeds must be deterministic across expanders"
    );

    let sampling_seed = expander_a.derive(SeedDomain::Sampling);
    assert_ne!(
        router_seed_a, sampling_seed,
        "Domain separation must yield different seeds"
    );
}

#[test]
fn test_router_ordering_and_q15_gates_are_stable() {
    // Deterministic features/priors derived from HKDF router seed
    let mut expander = HkdfSeedExpander::new(&[0x34u8; 32]);
    let router_seed = expander.derive(SeedDomain::Router);
    let mut rng = ChaCha20Rng::from_seed(router_seed);

    let adapter_count = 5;
    let features: Vec<f32> = (0..adapter_count)
        .map(|_| rng.gen_range(0.0f32..1.0f32))
        .collect();
    let priors: Vec<f32> = (0..adapter_count)
        .map(|_| rng.gen_range(0.0f32..1.0f32))
        .collect();
    let adapter_info: Vec<AdapterInfo> = (0..adapter_count)
        .map(|i| AdapterInfo {
            id: format!("adapter-{i}"),
            framework: None,
            languages: vec![0],
            tier: "persistent".to_string(),
        })
        .collect();

    let determinism = RouterDeterminismConfig {
        ieee754_deterministic: true,
        enable_decision_hashing: true,
    };

    let mut router1 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router1.set_determinism_config(determinism.clone());
    let mut router2 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router2.set_determinism_config(determinism);

    let decision1 = router1.route_with_adapter_info(&features, &priors, &adapter_info);
    let decision2 = router2.route_with_adapter_info(&features, &priors, &adapter_info);

    assert_eq!(
        decision1.indices, decision2.indices,
        "Routing indices must be identical across runs"
    );
    assert_eq!(
        decision1.gates_q15, decision2.gates_q15,
        "Q15 gates must be identical across runs"
    );

    // Ordering should be stable and non-increasing by gate magnitude
    for window in decision1.gates_q15.windows(2) {
        assert!(
            window[0] >= window[1],
            "Gates must be sorted by descending score (Q15)"
        );
    }

    // Q15 quantization should map back using denominator 32767.0
    for (&q15, gate_f32) in decision1.gates_q15.iter().zip(decision1.gates_f32()) {
        let expected = q15 as f32 / 32767.0;
        assert!(
            (gate_f32 - expected).abs() < 1e-6,
            "Gate quantization must use 32767.0 denominator"
        );
    }

    let hash1 = decision1
        .decision_hash
        .as_ref()
        .expect("Decision hash missing");
    let hash2 = decision2
        .decision_hash
        .as_ref()
        .expect("Decision hash missing");
    assert_eq!(
        hash1.combined_hash, hash2.combined_hash,
        "Decision hash must be stable across runs"
    );
}

#[tokio::test]
async fn test_replay_metadata_round_trip() {
    let db = Db::new_in_memory().await.expect("in-memory DB should init");

    let router_seed_bytes = {
        let mut expander = HkdfSeedExpander::new(&[0x99u8; 32]);
        expander.derive(SeedDomain::Router)
    };
    let router_seed = B3Hash::hash(&router_seed_bytes).to_hex();
    let adapter_ids = vec!["adapter-alpha".to_string(), "adapter-beta".to_string()];
    let rag_doc_ids = vec!["doc-1".to_string(), "doc-2".to_string()];
    let sampling_params_json = serde_json::json!({
        "temperature": 0.0,
        "top_k": 4,
        "top_p": 0.9,
        "max_tokens": 32,
        "seed": 7
    })
    .to_string();

    let params = CreateReplayMetadataParams {
        inference_id: "det-core-suite-replay".to_string(),
        tenant_id: "tenant-det".to_string(),
        manifest_hash: "determinism-manifest-hash".to_string(),
        base_model_id: Some("base-det".to_string()),
        router_seed: Some(router_seed.clone()),
        sampling_params_json: sampling_params_json.clone(),
        backend: "metal".to_string(),
        backend_version: Some("v1.0.0-test".to_string()),
        sampling_algorithm_version: Some("v1.0.0-test".to_string()),
        rag_snapshot_hash: Some("rag-snapshot-xyz".to_string()),
        adapter_ids: Some(adapter_ids.clone()),
        prompt_text: "prompt determinism".to_string(),
        prompt_truncated: false,
        response_text: Some("response determinism".to_string()),
        response_truncated: false,
        rag_doc_ids: Some(rag_doc_ids.clone()),
        chat_context_hash: Some("chat-hash-001".to_string()),
        replay_status: Some("available".to_string()),
        latency_ms: Some(42),
        tokens_generated: Some(64),
        determinism_mode: Some("strict".to_string()),
        fallback_triggered: true,
        replay_guarantee: Some("exact".to_string()),
        execution_policy_id: Some("policy-001".to_string()),
        execution_policy_version: Some(1),
    };

    db.create_replay_metadata(params)
        .await
        .expect("replay metadata insert must succeed");

    let stored = db
        .get_replay_metadata_by_inference("det-core-suite-replay")
        .await
        .expect("query should succeed")
        .expect("metadata should exist");

    assert_eq!(
        stored.router_seed.as_deref(),
        Some(router_seed.as_str()),
        "Router seed must round-trip"
    );
    assert_eq!(
        stored.determinism_mode.as_deref(),
        Some("strict"),
        "Determinism mode must round-trip"
    );
    assert_eq!(
        stored.fallback_triggered,
        Some(true),
        "Fallback flag must round-trip"
    );
    assert_eq!(
        stored.replay_guarantee.as_deref(),
        Some("exact"),
        "Replay guarantee must round-trip"
    );
    assert_eq!(
        stored.sampling_params_json, sampling_params_json,
        "Sampling params must stay canonical"
    );

    let stored_adapter_ids: Vec<String> =
        serde_json::from_str(stored.adapter_ids_json.as_ref().unwrap()).expect("valid adapter IDs");
    assert_eq!(
        stored_adapter_ids, adapter_ids,
        "Adapter ID order must persist"
    );

    let stored_rag_doc_ids: Vec<String> =
        serde_json::from_str(stored.rag_doc_ids_json.as_ref().unwrap()).expect("valid doc IDs");
    assert_eq!(
        stored_rag_doc_ids, rag_doc_ids,
        "RAG doc ID order must persist"
    );
}
