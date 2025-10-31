#![cfg(all(test, feature = "extended-tests"))]

//! Comprehensive RNG determinism test suite
//!
//! Covers all critical scenarios for Phase 1 determinism requirements.

use adapteros_core::{
    clear_seed_registry, derive_adapter_seed, derive_seed_typed, B3Hash, SeedLabel,
};
use adapteros_lora_worker::deterministic_rng::{set_global_nonce, DeterministicRng, RngCheckpoint};

#[test]
fn test_rng_state_serialization() {
    let seed = [42u8; 32];
    let mut rng = DeterministicRng::new(&seed, "serialization_test").unwrap();

    // Generate values
    let mut original_values = Vec::new();
    for _ in 0..100 {
        original_values.push(rng.next_u64());
    }

    // Serialize
    let state = rng.serialize_state();
    let serialized = serde_json::to_string(&state).unwrap();

    // Deserialize
    let deserialized_state: adapteros_lora_worker::deterministic_rng::RngState =
        serde_json::from_str(&serialized).unwrap();

    // Restore
    let mut restored_rng = DeterministicRng::restore_state(&deserialized_state, &seed).unwrap();

    // Continue generating and verify continuation
    let next_original = rng.next_u64();
    let next_restored = restored_rng.next_u64();

    assert_eq!(
        next_original, next_restored,
        "Serialization round-trip must preserve RNG state"
    );
}

#[test]
fn test_rng_checkpoint_restore() {
    set_global_nonce(1000);

    let seed = [43u8; 32];
    let mut rng = DeterministicRng::new(&seed, "checkpoint_test").unwrap();

    // Phase 1: Router
    for _ in 0..50 {
        rng.next_u64();
    }
    let checkpoint_router = rng.checkpoint("router", 1);

    // Phase 2: Dropout
    for _ in 0..30 {
        rng.next_u64();
    }
    let checkpoint_dropout = rng.checkpoint("dropout", 2);

    // Phase 3: Sampling
    for _ in 0..20 {
        rng.next_u64();
    }
    let checkpoint_sampling = rng.checkpoint("sampling", 3);

    // Verify checkpoint metadata
    assert_eq!(checkpoint_router.phase, "router");
    assert_eq!(checkpoint_router.state.step_count, 50);
    assert_eq!(checkpoint_dropout.phase, "dropout");
    assert_eq!(checkpoint_dropout.state.step_count, 80);
    assert_eq!(checkpoint_sampling.phase, "sampling");
    assert_eq!(checkpoint_sampling.state.step_count, 100);

    // Restore from dropout checkpoint
    let mut restored = DeterministicRng::restore_state(&checkpoint_dropout.state, &seed).unwrap();

    // Continue and verify
    for _ in 0..20 {
        restored.next_u64();
    }
    assert_eq!(restored.step_count(), 100);
}

#[test]
fn test_replay_rng_identity() {
    set_global_nonce(2000);

    let seed = [44u8; 32];

    // Original run
    let mut rng_original = DeterministicRng::new(&seed, "replay_test").unwrap();
    let mut original_outputs = Vec::new();
    for _ in 0..1000 {
        original_outputs.push(rng_original.next_u64());
    }
    let final_state = rng_original.serialize_state();

    // Replay run from initial state
    let mut rng_replay = DeterministicRng::new(&seed, "replay_test").unwrap();
    let mut replay_outputs = Vec::new();
    for _ in 0..1000 {
        replay_outputs.push(rng_replay.next_u64());
    }
    let replay_state = rng_replay.serialize_state();

    // Verify outputs are identical
    assert_eq!(
        original_outputs, replay_outputs,
        "Replay must produce identical outputs"
    );
    assert_eq!(final_state.step_count, replay_state.step_count);
}

#[test]
fn test_seed_label_enum_exhaustive() {
    let global_seed = B3Hash::hash(b"seed_label_test");
    let manifest_hash = B3Hash::hash(b"manifest_v1");
    let worker_id = 1;
    let nonce = 42;

    // Test all SeedLabel variants
    let seed_router = derive_seed_typed(
        &global_seed,
        SeedLabel::Router,
        &manifest_hash,
        worker_id,
        nonce,
    );
    let seed_dropout = derive_seed_typed(
        &global_seed,
        SeedLabel::Dropout,
        &manifest_hash,
        worker_id,
        nonce,
    );
    let seed_sampling = derive_seed_typed(
        &global_seed,
        SeedLabel::Sampling,
        &manifest_hash,
        worker_id,
        nonce,
    );
    let seed_adapter_0 = derive_seed_typed(
        &global_seed,
        SeedLabel::Adapter(0),
        &manifest_hash,
        worker_id,
        nonce,
    );
    let seed_adapter_1 = derive_seed_typed(
        &global_seed,
        SeedLabel::Adapter(1),
        &manifest_hash,
        worker_id,
        nonce,
    );

    // All seeds should be different
    let seeds = vec![
        seed_router,
        seed_dropout,
        seed_sampling,
        seed_adapter_0,
        seed_adapter_1,
    ];
    for i in 0..seeds.len() {
        for j in i + 1..seeds.len() {
            assert_ne!(seeds[i], seeds[j], "Seeds {} and {} must differ", i, j);
        }
    }

    // Verify as_str() for each variant
    assert_eq!(SeedLabel::Router.as_str(), "router");
    assert_eq!(SeedLabel::Dropout.as_str(), "dropout");
    assert_eq!(SeedLabel::Sampling.as_str(), "sampling");
    assert_eq!(SeedLabel::Adapter(5).as_str(), "adapter_5");
}

#[test]
fn test_hkdf_output_validation() {
    let global_seed = B3Hash::hash(b"hkdf_validation_test");
    let manifest_hash = B3Hash::hash(b"manifest_v1");
    let worker_id = 1;
    let nonce = 42;

    // Derive seed (internally validates 32 bytes)
    let seed = derive_seed_typed(
        &global_seed,
        SeedLabel::Router,
        &manifest_hash,
        worker_id,
        nonce,
    );

    // Verify it's exactly 32 bytes
    assert_eq!(seed.len(), 32, "HKDF output must be exactly 32 bytes");

    // Verify it's not all zeros
    assert_ne!(seed, [0u8; 32], "Seed should not be all zeros");
}

#[test]
fn test_seed_reuse_detection() {
    clear_seed_registry();

    let global_seed = B3Hash::hash(b"reuse_detection_test");
    let nonce = 100;

    // First use should succeed
    let seed1 = derive_adapter_seed(&global_seed, 0, 0, nonce);
    assert!(seed1.is_ok(), "First seed derivation should succeed");

    // Reuse with same adapter_id, layer, and nonce should fail
    let seed2 = derive_adapter_seed(&global_seed, 0, 0, nonce);
    assert!(seed2.is_err(), "Seed reuse should be detected");

    // Different nonce should succeed
    let seed3 = derive_adapter_seed(&global_seed, 0, 0, nonce + 1);
    assert!(seed3.is_ok(), "Different nonce should allow derivation");

    // Different layer should succeed
    let seed4 = derive_adapter_seed(&global_seed, 0, 1, nonce);
    assert!(seed4.is_ok(), "Different layer should allow derivation");

    clear_seed_registry();
}

#[test]
fn test_rng_deterministic_across_restarts() {
    // Simulate process restart by creating new RNG instances with same seed
    let seed = [45u8; 32];

    let mut values_run1 = Vec::new();
    {
        let mut rng = DeterministicRng::new(&seed, "restart_test").unwrap();
        for _ in 0..500 {
            values_run1.push(rng.next_u64());
        }
    } // RNG dropped, simulating process end

    let mut values_run2 = Vec::new();
    {
        let mut rng = DeterministicRng::new(&seed, "restart_test").unwrap();
        for _ in 0..500 {
            values_run2.push(rng.next_u64());
        }
    }

    assert_eq!(
        values_run1, values_run2,
        "RNG must be deterministic across process restarts"
    );
}

#[test]
fn test_gen_range_uniform_distribution() {
    let seed = [46u8; 32];
    let mut rng = DeterministicRng::new(&seed, "uniform_test").unwrap();

    // Generate many values in range [0, 10)
    let mut counts = vec![0usize; 10];
    let num_samples = 10000;

    for _ in 0..num_samples {
        let val = rng.gen_range_u32(10);
        assert!(val < 10, "Value out of range");
        counts[val as usize] += 1;
    }

    // Verify each value appears at least once (probabilistically certain with 10k samples)
    for (i, &count) in counts.iter().enumerate() {
        assert!(count > 0, "Value {} never appeared", i);
    }

    // Verify rough uniformity (each should be ~1000 ± 30%)
    let expected = num_samples / 10;
    let tolerance = expected / 3; // 33% tolerance
    for (i, &count) in counts.iter().enumerate() {
        let diff = if count > expected {
            count - expected
        } else {
            expected - count
        };
        assert!(
            diff < tolerance,
            "Value {} distribution off: {} vs {} expected",
            i,
            count,
            expected
        );
    }
}
