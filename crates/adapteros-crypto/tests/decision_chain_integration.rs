//! Integration tests for decision chain hash and bundle verification.
//!
//! Tests deterministic hashing across runs and Ed25519 bundle signature verification.

use adapteros_core::B3Hash;
use adapteros_crypto::decision_chain::{
    DecisionChainBuilder, EnvironmentIdentity, MerkleBundleCommits, RouterEventDigest,
};
use adapteros_crypto::{sign_bundle, Keypair};

/// Test that decision_chain_hash is stable across runs with same inputs.
#[test]
fn test_decision_chain_hash_stable_across_runs() {
    // Simulate deterministic router decisions from an inference run
    let build_chain = || {
        let mut builder = DecisionChainBuilder::new();

        // Token 0: Initial routing decision
        builder.push_event(
            0,
            Some(1234), // input token
            vec![0, 1, 2],
            vec![16384, 8192, 8191], // Q15 gates
            24576,                   // ~0.75 entropy in Q15
            None,
        );

        // Token 1: Second decision
        builder.push_event(
            1,
            Some(5678),
            vec![1, 2],
            vec![20000, 12767],
            20000,
            Some(B3Hash::hash(b"policy_mask_1")),
        );

        // Token 2: Third decision with different adapters
        builder.push_event(
            2,
            Some(9012),
            vec![0, 3],
            vec![16384, 16383],
            16384,
            Some(B3Hash::hash(b"policy_mask_2")),
        );

        builder.finalize()
    };

    // Run multiple times and verify stability
    let hash1 = build_chain();
    let hash2 = build_chain();
    let hash3 = build_chain();

    assert_eq!(hash1, hash2, "Hash should be identical across runs");
    assert_eq!(hash2, hash3, "Hash should be identical across runs");
}

/// Test verify_bundle_signature() passes with valid signature.
#[test]
fn test_verify_bundle_signature_passes() {
    // Build decision chain
    let mut decision_chain = DecisionChainBuilder::new();
    for step in 0..5 {
        decision_chain.push_event(
            step,
            Some(step as u32 * 100),
            vec![0, 1],
            vec![16384, 16383],
            24576,
            None,
        );
    }
    let decision_chain_hash = decision_chain.finalize();

    // Create environment identity
    let env = EnvironmentIdentity::new("mlx-0.21.0")
        .with_git_commit("abc123def456789012345678901234567890abcd")
        .with_build_timestamp(1704067200);
    let backend_identity_hash = env.hash();

    // Create bundle commits
    let request_hash = B3Hash::hash(b"prompt: hello world");
    let commits = MerkleBundleCommits::new(
        request_hash,
        decision_chain_hash,
        backend_identity_hash,
        vec![
            "adapter-safety-v1".to_string(),
            "adapter-docs-v2".to_string(),
        ],
    )
    .with_manifest_hash(B3Hash::hash(b"manifest"))
    .with_model_identity(B3Hash::hash(b"model_weights"));

    // Compute bundle hash (Merkle root of commits)
    let bundle_hash = commits.merkle_root();
    let merkle_root = commits.combined_hash();

    // Sign the bundle
    let keypair = Keypair::generate();
    let signature = sign_bundle(&bundle_hash, &merkle_root, &keypair).unwrap();

    // Verify signature passes
    assert!(
        signature.verify().is_ok(),
        "Bundle signature verification should pass"
    );

    // Verify bundle hash matches
    assert_eq!(signature.bundle_hash, bundle_hash);
    assert_eq!(signature.merkle_root, merkle_root);
}

/// Test that tampered bundle fails verification.
#[test]
fn test_tampered_bundle_fails_verification() {
    // Create and sign a bundle
    let bundle_hash = B3Hash::hash(b"original_bundle");
    let merkle_root = B3Hash::hash(b"merkle_root");

    let keypair = Keypair::generate();
    let mut signature = sign_bundle(&bundle_hash, &merkle_root, &keypair).unwrap();

    // Tamper with bundle hash
    signature.bundle_hash = B3Hash::hash(b"tampered_bundle");

    // Verification should fail
    assert!(
        signature.verify().is_err(),
        "Tampered bundle should fail verification"
    );
}

/// Test export bundle verifies offline (no network needed).
#[test]
fn test_export_bundle_verifies_offline() {
    // This test simulates the complete flow:
    // 1. Build decision chain during inference
    // 2. Compute all hashes
    // 3. Create bundle commits
    // 4. Sign and export
    // 5. Later: load and verify offline

    // Step 1: Build decision chain
    let mut chain = DecisionChainBuilder::new();
    for step in 0..10 {
        chain.push_event(
            step,
            Some((step + 1) as u32 * 42),
            vec![0, 1, 2],
            vec![10922, 10923, 10922], // ~1/3 each
            16384,
            if step % 2 == 0 {
                Some(B3Hash::hash(format!("policy_{}", step).as_bytes()))
            } else {
                None
            },
        );
    }

    // Step 2: Finalize decision chain
    let decision_chain_hash = chain.finalize();
    assert!(chain.verify_chain(), "Internal chain should be valid");

    // Step 3: Create environment identity
    let env = EnvironmentIdentity::new("coreml-17.4")
        .with_git_commit("1234567890abcdef1234567890abcdef12345678")
        .with_build_timestamp(1704153600)
        .with_model_identity(B3Hash::hash(b"qwen2.5-7b-instruct"));

    // Step 4: Create bundle commits
    let commits = MerkleBundleCommits::new(
        B3Hash::hash(b"request_content"),
        decision_chain_hash,
        env.hash(),
        vec![
            "adapter-1".to_string(),
            "adapter-2".to_string(),
            "adapter-3".to_string(),
        ],
    )
    .with_manifest_hash(B3Hash::hash(b"manifest_v2"))
    .with_model_identity(B3Hash::hash(b"qwen2.5-7b-instruct"));

    // Step 5: Sign
    let keypair = Keypair::generate();
    let bundle_hash = commits.merkle_root();
    let merkle_root = commits.combined_hash();
    let signature = sign_bundle(&bundle_hash, &merkle_root, &keypair).unwrap();

    // Step 6: Serialize to JSON (simulate export)
    let signature_json = serde_json::to_string_pretty(&signature).unwrap();

    // Step 7: Deserialize (simulate import on different machine)
    let loaded_signature: adapteros_crypto::BundleSignature =
        serde_json::from_str(&signature_json).unwrap();

    // Step 8: Verify offline (no network needed)
    assert!(
        loaded_signature.verify().is_ok(),
        "Exported bundle should verify offline"
    );

    // Verify the chain of hashes
    assert_eq!(loaded_signature.bundle_hash, bundle_hash);
    assert_eq!(loaded_signature.merkle_root, merkle_root);
}

/// Test RouterEventDigest canonical encoding is deterministic.
#[test]
fn test_router_event_digest_canonical_encoding() {
    let event1 = RouterEventDigest::new(
        5,
        Some(12345),
        vec![0, 1, 2, 3],
        vec![8192, 8192, 8192, 8191],
        16384,
        Some(B3Hash::hash(b"policy")),
        Some(B3Hash::hash(b"previous")),
    );

    // Create same event with different construction order (Rust guarantees field order)
    let event2 = RouterEventDigest {
        step: 5,
        input_token_id: Some(12345),
        adapter_indices: vec![0, 1, 2, 3],
        gates_q15: vec![8192, 8192, 8192, 8191],
        entropy_q15: 16384,
        policy_mask_digest_b3: Some(B3Hash::hash(b"policy")),
        adapter_training_digests: None,
        previous_hash: Some(B3Hash::hash(b"previous")),
    };

    assert_eq!(
        event1.canonical_bytes(),
        event2.canonical_bytes(),
        "Canonical bytes should be identical"
    );
    assert_eq!(event1.hash(), event2.hash(), "Hashes should be identical");
}

/// Test that environment identity with MLX bridge script hash works.
#[test]
fn test_environment_identity_with_mlx_bridge() {
    // Simulate hashing MLX bridge script content
    let script_content = r#"
import mlx
import json
# Bridge script for MLX inference
"#;
    let script_hash = B3Hash::hash(script_content.as_bytes());

    let env = EnvironmentIdentity::new("mlx-bridge-subprocess")
        .with_mlx_bridge_hash(script_hash)
        .with_model_identity(B3Hash::hash(b"model_weights"));

    // Hash should be stable
    let hash1 = env.hash();
    let hash2 = env.clone().hash();
    assert_eq!(hash1, hash2);

    // Different script should produce different hash
    let different_script_hash = B3Hash::hash(b"different script");
    let env2 = EnvironmentIdentity::new("mlx-bridge-subprocess")
        .with_mlx_bridge_hash(different_script_hash)
        .with_model_identity(B3Hash::hash(b"model_weights"));

    assert_ne!(env.hash(), env2.hash());
}

/// Test MerkleBundleCommits leaf hash order.
#[test]
fn test_merkle_bundle_commits_leaf_hashes() {
    let commits = MerkleBundleCommits::new(
        B3Hash::hash(b"request"),
        B3Hash::hash(b"decision"),
        B3Hash::hash(b"backend"),
        vec!["a".to_string(), "b".to_string()],
    )
    .with_manifest_hash(B3Hash::hash(b"manifest"))
    .with_model_identity(B3Hash::hash(b"model"));

    let leaves = commits.leaf_hashes();

    // Should have: request, decision, backend, manifest, model, adapter_stack
    assert_eq!(leaves.len(), 6, "Should have 6 leaf hashes");

    // First three are always present
    assert_eq!(leaves[0], B3Hash::hash(b"request"));
    assert_eq!(leaves[1], B3Hash::hash(b"decision"));
    assert_eq!(leaves[2], B3Hash::hash(b"backend"));
}

/// Test chain verification detects tampering.
#[test]
fn test_chain_verification_detects_tampering() {
    let mut builder = DecisionChainBuilder::new();

    // Add some events
    builder.push_event(0, Some(1), vec![0], vec![32767], 16384, None);
    builder.push_event(1, Some(2), vec![1], vec![32767], 16384, None);
    builder.push_event(2, Some(3), vec![2], vec![32767], 16384, None);

    // Chain should be valid
    assert!(builder.verify_chain());

    // Manually create a broken chain
    let broken_event = RouterEventDigest::new(
        10,
        Some(999),
        vec![5],
        vec![32767],
        16384,
        None,
        Some(B3Hash::hash(b"wrong_previous")), // Wrong previous hash
    );

    let mut broken_builder = DecisionChainBuilder::new();
    broken_builder.push_raw_event(broken_event);

    // This chain has only one event, so it's "valid" in terms of internal consistency
    // but the previous_hash field points to something that doesn't exist
    // The verify_chain checks that each event links to its predecessor in the list
    // Since this is the first event, previous_hash should be None for a valid chain
    assert!(
        !broken_builder.verify_chain(),
        "First event with non-None previous_hash should fail verification"
    );
}
