#![no_main]

use adapteros_core::B3Hash;
use adapteros_db::{
    recompute_receipt, SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput,
};
use blake3::Hasher;
use libfuzzer_sys::fuzz_target;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::sync::Arc;
use tokio::runtime::Builder;

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
    let rt = match Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(_) => return,
    };

    let _ = rt.block_on(async {
        let db = Arc::new(adapteros_db::Db::new_in_memory().await?);

        let trace_id = format!("trace-{}", rng.gen::<u64>());
        let context_digest = B3Hash::hash(&rng.gen::<u128>().to_le_bytes()).to_bytes();

        let start = TraceStart {
            trace_id: trace_id.clone(),
            tenant_id: "tenant-fuzz".to_string(),
            request_id: None,
            context_digest,
            stack_id: None,
            model_id: None,
            policy_id: None,
        };

        let mut sink = SqlTraceSink::new(db.clone(), start, 8).await?;

        let token_count = rng.gen_range(0..=4usize);
        for idx in 0..token_count {
            let adapter_count = rng.gen_range(1..=4usize);
            let adapters: Vec<String> = (0..adapter_count)
                .map(|i| format!("adapter-{i}"))
                .collect();
            let gates: Vec<i16> = (0..adapter_count)
                .map(|_| rng.gen_range(-32000i16..=32767i16))
                .collect();

            let token = TraceTokenInput {
                token_index: idx as u32,
                adapter_ids: adapters,
                gates_q15: gates,
                policy_mask_digest_b3: if rng.gen_bool(0.3) {
                    Some(B3Hash::hash(&rng.gen::<u128>().to_le_bytes()).to_bytes())
                } else {
                    None
                },
                allowed_mask: None,
                policy_overrides_applied: None,
                backend_id: Some(if rng.gen_bool(0.5) {
                    "coreml".to_string()
                } else {
                    "mlx".to_string()
                }),
                kernel_version_id: Some("k1".to_string()),
            };

            let _ = sink.record_token(token).await;
        }

        let outputs: Vec<u32> = (0..rng.gen_range(0..=5)).map(|_| rng.gen()).collect();
        let _ = sink
            .finalize(TraceFinalization {
                output_tokens: &outputs,
                logical_prompt_tokens: 0,
                prefix_cached_token_count: 0,
                billed_input_tokens: 0,
                logical_output_tokens: outputs.len() as u32,
                billed_output_tokens: outputs.len() as u32,
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
                equipment_profile: None,
                // Phase 3: Crypto Receipt Dual-Write
                crypto_receipt_digest_b3: None,
                receipt_parity_verified: None,
                tenant_id: None,
                // P0-1: Cache attestation for provable cache credits
                cache_attestation: None,
                worker_public_key: None,
                // UMA telemetry (PRD §5.5)
                copy_bytes: None,
                // V7: Tokenizer identity
                tokenizer_hash_b3: None,
                tokenizer_version: None,
                tokenizer_normalization: None,
                // V7: Model/build provenance
                model_build_hash_b3: None,
                adapter_build_hash_b3: None,
                // V7: Decoder config
                decode_algo: None,
                temperature_q15: None,
                top_p_q15: None,
                top_k: None,
                seed_digest_b3: None,
                sampling_backend: None,
                // V7: Concurrency determinism
                thread_count: None,
                reduction_strategy: None,
                // V7: Stop controller inputs
                stop_eos_q15: None,
                stop_window_digest_b3: None,
                // V7: Cache proof
                cache_scope: None,
                cached_prefix_digest_b3: None,
                cached_prefix_len: None,
                cache_key_b3: None,
                // V7: Retrieval/tool binding
                retrieval_merkle_root_b3: None,
                retrieval_order_digest_b3: None,
                tool_call_inputs_digest_b3: None,
                tool_call_outputs_digest_b3: None,
                // V7: Disclosure level
                disclosure_level: None,
                // V7: Receipt signing metadata
                receipt_signing_kid: None,
                receipt_signed_at: None,
            })
            .await;

        // Corrupt one row with arbitrary fuzz data to exercise decode robustness
        let blob: Vec<u8> = data.iter().cloned().take(128).collect();
        let _ = adapteros_db::sqlx::query(
            "UPDATE inference_trace_tokens SET selected_adapter_ids = ?, gates_q15 = ? WHERE trace_id = ? AND token_index = 0",
        )
        .bind(blob.clone())
        .bind(blob)
        .bind(&trace_id)
        .execute(db.pool_result()?)
        .await;

        // Receipt recomputation exercises decoding paths; errors are fine, panics are not
        let _ = recompute_receipt(&db, &trace_id).await;

        Ok::<(), adapteros_core::AosError>(())
    });
});
