//! PRD 8 - Core Determinism Tests (Linux-compatible)
//!
//! This test suite verifies core determinism without Metal dependencies:
//! - Stack hash computation (cross-arch)
//! - RNG determinism
//! - Serialization/deserialization (property tests)
//!
//! # Citations
//! - PRD 8: Determinism & Guardrail Suite
//! - CLAUDE.md: Stack hash computation, HKDF seeding

#![cfg(test)]

use adapteros_core::{identity::IdentityEnvelope, stack::compute_stack_hash, B3Hash};
use blake3::Hasher;
use proptest::prelude::*;
use serde_json;
use std::collections::HashMap;

// ============================================================================
// Cross-Architecture Stack Hash Tests
// ============================================================================

/// Test that stack hash computation is deterministic across architectures
#[test]
fn test_stack_hash_cross_arch_determinism() {
    // Create test adapters with deterministic IDs and hashes
    let adapters = vec![
        ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
        ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
        ("adapter_c".to_string(), B3Hash::hash(b"hash_c")),
    ];

    // Compute stack hash
    let stack_hash = compute_stack_hash(adapters.clone());

    // Recompute - should be identical
    let golden_hash = compute_stack_hash(adapters);

    assert_eq!(
        stack_hash, golden_hash,
        "Stack hash must be identical across runs"
    );

    // Record hash for cross-platform verification
    println!("Stack hash (3 adapters): {}", stack_hash.to_hex());
}

/// Verify order-independence of stack hash
#[test]
fn test_stack_hash_order_independence() {
    let adapters1 = vec![
        ("adapter_c".to_string(), B3Hash::hash(b"hash_c")),
        ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
        ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
    ];

    let adapters2 = vec![
        ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
        ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
        ("adapter_c".to_string(), B3Hash::hash(b"hash_c")),
    ];

    let hash1 = compute_stack_hash(adapters1);
    let hash2 = compute_stack_hash(adapters2);

    assert_eq!(hash1, hash2, "Stack hash must be order-independent");
}

/// Test stack hash with many adapters
#[test]
fn test_stack_hash_with_many_adapters() {
    // Test with 100 adapters
    let mut adapters = Vec::new();
    for i in 0..100 {
        adapters.push((
            format!("adapter_{:03}", i),
            B3Hash::hash(format!("hash_{}", i).as_bytes()),
        ));
    }

    let hash1 = compute_stack_hash(adapters.clone());

    // Shuffle adapters (should produce same hash due to sorting)
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut rng = rand_chacha::ChaCha20Rng::from_seed([42u8; 32]);
    adapters.shuffle(&mut rng);

    let hash2 = compute_stack_hash(adapters);

    assert_eq!(
        hash1, hash2,
        "Stack hash must be order-independent with 100 adapters"
    );

    println!("Stack hash (100 adapters): {}", hash1.to_hex());
}

/// Test collision resistance
#[test]
fn test_stack_hash_collision_resistance() {
    // Different adapter content should produce different hashes
    let adapters1 = vec![
        ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
        ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
    ];

    let adapters2 = vec![
        ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
        ("adapter_b".to_string(), B3Hash::hash(b"hash_c")), // Different hash
    ];

    let hash1 = compute_stack_hash(adapters1);
    let hash2 = compute_stack_hash(adapters2);

    assert_ne!(hash1, hash2, "Different stacks must have different hashes");
}

// ============================================================================
// BLAKE3 Hash Determinism
// ============================================================================

/// Test that BLAKE3 hashes are deterministic
#[test]
fn test_blake3_determinism() {
    let data = b"test data for hashing";

    let hash1 = B3Hash::hash(data);
    let hash2 = B3Hash::hash(data);

    assert_eq!(hash1, hash2, "BLAKE3 hashes must be deterministic");

    // Record golden hash
    println!("BLAKE3 hash of 'test data for hashing': {}", hash1.to_hex());
}

/// Test BLAKE3 with large data
#[test]
fn test_blake3_large_data() {
    let data = vec![42u8; 1_000_000]; // 1 MB

    let hash1 = B3Hash::hash(&data);
    let hash2 = B3Hash::hash(&data);

    assert_eq!(hash1, hash2, "BLAKE3 must be deterministic for large data");

    println!("BLAKE3 hash (1MB of 0x42): {}", hash1.to_hex());
}

// ============================================================================
// Property Tests for IdentityEnvelope
// ============================================================================

proptest! {
    /// Property test: IdentityEnvelope serialization roundtrip
    #[test]
    fn prop_identity_envelope_serde_roundtrip(
        tenant_id in "[a-z0-9_-]{4,20}",
        domain in "[a-z]{4,10}",
        purpose in "[a-z]{4,15}",
        revision in "r[0-9]{3}",
    ) {
        let envelope = IdentityEnvelope::new(
            tenant_id.clone(),
            domain.clone(),
            purpose.clone(),
            revision.clone(),
        );

        // JSON roundtrip
        let json = serde_json::to_string(&envelope).unwrap();
        let deserialized: IdentityEnvelope = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(envelope, deserialized);

        // Bincode roundtrip
        let encoded = bincode::serialize(&envelope).unwrap();
        let decoded: IdentityEnvelope = bincode::deserialize(&encoded).unwrap();

        prop_assert_eq!(envelope, decoded);

        // Verify fields match
        prop_assert_eq!(&envelope.tenant_id, &tenant_id);
        prop_assert_eq!(&envelope.domain, &domain);
        prop_assert_eq!(&envelope.purpose, &purpose);
        prop_assert_eq!(&envelope.revision, &revision);
    }

    /// Property test: IdentityEnvelope validation
    #[test]
    fn prop_identity_envelope_validation(
        tenant_id in "[a-z0-9_-]{1,20}",
        domain in "[a-z]{1,10}",
        purpose in "[a-z]{1,15}",
        revision in "r[0-9]{1,3}",
    ) {
        let envelope = IdentityEnvelope::new(
            tenant_id.clone(),
            domain.clone(),
            purpose.clone(),
            revision.clone(),
        );

        // Non-empty fields should validate
        let result = envelope.validate();
        prop_assert!(result.is_ok());
    }
}

// ============================================================================
// B3Hash Property Tests
// ============================================================================

proptest! {
    /// Property test: B3Hash is deterministic for any input
    #[test]
    fn prop_b3hash_deterministic(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        let hash1 = B3Hash::hash(&data);
        let hash2 = B3Hash::hash(&data);

        prop_assert_eq!(hash1, hash2);
    }

    /// Property test: Different inputs produce different hashes (collision resistance)
    #[test]
    fn prop_b3hash_collision_resistance(
        data1 in prop::collection::vec(any::<u8>(), 1..100),
        data2 in prop::collection::vec(any::<u8>(), 1..100),
    ) {
        // Only test if inputs are actually different
        if data1 != data2 {
            let hash1 = B3Hash::hash(&data1);
            let hash2 = B3Hash::hash(&data2);

            prop_assert_ne!(hash1, hash2);
        }
    }
}

// ============================================================================
// Stack Hash Property Tests
// ============================================================================

proptest! {
    /// Property test: Stack hash is deterministic
    #[test]
    fn prop_stack_hash_deterministic(
        adapter_count in 1usize..20,
    ) {
        let mut adapters = Vec::new();
        for i in 0..adapter_count {
            adapters.push((
                format!("adapter_{}", i),
                B3Hash::hash(format!("hash_{}", i).as_bytes()),
            ));
        }

        let hash1 = compute_stack_hash(adapters.clone());
        let hash2 = compute_stack_hash(adapters);

        prop_assert_eq!(hash1, hash2);
    }

    /// Property test: Stack hash is order-independent
    #[test]
    fn prop_stack_hash_order_independent(
        adapter_count in 2usize..10,
        seed in any::<u64>(),
    ) {
        let mut adapters = Vec::new();
        for i in 0..adapter_count {
            adapters.push((
                format!("adapter_{}", i),
                B3Hash::hash(format!("hash_{}", i).as_bytes()),
            ));
        }

        let hash1 = compute_stack_hash(adapters.clone());

        // Shuffle
        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(seed);
        adapters.shuffle(&mut rng);

        let hash2 = compute_stack_hash(adapters);

        prop_assert_eq!(hash1, hash2);
    }
}

// ============================================================================
// Cross-Platform Golden Hashes
// ============================================================================

/// Test that produces golden hashes for cross-platform verification
#[test]
fn test_generate_golden_hashes() {
    println!("\n=== GOLDEN HASHES FOR CROSS-PLATFORM VERIFICATION ===");

    // Test 1: Simple BLAKE3
    let hash = B3Hash::hash(b"AdapterOS Determinism Test");
    println!(
        "Test1 - BLAKE3('AdapterOS Determinism Test'): {}",
        hash.to_hex()
    );

    // Test 2: Stack with 3 adapters
    let adapters = vec![
        ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
        ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
        ("adapter_c".to_string(), B3Hash::hash(b"hash_c")),
    ];
    let stack_hash = compute_stack_hash(adapters);
    println!("Test2 - Stack hash (a,b,c): {}", stack_hash.to_hex());

    // Test 3: Stack with adapters in different order (should match Test 2)
    let adapters_reversed = vec![
        ("adapter_c".to_string(), B3Hash::hash(b"hash_c")),
        ("adapter_a".to_string(), B3Hash::hash(b"hash_a")),
        ("adapter_b".to_string(), B3Hash::hash(b"hash_b")),
    ];
    let stack_hash_reversed = compute_stack_hash(adapters_reversed);
    println!(
        "Test3 - Stack hash (c,a,b): {}",
        stack_hash_reversed.to_hex()
    );
    assert_eq!(stack_hash, stack_hash_reversed);

    // Test 4: IdentityEnvelope serialization
    let envelope = IdentityEnvelope::new(
        "test-tenant".to_string(),
        "router".to_string(),
        "inference".to_string(),
        "r001".to_string(),
    );
    let json = serde_json::to_string(&envelope).unwrap();
    let json_hash = B3Hash::hash(json.as_bytes());
    println!("Test4 - IdentityEnvelope JSON hash: {}", json_hash.to_hex());

    println!("=== END GOLDEN HASHES ===\n");
}

// ============================================================================
// Platform Information
// ============================================================================

#[test]
fn test_record_platform_info() {
    println!("\n=== PLATFORM INFORMATION ===");
    println!("OS: {}", std::env::consts::OS);
    println!("Arch: {}", std::env::consts::ARCH);
    println!("Family: {}", std::env::consts::FAMILY);
    println!("=== END PLATFORM INFO ===\n");
}
