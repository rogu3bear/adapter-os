#![no_main]

use adapteros_core::evidence_envelope::{
    BundleMetadataRef, EvidenceEnvelope, EvidenceScope, InferenceReceiptRef, PolicyAuditRef,
};
use adapteros_core::B3Hash;
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

/// Fuzz evidence envelope creation, canonical bytes, and validation
///
/// Tests:
/// - Envelope creation for all three scopes (telemetry, policy, inference)
/// - Canonical byte encoding determinism
/// - Validation logic
/// - Digest computation
/// - Chain linking with various previous_root combinations
fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // Select scope randomly
    let scope_selector = match u.int_in_range::<u8>(0..=2) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Generate previous_root (50% chance)
    let previous_root = if u.arbitrary::<bool>().unwrap_or(false) {
        let bytes: [u8; 32] = match u.arbitrary() {
            Ok(b) => b,
            Err(_) => return,
        };
        Some(B3Hash::from_bytes(bytes))
    } else {
        None
    };

    let envelope = match scope_selector {
        0 => {
            // Telemetry envelope
            let bundle_hash_bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
            let merkle_root_bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
            let event_count: u32 = u.arbitrary().unwrap_or(0);

            let cpid = if u.arbitrary::<bool>().unwrap_or(false) {
                let len = u.int_in_range::<usize>(1..=64).unwrap_or(8);
                let id_bytes: Vec<u8> = (0..len).filter_map(|_| u.arbitrary::<u8>().ok()).collect();
                if id_bytes.is_empty() {
                    None
                } else {
                    String::from_utf8(id_bytes).ok()
                }
            } else {
                None
            };

            let sequence_no = if u.arbitrary::<bool>().unwrap_or(false) {
                Some(u.arbitrary::<u64>().unwrap_or(0))
            } else {
                None
            };

            let bundle_ref = BundleMetadataRef {
                bundle_hash: B3Hash::from_bytes(bundle_hash_bytes),
                merkle_root: B3Hash::from_bytes(merkle_root_bytes),
                event_count,
                cpid,
                sequence_no,
            };

            EvidenceEnvelope::new_telemetry("tenant-fuzz".to_string(), bundle_ref, previous_root)
        }
        1 => {
            // Policy envelope
            let decision_id_len = u.int_in_range::<usize>(1..=32).unwrap_or(8);
            let decision_id_bytes: Vec<u8> = (0..decision_id_len)
                .filter_map(|_| u.arbitrary::<u8>().ok())
                .collect();
            let decision_id =
                String::from_utf8(decision_id_bytes).unwrap_or_else(|_| "dec-fuzz".to_string());

            let entry_hash_bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
            let chain_sequence: i64 = u.arbitrary().unwrap_or(0);

            let pack_id_bytes: Vec<u8> = (0..8).filter_map(|_| u.arbitrary::<u8>().ok()).collect();
            let policy_pack_id =
                String::from_utf8(pack_id_bytes).unwrap_or_else(|_| "pack-fuzz".to_string());

            let hook = match u.int_in_range::<u8>(0..=2).unwrap_or(0) {
                0 => "OnBeforeInference",
                1 => "OnAfterInference",
                _ => "OnRequestBeforeRouting",
            }
            .to_string();

            let decision = if u.arbitrary::<bool>().unwrap_or(true) {
                "allow"
            } else {
                "deny"
            }
            .to_string();

            let policy_ref = PolicyAuditRef {
                decision_id,
                entry_hash: B3Hash::from_bytes(entry_hash_bytes),
                chain_sequence,
                policy_pack_id,
                hook,
                decision,
            };

            EvidenceEnvelope::new_policy("tenant-fuzz".to_string(), policy_ref, previous_root)
        }
        _ => {
            // Inference envelope
            let trace_id_len = u.int_in_range::<usize>(1..=32).unwrap_or(8);
            let trace_id_bytes: Vec<u8> = (0..trace_id_len)
                .filter_map(|_| u.arbitrary::<u8>().ok())
                .collect();
            let trace_id =
                String::from_utf8(trace_id_bytes).unwrap_or_else(|_| "trace-fuzz".to_string());

            let run_head_hash: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
            let output_digest: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
            let receipt_digest: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);

            let logical_prompt_tokens: u32 = u.arbitrary().unwrap_or(0);
            let prefix_cached_token_count: u32 = u.arbitrary().unwrap_or(0);
            let billed_input_tokens: u32 =
                logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
            let logical_output_tokens: u32 = u.arbitrary().unwrap_or(0);
            let billed_output_tokens: u32 = logical_output_tokens;

            let stop_reason_code = if u.arbitrary::<bool>().unwrap_or(false) {
                Some("BUDGET_MAX".to_string())
            } else {
                None
            };

            let stop_reason_token_index = if u.arbitrary::<bool>().unwrap_or(false) {
                Some(u.arbitrary::<u32>().unwrap_or(0))
            } else {
                None
            };

            let stop_policy_digest_b3 = if u.arbitrary::<bool>().unwrap_or(false) {
                let bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
                Some(B3Hash::from_bytes(bytes))
            } else {
                None
            };

            let model_cache_identity_v2_digest_b3 = if u.arbitrary::<bool>().unwrap_or(false) {
                let bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
                Some(B3Hash::from_bytes(bytes))
            } else {
                None
            };

            // PRD-DET-001: Generate backend identity fields for fuzz testing
            let backend_used = match u.int_in_range::<u8>(0..=2).unwrap_or(0) {
                0 => "metal".to_string(),
                1 => "coreml".to_string(),
                _ => "mlx".to_string(),
            };
            let backend_attestation_b3 = if u.arbitrary::<bool>().unwrap_or(false) {
                let bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
                Some(B3Hash::from_bytes(bytes))
            } else {
                None
            };

            let seed_lineage_hash = if u.arbitrary::<bool>().unwrap_or(false) {
                let bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
                Some(B3Hash::from_bytes(bytes))
            } else {
                None
            };
            let adapter_training_lineage_digest = if u.arbitrary::<bool>().unwrap_or(false) {
                let bytes: [u8; 32] = u.arbitrary().unwrap_or([0u8; 32]);
                Some(B3Hash::from_bytes(bytes))
            } else {
                None
            };

            let inference_ref = InferenceReceiptRef {
                trace_id,
                run_head_hash: B3Hash::from_bytes(run_head_hash),
                output_digest: B3Hash::from_bytes(output_digest),
                receipt_digest: B3Hash::from_bytes(receipt_digest),
                logical_prompt_tokens,
                prefix_cached_token_count,
                billed_input_tokens,
                logical_output_tokens,
                billed_output_tokens,
                stop_reason_code,
                stop_reason_token_index,
                stop_policy_digest_b3,
                model_cache_identity_v2_digest_b3,
                backend_used,
                backend_attestation_b3,
                seed_lineage_hash,
                adapter_training_lineage_digest,
                // V6 cross-run lineage
                previous_receipt_digest: None,
                session_sequence: 0,
            };

            EvidenceEnvelope::new_inference(
                "tenant-fuzz".to_string(),
                inference_ref,
                previous_root,
            )
        }
    };

    // Test validation - should not panic
    let _ = envelope.validate();

    // Test canonical bytes encoding - should be deterministic
    let canonical1 = envelope.to_canonical_bytes();
    let canonical2 = envelope.to_canonical_bytes();
    assert_eq!(
        canonical1, canonical2,
        "Canonical bytes must be deterministic"
    );

    // Test digest computation
    let digest1 = envelope.digest();
    let digest2 = envelope.digest();
    assert_eq!(digest1, digest2, "Digest must be deterministic");

    // Test JSON roundtrip
    if let Ok(json) = serde_json::to_string(&envelope) {
        let _ = serde_json::from_str::<EvidenceEnvelope>(&json);
    }

    // Test scope matching
    match envelope.scope {
        EvidenceScope::Telemetry => {
            assert!(envelope.bundle_metadata_ref.is_some());
            assert!(envelope.policy_audit_ref.is_none());
            assert!(envelope.inference_receipt_ref.is_none());
        }
        EvidenceScope::Policy => {
            assert!(envelope.bundle_metadata_ref.is_none());
            assert!(envelope.policy_audit_ref.is_some());
            assert!(envelope.inference_receipt_ref.is_none());
        }
        EvidenceScope::Inference => {
            assert!(envelope.bundle_metadata_ref.is_none());
            assert!(envelope.policy_audit_ref.is_none());
            assert!(envelope.inference_receipt_ref.is_some());
        }
    }
});
