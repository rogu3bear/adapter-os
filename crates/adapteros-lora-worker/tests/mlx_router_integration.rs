//! MLX Router Integration Tests
//!
//! These tests verify the integration between the router decision system
//! and the worker infrastructure, including decision-to-ring conversion
//! and batch processing scenarios.
//!
//! Note: These tests use the router_bridge module which provides the
//! interface between router decisions and kernel ring buffers.

use adapteros_lora_router::{Decision, DecisionCandidate};
use adapteros_lora_worker::router_bridge::{
    batch_decision_to_router_ring, decision_to_router_ring, decision_to_router_ring_unchecked,
};
use smallvec::SmallVec;

/// Helper to create a test Decision with specified parameters
fn make_decision(indices: &[u16], gates: &[i16], entropy: f32) -> Decision {
    Decision {
        indices: SmallVec::from_slice(indices),
        gates_q15: SmallVec::from_slice(gates),
        entropy,
        candidates: vec![],
    }
}

/// Helper to create a Decision with candidate information
fn make_decision_with_candidates(
    indices: &[u16],
    gates: &[i16],
    entropy: f32,
    candidates: Vec<DecisionCandidate>,
) -> Decision {
    Decision {
        indices: SmallVec::from_slice(indices),
        gates_q15: SmallVec::from_slice(gates),
        entropy,
        candidates,
    }
}

#[test]
fn test_router_to_kernel_ring_basic_conversion() {
    // Test basic conversion from router Decision to RouterRing
    let decision = make_decision(&[0, 1, 2], &[16383, 8191, 4095], 0.5);
    let ring = decision_to_router_ring(&decision, 100);

    assert_eq!(ring.k, 3, "K should match number of selected adapters");
    assert_eq!(ring.active_indices(), &[0, 1, 2]);
    assert_eq!(ring.active_gates(), &[16383, 8191, 4095]);
}

#[test]
fn test_router_decision_order_preservation() {
    // Critical: router order must be preserved for deterministic execution
    let decision = make_decision(&[7, 3, 1, 5], &[-1000, 500, 2000, -500], 0.3);
    let ring = decision_to_router_ring(&decision, 100);

    // Verify exact order preservation (not sorted)
    assert_eq!(
        ring.active_indices(),
        &[7, 3, 1, 5],
        "Adapter order must be preserved"
    );
    assert_eq!(
        ring.active_gates(),
        &[-1000, 500, 2000, -500],
        "Gate order must match adapter order"
    );
}

#[test]
fn test_router_batch_processing() {
    // Simulate batch inference with multiple decisions
    let decisions = vec![
        make_decision(&[0, 1], &[1000, 2000], 0.5),
        make_decision(&[2, 3, 4], &[3000, 4000, 5000], 0.6),
        make_decision(&[5], &[6000], 0.3),
        make_decision(&[0, 2, 4, 6], &[100, 200, 300, 400], 0.7),
    ];

    let rings = batch_decision_to_router_ring(&decisions, 100);

    assert_eq!(rings.len(), 4, "Should have one ring per decision");
    assert_eq!(rings[0].k, 2);
    assert_eq!(rings[1].k, 3);
    assert_eq!(rings[2].k, 1);
    assert_eq!(rings[3].k, 4);
}

#[test]
fn test_router_empty_decision() {
    // Edge case: no adapters selected (e.g., confidence too low)
    let decision = make_decision(&[], &[], 0.0);
    let ring = decision_to_router_ring(&decision, 100);

    assert_eq!(ring.k, 0, "Empty decision should have K=0");
    assert!(ring.active_indices().is_empty());
    assert!(ring.active_gates().is_empty());
}

#[test]
fn test_router_max_k_adapters() {
    // Test with maximum K=8 adapters
    let indices = [0, 1, 2, 3, 4, 5, 6, 7];
    let gates = [1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000];
    let decision = make_decision(&indices, &gates, 0.9);
    let ring = decision_to_router_ring(&decision, 100);

    assert_eq!(ring.k, 8, "Should support K=8");
    assert_eq!(ring.active_indices(), &indices);
    assert_eq!(ring.active_gates(), &gates);
}

#[test]
fn test_router_q15_signed_range() {
    // Test full Q15 signed range: -32767 to +32767
    let decision = make_decision(&[0, 1, 2, 3], &[-32767, -1, 0, 32767], 0.5);
    let ring = decision_to_router_ring(&decision, 100);

    assert_eq!(ring.active_gates(), &[-32767, -1, 0, 32767]);
}

#[test]
fn test_router_unchecked_conversion() {
    // Test unchecked conversion (skips bounds checking)
    let decision = make_decision(&[0, 1, 2], &[100, 200, 300], 0.4);
    let ring = decision_to_router_ring_unchecked(&decision);

    assert_eq!(ring.k, 3);
    assert_eq!(ring.active_indices(), &[0, 1, 2]);
}

#[test]
fn test_router_position_tracking() {
    // RouterRing position should be initialized to 0
    let decision = make_decision(&[0, 1], &[1000, 2000], 0.5);
    let mut ring = decision_to_router_ring(&decision, 100);

    assert_eq!(ring.position, 0, "Initial position should be 0");

    // Position can be updated for token-level tracking
    ring.position = 42;
    assert_eq!(ring.position, 42);
}

#[test]
fn test_router_ring_zero_fill() {
    // Verify unused entries are zero-filled
    let decision = make_decision(&[0, 1, 2], &[1000, 2000, 3000], 0.5);
    let ring = decision_to_router_ring(&decision, 100);

    // Entries beyond K should be zero
    assert_eq!(ring.indices[3..], [0; 5]);
    assert_eq!(ring.gates_q15[3..], [0; 5]);
}

#[test]
fn test_router_with_candidate_metadata() {
    // Test that candidate metadata is preserved through Decision (not in ring)
    let candidates = vec![
        DecisionCandidate {
            adapter_idx: 0,
            raw_score: 0.8,
            gate_q15: 16383,
        },
        DecisionCandidate {
            adapter_idx: 1,
            raw_score: 0.6,
            gate_q15: 8191,
        },
    ];

    let decision = make_decision_with_candidates(&[0, 1], &[16383, 8191], 0.5, candidates.clone());

    // Candidates are metadata, not transferred to ring
    assert_eq!(decision.candidates.len(), 2);
    assert_eq!(decision.candidates[0].adapter_idx, 0);
    assert_eq!(decision.candidates[0].raw_score, 0.8);

    // Ring only has indices and gates
    let ring = decision_to_router_ring(&decision, 100);
    assert_eq!(ring.k, 2);
}

#[test]
fn test_router_deterministic_conversion() {
    // Same Decision should produce identical RouterRing
    let decision = make_decision(&[3, 1, 4], &[1000, 2000, 3000], 0.5);

    let ring1 = decision_to_router_ring(&decision, 100);
    let ring2 = decision_to_router_ring(&decision, 100);

    assert_eq!(ring1.k, ring2.k);
    assert_eq!(ring1.indices, ring2.indices);
    assert_eq!(ring1.gates_q15, ring2.gates_q15);
}

#[test]
fn test_router_adapter_count_boundary() {
    // Test with max_adapter_count at boundary
    let decision = make_decision(&[98, 99], &[1000, 2000], 0.5);
    let ring = decision_to_router_ring(&decision, 100);

    // Indices 98 and 99 are valid for max_adapter=100 (0-99)
    assert_eq!(ring.k, 2);
    assert_eq!(ring.active_indices(), &[98, 99]);
}

#[test]
fn test_router_batch_empty() {
    // Test batch conversion with empty input
    let rings = batch_decision_to_router_ring(&[], 100);
    assert!(rings.is_empty());
}

#[test]
fn test_router_batch_single() {
    // Test batch conversion with single decision
    let decisions = vec![make_decision(&[5], &[5000], 0.8)];
    let rings = batch_decision_to_router_ring(&decisions, 100);

    assert_eq!(rings.len(), 1);
    assert_eq!(rings[0].k, 1);
    assert_eq!(rings[0].active_indices(), &[5]);
}
