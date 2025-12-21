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
        let trace_id = format!("verify-{}", rng.gen::<u64>());
        let context_digest = B3Hash::hash(&rng.gen::<u128>().to_le_bytes()).to_bytes();

        let start = TraceStart {
            trace_id: trace_id.clone(),
            tenant_id: "tenant-verify".to_string(),
            request_id: Some("req".to_string()),
            context_digest,
        };

        let mut sink = SqlTraceSink::new(db.clone(), start, 4).await?;
        let adapter_ids = vec!["adapter-x".to_string(), "adapter-y".to_string()];
        let gates_q15 = vec![12345i16, -2345i16];

        // Single token to keep runtime small
        let token = TraceTokenInput {
            token_index: 0,
            adapter_ids: adapter_ids.clone(),
            gates_q15: gates_q15.clone(),
            policy_mask_digest: None,
            allowed_mask: Some(vec![true, true]),
            policy_overrides_applied: None,
            backend_id: Some("coreml".to_string()),
            kernel_version_id: Some("k1".to_string()),
        };
        let _ = sink.record_token(token).await;
        let output_tokens = [rng.gen()];

        // PRD-06: Fuzz with random model_cache_identity_v2_digest_b3 values
        let model_cache_identity_v2_digest_b3 = if rng.gen::<bool>() {
            // 50% chance of having a V2 digest
            let digest_bytes: [u8; 32] = rng.gen();
            Some(B3Hash::from_bytes(digest_bytes))
        } else {
            None
        };

        let _ = sink
            .finalize(TraceFinalization {
                output_tokens: &output_tokens,
                logical_prompt_tokens: 0,
                prefix_cached_token_count: 0,
                billed_input_tokens: 0,
                logical_output_tokens: output_tokens.len() as u32,
                billed_output_tokens: output_tokens.len() as u32,
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
                model_cache_identity_v2_digest_b3,
            })
            .await;

        // Corrupt stored receipt data to fuzz verification
        let corrupt_hash: Vec<u8> = data.iter().cloned().take(64).collect();
        let _ = adapteros_db::sqlx::query(
            "UPDATE inference_trace_receipts SET receipt_digest = ?, run_head_hash = ? WHERE trace_id = ?",
        )
        .bind(corrupt_hash.clone())
        .bind(corrupt_hash)
        .bind(&trace_id)
        .execute(db.pool())
        .await;

        // Trigger recomputation; mismatches/errors are fine, panics are not
        let _ = recompute_receipt(&db, &trace_id).await;

        Ok::<(), adapteros_core::AosError>(())
    });
});
