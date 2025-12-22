#![no_main]

use adapteros_core::evidence_envelope::{BundleMetadataRef, EvidenceEnvelope};
use adapteros_core::evidence_verifier::EvidenceVerifier;
use adapteros_core::B3Hash;
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

/// Fuzz evidence chain verification logic
///
/// Tests:
/// - Chain verification with valid chains
/// - Chain verification with broken links
/// - Chain verification with corrupted roots
/// - Single envelope verification
/// - Empty chain handling
fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // Generate chain length (0-8 envelopes)
    let chain_len = match u.int_in_range::<usize>(0..=8) {
        Ok(v) => v,
        Err(_) => return,
    };

    if chain_len == 0 {
        // Test empty chain
        let verifier = EvidenceVerifier::new();
        let result = verifier.verify_chain(&[]);
        assert!(result.is_ok());
        if let Ok(r) = result {
            assert!(r.is_valid);
            assert_eq!(r.envelopes_checked, 0);
        }
        return;
    }

    let mut envelopes = Vec::with_capacity(chain_len);
    let mut previous_root: Option<B3Hash> = None;

    for i in 0..chain_len {
        let bundle_hash_bytes: [u8; 32] = u.arbitrary().unwrap_or_else(|_| {
            let mut b = [0u8; 32];
            b[0] = i as u8;
            b
        });
        let merkle_root_bytes: [u8; 32] = u.arbitrary().unwrap_or_else(|_| {
            let mut b = [0u8; 32];
            b[0] = (i + 100) as u8;
            b
        });
        let event_count: u32 = u.arbitrary().unwrap_or(100);

        let bundle_ref = BundleMetadataRef {
            bundle_hash: B3Hash::from_bytes(bundle_hash_bytes),
            merkle_root: B3Hash::from_bytes(merkle_root_bytes),
            event_count,
            cpid: Some(format!("cp-{}", i)),
            sequence_no: Some(i as u64),
        };

        // Decide whether to break the chain (10% chance)
        let should_break_chain = u.arbitrary::<u8>().unwrap_or(0) < 26; // ~10%

        let envelope_previous_root = if should_break_chain && previous_root.is_some() {
            // Inject wrong previous_root to break chain
            let wrong_bytes: [u8; 32] = u.arbitrary().unwrap_or([0xff; 32]);
            Some(B3Hash::from_bytes(wrong_bytes))
        } else {
            previous_root
        };

        let envelope = EvidenceEnvelope::new_telemetry(
            "tenant-fuzz".to_string(),
            bundle_ref,
            envelope_previous_root,
        );

        previous_root = Some(envelope.root);
        envelopes.push(envelope);
    }

    // Verify the chain
    let verifier = EvidenceVerifier::new();
    let result = verifier.verify_chain(&envelopes);

    // Verification should never panic
    assert!(result.is_ok());

    if let Ok(r) = result {
        // If chain is valid, all envelopes should be checked
        if r.is_valid {
            assert_eq!(r.envelopes_checked, chain_len);
            assert!(r.first_invalid_index.is_none());
            assert!(!r.divergence_detected);
        } else {
            // If invalid, should have details
            assert!(r.first_invalid_index.is_some());
            assert!(r.envelopes_checked > 0);
        }
    }

    // Test individual envelope verification
    if !envelopes.is_empty() {
        let first = &envelopes[0];
        let result = verifier.verify_envelope(first, None);
        assert!(result.is_ok());

        if envelopes.len() > 1 {
            let second = &envelopes[1];
            let result = verifier.verify_envelope(second, Some(&first.root));
            assert!(result.is_ok());
        }
    }

    // Test corrupted envelope (wrong schema version)
    if !envelopes.is_empty() {
        let mut corrupted = envelopes[0].clone();
        corrupted.schema_version = u.arbitrary::<u8>().unwrap_or(99);
        let result = verifier.verify_envelope(&corrupted, None);
        assert!(result.is_ok());
        if let Ok(r) = result {
            if corrupted.schema_version != 1 {
                assert!(!r.is_valid);
                assert!(!r.schema_version_ok);
            }
        }
    }

    // Test corrupted root
    if !envelopes.is_empty() {
        let mut corrupted = envelopes[0].clone();
        let wrong_root_bytes: [u8; 32] = u.arbitrary().unwrap_or([0xaa; 32]);
        corrupted.root = B3Hash::from_bytes(wrong_root_bytes);
        let result = verifier.verify_envelope(&corrupted, None);
        assert!(result.is_ok());
        if let Ok(r) = result {
            // Should fail root verification unless we got lucky
            if corrupted.root != envelopes[0].root {
                assert!(!r.root_matches);
            }
        }
    }
});
