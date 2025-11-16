<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! Cross-worker RNG determinism verification test
//!
//! Verifies that two workers with identical seeds produce bit-identical RNG outputs.

use adapteros_core::{derive_seed_typed, B3Hash, SeedLabel};
use adapteros_lora_worker::deterministic_rng::{
    get_global_nonce, set_global_nonce, DeterministicRng,
};

#[test]
fn test_cross_worker_rng_identity() {
    // Reset global nonce
    set_global_nonce(0);

    // Common parameters
    let global_seed = B3Hash::hash(b"test_cross_worker");
    let manifest_hash = B3Hash::hash(b"manifest_v1");
    let worker_id_a = 1;
    let worker_id_b = 2;
    let nonce = 42;

    // Derive seeds for two workers with same nonce
    let seed_a = derive_seed_typed(
        &global_seed,
        SeedLabel::Router,
        &manifest_hash,
        worker_id_a,
        nonce,
    );
    let seed_b = derive_seed_typed(
        &global_seed,
        SeedLabel::Router,
        &manifest_hash,
        worker_id_a,
        nonce,
    );

    // Seeds for same worker should be identical
    assert_eq!(seed_a, seed_b, "Same worker should produce identical seeds");

    // Create RNGs
    let mut rng_a = DeterministicRng::new(&seed_a, "router:worker_1").expect("RNG creation failed");
    let mut rng_b = DeterministicRng::new(&seed_b, "router:worker_1").expect("RNG creation failed");

    // Generate 10,000 values and verify they're identical
    for i in 0..10_000 {
        let val_a = rng_a.next_u64();
        let val_b = rng_b.next_u64();
        assert_eq!(val_a, val_b, "Divergence at draw {}", i);
    }

    // Verify step counts match
    assert_eq!(rng_a.step_count(), rng_b.step_count());
}

#[test]
fn test_different_workers_produce_different_sequences() {
    set_global_nonce(0);

    let global_seed = B3Hash::hash(b"test_different_workers");
    let manifest_hash = B3Hash::hash(b"manifest_v1");
    let nonce = 42;

    // Different worker IDs should produce different seeds
    let seed_worker_1 =
        derive_seed_typed(&global_seed, SeedLabel::Router, &manifest_hash, 1, nonce);
    let seed_worker_2 =
        derive_seed_typed(&global_seed, SeedLabel::Router, &manifest_hash, 2, nonce);

    assert_ne!(
        seed_worker_1, seed_worker_2,
        "Different workers should have different seeds"
    );

    let mut rng_1 = DeterministicRng::new(&seed_worker_1, "router:worker_1").unwrap();
    let mut rng_2 = DeterministicRng::new(&seed_worker_2, "router:worker_2").unwrap();

    // First values should differ
    let val_1 = rng_1.next_u64();
    let val_2 = rng_2.next_u64();
    assert_ne!(
        val_1, val_2,
        "Different workers should produce different sequences"
    );
}

#[test]
fn test_rng_state_serialization_round_trip() {
    set_global_nonce(100);

    let seed = [42u8; 32];
    let mut rng = DeterministicRng::new(&seed, "test_serialization").unwrap();

    // Generate some values
    for _ in 0..50 {
        rng.next_u64();
    }

    // Serialize state
    let state = rng.serialize_state();
    assert_eq!(state.step_count, 50);
    assert_eq!(state.label, "test_serialization");
    assert_eq!(state.nonce, 100);

    // Continue generating
    let next_val_original = rng.next_u64();

    // Restore from state
    let mut rng_restored = DeterministicRng::restore_state(&state, &seed).unwrap();
    let next_val_restored = rng_restored.next_u64();

    assert_eq!(
        next_val_original, next_val_restored,
        "Restored RNG should continue from same state"
    );
}

#[test]
fn test_rng_checkpoint_capture() {
    set_global_nonce(200);

    let seed = [42u8; 32];
    let mut rng = DeterministicRng::new(&seed, "test_checkpoint").unwrap();

    // Generate values
    for _ in 0..25 {
        rng.next_u64();
    }

    // Create checkpoint
    let checkpoint = rng.checkpoint("router_phase", 1000);
    assert_eq!(checkpoint.phase, "router_phase");
    assert_eq!(checkpoint.timestamp_ticks, 1000);
    assert_eq!(checkpoint.state.step_count, 25);
    assert_eq!(checkpoint.state.nonce, 200);
}

#[test]
fn test_global_nonce_monotonicity() {
    let initial = get_global_nonce();
    set_global_nonce(initial + 1000);

    let nonce_1 = get_global_nonce();
    set_global_nonce(nonce_1 + 1);
    let nonce_2 = get_global_nonce();

    assert_eq!(nonce_2, nonce_1 + 1, "Nonce should increment");
}
