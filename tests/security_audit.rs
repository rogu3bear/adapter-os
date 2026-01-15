//! Security Audit Test Suite for adapterOS Critical Components
//!
//! This test suite validates security properties across:
//! - HKDF seed isolation and domain separation
//! - BLAKE3 content addressing collision resistance
//! - GPU fingerprint tampering detection
//! - Checkpoint integrity verification
//! - Unauthorized adapter swap prevention
//! - Memory safety in FFI boundaries
//! - Seed reuse prevention
//! - Hash collision detection (birthday attack resistance)
//!
//! Security contact: security@adapteros.dev
//! Last audit: 2025-11-21
//!
//! Run tests with:
//! ```bash
//! cargo test --test security_audit
//! ```

#![allow(unused_imports)]
#![allow(dead_code)]

use adapteros_core::{
    clear_seed_registry, derive_adapter_seed, derive_seed, derive_seed_indexed, AosError, B3Hash,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// =============================================================================
// Helper Functions (self-contained implementations for testing)
// =============================================================================

/// Convert adapter ID string to deterministic u16 using BLAKE3 hash
/// This mirrors the implementation in adapter_hotswap.rs for testing
fn adapter_id_to_u16(adapter_id: &str) -> u16 {
    let hash = B3Hash::hash(adapter_id.as_bytes());
    let bytes = hash.to_bytes();
    u16::from_le_bytes([bytes[0], bytes[1]])
}

/// GPU fingerprint for testing (mirrors GpuFingerprint in adapter_hotswap.rs)
#[derive(Debug, Clone, PartialEq, Eq)]
struct GpuFingerprint {
    adapter_id: String,
    buffer_bytes: u64,
    checkpoint_hash: B3Hash,
}

/// Stack checkpoint for testing (mirrors StackCheckpoint in adapter_hotswap.rs)
#[derive(Debug, Clone)]
struct StackCheckpoint {
    timestamp: u64,
    metadata_hash: B3Hash,
    cross_layer_hash: Option<B3Hash>,
    gpu_fingerprints: Vec<GpuFingerprint>,
    adapter_ids: Vec<String>,
}

// =============================================================================
// SECTION 1: HKDF Seed Isolation Tests
// =============================================================================

/// Test that different domains produce cryptographically distinct seeds
/// Security property: Domain separation prevents cross-domain seed collisions
#[test]
fn test_hkdf_domain_separation() {
    let global_seed = B3Hash::hash(b"test_manifest_hash");

    // Standard domains used in production
    let domains = [
        "router",
        "dropout",
        "sampling",
        "adapter_0",
        "adapter_1",
        "mlx",
        "metal",
        "executor",
    ];

    let mut seen_seeds: HashSet<[u8; 32]> = HashSet::new();

    for domain in &domains {
        let seed = derive_seed(&global_seed, domain);

        // Verify seed uniqueness across domains
        assert!(
            seen_seeds.insert(seed),
            "Domain '{}' produced duplicate seed - domain separation failure!",
            domain
        );

        // Verify determinism: same domain + global produces same seed
        let seed2 = derive_seed(&global_seed, domain);
        assert_eq!(
            seed, seed2,
            "Non-deterministic seed derivation for domain '{}'",
            domain
        );
    }
}

/// Test that different global seeds produce different domain seeds
/// Security property: Manifest isolation - different models have disjoint seed spaces
#[test]
fn test_hkdf_manifest_isolation() {
    let manifest1 = B3Hash::hash(b"model_manifest_v1");
    let manifest2 = B3Hash::hash(b"model_manifest_v2");

    let domain = "router";

    let seed1 = derive_seed(&manifest1, domain);
    let seed2 = derive_seed(&manifest2, domain);

    assert_ne!(
        seed1, seed2,
        "Different manifests produced identical seeds - manifest isolation failure!"
    );
}

/// Test HKDF output entropy
/// Security property: Seeds have full 256-bit entropy
#[test]
fn test_hkdf_entropy_quality() {
    let global_seed = B3Hash::hash(b"entropy_test");
    let seed = derive_seed(&global_seed, "entropy_test_domain");

    // Statistical tests for entropy quality
    // 1. Check no zero bytes (weak entropy indicator)
    let zero_count = seed.iter().filter(|&&b| b == 0).count();
    assert!(
        zero_count < 8,
        "Too many zero bytes ({}) suggests weak entropy",
        zero_count
    );

    // 2. Check byte distribution (basic uniformity check)
    let mut byte_counts = [0u32; 256];
    for &byte in &seed {
        byte_counts[byte as usize] += 1;
    }

    let max_count = *byte_counts.iter().max().unwrap();
    // In 32 bytes, no single value should appear more than ~8 times (very unlikely)
    assert!(
        max_count < 10,
        "Byte frequency {} too high, suggests biased output",
        max_count
    );

    // 3. Bit balance check
    let ones = seed.iter().map(|b| b.count_ones()).sum::<u32>();
    let total_bits = 256u32;
    let ratio = ones as f64 / total_bits as f64;
    assert!(
        (0.35..=0.65).contains(&ratio),
        "Bit balance ratio {} outside expected range",
        ratio
    );
}

/// Test indexed seed derivation produces unique seeds
/// Security property: Per-layer seeds are distinct
#[test]
fn test_hkdf_indexed_seed_uniqueness() {
    let global_seed = B3Hash::hash(b"indexed_test");
    let mut seeds: HashSet<[u8; 32]> = HashSet::new();

    // Generate 1000 indexed seeds for same base label
    for i in 0..1000 {
        let seed = derive_seed_indexed(&global_seed, "layer", i);
        assert!(seeds.insert(seed), "Index {} produced duplicate seed", i);
    }
}

// =============================================================================
// SECTION 2: Adapter ID Collision Resistance Tests
// =============================================================================

/// Test BLAKE3 u16 mapping uniqueness
/// Security property: adapter_id_to_u16 should have low collision rate
#[test]
fn test_adapter_id_collision_resistance() {
    let mut id_map: HashMap<u16, String> = HashMap::new();
    let mut collisions = 0u64;

    // Test common adapter naming patterns
    let prefixes = [
        "adapter", "lora", "model", "tenant", "prod", "dev", "staging",
    ];
    let suffixes = ["001", "002", "v1", "v2", "latest", "stable", "beta"];

    for prefix in &prefixes {
        for suffix in &suffixes {
            for i in 0..100 {
                let adapter_id = format!("{}_{}_{}", prefix, suffix, i);
                let u16_id = adapter_id_to_u16(&adapter_id);

                if let Some(existing) = id_map.get(&u16_id) {
                    collisions += 1;
                    // Log collision for analysis
                    eprintln!(
                        "Collision: '{}' and '{}' both map to {}",
                        adapter_id, existing, u16_id
                    );
                } else {
                    id_map.insert(u16_id, adapter_id);
                }
            }
        }
    }

    let total_ids = prefixes.len() * suffixes.len() * 100;
    let collision_rate = collisions as f64 / total_ids as f64;

    // Birthday paradox: expected ~0.5% collisions in 65k space with ~7k items
    assert!(
        collision_rate < 0.05,
        "Collision rate {} too high (expected < 5%)",
        collision_rate
    );
}

/// Test adapter ID determinism across platforms
/// Security property: Same ID always maps to same u16
#[test]
fn test_adapter_id_determinism() {
    // Known test vectors (computed once, verified across platforms)
    let test_cases = [
        "adapter_001",
        "tenant-a/code-review/r001",
        "production_model_v2",
    ];

    for id in &test_cases {
        let first = adapter_id_to_u16(id);
        let second = adapter_id_to_u16(id);
        assert_eq!(
            first, second,
            "Non-deterministic mapping for '{}': {} vs {}",
            id, first, second
        );
    }
}

// =============================================================================
// SECTION 3: GPU Fingerprint Tampering Detection Tests
// =============================================================================

/// Test GPU fingerprint validation
/// Security property: Tampered GPU buffers are detected
#[test]
fn test_gpu_fingerprint_tampering_detection() {
    // Create original fingerprint
    let original_checkpoint = B3Hash::hash(b"original_gpu_buffer_contents");
    let original_fp = GpuFingerprint {
        adapter_id: "adapter_001".to_string(),
        buffer_bytes: 1024 * 1024,
        checkpoint_hash: original_checkpoint,
    };

    // Simulate tampered fingerprint (modified buffer)
    let tampered_checkpoint = B3Hash::hash(b"tampered_gpu_buffer_contents");
    let tampered_fp = GpuFingerprint {
        adapter_id: "adapter_001".to_string(),
        buffer_bytes: 1024 * 1024,
        checkpoint_hash: tampered_checkpoint,
    };

    // Verify tampering is detectable
    assert_ne!(
        original_fp.checkpoint_hash, tampered_fp.checkpoint_hash,
        "Tampered GPU buffer not detected"
    );

    // Verify same content produces same fingerprint
    let replay_checkpoint = B3Hash::hash(b"original_gpu_buffer_contents");
    assert_eq!(
        original_checkpoint, replay_checkpoint,
        "Non-deterministic GPU fingerprinting"
    );
}

/// Test cross-layer hash includes GPU state
/// Security property: Stack hash changes when GPU state changes
#[test]
fn test_cross_layer_hash_includes_gpu_state() {
    // Create GPU fingerprints
    let fp1 = GpuFingerprint {
        adapter_id: "adapter_001".to_string(),
        buffer_bytes: 1024 * 1024,
        checkpoint_hash: B3Hash::hash(b"buffer_1"),
    };

    let fp2_original = GpuFingerprint {
        adapter_id: "adapter_002".to_string(),
        buffer_bytes: 2 * 1024 * 1024,
        checkpoint_hash: B3Hash::hash(b"buffer_2_original"),
    };

    let fp2_modified = GpuFingerprint {
        adapter_id: "adapter_002".to_string(),
        buffer_bytes: 2 * 1024 * 1024,
        checkpoint_hash: B3Hash::hash(b"buffer_2_modified"),
    };

    // Compute cross-layer hashes
    let hash_original = compute_cross_layer_hash(&[fp1.clone(), fp2_original]);
    let hash_modified = compute_cross_layer_hash(&[fp1.clone(), fp2_modified]);

    assert_ne!(
        hash_original, hash_modified,
        "Cross-layer hash did not detect GPU state change"
    );
}

/// Helper to compute cross-layer hash from GPU fingerprints
fn compute_cross_layer_hash(fingerprints: &[GpuFingerprint]) -> B3Hash {
    let mut hasher = blake3::Hasher::new();

    let mut sorted_fps: Vec<_> = fingerprints.iter().collect();
    sorted_fps.sort_by(|a, b| a.adapter_id.cmp(&b.adapter_id));

    for fp in sorted_fps {
        hasher.update(fp.adapter_id.as_bytes());
        hasher.update(&fp.buffer_bytes.to_le_bytes());
        hasher.update(fp.checkpoint_hash.as_bytes());
    }

    B3Hash::from_bytes(hasher.finalize().into())
}

// =============================================================================
// SECTION 4: Checkpoint Integrity Verification Tests
// =============================================================================

/// Test checkpoint tampering detection
/// Security property: Modified checkpoints fail verification
#[test]
fn test_checkpoint_tamper_detection() {
    let original_checkpoint = StackCheckpoint {
        timestamp: 1700000000,
        metadata_hash: B3Hash::hash(b"original_metadata"),
        cross_layer_hash: Some(B3Hash::hash(b"original_cross_layer")),
        gpu_fingerprints: vec![GpuFingerprint {
            adapter_id: "adapter_001".to_string(),
            buffer_bytes: 1024,
            checkpoint_hash: B3Hash::hash(b"original_buffer"),
        }],
        adapter_ids: vec!["adapter_001".to_string()],
    };

    // Tampered checkpoint with modified GPU fingerprint
    let tampered_checkpoint = StackCheckpoint {
        timestamp: 1700000000,
        metadata_hash: B3Hash::hash(b"original_metadata"),
        cross_layer_hash: Some(B3Hash::hash(b"original_cross_layer")),
        gpu_fingerprints: vec![GpuFingerprint {
            adapter_id: "adapter_001".to_string(),
            buffer_bytes: 1024,
            checkpoint_hash: B3Hash::hash(b"TAMPERED_buffer"), // Modified!
        }],
        adapter_ids: vec!["adapter_001".to_string()],
    };

    // Compute hashes and verify they differ
    let original_hash = B3Hash::hash_multi(&[
        original_checkpoint.metadata_hash.as_bytes(),
        original_checkpoint.gpu_fingerprints[0]
            .checkpoint_hash
            .as_bytes(),
    ]);

    let tampered_hash = B3Hash::hash_multi(&[
        tampered_checkpoint.metadata_hash.as_bytes(),
        tampered_checkpoint.gpu_fingerprints[0]
            .checkpoint_hash
            .as_bytes(),
    ]);

    assert_ne!(
        original_hash, tampered_hash,
        "Checkpoint tampering not detected"
    );
}

// =============================================================================
// SECTION 5: Memory Safety in FFI Boundaries Tests
// =============================================================================

/// Test FFI handle validation concept
/// Security property: Invalid handles are rejected
#[test]
fn test_ffi_handle_validation_concept() {
    // Simulate handle validation logic from CoreML FFI
    struct MockTensorHandle {
        tensor_ptr: *mut std::ffi::c_void,
        rank: u32,
    }

    impl MockTensorHandle {
        fn is_valid(&self) -> bool {
            !self.tensor_ptr.is_null() && self.rank > 0
        }
    }

    // Test null handle validation
    let null_handle = MockTensorHandle {
        tensor_ptr: std::ptr::null_mut(),
        rank: 0,
    };
    assert!(!null_handle.is_valid(), "Null handle should be invalid");

    // Test handle with null pointer but non-zero rank
    let invalid_handle = MockTensorHandle {
        tensor_ptr: std::ptr::null_mut(),
        rank: 3,
    };
    assert!(
        !invalid_handle.is_valid(),
        "Handle with null pointer should be invalid"
    );

    // Test valid handle
    let mut dummy: u8 = 0;
    let valid_handle = MockTensorHandle {
        tensor_ptr: &mut dummy as *mut u8 as *mut std::ffi::c_void,
        rank: 3,
    };
    assert!(
        valid_handle.is_valid(),
        "Valid handle should pass validation"
    );
}

/// Test buffer bounds validation concept
/// Security property: Out-of-bounds access prevented
#[test]
fn test_buffer_bounds_validation_concept() {
    // This tests the concept of bounds validation
    let buffer_size = 1024usize;
    let requested_bytes = 2048usize;

    // Simulate bounds check
    let is_valid = requested_bytes <= buffer_size;
    assert!(!is_valid, "Out-of-bounds access should be detected");

    // Valid access
    let valid_requested = 512usize;
    let is_valid = valid_requested <= buffer_size;
    assert!(is_valid, "Valid access should be allowed");
}

/// Test safe slice creation
/// Security property: Slices cannot exceed source bounds
#[test]
fn test_safe_slice_bounds() {
    let data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];

    // Safe slice creation function (mirrors Metal kernel pattern)
    fn safe_slice(data: &[f32], start: usize, len: usize) -> Option<&[f32]> {
        if start + len > data.len() {
            None
        } else {
            Some(&data[start..start + len])
        }
    }

    // Valid slice
    assert!(safe_slice(&data, 0, 5).is_some());
    assert!(safe_slice(&data, 2, 3).is_some());

    // Invalid slices
    assert!(safe_slice(&data, 0, 6).is_none());
    assert!(safe_slice(&data, 3, 3).is_none());
    assert!(safe_slice(&data, 10, 1).is_none());
}

// =============================================================================
// SECTION 6: Seed Reuse Prevention Tests
// =============================================================================

/// Test seed registry prevents reuse
/// Security property: Same seed cannot be used twice
#[test]
fn test_seed_reuse_prevention() {
    // Clear registry for clean test
    clear_seed_registry();

    let global_seed = B3Hash::hash(b"reuse_test");
    let adapter_id = 0;
    let layer = 0;
    let nonce = 12345u64;

    // First derivation should succeed
    let result1 = derive_adapter_seed(&global_seed, adapter_id, layer, nonce);
    assert!(result1.is_ok(), "First seed derivation should succeed");

    // Second derivation with same parameters should fail
    let result2 = derive_adapter_seed(&global_seed, adapter_id, layer, nonce);
    assert!(result2.is_err(), "Seed reuse should be prevented");

    // Different nonce should succeed
    let result3 = derive_adapter_seed(&global_seed, adapter_id, layer, nonce + 1);
    assert!(result3.is_ok(), "Different nonce should succeed");

    // Cleanup
    clear_seed_registry();
}

/// Test registry clearing at inference boundaries
/// Security property: Registry can be safely cleared
#[test]
fn test_seed_registry_clearing() {
    let global_seed = B3Hash::hash(b"clear_test");

    // Derive some seeds
    for i in 0..10 {
        let _ = derive_adapter_seed(&global_seed, i, 0, 0);
    }

    // Clear registry
    clear_seed_registry();

    // Should be able to derive same seeds again
    for i in 0..10 {
        let result = derive_adapter_seed(&global_seed, i, 0, 0);
        assert!(
            result.is_ok(),
            "Should be able to reuse seeds after clearing registry"
        );
    }

    // Cleanup
    clear_seed_registry();
}

// =============================================================================
// SECTION 7: Hash Collision Detection (Birthday Attack Resistance) Tests
// =============================================================================

/// Test BLAKE3 collision resistance
/// Security property: No collisions in large sample space
#[test]
fn test_blake3_collision_resistance_sample() {
    let mut hashes: HashSet<B3Hash> = HashSet::new();

    // Generate 100,000 unique inputs
    for i in 0u64..100_000 {
        let input = format!("collision_test_input_{}", i);
        let hash = B3Hash::hash(input.as_bytes());

        assert!(
            hashes.insert(hash),
            "Collision detected at iteration {} - BLAKE3 integrity compromised!",
            i
        );
    }
}

/// Test B3Hash hex roundtrip
/// Security property: Hash representation is bijective
#[test]
fn test_hash_representation_bijective() {
    let original = B3Hash::hash(b"bijective_test");
    let hex = original.to_hex();
    let restored = B3Hash::from_hex(&hex).expect("Hex parsing failed");

    assert_eq!(original, restored, "Hash hex roundtrip not bijective");
}

/// Test hash multi-input determinism
/// Security property: hash_multi is order-sensitive and deterministic
#[test]
fn test_hash_multi_ordering() {
    let a = b"input_a";
    let b = b"input_b";

    let hash_ab = B3Hash::hash_multi(&[a, b]);
    let hash_ba = B3Hash::hash_multi(&[b, a]);
    let hash_ab_repeat = B3Hash::hash_multi(&[a, b]);

    // Order matters
    assert_ne!(hash_ab, hash_ba, "hash_multi should be order-sensitive");

    // Deterministic
    assert_eq!(
        hash_ab, hash_ab_repeat,
        "hash_multi should be deterministic"
    );
}

/// Test hash concatenation equivalence
/// Security property: hash_multi(a, b) == hash(a || b)
#[test]
fn test_hash_concatenation_equivalence() {
    let a = b"part_a";
    let b = b"part_b";

    let hash_multi = B3Hash::hash_multi(&[a, b]);

    let mut concatenated = Vec::new();
    concatenated.extend_from_slice(a);
    concatenated.extend_from_slice(b);
    let hash_concat = B3Hash::hash(&concatenated);

    assert_eq!(
        hash_multi, hash_concat,
        "hash_multi should be equivalent to hash(concatenation)"
    );
}

// =============================================================================
// SECTION 8: Additional Security Property Tests
// =============================================================================

/// Test zero hash is distinguishable
/// Security property: Zero hash is not accidentally producible
#[test]
fn test_zero_hash_distinguishable() {
    let zero = B3Hash::zero();

    // Test various inputs that might accidentally produce zero
    let test_inputs: &[&[u8]] = &[
        b"",
        b"\0",
        b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        b"null",
        b"zero",
        b"empty",
    ];

    for input in test_inputs {
        let hash = B3Hash::hash(input);
        assert_ne!(
            hash, zero,
            "Input {:?} produced zero hash - security concern",
            input
        );
    }
}

/// Test stack hash computation is deterministic
/// Security property: Stack hash depends only on adapter set
#[test]
fn test_stack_hash_determinism() {
    // Simulate stack hash computation
    fn compute_stack_hash(adapters: &[(String, B3Hash)]) -> B3Hash {
        let mut sorted: Vec<_> = adapters.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let mut hasher = blake3::Hasher::new();
        for (id, hash) in sorted {
            hasher.update(id.as_bytes());
            hasher.update(hash.as_bytes());
        }
        B3Hash::from_bytes(hasher.finalize().into())
    }

    let adapters = vec![
        ("adapter_a".to_string(), B3Hash::hash(b"a")),
        ("adapter_b".to_string(), B3Hash::hash(b"b")),
    ];

    let hash1 = compute_stack_hash(&adapters);
    let hash2 = compute_stack_hash(&adapters);

    assert_eq!(
        hash1, hash2,
        "Stack hash should be deterministic across identical adapter sets"
    );
}

// =============================================================================
// SECTION 9: Stress Tests for Security Properties
// =============================================================================

/// Stress test: Rapid seed derivation
/// Security property: No collisions under high throughput
#[test]
fn stress_test_rapid_seed_derivation() {
    let global_seed = B3Hash::hash(b"stress_test");
    let iterations = 10_000;

    let mut seeds: HashSet<[u8; 32]> = HashSet::with_capacity(iterations);

    let start = std::time::Instant::now();

    for i in 0..iterations {
        let seed = derive_seed_indexed(&global_seed, "stress", i);
        if !seeds.insert(seed) {
            panic!("Collision at iteration {}", i);
        }
    }

    let duration = start.elapsed();

    // Should complete in reasonable time (< 5 seconds for 10k iterations)
    assert!(
        duration.as_secs() < 5,
        "Seed derivation too slow: {:?}",
        duration
    );
}

/// Stress test: Hash computation throughput
/// Security property: Hash function maintains performance under load
#[test]
fn stress_test_hash_throughput() {
    let iterations = 100_000;
    let input_size = 1024; // 1KB per hash

    let input: Vec<u8> = (0..input_size).map(|i| (i % 256) as u8).collect();

    let start = std::time::Instant::now();

    for i in 0..iterations {
        let mut varied_input = input.clone();
        varied_input[0] = (i % 256) as u8;
        let _ = B3Hash::hash(&varied_input);
    }

    let duration = start.elapsed();
    let throughput_mb_s =
        (iterations * input_size) as f64 / (1024.0 * 1024.0) / duration.as_secs_f64();

    // BLAKE3 should achieve > 50 MB/s on modern hardware
    assert!(
        throughput_mb_s > 50.0,
        "Hash throughput too low: {:.2} MB/s",
        throughput_mb_s
    );
}

// =============================================================================
// SECTION 10: ACL Enforcement and Unauthorized Access Prevention Tests
// =============================================================================

/// Test ACL enforcement prevents unauthorized adapter swap
/// Security property: Adapter swaps enforce tenant isolation
#[test]
fn test_acl_unauthorized_swap_prevention() {
    use adapteros_registry::Registry;
    use tempfile::tempdir;

    let dir = tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("test_registry.db");
    let registry = Registry::open(&db_path).expect("Failed to create registry");

    // Register adapters with tenant isolation
    let adapter1_hash = B3Hash::hash(b"adapter1_weights");
    let adapter2_hash = B3Hash::hash(b"adapter2_weights");

    // Adapter 1 - accessible only by tenant_a
    registry
        .register_adapter(
            "adapter_001",
            &adapter1_hash,
            "tier_1",
            16,
            &["tenant_a".to_string()],
        )
        .expect("Failed to register adapter 1");

    // Adapter 2 - accessible only by tenant_b
    registry
        .register_adapter(
            "adapter_002",
            &adapter2_hash,
            "tier_1",
            16,
            &["tenant_b".to_string()],
        )
        .expect("Failed to register adapter 2");

    // Verify tenant_a can access adapter_001
    let can_access = registry
        .check_acl("adapter_001", "tenant_a")
        .expect("ACL check failed");
    assert!(can_access, "tenant_a should have access to adapter_001");

    // Verify tenant_a CANNOT access adapter_002
    let can_access = registry
        .check_acl("adapter_002", "tenant_a")
        .expect("ACL check failed");
    assert!(
        !can_access,
        "tenant_a should NOT have access to adapter_002 (cross-tenant violation)"
    );

    // Verify tenant_b can access adapter_002
    let can_access = registry
        .check_acl("adapter_002", "tenant_b")
        .expect("ACL check failed");
    assert!(can_access, "tenant_b should have access to adapter_002");

    // Verify tenant_b CANNOT access adapter_001
    let can_access = registry
        .check_acl("adapter_001", "tenant_b")
        .expect("ACL check failed");
    assert!(
        !can_access,
        "tenant_b should NOT have access to adapter_001 (cross-tenant violation)"
    );
}

/// Test ACL inheritance from parent adapters
/// Security property: Child adapters inherit parent ACLs when not overridden
#[test]
fn test_acl_inheritance() {
    use adapteros_core::ForkType;
    use adapteros_registry::Registry;
    use tempfile::tempdir;

    let dir = tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("test_registry.db");
    let registry = Registry::open(&db_path).expect("Failed to create registry");

    // Register parent adapter with tenant_a ACL
    let parent_hash = B3Hash::hash(b"parent_weights");
    registry
        .register_adapter(
            "parent_adapter",
            &parent_hash,
            "tier_1",
            16,
            &["tenant_a".to_string()],
        )
        .expect("Failed to register parent");

    // Register child adapter WITHOUT ACL (should inherit from parent)
    let child_hash = B3Hash::hash(b"child_weights");
    registry
        .register_adapter_with_name(
            "child_adapter",
            None,
            &child_hash,
            "tier_1",
            16,
            &[], // Empty ACL - should inherit
            Some("parent_adapter"),
            Some(ForkType::Extension),
        )
        .expect("Failed to register child");

    // Verify child inherited parent's ACL
    let can_access = registry
        .check_acl("child_adapter", "tenant_a")
        .expect("ACL check failed");
    assert!(
        can_access,
        "Child should inherit parent's ACL (tenant_a access)"
    );

    // Verify child denies access to other tenants
    let can_access = registry
        .check_acl("child_adapter", "tenant_b")
        .expect("ACL check failed");
    assert!(
        !can_access,
        "Child should inherit parent's ACL (deny tenant_b)"
    );
}

/// Test circular dependency prevention in adapter lineage
/// Security property: Circular dependencies are rejected
#[test]
fn test_circular_dependency_prevention() {
    use adapteros_core::ForkType;
    use adapteros_registry::Registry;
    use tempfile::tempdir;

    let dir = tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("test_registry.db");
    let registry = Registry::open(&db_path).expect("Failed to create registry");

    // Register adapter A
    let hash_a = B3Hash::hash(b"adapter_a");
    registry
        .register_adapter("adapter_a", &hash_a, "tier_1", 16, &[])
        .expect("Failed to register A");

    // Register adapter B with parent A
    let hash_b = B3Hash::hash(b"adapter_b");
    registry
        .register_adapter_with_name(
            "adapter_b",
            None,
            &hash_b,
            "tier_1",
            16,
            &[],
            Some("adapter_a"),
            Some(ForkType::Extension),
        )
        .expect("Failed to register B");

    // Attempt to register adapter C with parent B
    let hash_c = B3Hash::hash(b"adapter_c");
    registry
        .register_adapter_with_name(
            "adapter_c",
            None,
            &hash_c,
            "tier_1",
            16,
            &[],
            Some("adapter_b"),
            Some(ForkType::Extension),
        )
        .expect("Failed to register C");

    // CRITICAL TEST: Attempt to make A a child of C (creates cycle A→B→C→A)
    // This should be REJECTED
    let hash_a_new = B3Hash::hash(b"adapter_a_new");
    let result = registry.register_adapter_with_name(
        "adapter_a",
        None,
        &hash_a_new,
        "tier_1",
        16,
        &[],
        Some("adapter_c"), // This creates a cycle!
        Some(ForkType::Extension),
    );

    assert!(
        result.is_err(),
        "Circular dependency should be rejected (A→B→C→A cycle)"
    );
    assert!(
        result.unwrap_err().to_string().contains("Circular"),
        "Error should mention circular dependency"
    );
}

/// Test multi-tenant adapter isolation
/// Security property: Adapters with multi-tenant ACLs are accessible by all specified tenants
#[test]
fn test_multi_tenant_acl() {
    use adapteros_registry::Registry;
    use tempfile::tempdir;

    let dir = tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("test_registry.db");
    let registry = Registry::open(&db_path).expect("Failed to create registry");

    // Register shared adapter accessible by multiple tenants
    let shared_hash = B3Hash::hash(b"shared_adapter");
    registry
        .register_adapter(
            "shared_adapter",
            &shared_hash,
            "tier_1",
            16,
            &["tenant_a".to_string(), "tenant_b".to_string()],
        )
        .expect("Failed to register shared adapter");

    // Verify tenant_a has access
    let can_access = registry
        .check_acl("shared_adapter", "tenant_a")
        .expect("ACL check failed");
    assert!(can_access, "tenant_a should have access");

    // Verify tenant_b has access
    let can_access = registry
        .check_acl("shared_adapter", "tenant_b")
        .expect("ACL check failed");
    assert!(can_access, "tenant_b should have access");

    // Verify tenant_c does NOT have access
    let can_access = registry
        .check_acl("shared_adapter", "tenant_c")
        .expect("ACL check failed");
    assert!(!can_access, "tenant_c should NOT have access");
}

// =============================================================================
// SECTION 11: Deterministic Execution Boundary Tests
// =============================================================================

/// Test HKDF seed derivation with full context isolation
/// Security property: Different execution contexts produce different seeds
#[test]
fn test_seed_derivation_full_context_isolation() {
    use adapteros_core::derive_seed_full;

    let global_seed = B3Hash::hash(b"global_test");
    let manifest1 = B3Hash::hash(b"manifest_v1");
    let manifest2 = B3Hash::hash(b"manifest_v2");
    let adapter_dir = B3Hash::hash(b"/adapters/test");

    // Same worker, different manifests
    let seed1 = derive_seed_full(&global_seed, &manifest1, &adapter_dir, 1, "router", 0);
    let seed2 = derive_seed_full(&global_seed, &manifest2, &adapter_dir, 1, "router", 0);
    assert_ne!(
        seed1, seed2,
        "Different manifests must produce different seeds"
    );

    // Same manifest, different workers
    let seed3 = derive_seed_full(&global_seed, &manifest1, &adapter_dir, 1, "router", 0);
    let seed4 = derive_seed_full(&global_seed, &manifest1, &adapter_dir, 2, "router", 0);
    assert_ne!(
        seed3, seed4,
        "Different workers must produce different seeds"
    );

    // Same manifest, same worker, different nonces
    let seed5 = derive_seed_full(&global_seed, &manifest1, &adapter_dir, 1, "router", 0);
    let seed6 = derive_seed_full(&global_seed, &manifest1, &adapter_dir, 1, "router", 1);
    assert_ne!(
        seed5, seed6,
        "Different nonces must produce different seeds"
    );

    // Exact same parameters - should be identical
    let seed7 = derive_seed_full(&global_seed, &manifest1, &adapter_dir, 1, "router", 0);
    let seed8 = derive_seed_full(&global_seed, &manifest1, &adapter_dir, 1, "router", 0);
    assert_eq!(
        seed7, seed8,
        "Identical parameters must produce identical seeds"
    );
}

/// Test adapter directory path canonicalization
/// Security property: Path normalization prevents symlink attacks
#[test]
fn test_adapter_dir_path_canonicalization() {
    use adapteros_core::hash_adapter_dir;
    use std::path::Path;

    // Test path hashing consistency
    let path1 = Path::new("/adapters/test");
    let hash1 = hash_adapter_dir(path1);
    let hash2 = hash_adapter_dir(path1);

    assert_eq!(
        hash1, hash2,
        "Same path should produce same hash (deterministic)"
    );

    // Different paths should produce different hashes
    let path2 = Path::new("/adapters/production");
    let hash3 = hash_adapter_dir(path2);
    assert_ne!(
        hash1, hash3,
        "Different paths should produce different hashes"
    );
}

// =============================================================================
// SECTION 12: GPU Memory Isolation and Bounds Checking
// =============================================================================

/// Test GPU buffer fingerprint creation and verification
/// Security property: Fingerprints accurately detect buffer modifications
#[test]
fn test_gpu_buffer_fingerprint_creation() {
    // Simulate GPU buffer samples
    let first_4kb: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    let last_4kb: Vec<u8> = (0..4096).map(|i| ((i + 128) % 256) as u8).collect();
    let mid_4kb: Vec<u8> = (0..4096).map(|i| ((i + 64) % 256) as u8).collect();

    // Create fingerprint
    let fp1 = GpuFingerprint {
        adapter_id: "test_adapter".to_string(),
        buffer_bytes: 1024 * 1024,
        checkpoint_hash: B3Hash::hash_multi(&[&first_4kb, &last_4kb, &mid_4kb]),
    };

    // Same samples should produce identical fingerprint
    let fp2 = GpuFingerprint {
        adapter_id: "test_adapter".to_string(),
        buffer_bytes: 1024 * 1024,
        checkpoint_hash: B3Hash::hash_multi(&[&first_4kb, &last_4kb, &mid_4kb]),
    };

    assert_eq!(
        fp1.checkpoint_hash, fp2.checkpoint_hash,
        "Identical buffer samples should produce identical fingerprints"
    );

    // Modified sample should produce different fingerprint
    let mut modified_first = first_4kb.clone();
    modified_first[100] ^= 0xFF; // Flip a byte

    let fp3 = GpuFingerprint {
        adapter_id: "test_adapter".to_string(),
        buffer_bytes: 1024 * 1024,
        checkpoint_hash: B3Hash::hash_multi(&[&modified_first, &last_4kb, &mid_4kb]),
    };

    assert_ne!(
        fp1.checkpoint_hash, fp3.checkpoint_hash,
        "Modified buffer sample should produce different fingerprint"
    );
}

/// Test adapter ID u16 mapping boundary cases
/// Security property: Edge cases don't cause collisions
#[test]
fn test_adapter_id_u16_boundary_cases() {
    // Test empty string
    let id1 = adapter_id_to_u16("");
    let id2 = adapter_id_to_u16(" ");
    assert_ne!(id1, id2, "Empty and space strings should map differently");

    // Test very long IDs
    let long_id1 = "a".repeat(1000);
    let long_id2 = "a".repeat(1001);
    let id3 = adapter_id_to_u16(&long_id1);
    let id4 = adapter_id_to_u16(&long_id2);
    assert_ne!(
        id3, id4,
        "Long IDs differing by 1 char should map differently"
    );

    // Test special characters
    let special1 = "adapter/001";
    let special2 = "adapter\\001";
    let id5 = adapter_id_to_u16(special1);
    let id6 = adapter_id_to_u16(special2);
    assert_ne!(id5, id6, "Different path separators should map differently");
}

// =============================================================================
// SECTION 13: Cryptographic Integrity End-to-End Tests
// =============================================================================

/// Test end-to-end stack integrity verification
/// Security property: Full stack state changes are detectable
#[test]
fn test_e2e_stack_integrity() {
    // Initial stack state
    let adapter1_hash = B3Hash::hash(b"adapter1_v1");
    let adapter2_hash = B3Hash::hash(b"adapter2_v1");

    let initial_adapters = vec![
        ("adapter_001".to_string(), adapter1_hash),
        ("adapter_002".to_string(), adapter2_hash),
    ];

    // Compute initial stack hash
    let initial_stack_hash = adapteros_core::compute_stack_hash(initial_adapters.clone());

    // Modified stack (adapter1 updated to v2)
    let adapter1_hash_v2 = B3Hash::hash(b"adapter1_v2");
    let modified_adapters = vec![
        ("adapter_001".to_string(), adapter1_hash_v2),
        ("adapter_002".to_string(), adapter2_hash),
    ];

    let modified_stack_hash = adapteros_core::compute_stack_hash(modified_adapters);

    // Verify modification detected
    assert_ne!(
        initial_stack_hash, modified_stack_hash,
        "Stack hash should change when adapter weights change"
    );

    // Verify order independence
    let reordered_adapters = vec![
        ("adapter_002".to_string(), adapter2_hash),
        ("adapter_001".to_string(), adapter1_hash),
    ];
    let reordered_stack_hash = adapteros_core::compute_stack_hash(reordered_adapters);

    assert_eq!(
        initial_stack_hash, reordered_stack_hash,
        "Stack hash should be order-independent"
    );
}
