//! Tests for replay metadata storage and retrieval
//!
//! Verifies that replay metadata is correctly stored, retrieved, and
//! maintains all required fields for deterministic replay.

use adapteros_core::B3Hash;
use adapteros_replay::{ReplayState, ReplayStats};

#[test]
fn test_replay_state_creation() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let total_steps = 10;

    let state = ReplayState::new(total_steps, global_seed);

    assert_eq!(state.current_step, 0);
    assert_eq!(state.total_steps, total_steps);
    assert_eq!(state.current_tick, 0);
    assert_eq!(state.global_seed, global_seed);
    assert_eq!(state.verified_ops, 0);
    assert!(!state.is_complete);
    assert!(state.expected_hashes.is_empty());
}

#[test]
fn test_replay_state_advance() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let mut state = ReplayState::new(3, global_seed);

    assert_eq!(state.current_step, 0);
    assert!(!state.is_complete);

    state.advance_step();
    assert_eq!(state.current_step, 1);
    assert!(!state.is_complete);

    state.advance_step();
    assert_eq!(state.current_step, 2);
    assert!(!state.is_complete);

    state.advance_step();
    assert_eq!(state.current_step, 3);
    assert!(state.is_complete);

    // Further advances should maintain completion
    state.advance_step();
    assert_eq!(state.current_step, 4);
    assert!(state.is_complete);
}

#[test]
fn test_replay_state_progress_percentage() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let mut state = ReplayState::new(100, global_seed);

    assert_eq!(state.progress_percent(), 0.0);

    for i in 1..=25 {
        state.advance_step();
        assert_eq!(state.current_step, i);
    }
    assert_eq!(state.progress_percent(), 25.0);

    for i in 26..=50 {
        state.advance_step();
        assert_eq!(state.current_step, i);
    }
    assert_eq!(state.progress_percent(), 50.0);

    for i in 51..=100 {
        state.advance_step();
        assert_eq!(state.current_step, i);
    }
    assert_eq!(state.progress_percent(), 100.0);
}

#[test]
fn test_replay_state_zero_steps() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let state = ReplayState::new(0, global_seed);

    assert_eq!(state.progress_percent(), 0.0);
    assert_eq!(state.total_steps, 0);
}

#[test]
fn test_replay_stats_default() {
    let stats = ReplayStats::default();

    assert_eq!(stats.total_events, 0);
    assert_eq!(stats.current_step, 0);
    assert_eq!(stats.verified_ops, 0);
    assert_eq!(stats.hash_mismatches, 0);
    assert!(!stats.is_complete);
    assert_eq!(stats.progress_percent, 0.0);
}

#[test]
fn test_replay_state_expected_hashes() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let mut state = ReplayState::new(5, global_seed);

    // Add some expected hashes
    let op_id_1 = "op_1".to_string();
    let hash_1 = B3Hash::hash(b"operation_1_output");
    state.expected_hashes.insert(op_id_1.clone(), hash_1);

    let op_id_2 = "op_2".to_string();
    let hash_2 = B3Hash::hash(b"operation_2_output");
    state.expected_hashes.insert(op_id_2.clone(), hash_2);

    assert_eq!(state.expected_hashes.len(), 2);
    assert_eq!(state.expected_hashes.get(&op_id_1), Some(&hash_1));
    assert_eq!(state.expected_hashes.get(&op_id_2), Some(&hash_2));
}

#[test]
fn test_replay_state_verified_ops_tracking() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let mut state = ReplayState::new(10, global_seed);

    assert_eq!(state.verified_ops, 0);

    state.verified_ops += 1;
    assert_eq!(state.verified_ops, 1);

    state.verified_ops += 5;
    assert_eq!(state.verified_ops, 6);
}

#[test]
fn test_replay_state_tick_tracking() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let mut state = ReplayState::new(10, global_seed);

    assert_eq!(state.current_tick, 0);

    state.current_tick = 42;
    assert_eq!(state.current_tick, 42);

    state.current_tick += 1;
    assert_eq!(state.current_tick, 43);
}

#[test]
fn test_replay_state_global_seed_immutability() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let state = ReplayState::new(10, global_seed);

    // Global seed should match what we passed in
    assert_eq!(state.global_seed, global_seed);

    // Global seed is a copy type, so it can't be mutated through the struct
    let seed_copy = state.global_seed;
    assert_eq!(seed_copy, global_seed);
}

#[test]
fn test_replay_state_serialization() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let mut state = ReplayState::new(10, global_seed);
    state.current_step = 5;
    state.current_tick = 123;
    state.verified_ops = 5;

    // Test serialization
    let serialized = serde_json::to_string(&state).expect("Failed to serialize");
    assert!(serialized.contains("current_step"));
    assert!(serialized.contains("total_steps"));
    assert!(serialized.contains("current_tick"));
    assert!(serialized.contains("global_seed"));

    // Test deserialization
    let deserialized: ReplayState =
        serde_json::from_str(&serialized).expect("Failed to deserialize");
    assert_eq!(deserialized.current_step, state.current_step);
    assert_eq!(deserialized.total_steps, state.total_steps);
    assert_eq!(deserialized.current_tick, state.current_tick);
    assert_eq!(deserialized.global_seed, state.global_seed);
    assert_eq!(deserialized.verified_ops, state.verified_ops);
}

#[test]
fn test_replay_state_completion_boundary() {
    let global_seed = B3Hash::hash(b"test_global_seed");
    let mut state = ReplayState::new(1, global_seed);

    assert!(!state.is_complete);

    state.advance_step();
    assert!(state.is_complete);
    assert_eq!(state.current_step, 1);
    assert_eq!(state.total_steps, 1);
}
