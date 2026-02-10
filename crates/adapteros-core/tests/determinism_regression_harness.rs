//! Minimal determinism regression harness.
//!
//! Non-goals:
//! - End-to-end inference
//! - Real workers/GPUs
//! - Timing assertions

use adapteros_core::{
    context_id::{compute_context_id, AdapterEntry, InferenceConfig},
    decode_q15_gate, encode_q15_gate,
    receipt_digest::{compute_receipt_digest, ReceiptDigestInput, RECEIPT_SCHEMA_V7},
    B3Hash, Q15_GATE_DENOMINATOR,
};

#[test]
fn stable_id_order_independence_context_id() {
    let base_model_hash = B3Hash::hash(b"model:v1");

    let a1 = AdapterEntry {
        stable_id: 2,
        hash_b3: B3Hash::hash(b"adapter:two"),
    };
    let a2 = AdapterEntry {
        stable_id: 1,
        hash_b3: B3Hash::hash(b"adapter:one"),
    };

    let config = InferenceConfig {
        temperature: Some(0.7),
        top_k: Some(40),
        max_tokens: Some(2048),
    };

    let ctx_12 = compute_context_id(&base_model_hash, &[a1.clone(), a2.clone()], &config);
    let ctx_21 = compute_context_id(&base_model_hash, &[a2, a1], &config);

    assert_eq!(
        ctx_12, ctx_21,
        "Input order must not affect context_id (adapters sorted by stable_id)"
    );
}

#[test]
fn q15_denominator_invariant() {
    assert_eq!(
        Q15_GATE_DENOMINATOR.to_bits(),
        32767.0_f32.to_bits(),
        "Q15 denominator MUST be 32767.0 for determinism"
    );
}

#[test]
fn q15_gate_roundtrip_is_deterministic() {
    let values = [0.0f32, 0.25, 0.5, 0.75, 1.0, 0.333333, 0.666666];
    for v in values {
        let q = encode_q15_gate(v);
        let restored = decode_q15_gate(q);
        let q2 = encode_q15_gate(restored);
        assert_eq!(q, q2, "Q15 roundtrip drifted for value={}", v);
    }
}

#[test]
fn v7_receipt_digest_golden() {
    // Golden vector covering most V7 fields (intentional brittleness).
    let input = ReceiptDigestInput::new([1u8; 32], [2u8; 32], [3u8; 32], 100, 10, 90, 50, 50)
        .with_stop_controller(Some("EOS".to_string()), Some(45), Some([4u8; 32]))
        .with_kv_quota(
            1024 * 1024,
            512 * 1024,
            0,
            Some("default".to_string()),
            true,
        )
        .with_prefix_cache(Some([5u8; 32]), true, 256 * 1024)
        .with_model_cache_identity(Some([6u8; 32]))
        .with_equipment_profile(
            Some([7u8; 32]),
            Some("Apple M4 Max:stepping-1".to_string()),
            Some("0.21.0".to_string()),
            Some("ANEv4-38core".to_string()),
        )
        .with_citations(Some([8u8; 32]), 5)
        .with_cross_run_lineage(Some([9u8; 32]), 42)
        .with_tokenizer_identity(
            Some([10u8; 32]),
            Some("qwen2.5".to_string()),
            Some("nfkc".to_string()),
        )
        .with_build_provenance(Some([11u8; 32]), Some([12u8; 32]))
        .with_decoder_config(
            Some("sampling".to_string()),
            Some(1234),
            Some(2345),
            Some(64),
            Some([13u8; 32]),
            Some("coreml".to_string()),
        )
        .with_concurrency_determinism(Some(8), Some("fixed".to_string()))
        .with_stop_controller_inputs(Some(2048), Some([14u8; 32]))
        .with_cache_proof(
            Some("global".to_string()),
            Some([15u8; 32]),
            Some(128),
            Some([16u8; 32]),
        )
        .with_retrieval_tool_binding(
            Some([17u8; 32]),
            Some([18u8; 32]),
            Some([19u8; 32]),
            Some([20u8; 32]),
        )
        .with_disclosure_level(Some("full".to_string()));

    let digest = compute_receipt_digest(&input, RECEIPT_SCHEMA_V7).unwrap();

    // NOTE: update only with proof (explicit intent: deterministic regression gate).
    const EXPECTED_HEX: &str = "5bf95ce5154a43faafcaebe4f02bec06c041ea91e89930b322b9e60349f9d3ee";
    assert_eq!(
        digest.to_hex(),
        EXPECTED_HEX,
        "V7 receipt digest golden drifted; this is a determinism-breaking change.\nactual={}\nexpected={}",
        digest.to_hex(),
        EXPECTED_HEX
    );
}

#[test]
fn stop_reason_token_index_none_uses_u32_max_sentinel_v7() {
    let base = ReceiptDigestInput::new([0xAAu8; 32], [0xBBu8; 32], [0xCCu8; 32], 1, 0, 1, 0, 0);

    let none = base.clone().with_stop_controller(None, None, None);
    let sentinel = base
        .clone()
        .with_stop_controller(None, Some(u32::MAX), None);

    let d_none = compute_receipt_digest(&none, RECEIPT_SCHEMA_V7).unwrap();
    let d_sentinel = compute_receipt_digest(&sentinel, RECEIPT_SCHEMA_V7).unwrap();

    assert_eq!(
        d_none, d_sentinel,
        "None stop_reason_token_index must encode as 0xFFFFFFFF sentinel"
    );
}

#[test]
fn stop_eos_q15_none_uses_i16_min_sentinel_v7() {
    let base = ReceiptDigestInput::new([0x11u8; 32], [0x22u8; 32], [0x33u8; 32], 1, 0, 1, 0, 0);

    let none = base.clone().with_stop_controller_inputs(None, None);
    let sentinel = base
        .clone()
        .with_stop_controller_inputs(Some(i16::MIN), None);

    let d_none = compute_receipt_digest(&none, RECEIPT_SCHEMA_V7).unwrap();
    let d_sentinel = compute_receipt_digest(&sentinel, RECEIPT_SCHEMA_V7).unwrap();

    assert_eq!(
        d_none, d_sentinel,
        "None stop_eos_q15 must encode as i16::MIN sentinel"
    );
}
