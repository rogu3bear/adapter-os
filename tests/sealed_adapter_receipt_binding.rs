//! Integration test: Sealed Adapter → Receipt Binding
//!
//! Verifies that sealed adapter integrity hashes flow correctly into
//! the receipt digest via ContextAdapterEntryV1.adapter_hash.
//!
//! The binding path is:
//! 1. SealedAdapterLoader verifies container integrity
//! 2. VerifiedAdapter.bundle.weights_hash is extracted
//! 3. weights_hash flows into ContextAdapterEntryV1.adapter_hash
//! 4. ContextManifestV1.to_bytes() encodes adapter_hash
//! 5. context_digest in ReceiptDigestInput includes the hash
//! 6. Receipt digest computation produces deterministic output

use adapteros_aos::{
    AdapterMetadata, LoadResult, RejectionReason, SealedAdapterLoader, SealedContainerHeader,
    SEALED_CONTAINER_VERSION, SEALED_HEADER_SIZE,
};
use adapteros_core::{
    context_manifest::{ContextAdapterEntryV1, ContextManifestV1},
    receipt_digest::{compute_receipt_digest, ReceiptDigestInput, RECEIPT_SCHEMA_V5},
    B3Hash, FusionInterval, SeedMode,
};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

/// Create a test sealed adapter container with known weights
fn create_test_sealed_container(
    adapter_id: &str,
    weights_data: &[u8],
    signing_key: &SigningKey,
) -> Vec<u8> {
    let manifest = AdapterMetadata {
        name: adapter_id.to_string(),
        version: "1.0.0".to_string(),
        ..Default::default()
    };
    let manifest_json = serde_json::to_vec(&manifest).unwrap();

    // Compute integrity hash over (version + manifest + payload)
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[SEALED_CONTAINER_VERSION]);
    hasher.update(&manifest_json);
    hasher.update(weights_data);
    let integrity_hash: [u8; 32] = hasher.finalize().into();

    // Sign the integrity hash
    let signature = signing_key.sign(&integrity_hash);

    // Build binary container
    let manifest_offset = SEALED_HEADER_SIZE as u64;
    let manifest_size = manifest_json.len() as u64;
    let payload_offset = manifest_offset + manifest_size;
    let payload_size = weights_data.len() as u64;

    let header = SealedContainerHeader {
        version: SEALED_CONTAINER_VERSION,
        integrity_hash,
        payload_offset,
        payload_size,
        manifest_offset,
        manifest_size,
        signature: signature.to_bytes(),
    };

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&header.to_bytes());
    bytes.extend_from_slice(&manifest_json);
    bytes.extend_from_slice(weights_data);

    bytes
}

#[test]
fn test_sealed_adapter_weights_hash_flows_to_context_manifest() {
    // Setup: Create a sealed adapter with known weights
    let signing_key = SigningKey::generate(&mut OsRng);
    let weights_data = b"deterministic adapter weights for testing";
    let expected_weights_hash = B3Hash::hash(weights_data);

    // Step 1: Create and load sealed container
    let sealed_bytes = create_test_sealed_container("test-adapter", weights_data, &signing_key);

    let loader = SealedAdapterLoader::new(vec![signing_key.verifying_key()]);
    let result = loader.load_from_bytes(&sealed_bytes);

    // Step 2: Extract verified adapter and get weights_hash
    let verified = match result {
        LoadResult::Verified(v) => *v,
        LoadResult::Rejected {
            reason, message, ..
        } => {
            panic!(
                "Expected verified adapter, got rejection: {:?} - {}",
                reason, message
            );
        }
    };

    // Verify the weights_hash matches what we expect
    let weights_hash = verified.weights_hash_for_receipt();
    assert_eq!(
        *weights_hash, expected_weights_hash,
        "weights_hash should be BLAKE3 of weights_data"
    );

    // Step 3: Build ContextAdapterEntryV1 with the weights_hash
    let adapter_entry = ContextAdapterEntryV1 {
        adapter_id: verified.adapter_id().to_string(),
        adapter_hash: *weights_hash,
        rank: 16,
        alpha_num: 1,
        alpha_den: 1,
        backend_id: "sealed".to_string(),
        kernel_version_id: "v1".to_string(),
    };

    // Step 4: Build context manifest and compute digest
    let manifest = ContextManifestV1 {
        base_model_id: "test-model".to_string(),
        base_model_hash: B3Hash::hash(b"test-model-hash"),
        adapter_dir_hash: B3Hash::hash(b"adapter-dir"),
        adapter_stack: vec![adapter_entry],
        router_version: "1.0.0".to_string(),
        fusion_interval: FusionInterval::PerRequest,
        seed_mode: SeedMode::Strict,
        seed_inputs_digest: B3Hash::hash(b"seed-inputs"),
        policy_digest: B3Hash::hash(b"test-policy"),
        sampler_params_digest: B3Hash::hash(b"sampler-params"),
        build_id: "test-build-001".to_string(),
        build_git_sha: "abcd1234".to_string(),
    };

    let context_digest = manifest.digest();

    // Step 5: Build receipt digest input with context_digest
    let receipt_input = ReceiptDigestInput {
        context_digest: context_digest.to_bytes(),
        run_head_hash: [0u8; 32],
        output_digest: B3Hash::hash(b"test output").to_bytes(),
        logical_prompt_tokens: 100,
        prefix_cached_token_count: 0,
        billed_input_tokens: 100,
        logical_output_tokens: 50,
        billed_output_tokens: 50,
        ..Default::default()
    };

    // Step 6: Compute final receipt digest
    let receipt_digest = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V5)
        .expect("V5 digest should compute");

    // Verify the binding is deterministic
    let receipt_digest_2 = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V5)
        .expect("V5 digest should compute");
    assert_eq!(
        receipt_digest, receipt_digest_2,
        "Receipt digest should be deterministic"
    );

    println!("Integration test passed:");
    println!("  weights_hash: {}", weights_hash.to_hex());
    println!("  context_digest: {}", context_digest.to_hex());
    println!("  receipt_digest: {}", receipt_digest.to_hex());
}

#[test]
fn test_different_adapters_produce_different_receipts() {
    let signing_key = SigningKey::generate(&mut OsRng);

    // Create two adapters with different weights
    let weights_a = b"adapter A weights - unique content";
    let weights_b = b"adapter B weights - different content";

    let sealed_a = create_test_sealed_container("adapter-a", weights_a, &signing_key);
    let sealed_b = create_test_sealed_container("adapter-b", weights_b, &signing_key);

    let loader = SealedAdapterLoader::new(vec![signing_key.verifying_key()]);

    let verified_a = match loader.load_from_bytes(&sealed_a) {
        LoadResult::Verified(v) => *v,
        _ => panic!("Expected verified adapter A"),
    };

    let verified_b = match loader.load_from_bytes(&sealed_b) {
        LoadResult::Verified(v) => *v,
        _ => panic!("Expected verified adapter B"),
    };

    // Different adapters should have different weights_hash
    assert_ne!(
        verified_a.weights_hash_for_receipt(),
        verified_b.weights_hash_for_receipt(),
        "Different adapters should have different weights hashes"
    );

    // Build context manifests
    let entry_a = ContextAdapterEntryV1 {
        adapter_id: verified_a.adapter_id().to_string(),
        adapter_hash: *verified_a.weights_hash_for_receipt(),
        rank: 16,
        alpha_num: 1,
        alpha_den: 1,
        backend_id: "sealed".to_string(),
        kernel_version_id: "v1".to_string(),
    };

    let entry_b = ContextAdapterEntryV1 {
        adapter_id: verified_b.adapter_id().to_string(),
        adapter_hash: *verified_b.weights_hash_for_receipt(),
        rank: 16,
        alpha_num: 1,
        alpha_den: 1,
        backend_id: "sealed".to_string(),
        kernel_version_id: "v1".to_string(),
    };

    let manifest_a = ContextManifestV1 {
        base_model_id: "test-model".to_string(),
        base_model_hash: B3Hash::hash(b"test-model"),
        adapter_dir_hash: B3Hash::hash(b"adapter-dir"),
        adapter_stack: vec![entry_a],
        router_version: "1.0.0".to_string(),
        fusion_interval: FusionInterval::PerRequest,
        seed_mode: SeedMode::Strict,
        seed_inputs_digest: B3Hash::hash(b"seed-inputs"),
        policy_digest: B3Hash::hash(b"test-policy"),
        sampler_params_digest: B3Hash::hash(b"sampler-params"),
        build_id: "test-build-001".to_string(),
        build_git_sha: "abcd1234".to_string(),
    };

    let manifest_b = ContextManifestV1 {
        base_model_id: "test-model".to_string(),
        base_model_hash: B3Hash::hash(b"test-model"),
        adapter_dir_hash: B3Hash::hash(b"adapter-dir"),
        adapter_stack: vec![entry_b],
        router_version: "1.0.0".to_string(),
        fusion_interval: FusionInterval::PerRequest,
        seed_mode: SeedMode::Strict,
        seed_inputs_digest: B3Hash::hash(b"seed-inputs"),
        policy_digest: B3Hash::hash(b"test-policy"),
        sampler_params_digest: B3Hash::hash(b"sampler-params"),
        build_id: "test-build-001".to_string(),
        build_git_sha: "abcd1234".to_string(),
    };

    // Different adapters should produce different context digests
    assert_ne!(
        manifest_a.digest(),
        manifest_b.digest(),
        "Different adapters should produce different context digests"
    );
}

#[test]
fn test_tampered_adapter_is_rejected() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let weights_data = b"original adapter weights";

    let mut sealed_bytes = create_test_sealed_container("test-adapter", weights_data, &signing_key);

    // Tamper with the weights (last bytes are payload)
    let len = sealed_bytes.len();
    sealed_bytes[len - 1] ^= 0xFF;

    let loader = SealedAdapterLoader::new(vec![signing_key.verifying_key()]);
    let result = loader.load_from_bytes(&sealed_bytes);

    match result {
        LoadResult::Rejected { reason, .. } => {
            assert!(
                matches!(
                    reason,
                    RejectionReason::IntegrityMismatch | RejectionReason::PayloadCorrupted
                ),
                "Tampered adapter should be rejected with integrity error"
            );
        }
        LoadResult::Verified(_) => {
            panic!("Tampered adapter should not be verified");
        }
    }
}

#[test]
fn test_untrusted_signer_is_rejected() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let untrusted_key = SigningKey::generate(&mut OsRng);
    let weights_data = b"adapter weights";

    let sealed_bytes = create_test_sealed_container("test-adapter", weights_data, &signing_key);

    // Load with different trusted key
    let loader = SealedAdapterLoader::new(vec![untrusted_key.verifying_key()]);
    let result = loader.load_from_bytes(&sealed_bytes);

    match result {
        LoadResult::Rejected { reason, .. } => {
            assert_eq!(
                reason,
                RejectionReason::UntrustedSigner,
                "Adapter signed by untrusted key should be rejected"
            );
        }
        LoadResult::Verified(_) => {
            panic!("Adapter with untrusted signer should not be verified");
        }
    }
}
