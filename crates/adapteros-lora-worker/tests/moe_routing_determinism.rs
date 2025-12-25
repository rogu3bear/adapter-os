//! MoE Routing Determinism Tests
//!
//! Verifies deterministic behavior for:
//! - ExpertHeatMap.finalize() ordering with ties
//! - Expert selection ordering stability
//!
//! These tests are designed to fail if tie-break logic changes,
//! protecting against accidental determinism regressions.
//!
//! Note: These tests require the `mlx-bridge` feature to be enabled.

#![cfg(feature = "mlx-bridge")]

use adapteros_lora_worker::moe_prefix_cache::ExpertHeatMap;

// ============================================================================
// EXPERT HEAT MAP FINALIZE() ORDERING TESTS
// ============================================================================

/// Golden test: ExpertHeatMap.finalize() with tied counts
///
/// When multiple experts have identical activation counts,
/// ordering must be deterministic by expert_id ascending.
#[test]
fn test_expert_heat_map_finalize_tie_breaking_golden() {
    let mut heat_map = ExpertHeatMap::new(2);

    // Layer 0: Experts 5, 3, 7 all have count=2, expert 1 has count=1
    // Expected order after finalize(3): [3, 5, 7] (tied by count=2, sorted by id)
    heat_map.record_activation(0, 5);
    heat_map.record_activation(0, 5);
    heat_map.record_activation(0, 3);
    heat_map.record_activation(0, 3);
    heat_map.record_activation(0, 7);
    heat_map.record_activation(0, 7);
    heat_map.record_activation(0, 1);

    // Layer 1: Expert 10 dominates (count=3), then 8 and 2 tied (count=1)
    // Expected: [10, 2, 8] (10 first by count, then 2,8 tied sorted by id)
    heat_map.record_activation(1, 10);
    heat_map.record_activation(1, 10);
    heat_map.record_activation(1, 10);
    heat_map.record_activation(1, 8);
    heat_map.record_activation(1, 2);

    heat_map.finalize(3);

    // GOLDEN ASSERTIONS - these values must not change
    assert_eq!(
        heat_map.get_hot_experts(0),
        &[3, 5, 7],
        "Layer 0: tied experts (count=2) should be ordered by ID ascending"
    );
    assert_eq!(
        heat_map.get_hot_experts(1),
        &[10, 2, 8],
        "Layer 1: expert 10 first (count=3), then tied 2,8 (count=1) by ID"
    );
}

/// Golden test: All experts tied with same count
///
/// When all experts have identical counts, order should be purely by expert_id.
#[test]
fn test_expert_heat_map_all_tied_golden() {
    let mut heat_map = ExpertHeatMap::new(1);

    // All experts have count=1, different IDs
    for expert_id in [5u8, 3, 7, 1, 9, 2] {
        heat_map.record_activation(0, expert_id);
    }

    heat_map.finalize(6);

    // GOLDEN: When all tied, should be sorted by expert_id ascending
    assert_eq!(
        heat_map.get_hot_experts(0),
        &[1, 2, 3, 5, 7, 9],
        "All tied experts should be ordered by ID ascending"
    );
}

/// Golden test: Mixed counts with partial ties
#[test]
fn test_expert_heat_map_mixed_counts_golden() {
    let mut heat_map = ExpertHeatMap::new(1);

    // Expert 10: count=5 (highest)
    // Experts 3, 7: count=3 (tied second)
    // Experts 1, 5: count=1 (tied third)
    for _ in 0..5 {
        heat_map.record_activation(0, 10);
    }
    for _ in 0..3 {
        heat_map.record_activation(0, 3);
        heat_map.record_activation(0, 7);
    }
    heat_map.record_activation(0, 1);
    heat_map.record_activation(0, 5);

    heat_map.finalize(5);

    // GOLDEN: [10, 3, 7, 1, 5]
    // - 10 first (highest count)
    // - 3, 7 next (tied, sorted by ID)
    // - 1, 5 last (tied, sorted by ID)
    assert_eq!(
        heat_map.get_hot_experts(0),
        &[10, 3, 7, 1, 5],
        "Mixed counts: highest first, then ties sorted by ID"
    );
}

/// Stability test: Multiple runs produce identical results
///
/// This test verifies that the deterministic ordering is stable
/// across 100 iterations. If HashMap iteration order affected results,
/// this test would be flaky.
#[test]
fn test_expert_heat_map_finalize_stability() {
    for run in 0..100 {
        let mut heat_map = ExpertHeatMap::new(2);

        // Create ties that would be non-deterministic without proper sort
        for expert_id in [5u8, 3, 7, 1] {
            heat_map.record_activation(0, expert_id);
            heat_map.record_activation(0, expert_id);
        }

        heat_map.finalize(4);

        // Must always produce [1, 3, 5, 7] when all have equal counts
        assert_eq!(
            heat_map.get_hot_experts(0),
            &[1, 3, 5, 7],
            "Run {}: Tied experts must be ordered deterministically by ID",
            run
        );
    }
}

/// Test top_k truncation with ties
///
/// When top_k is smaller than the number of tied experts,
/// we should still get deterministic selection of which ones.
#[test]
fn test_expert_heat_map_topk_truncation_with_ties() {
    let mut heat_map = ExpertHeatMap::new(1);

    // 6 experts all with count=2
    for expert_id in [8u8, 2, 6, 4, 10, 1] {
        heat_map.record_activation(0, expert_id);
        heat_map.record_activation(0, expert_id);
    }

    heat_map.finalize(3); // Only take top 3

    // GOLDEN: Should take lowest 3 IDs when all tied
    assert_eq!(
        heat_map.get_hot_experts(0),
        &[1, 2, 4],
        "Top-3 of 6 tied experts should be lowest 3 IDs"
    );
}

/// Test empty layer handling
#[test]
fn test_expert_heat_map_empty_layer() {
    let mut heat_map = ExpertHeatMap::new(2);

    // Only populate layer 1, leave layer 0 empty
    heat_map.record_activation(1, 5);

    heat_map.finalize(3);

    assert_eq!(
        heat_map.get_hot_experts(0),
        &[] as &[u8],
        "Empty layer should have no hot experts"
    );
    assert_eq!(
        heat_map.get_hot_experts(1),
        &[5],
        "Layer 1 should have expert 5"
    );
}

/// Test sample_count tracking
#[test]
fn test_expert_heat_map_sample_count() {
    let mut heat_map = ExpertHeatMap::new(2);

    heat_map.record_token_routing(&[(0, 5), (1, 10)]);
    heat_map.record_token_routing(&[(0, 3), (1, 10)]);
    heat_map.record_token_routing(&[(0, 5), (1, 8)]);

    assert_eq!(heat_map.sample_count, 3, "Should track 3 token routings");

    heat_map.finalize(2);

    // After finalize, sample_count should be preserved
    assert_eq!(heat_map.sample_count, 3);
}

// ============================================================================
// ROUTING STABILITY PROPERTY TESTS
// ============================================================================

/// Test that routing stability is computed correctly
#[test]
fn test_expert_heat_map_routing_stability() {
    // High concentration (single expert dominates)
    // Note: must use record_token_routing to increment sample_count
    let mut heat_map_high = ExpertHeatMap::new(1);
    for _ in 0..10 {
        heat_map_high.record_token_routing(&[(0, 5)]);
    }
    heat_map_high.record_token_routing(&[(0, 3)]);
    heat_map_high.finalize(2);

    // With 10/11 = 0.909 concentration, stability should be high
    assert!(
        heat_map_high.routing_stability > 0.8,
        "High concentration should yield high stability, got {}",
        heat_map_high.routing_stability
    );

    // Low concentration (uniform distribution)
    let mut heat_map_low = ExpertHeatMap::new(1);
    for expert_id in 0..8u8 {
        heat_map_low.record_token_routing(&[(0, expert_id)]);
    }
    heat_map_low.finalize(4);

    // With 1/8 = 0.125 concentration, stability should be low
    assert!(
        heat_map_low.routing_stability < 0.3,
        "Uniform distribution should yield low stability, got {}",
        heat_map_low.routing_stability
    );
}

/// Test is_stable() threshold
#[test]
fn test_expert_heat_map_is_stable_threshold() {
    let mut heat_map = ExpertHeatMap::new(1);

    // Not enough samples
    heat_map.record_activation(0, 5);
    heat_map.finalize(1);
    assert!(
        !heat_map.is_stable(0.5),
        "Should not be stable with < 10 samples"
    );

    // Add more samples
    let mut heat_map2 = ExpertHeatMap::new(1);
    for _ in 0..15 {
        heat_map2.record_token_routing(&[(0, 5)]);
    }
    heat_map2.finalize(1);
    assert!(
        heat_map2.is_stable(0.5),
        "Should be stable with >= 10 samples and high concentration"
    );
}
