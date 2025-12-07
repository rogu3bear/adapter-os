#![cfg(all(test, feature = "extended-tests"))]
//! HKDF seeding verification tests for AdapterOS determinism
//!
//! Verifies that HKDF-based seeding produces deterministic, isolated randomness
//! for all components requiring random number generation.

use super::utils::*;
use adapteros_core::{B3Hash, derive_seed};

/// Test basic HKDF seed derivation
#[test]
fn test_hkdf_seed_derivation() {
    let mut verifier = HkdfSeedingVerifier::new([0x42; 32]);

    // Test basic seed derivation
    let seed = derive_seed(&B3Hash::from_bytes([0x42; 32]), "test_label");
    assert_eq!(seed.len(), 32, "HKDF output should be 32 bytes");

    // Verify derivation is deterministic
    let seed2 = derive_seed(&B3Hash::from_bytes([0x42; 32]), "test_label");
    assert_eq!(seed, seed2, "HKDF derivation should be deterministic");
}

/// Test HKDF domain separation
#[test]
fn test_hkdf_domain_separation() {
    let mut verifier = HkdfSeedingVerifier::new([0x42; 32]);

    // Different labels should produce different seeds
    let seed_router = derive_seed(&B3Hash::from_bytes([0x42; 32]), "router");
    let seed_dropout = derive_seed(&B3Hash::from_bytes([0x42; 32]), "dropout");
    let seed_sampling = derive_seed(&B3Hash::from_bytes([0x42; 32]), "sampling");

    assert_ne!(seed_router, seed_dropout, "Different domains should have different seeds");
    assert_ne!(seed_dropout, seed_sampling, "Different domains should have different seeds");
    assert_ne!(seed_router, seed_sampling, "Different domains should have different seeds");

    // Same label should produce same seed
    let seed_router2 = derive_seed(&B3Hash::from_bytes([0x42; 32]), "router");
    assert_eq!(seed_router, seed_router2, "Same domain should have same seed");
}

/// Test HKDF seed consistency across runs
#[test]
fn test_hkdf_seed_consistency() {
    let mut verifier = HkdfSeedingVerifier::new([0x42; 32]);

    // Test consistency across multiple runs
    for run in 0..10 {
        let seed = derive_seed(&B3Hash::from_bytes([0x42; 32]), "consistency_test");
        let expected = derive_seed(&B3Hash::from_bytes([0x42; 32]), "consistency_test");

        assert_eq!(seed, expected, "HKDF should be consistent across runs {}", run);
    }
}

/// Test HKDF with different global seeds
#[test]
fn test_hkdf_different_global_seeds() {
    let global_seed1 = [0x11; 32];
    let global_seed2 = [0x22; 32];

    let seed1 = derive_seed(&B3Hash::from_bytes(global_seed1), "test");
    let seed2 = derive_seed(&B3Hash::from_bytes(global_seed2), "test");

    // Different global seeds should produce different derived seeds
    assert_ne!(seed1, seed2, "Different global seeds should produce different derived seeds");

    // But same global seed should produce same derived seed
    let seed1_again = derive_seed(&B3Hash::from_bytes(global_seed1), "test");
    assert_eq!(seed1, seed1_again, "Same global seed should produce same derived seed");
}

/// Test HKDF seed hierarchy
#[test]
fn test_hkdf_seed_hierarchy() {
    let global_seed = [0x42; 32];

    // Derive hierarchical seeds
    let level1_seed = derive_seed(&B3Hash::from_bytes(global_seed), "level1");
    let level2_seed = derive_seed(&B3Hash::from_bytes(level1_seed), "level2");
    let level3_seed = derive_seed(&B3Hash::from_bytes(level2_seed), "level3");

    // All levels should be different
    assert_ne!(level1_seed, level2_seed);
    assert_ne!(level2_seed, level3_seed);
    assert_ne!(level1_seed, level3_seed);

    // Hierarchy should be deterministic
    let level1_seed2 = derive_seed(&B3Hash::from_bytes(global_seed), "level1");
    let level2_seed2 = derive_seed(&B3Hash::from_bytes(level1_seed2), "level2");
    let level3_seed2 = derive_seed(&B3Hash::from_bytes(level2_seed2), "level3");

    assert_eq!(level1_seed, level1_seed2);
    assert_eq!(level2_seed, level2_seed2);
    assert_eq!(level3_seed, level3_seed2);
}

/// Test HKDF seed for RNG initialization
#[test]
fn test_hkdf_rng_seeding() {
    use adapteros_lora_worker::deterministic_rng::DeterministicRng;

    let global_seed = [0x42; 32];

    // Create RNG with HKDF-derived seed
    let mut rng = DeterministicRng::new(&global_seed, "rng_test").unwrap();

    // Generate a sequence
    let mut sequence1 = Vec::new();
    for _ in 0..10 {
        sequence1.push(rng.next_u64());
    }

    // Create another RNG with same parameters
    let mut rng2 = DeterministicRng::new(&global_seed, "rng_test").unwrap();
    let mut sequence2 = Vec::new();
    for _ in 0..10 {
        sequence2.push(rng2.next_u64());
    }

    // Sequences should be identical
    assert_eq!(sequence1, sequence2, "HKDF-seeded RNG should be deterministic");

    // Different labels should produce different sequences
    let mut rng3 = DeterministicRng::new(&global_seed, "different_label").unwrap();
    let mut sequence3 = Vec::new();
    for _ in 0..10 {
        sequence3.push(rng3.next_u64());
    }

    assert_ne!(sequence1, sequence3, "Different labels should produce different RNG sequences");
}

/// Test HKDF seed for dropout masking
#[test]
fn test_hkdf_dropout_seeding() {
    let global_seed = [0x42; 32];

    // Simulate dropout mask generation for different layers
    let mut masks = Vec::new();

    for layer in 0..5 {
        let layer_seed = derive_seed(&B3Hash::from_bytes(global_seed), &format!("dropout_layer_{}", layer));

        // Generate dropout mask (simplified)
        let mut mask = Vec::new();
        let mut current = layer_seed;
        for _ in 0..10 {
            current = B3Hash::hash(&current).into_bytes();
            mask.push(current[0] > 128); // Simple threshold
        }

        masks.push(mask);
    }

    // All masks should be deterministic
    let mut masks2 = Vec::new();
    for layer in 0..5 {
        let layer_seed = derive_seed(&B3Hash::from_bytes(global_seed), &format!("dropout_layer_{}", layer));

        let mut mask = Vec::new();
        let mut current = layer_seed;
        for _ in 0..10 {
            current = B3Hash::hash(&current).into_bytes();
            mask.push(current[0] > 128);
        }

        masks2.push(mask);
    }

    assert_eq!(masks, masks2, "Dropout masks should be deterministic");

    // Different layers should have different masks
    for i in 0..masks.len() {
        for j in (i+1)..masks.len() {
            assert_ne!(masks[i], masks[j], "Different layers should have different dropout masks");
        }
    }
}

/// Test HKDF seed for sampling operations
#[test]
fn test_hkdf_sampling_seeding() {
    let global_seed = [0x42; 32];

    // Simulate token sampling with temperature
    let mut samples = Vec::new();

    for sample_round in 0..3 {
        let sample_seed = derive_seed(&B3Hash::from_bytes(global_seed), &format!("sampling_round_{}", sample_round));

        // Generate sampling decisions (simplified)
        let mut decisions = Vec::new();
        let mut current = sample_seed;
        for _ in 0..5 {
            current = B3Hash::hash(&current).into_bytes();
            decisions.push(current[0] % 100); // Token probabilities
        }

        samples.push(decisions);
    }

    // Verify determinism
    let mut samples2 = Vec::new();
    for sample_round in 0..3 {
        let sample_seed = derive_seed(&B3Hash::from_bytes(global_seed), &format!("sampling_round_{}", sample_round));

        let mut decisions = Vec::new();
        let mut current = sample_seed;
        for _ in 0..5 {
            current = B3Hash::hash(&current).into_bytes();
            decisions.push(current[0] % 100);
        }

        samples2.push(decisions);
    }

    assert_eq!(samples, samples2, "Sampling decisions should be deterministic");

    // Different rounds should have different samples
    assert_ne!(samples[0], samples[1]);
    assert_ne!(samples[1], samples[2]);
}

/// Test HKDF seed isolation between components
#[test]
fn test_hkdf_component_isolation() {
    let global_seed = [0x42; 32];

    // Different components should have isolated seeds
    let router_seed = derive_seed(&B3Hash::from_bytes(global_seed), "router");
    let worker_seed = derive_seed(&B3Hash::from_bytes(global_seed), "worker");
    let telemetry_seed = derive_seed(&B3Hash::from_bytes(global_seed), "telemetry");

    assert_ne!(router_seed, worker_seed);
    assert_ne!(worker_seed, telemetry_seed);
    assert_ne!(router_seed, telemetry_seed);

    // Components should not be able to derive each other's seeds
    // (This is more of a policy test - in practice, all derive from global)
    let derived_router = derive_seed(&B3Hash::from_bytes(worker_seed), "router_attempt");
    assert_ne!(derived_router, router_seed, "Components should not be able to derive other component seeds");
}

/// Test HKDF seed performance
#[test]
fn test_hkdf_performance() {
    let global_seed = [0x42; 32];

    let start = std::time::Instant::now();

    // Derive many seeds
    for i in 0..1000 {
        let _seed = derive_seed(&B3Hash::from_bytes(global_seed), &format!("perf_test_{}", i));
    }

    let duration = start.elapsed();

    // Should be fast (< 100ms for 1000 derivations)
    assert!(duration < std::time::Duration::from_millis(100),
            "HKDF derivation should be performant: {:?}", duration);
}

/// Test HKDF seed entropy
#[test]
fn test_hkdf_entropy() {
    let global_seed = [0x42; 32];

    // Collect many derived seeds
    let mut seeds = Vec::new();
    for i in 0..100 {
        let seed = derive_seed(&B3Hash::from_bytes(global_seed), &format!("entropy_test_{}", i));
        seeds.push(seed);
    }

    // Check that seeds have good entropy (no obvious patterns)
    // This is a statistical test - in practice, HKDF should provide good entropy

    // Count how many seeds have each byte value in first position
    let mut byte_counts = [0u32; 256];
    for seed in &seeds {
        byte_counts[seed[0] as usize] += 1;
    }

    // Chi-square test (simplified)
    let expected = seeds.len() as f64 / 256.0;
    let mut chi_square = 0.0;

    for &count in &byte_counts {
        let diff = count as f64 - expected;
        chi_square += diff * diff / expected;
    }

    // For 256 bins and 100 samples, chi-square should be reasonable
    // (This is a very basic test - in practice, use proper statistical tests)
    assert!(chi_square < 500.0, "HKDF should provide good entropy distribution");

    // Verify no two seeds are identical
    for i in 0..seeds.len() {
        for j in (i+1)..seeds.len() {
            assert_ne!(seeds[i], seeds[j], "All derived seeds should be unique");
        }
    }
}