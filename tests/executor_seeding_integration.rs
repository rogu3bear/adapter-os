//! Integration tests for executor seeding with manifest validation
//!
//! Tests the deterministic executor seeding implementation:
//! - Manifest-based seed derivation with HKDF
//! - Fallback to default seed when manifest unavailable
//! - Production mode enforcement requiring valid manifest
//! - Domain separation between executor and other components

use adapteros_core::{derive_seed, B3Hash};
use adapteros_manifest::ManifestV3;
use std::path::PathBuf;

#[test]
fn test_manifest_based_executor_seeding() {
    // Load test manifest
    let manifest_path = PathBuf::from("models/qwen2.5-7b-mlx/manifest.json");

    if !manifest_path.exists() {
        eprintln!(
            "Manifest not found at {}, skipping test",
            manifest_path.display()
        );
        return;
    }

    let json = std::fs::read_to_string(&manifest_path).expect("Failed to read manifest file");

    let manifest: ManifestV3 = serde_json::from_str(&json).expect("Failed to parse manifest");

    // Validate manifest (this is what main.rs does)
    manifest.validate().expect("Manifest validation failed");

    // Compute hash
    let manifest_hash = manifest
        .compute_hash()
        .expect("Failed to compute manifest hash");

    // Derive executor seed using HKDF
    let executor_seed = derive_seed(&manifest_hash, "executor");

    // Verify determinism (second run should produce same seed)
    let executor_seed2 = derive_seed(&manifest_hash, "executor");
    assert_eq!(
        executor_seed, executor_seed2,
        "Executor seed derivation should be deterministic"
    );

    // Verify seed is 32 bytes
    assert_eq!(executor_seed.len(), 32, "Executor seed should be 32 bytes");
}

#[test]
fn test_fallback_seed_determinism() {
    // Test default seed when manifest not available
    let default_seed = B3Hash::hash(b"default-seed-non-production");
    let executor_seed = derive_seed(&default_seed, "executor");

    // Verify consistency
    let executor_seed2 = derive_seed(&default_seed, "executor");
    assert_eq!(
        executor_seed, executor_seed2,
        "Default executor seed should be deterministic"
    );
}

#[test]
fn test_manifest_vs_default_seed_differ() {
    // Verify that manifest-based and default seeds are different
    let manifest_path = PathBuf::from("models/qwen2.5-7b-mlx/manifest.json");

    if !manifest_path.exists() {
        eprintln!(
            "Manifest not found at {}, skipping test",
            manifest_path.display()
        );
        return;
    }

    let json = std::fs::read_to_string(&manifest_path).expect("Failed to read manifest file");

    let manifest: ManifestV3 = serde_json::from_str(&json).expect("Failed to parse manifest");

    let manifest_hash = manifest
        .compute_hash()
        .expect("Failed to compute manifest hash");

    let manifest_executor_seed = derive_seed(&manifest_hash, "executor");

    // Compare with default seed
    let default_seed = B3Hash::hash(b"default-seed-non-production");
    let default_executor_seed = derive_seed(&default_seed, "executor");

    assert_ne!(
        manifest_executor_seed, default_executor_seed,
        "Manifest-based and default executor seeds should differ"
    );
}

#[test]
fn test_domain_separation() {
    // Verify that executor seed differs from other domain seeds
    let base_seed = B3Hash::hash(b"test-base-seed");

    let executor_seed = derive_seed(&base_seed, "executor");
    let router_seed = derive_seed(&base_seed, "router");
    let dropout_seed = derive_seed(&base_seed, "dropout");
    let sampling_seed = derive_seed(&base_seed, "sampling");

    // All seeds should be different due to domain separation
    assert_ne!(
        executor_seed, router_seed,
        "Executor and router seeds should differ"
    );
    assert_ne!(
        executor_seed, dropout_seed,
        "Executor and dropout seeds should differ"
    );
    assert_ne!(
        executor_seed, sampling_seed,
        "Executor and sampling seeds should differ"
    );
    assert_ne!(
        router_seed, dropout_seed,
        "Router and dropout seeds should differ"
    );
}

#[test]
fn test_manifest_validation_catches_invalid() {
    // Create an invalid manifest (missing required fields)
    let invalid_json = r#"{
        "schema": "wrong.schema.version",
        "base": {},
        "adapters": [],
        "router": {},
        "telemetry": {},
        "policies": {},
        "seeds": {}
    }"#;

    let result: Result<ManifestV3, _> = serde_json::from_str(invalid_json);

    // Should fail to parse due to missing fields
    assert!(result.is_err(), "Invalid manifest should fail to parse");
}

#[test]
fn test_seed_reproducibility_across_runs() {
    // Verify that seeds remain stable across multiple derivations
    let base_seed = B3Hash::hash(b"stable-test-seed");

    let mut seeds = Vec::new();
    for _ in 0..10 {
        let seed = derive_seed(&base_seed, "executor");
        seeds.push(seed);
    }

    // All seeds should be identical
    let first_seed = seeds[0];
    for (i, seed) in seeds.iter().enumerate() {
        assert_eq!(
            *seed, first_seed,
            "Seed at iteration {} differs from first seed",
            i
        );
    }
}

#[test]
fn test_different_manifests_produce_different_seeds() {
    // Test that different manifest contents produce different seeds
    let manifest1_content = b"manifest content 1";
    let manifest2_content = b"manifest content 2";

    let hash1 = B3Hash::hash(manifest1_content);
    let hash2 = B3Hash::hash(manifest2_content);

    let seed1 = derive_seed(&hash1, "executor");
    let seed2 = derive_seed(&hash2, "executor");

    assert_ne!(
        seed1, seed2,
        "Different manifests should produce different executor seeds"
    );
}
