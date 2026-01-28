#![no_main]

//! Fuzz target for receipt digest parsing and validation.
//!
//! This target tests security-critical parsing paths:
//! - ReceiptDigestInput construction with malformed data
//! - Digest computation for all schema versions (V1-V5)
//! - Blob encoding/decoding (adapter IDs, gates, masks)
//! - Hash chain operations
//!
//! Goal: Ensure malformed inputs never cause panics.

use adapteros_core::receipt_digest::{
    compute_output_digest, compute_receipt_digest, decode_allowed_mask, encode_adapter_ids,
    encode_allowed_mask, encode_gates_q15, hash_token_decision, update_run_head,
    ReceiptDigestInput, RECEIPT_SCHEMA_CURRENT, RECEIPT_SCHEMA_V1, RECEIPT_SCHEMA_V2,
    RECEIPT_SCHEMA_V3, RECEIPT_SCHEMA_V4, RECEIPT_SCHEMA_V5,
};
use adapteros_core::B3Hash;
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

/// Generate a potentially malformed 32-byte array from fuzzer input
fn next_hash_bytes(u: &mut Unstructured<'_>) -> Option<[u8; 32]> {
    u.arbitrary().ok()
}

/// Generate an optional string with potential edge cases
fn next_string(u: &mut Unstructured<'_>, max_len: usize) -> Option<Option<String>> {
    let include = u.arbitrary::<bool>().ok()?;
    if !include {
        return Some(None);
    }
    let len = u.int_in_range::<usize>(0..=max_len).ok()?;
    if len == 0 {
        return Some(Some(String::new()));
    }
    let bytes: Vec<u8> = (0..len).filter_map(|_| u.arbitrary::<u8>().ok()).collect();
    // Allow invalid UTF-8 to be handled gracefully
    Some(String::from_utf8(bytes).ok())
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // Test 1: ReceiptDigestInput with arbitrary bytes
    let context_digest = match next_hash_bytes(&mut u) {
        Some(v) => v,
        None => return,
    };
    let run_head_hash = match next_hash_bytes(&mut u) {
        Some(v) => v,
        None => return,
    };
    let output_digest = match next_hash_bytes(&mut u) {
        Some(v) => v,
        None => return,
    };

    let logical_prompt_tokens = u.arbitrary::<u32>().unwrap_or(0);
    let prefix_cached_token_count = u.arbitrary::<u32>().unwrap_or(0);
    let billed_input_tokens = u.arbitrary::<u32>().unwrap_or(0);
    let logical_output_tokens = u.arbitrary::<u32>().unwrap_or(0);
    let billed_output_tokens = u.arbitrary::<u32>().unwrap_or(0);

    // Create base input
    let mut input = ReceiptDigestInput::new(
        context_digest,
        run_head_hash,
        output_digest,
        logical_prompt_tokens,
        prefix_cached_token_count,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
    );

    // Add optional V2+ fields
    if let Some(backend) = next_string(&mut u, 64).flatten() {
        let attestation = next_hash_bytes(&mut u);
        input = input.with_backend(Some(backend), attestation);
    }

    // Add optional V3+ fields
    if u.arbitrary::<bool>().unwrap_or(false) {
        let root_seed = next_hash_bytes(&mut u);
        let seed_mode = next_string(&mut u, 32).flatten();
        let has_manifest = u.arbitrary::<bool>().ok();
        input = input.with_seed_lineage(root_seed, seed_mode, has_manifest);
    }

    // Add optional V4 stop controller fields
    if u.arbitrary::<bool>().unwrap_or(false) {
        let stop_code = next_string(&mut u, 32).flatten();
        let stop_idx = u.arbitrary::<u32>().ok();
        let stop_policy = next_hash_bytes(&mut u);
        input = input.with_stop_controller(stop_code, stop_idx, stop_policy);
    }

    // Add optional V4 KV quota fields
    if u.arbitrary::<bool>().unwrap_or(false) {
        let quota_bytes = u.arbitrary::<u64>().unwrap_or(0);
        let used_bytes = u.arbitrary::<u64>().unwrap_or(0);
        let evictions = u.arbitrary::<u32>().unwrap_or(0);
        let policy_id = next_string(&mut u, 64).flatten();
        let enforced = u.arbitrary::<bool>().unwrap_or(false);
        input = input.with_kv_quota(quota_bytes, used_bytes, evictions, policy_id, enforced);
    }

    // Add optional V4 prefix cache fields
    if u.arbitrary::<bool>().unwrap_or(false) {
        let prefix_key = next_hash_bytes(&mut u);
        let cache_hit = u.arbitrary::<bool>().unwrap_or(false);
        let prefix_bytes = u.arbitrary::<u64>().unwrap_or(0);
        input = input.with_prefix_cache(prefix_key, cache_hit, prefix_bytes);
    }

    // Add optional V4 model cache identity
    if u.arbitrary::<bool>().unwrap_or(false) {
        let model_cache = next_hash_bytes(&mut u);
        input = input.with_model_cache_identity(model_cache);
    }

    // Add optional V5 equipment profile fields
    if u.arbitrary::<bool>().unwrap_or(false) {
        let equipment_digest = next_hash_bytes(&mut u);
        let processor_id = next_string(&mut u, 64).flatten();
        let mlx_version = next_string(&mut u, 32).flatten();
        let ane_version = next_string(&mut u, 32).flatten();
        input =
            input.with_equipment_profile(equipment_digest, processor_id, mlx_version, ane_version);
    }

    // Add optional V5 citation binding
    if u.arbitrary::<bool>().unwrap_or(false) {
        let citations_root = next_hash_bytes(&mut u);
        let citation_count = u.arbitrary::<u32>().unwrap_or(0);
        input = input.with_citations(citations_root, citation_count);
    }

    // Test 2: Compute digest for all schema versions - should never panic
    let versions = [
        RECEIPT_SCHEMA_V1,
        RECEIPT_SCHEMA_V2,
        RECEIPT_SCHEMA_V3,
        RECEIPT_SCHEMA_V4,
        RECEIPT_SCHEMA_V5,
        RECEIPT_SCHEMA_CURRENT,
        0,   // Invalid low
        99,  // Invalid high
        255, // Max u8
    ];

    for version in versions {
        let result = compute_receipt_digest(&input, version);
        // Valid versions should return Some, invalid should return None
        match version {
            RECEIPT_SCHEMA_V1 | RECEIPT_SCHEMA_V2 | RECEIPT_SCHEMA_V3 | RECEIPT_SCHEMA_V4
            | RECEIPT_SCHEMA_V5 => {
                assert!(
                    result.is_some(),
                    "Valid schema version {} should succeed",
                    version
                );
            }
            _ => {
                // Invalid versions may return None - that's expected
            }
        }
    }

    // Test 3: Output digest with arbitrary tokens
    let token_count = u.int_in_range::<usize>(0..=1000).unwrap_or(0);
    let tokens: Vec<u32> = (0..token_count)
        .filter_map(|_| u.arbitrary::<u32>().ok())
        .collect();

    let digest1 = compute_output_digest(&tokens);
    let digest2 = compute_output_digest(&tokens);
    assert_eq!(digest1, digest2, "Output digest must be deterministic");

    // Test 4: Adapter ID encoding with edge cases
    let adapter_count = u.int_in_range::<usize>(0..=100).unwrap_or(0);
    let mut adapter_ids = Vec::with_capacity(adapter_count);
    for i in 0..adapter_count {
        let id_bytes: Vec<u8> = (0..u.int_in_range::<usize>(0..=64).unwrap_or(0))
            .filter_map(|_| u.arbitrary::<u8>().ok())
            .collect();
        // Use valid UTF-8 or fallback
        let id = String::from_utf8(id_bytes).unwrap_or_else(|_| format!("adapter-{}", i));
        adapter_ids.push(id);
    }

    let encoded = encode_adapter_ids(&adapter_ids);
    // Encoding should never panic and produce deterministic output
    let encoded2 = encode_adapter_ids(&adapter_ids);
    assert_eq!(
        encoded, encoded2,
        "Adapter ID encoding must be deterministic"
    );

    // Test 5: Q15 gate encoding with edge cases
    let gate_count = u.int_in_range::<usize>(0..=100).unwrap_or(0);
    let gates: Vec<i16> = (0..gate_count)
        .filter_map(|_| u.arbitrary::<i16>().ok())
        .collect();

    let encoded_gates = encode_gates_q15(&gates);
    let encoded_gates2 = encode_gates_q15(&gates);
    assert_eq!(
        encoded_gates, encoded_gates2,
        "Gate encoding must be deterministic"
    );

    // Test 6: Allowed mask encoding/decoding roundtrip
    let mask_count = u.int_in_range::<usize>(0..=100).unwrap_or(0);
    let mask: Vec<bool> = (0..mask_count)
        .filter_map(|_| u.arbitrary::<bool>().ok())
        .collect();

    let encoded_mask = encode_allowed_mask(&mask);
    let decoded = decode_allowed_mask(&encoded_mask);
    assert!(decoded.is_ok(), "Valid encoded mask should decode");
    assert_eq!(decoded.unwrap(), mask, "Mask roundtrip should be lossless");

    // Test 7: Malformed mask decoding - should not panic
    let malformed_len = u.int_in_range::<usize>(0..=200).unwrap_or(0);
    let malformed_bytes: Vec<u8> = (0..malformed_len)
        .filter_map(|_| u.arbitrary::<u8>().ok())
        .collect();
    let _ = decode_allowed_mask(&malformed_bytes); // May fail, but should not panic

    // Test 8: Token decision hashing
    let token_index = u.arbitrary::<u32>().unwrap_or(0);
    let policy_mask_digest = if u.arbitrary::<bool>().unwrap_or(false) {
        next_hash_bytes(&mut u)
    } else {
        None
    };
    let allowed_mask_bytes = if u.arbitrary::<bool>().unwrap_or(false) {
        Some(encode_allowed_mask(&mask))
    } else {
        None
    };
    let policy_overrides = next_string(&mut u, 256).flatten();
    let backend_id = next_string(&mut u, 32).flatten();
    let kernel_version = next_string(&mut u, 32).flatten();

    let decision_hash = hash_token_decision(
        &context_digest,
        token_index,
        &encoded,
        &encoded_gates,
        policy_mask_digest,
        allowed_mask_bytes.as_deref(),
        policy_overrides.as_deref(),
        backend_id.as_deref(),
        kernel_version.as_deref(),
    );

    // Decision hash should be deterministic
    let decision_hash2 = hash_token_decision(
        &context_digest,
        token_index,
        &encoded,
        &encoded_gates,
        policy_mask_digest,
        allowed_mask_bytes.as_deref(),
        policy_overrides.as_deref(),
        backend_id.as_deref(),
        kernel_version.as_deref(),
    );
    assert_eq!(
        decision_hash, decision_hash2,
        "Decision hash must be deterministic"
    );

    // Test 9: Run head chain updates
    let prev_hash = B3Hash::from_bytes(context_digest);
    let updated = update_run_head(&prev_hash, token_index, &decision_hash);
    let updated2 = update_run_head(&prev_hash, token_index, &decision_hash);
    assert_eq!(updated, updated2, "Run head update must be deterministic");

    // Test 10: Empty inputs edge cases
    let empty_ids: Vec<String> = vec![];
    let _ = encode_adapter_ids(&empty_ids);

    let empty_gates: Vec<i16> = vec![];
    let _ = encode_gates_q15(&empty_gates);

    let empty_mask: Vec<bool> = vec![];
    let encoded_empty = encode_allowed_mask(&empty_mask);
    let _ = decode_allowed_mask(&encoded_empty);

    let empty_tokens: Vec<u32> = vec![];
    let _ = compute_output_digest(&empty_tokens);
});
