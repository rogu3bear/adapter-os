//! Cache attestation enforcement regression tests (P0-1).
//!
//! Non-goals:
//! - End-to-end inference
//! - Real workers/GPUs
//! - Timing assertions

use adapteros_core::{cache_attestation::CacheAttestationBuilder, B3Hash};
use adapteros_db::{
    inference_trace::{SqlTraceSink, TraceFinalization, TraceSink, TraceStart},
    Db,
};
use std::sync::Arc;

const TEST_TENANT_ID: &str = "tenant-test-0001";

async fn new_test_db_with_tenant() -> Arc<Db> {
    let db = Db::new_in_memory().await.unwrap();

    // Deterministic tenant id: avoid RNG-based IDs in the regression harness.
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
        .bind(TEST_TENANT_ID)
        .bind("Test Tenant")
        .bind(false)
        .execute(db.pool())
        .await
        .unwrap();

    Arc::new(db)
}

fn test_trace_start() -> TraceStart {
    TraceStart {
        trace_id: "trace-test-0001".to_string(),
        tenant_id: TEST_TENANT_ID.to_string(),
        request_id: None,
        context_digest: [0xABu8; 32],
        stack_id: None,
        model_id: None,
        policy_id: None,
    }
}

fn minimal_finalization<'a>(output_tokens: &'a [u32]) -> TraceFinalization<'a> {
    TraceFinalization {
        output_tokens,
        logical_prompt_tokens: 100,
        prefix_cached_token_count: 0,
        billed_input_tokens: 100,
        logical_output_tokens: 0,
        billed_output_tokens: 0,
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
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
        cache_attestation: None,
        worker_public_key: None,
        copy_bytes: None,
        tokenizer_hash_b3: None,
        tokenizer_version: None,
        tokenizer_normalization: None,
        model_build_hash_b3: None,
        adapter_build_hash_b3: None,
        decode_algo: None,
        temperature_q15: None,
        top_p_q15: None,
        top_k: None,
        seed_digest_b3: None,
        sampling_backend: None,
        thread_count: None,
        reduction_strategy: None,
        stop_eos_q15: None,
        stop_window_digest_b3: None,
        cache_scope: None,
        cached_prefix_digest_b3: None,
        cached_prefix_len: None,
        cache_key_b3: None,
        retrieval_merkle_root_b3: None,
        retrieval_order_digest_b3: None,
        tool_call_inputs_digest_b3: None,
        tool_call_outputs_digest_b3: None,
        disclosure_level: None,
        receipt_signing_kid: None,
        receipt_signed_at: None,
    }
}

fn ed25519_public_key_from_seed(seed: &[u8; 32]) -> [u8; 32] {
    use ed25519_dalek::SigningKey;
    let signing_key = SigningKey::from_bytes(seed);
    signing_key.verifying_key().to_bytes()
}

#[tokio::test(flavor = "current_thread")]
async fn missing_attestation_hard_fails_when_cached_tokens_gt_zero() {
    let db = new_test_db_with_tenant().await;
    let mut sink = SqlTraceSink::new(db, test_trace_start(), 1024)
        .await
        .unwrap();

    let out: [u32; 0] = [];
    let mut finalization = minimal_finalization(&out);
    finalization.prefix_cached_token_count = 10;
    finalization.billed_input_tokens = 90; // logical_prompt_tokens - cached

    let err = sink.finalize(finalization).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("cache_attestation required when prefix_cached_token_count > 0"),
        "unexpected error: {msg}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn bad_signature_is_rejected_for_cached_tokens() {
    let db = new_test_db_with_tenant().await;
    let mut sink = SqlTraceSink::new(db, test_trace_start(), 1024)
        .await
        .unwrap();

    let cache_key = B3Hash::hash(b"cache-key-1");

    let signing_seed = [0x11u8; 32];
    let good_public_key = ed25519_public_key_from_seed(&signing_seed);

    let attestation = CacheAttestationBuilder::new()
        .cache_key_b3(&cache_key)
        .token_count(10)
        .worker_id("worker-001")
        .timestamp_tick(42)
        .build_and_sign(&signing_seed)
        .unwrap();

    // Verify with a different public key so signature verification fails.
    let wrong_public_key = ed25519_public_key_from_seed(&[0x22u8; 32]);
    assert_ne!(wrong_public_key, good_public_key);

    let out: [u32; 0] = [];
    let mut finalization = minimal_finalization(&out);
    finalization.prefix_cached_token_count = 10;
    finalization.billed_input_tokens = 90;
    finalization.prefix_kv_key_b3 = Some(cache_key);
    finalization.cache_attestation = Some(attestation);
    finalization.worker_public_key = Some(wrong_public_key);

    let err = sink.finalize(finalization).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("cache attestation verification failed"),
        "unexpected error: {msg}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn token_count_mismatch_is_rejected_for_cached_tokens() {
    let db = new_test_db_with_tenant().await;
    let mut sink = SqlTraceSink::new(db, test_trace_start(), 1024)
        .await
        .unwrap();

    let cache_key = B3Hash::hash(b"cache-key-1");

    let signing_seed = [0x33u8; 32];
    let public_key = ed25519_public_key_from_seed(&signing_seed);

    let attestation = CacheAttestationBuilder::new()
        .cache_key_b3(&cache_key)
        .token_count(9) // mismatch
        .worker_id("worker-001")
        .timestamp_tick(42)
        .build_and_sign(&signing_seed)
        .unwrap();

    let out: [u32; 0] = [];
    let mut finalization = minimal_finalization(&out);
    finalization.prefix_cached_token_count = 10;
    finalization.billed_input_tokens = 90;
    finalization.prefix_kv_key_b3 = Some(cache_key);
    finalization.cache_attestation = Some(attestation);
    finalization.worker_public_key = Some(public_key);

    let err = sink.finalize(finalization).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains(
            "cache attestation token_count (9) does not match prefix_cached_token_count (10)"
        ),
        "unexpected error: {msg}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cache_key_mismatch_is_rejected_when_prefix_kv_key_present() {
    let db = new_test_db_with_tenant().await;
    let mut sink = SqlTraceSink::new(db, test_trace_start(), 1024)
        .await
        .unwrap();

    let prefix_kv_key = B3Hash::hash(b"cache-key-expected");
    let attested_key = B3Hash::hash(b"cache-key-attested");
    assert_ne!(prefix_kv_key, attested_key);

    let signing_seed = [0x44u8; 32];
    let public_key = ed25519_public_key_from_seed(&signing_seed);

    let attestation = CacheAttestationBuilder::new()
        .cache_key_b3(&attested_key)
        .token_count(10)
        .worker_id("worker-001")
        .timestamp_tick(42)
        .build_and_sign(&signing_seed)
        .unwrap();

    let out: [u32; 0] = [];
    let mut finalization = minimal_finalization(&out);
    finalization.prefix_cached_token_count = 10;
    finalization.billed_input_tokens = 90;
    finalization.prefix_kv_key_b3 = Some(prefix_kv_key);
    finalization.cache_attestation = Some(attestation);
    finalization.worker_public_key = Some(public_key);

    let err = sink.finalize(finalization).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("cache attestation cache_key_hash does not match prefix_kv_key_b3"),
        "unexpected error: {msg}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn valid_attestation_allows_nonzero_cache_credits() {
    let db = new_test_db_with_tenant().await;
    let mut sink = SqlTraceSink::new(db, test_trace_start(), 1024)
        .await
        .unwrap();

    let cache_key = B3Hash::hash(b"cache-key-1");

    let signing_seed = [0x55u8; 32];
    let public_key = ed25519_public_key_from_seed(&signing_seed);

    let attestation = CacheAttestationBuilder::new()
        .cache_key_b3(&cache_key)
        .token_count(10)
        .worker_id("worker-001")
        .timestamp_tick(42)
        .build_and_sign(&signing_seed)
        .unwrap();

    let out: [u32; 0] = [];
    let mut finalization = minimal_finalization(&out);
    finalization.prefix_cached_token_count = 10;
    finalization.billed_input_tokens = 90;
    finalization.prefix_kv_key_b3 = Some(cache_key);
    finalization.cache_attestation = Some(attestation);
    finalization.worker_public_key = Some(public_key);

    let receipt = sink.finalize(finalization).await.unwrap();
    assert_eq!(receipt.prefix_cached_token_count, 10);
}
