//! PRD 8 - Core Determinism Tests (Linux-compatible)
//! Verifies end-to-end determinism primitives: HKDF router seeds, routing
//! ordering with Q15 gates/decision hashes, and replay metadata round-trips.

use adapteros_api_types::inference::PolicyOverrideFlags as ApiPolicyOverrideFlags;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::{
    CreateReplayMetadataParams, Db, SqlTraceSink, TraceFinalization, TraceSink, TraceStart,
    TraceTokenInput,
};
use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Router, RouterDeterminismConfig, RouterWeights,
};
use adapteros_lora_worker::services::determinism_policy::{HkdfSeedExpander, SeedDomain};
use adapteros_lora_worker::DeterministicRng;
use adapteros_policy::{DeterminismConfig, DeterminismPolicy, RngSeedingMethod};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use sqlx;
use std::sync::Arc;

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
            ..Default::default()
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

    let mask = {
        let ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
        PolicyMask::allow_all(&ids, None)
    };
    let decision1 = router1
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision2 = router2
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("router decision");

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

    ensure_base_only_column(&db).await;
    seed_tenant(&db).await;

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
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        sampling_algorithm_version: Some("v1.0.0-test".to_string()),
        rag_snapshot_hash: Some("rag-snapshot-xyz".to_string()),
        dataset_version_id: None,
        adapter_ids: Some(adapter_ids.clone()),
        base_only: None,
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
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        replay_guarantee: Some("exact".to_string()),
        execution_policy_id: Some("policy-001".to_string()),
        execution_policy_version: Some(1),
        stop_policy_json: None,
        policy_mask_digest_b3: None,
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

#[tokio::test]
async fn reasoning_swap_flow_is_deterministic_and_traced() -> Result<()> {
    let seed = B3Hash::hash(b"reasoning-determinism-core").to_bytes();
    let first = run_reasoning_trace(seed, "trace-reasoning-1", 6).await?;
    let second = run_reasoning_trace(seed, "trace-reasoning-2", 6).await?;

    assert_eq!(
        first.reasoning_segments, second.reasoning_segments,
        "Reasoning text must be identical for identical seeds"
    );
    assert_eq!(
        first.adapter_choices, second.adapter_choices,
        "Adapter choices must be deterministic under reasoning swaps"
    );
    assert_eq!(
        first.receipt_digest.to_hex(),
        second.receipt_digest.to_hex(),
        "Trace receipts must be stable across runs"
    );
    assert_eq!(
        first.stored_adapter_sequences, second.stored_adapter_sequences,
        "Persisted swap chain must be deterministic"
    );
    assert_eq!(
        first.stored_adapter_sequences.len(),
        first.adapter_choices.len(),
        "Trace DB must capture every reasoning swap, not just the final state"
    );

    for (idx, stored_ids) in first.stored_adapter_sequences.iter().enumerate() {
        assert_eq!(
            stored_ids,
            &vec![first.adapter_choices[idx].clone()],
            "Trace DB must preserve swap order"
        );
    }

    for kernel_id in &first.kernel_versions {
        assert_eq!(
            kernel_id.as_deref(),
            Some("determinism-kernel|thought_swap"),
            "Trace entries should tag reasoning swaps"
        );
    }

    Ok(())
}

struct ReasoningRunTrace {
    reasoning_segments: Vec<String>,
    adapter_choices: Vec<String>,
    stored_adapter_sequences: Vec<Vec<String>>,
    kernel_versions: Vec<Option<String>>,
    receipt_digest: B3Hash,
}

async fn run_reasoning_trace(
    seed: [u8; 32],
    trace_id: &str,
    swap_count: usize,
) -> Result<ReasoningRunTrace> {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Arc::new(Db::new_in_memory().await?);
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-reasoning")
        .bind("Reasoning Determinism Tenant")
        .execute(db.pool())
        .await?;

    let mut sink = SqlTraceSink::new(
        db.clone(),
        TraceStart {
            trace_id: trace_id.to_string(),
            tenant_id: "tenant-reasoning".to_string(),
            request_id: Some("req-reasoning".to_string()),
            context_digest: seed,
        },
        8,
    )
    .await?;

    let (mut router, adapter_info, priors, policy_mask) = reasoning_router_fixture();
    let reasoning_segments = generate_reasoning_segments(&seed, swap_count)?;
    let mut adapter_choices = Vec::with_capacity(swap_count);

    for (idx, rationale) in reasoning_segments.iter().enumerate() {
        let decision =
            router.route_on_reasoning(rationale, &priors, &adapter_info, &policy_mask, None)?;
        let adapter_idx = decision
            .indices
            .first()
            .copied()
            .ok_or_else(|| AosError::Routing("Empty reasoning decision".to_string()))?;
        let adapter_id = adapter_info[adapter_idx as usize].id.clone();
        adapter_choices.push(adapter_id.clone());

        let policy_mask_digest_b3 = decision
            .policy_mask_digest_b3
            .map(|digest| digest.to_bytes());
        let policy_overrides_applied =
            decision
                .policy_overrides_applied
                .as_ref()
                .map(|flags| ApiPolicyOverrideFlags {
                    allow_list: flags.allow_list,
                    deny_list: flags.deny_list,
                    trust_state: flags.trust_state,
                });

        sink.record_token(TraceTokenInput {
            token_index: idx as u32,
            adapter_ids: vec![adapter_id],
            gates_q15: decision.gates_q15.iter().copied().collect(),
            policy_mask_digest_b3,
            allowed_mask: Some(policy_mask.allowed.clone()),
            policy_overrides_applied,
            backend_id: Some("determinism-backend".to_string()),
            kernel_version_id: Some("determinism-kernel|thought_swap".to_string()),
        })
        .await?;
    }

    let receipt = sink
        .finalize(TraceFinalization {
            output_tokens: &[7, 11, 13],
            logical_prompt_tokens: 4,
            prefix_cached_token_count: 0,
            billed_input_tokens: 4,
            logical_output_tokens: 3,
            billed_output_tokens: 3,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            tenant_kv_quota_bytes: 0,
            tenant_kv_bytes_used: 0,
            kv_evictions: 0,
            kv_residency_policy_id: None,
            kv_quota_enforced: false,
            prefix_kv_key_b3: None,
            prefix_cache_hit: false,
            prefix_kv_bytes: 0,
            model_cache_identity_v2_digest_b3: None,
            attestation: None,
        })
        .await?;

    let stored_rows: Vec<(i64, Vec<u8>, Option<String>)> = sqlx::query_as(
        "SELECT token_index, selected_adapter_ids, kernel_version_id FROM inference_trace_tokens WHERE trace_id = ? ORDER BY token_index ASC",
    )
    .bind(trace_id)
    .fetch_all(db.pool())
    .await?;

    let mut stored_adapter_sequences = Vec::with_capacity(stored_rows.len());
    let mut kernel_versions = Vec::with_capacity(stored_rows.len());
    for (_, adapter_blob, kernel_version_id) in stored_rows {
        stored_adapter_sequences.push(decode_adapter_ids(&adapter_blob)?);
        kernel_versions.push(kernel_version_id);
    }

    Ok(ReasoningRunTrace {
        reasoning_segments,
        adapter_choices,
        stored_adapter_sequences,
        kernel_versions,
        receipt_digest: receipt.receipt_digest,
    })
}

fn reasoning_router_fixture() -> (Router, Vec<AdapterInfo>, Vec<f32>, PolicyMask) {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
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
    let priors = vec![0.55, 0.55];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    (router, adapter_info, priors, policy_mask)
}

fn generate_reasoning_segments(seed: &[u8; 32], count: usize) -> Result<Vec<String>> {
    let mut rng = DeterministicRng::new(seed, "reasoning-swap-trace")?;
    let mut segments = Vec::with_capacity(count);
    for idx in 0..count {
        let pick = rng.next_u32() % 2;
        let rationale = if pick == 0 {
            format!("<thinking>Step {idx}: python helpers</thinking>")
        } else {
            format!("<thinking>Step {idx}: creative narrative</thinking>")
        };
        segments.push(rationale);
    }
    Ok(segments)
}

fn decode_adapter_ids(bytes: &[u8]) -> Result<Vec<String>> {
    if bytes.len() < 4 {
        return Err(AosError::InvalidHash(
            "adapter_ids blob missing length".to_string(),
        ));
    }

    let mut cursor = 4;
    let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        if bytes.len() < cursor + 4 {
            return Err(AosError::InvalidHash(
                "adapter_ids blob truncated before length".to_string(),
            ));
        }
        let len = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;
        let end = cursor + len;
        if bytes.len() < end {
            return Err(AosError::InvalidHash(
                "adapter_ids blob truncated before data".to_string(),
            ));
        }
        ids.push(
            String::from_utf8(bytes[cursor..end].to_vec())
                .map_err(|e| AosError::InvalidHash(format!("adapter_ids decode error: {e}")))?,
        );
        cursor = end;
    }
    Ok(ids)
}

async fn ensure_base_only_column(db: &Db) {
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM pragma_table_info('inference_replay_metadata') WHERE name = 'base_only'",
    )
    .fetch_optional(db.pool())
    .await
    .expect("pragma table info should succeed");

    if exists.is_none() {
        sqlx::query("ALTER TABLE inference_replay_metadata ADD COLUMN base_only INTEGER")
            .execute(db.pool())
            .await
            .expect("should be able to add base_only column in tests");
    }
}

async fn seed_tenant(db: &Db) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-det")
        .bind("Determinism Tenant")
        .execute(db.pool())
        .await
        .expect("tenant insert should succeed for FK constraints");
}