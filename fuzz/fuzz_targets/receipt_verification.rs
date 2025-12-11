#![no_main]

use adapteros_core::B3Hash;
use adapteros_db::{recompute_receipt, SqlTraceSink, TraceStart, TraceTokenInput};
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
            backend_id: Some("coreml".to_string()),
            kernel_version_id: Some("k1".to_string()),
        };
        let _ = sink.record_token(token).await;
        let _ = sink.finalize(&[rng.gen()]).await;

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
