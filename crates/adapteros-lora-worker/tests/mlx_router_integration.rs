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
    batch_decision_to_router_ring, decision_to_router_ring,
};
use smallvec::SmallVec;

/// Helper to create a test Decision with specified parameters
fn make_decision(indices: &[u16], gates: &[i16], entropy: f32) -> Decision {
    Decision {
        indices: SmallVec::from_slice(indices),
        gates_q15: SmallVec::from_slice(gates),
        entropy,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
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
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    }
}

#[test]
fn test_router_to_kernel_ring_basic_conversion() {
    // Test basic conversion from router Decision to RouterRing
    let decision = make_decision(&[0, 1, 2], &[16383, 8191, 4095], 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.k, 3, "K should match number of selected adapters");
    assert_eq!(ring.active_indices(), &[0, 1, 2]);
    assert_eq!(ring.active_gates(), &[16383, 8191, 4095]);
}

#[test]
fn test_router_decision_order_preservation() {
    // Critical: router order must be preserved for deterministic execution
    let decision = make_decision(&[7, 3, 1, 5], &[-1000, 500, 2000, -500], 0.3);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

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

    let rings = batch_decision_to_router_ring(&decisions, 100).unwrap();

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
    let ring = decision_to_router_ring(&decision, 100).unwrap();

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
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.k, 8, "Should support K=8");
    assert_eq!(ring.active_indices(), &indices);
    assert_eq!(ring.active_gates(), &gates);
}

#[test]
fn test_router_q15_signed_range() {
    // Test full Q15 signed range: -32767 to +32767
    let decision = make_decision(&[0, 1, 2, 3], &[-32767, -1, 0, 32767], 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.active_gates(), &[-32767, -1, 0, 32767]);
}

#[test]
fn test_router_position_tracking() {
    // RouterRing position should be initialized to 0
    let decision = make_decision(&[0, 1], &[1000, 2000], 0.5);
    let mut ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.position, 0, "Initial position should be 0");

    // Position can be updated for token-level tracking
    ring.position = 42;
    assert_eq!(ring.position, 42);
}

#[test]
fn test_router_ring_zero_fill() {
    // Verify unused entries are zero-filled
    let decision = make_decision(&[0, 1, 2], &[1000, 2000, 3000], 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

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
    let ring = decision_to_router_ring(&decision, 100).unwrap();
    assert_eq!(ring.k, 2);
}

#[test]
fn test_router_deterministic_conversion() {
    // Same Decision should produce identical RouterRing
    let decision = make_decision(&[3, 1, 4], &[1000, 2000, 3000], 0.5);

    let ring1 = decision_to_router_ring(&decision, 100).unwrap();
    let ring2 = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring1.k, ring2.k);
    assert_eq!(ring1.indices, ring2.indices);
    assert_eq!(ring1.gates_q15, ring2.gates_q15);
}

#[test]
fn test_router_adapter_count_boundary() {
    // Test with max_adapter_count at boundary
    let decision = make_decision(&[98, 99], &[1000, 2000], 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    // Indices 98 and 99 are valid for max_adapter=100 (0-99)
    assert_eq!(ring.k, 2);
    assert_eq!(ring.active_indices(), &[98, 99]);
}

#[test]
fn test_router_batch_empty() {
    // Test batch conversion with empty input
    let rings = batch_decision_to_router_ring(&[], 100).unwrap();
    assert!(rings.is_empty());
}

#[test]
fn test_router_batch_single() {
    // Test batch conversion with single decision
    let decisions = vec![make_decision(&[5], &[5000], 0.8)];
    let rings = batch_decision_to_router_ring(&decisions, 100).unwrap();

    assert_eq!(rings.len(), 1);
    assert_eq!(rings[0].k, 1);
    assert_eq!(rings[0].active_indices(), &[5]);
}

// ========================================================================
// Q15 Gate Weighting Validation Integration Tests
// ========================================================================
// Q15 format: 16-bit signed integer representing fixed-point [-1.0, 1.0)
// Max positive: 32767 = ~0.99997
// Encoding: (float * 32767.0).round() as i16
// Decoding: i16 as f32 / 32767.0

/// Helper to encode f32 to Q15 (signed i16)
fn encode_q15(value: f32) -> i16 {
    (value * 32767.0).round() as i16
}

/// Helper to decode Q15 (signed i16) to f32
fn decode_q15(q15: i16) -> f32 {
    q15 as f32 / 32767.0
}

#[test]
fn test_q15_single_adapter_full_weight() {
    // Single adapter with max gate should get ~100% weight
    let decision = make_decision(&[0], &[32767], 0.0);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.k, 1);
    let weight = decode_q15(ring.gates_q15[0]);
    assert!(
        weight > 0.999,
        "Single adapter with max gate should have weight > 0.999, got {}",
        weight
    );
}

#[test]
fn test_q15_multi_adapter_proportional_blend() {
    // Three adapters with gates 0.5, 0.3, 0.2
    let gates = [
        encode_q15(0.5), // 16384
        encode_q15(0.3), // 9830
        encode_q15(0.2), // 6553
    ];

    let decision = make_decision(&[0, 1, 2], &gates, 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.k, 3);

    // Verify proportions
    let total: f32 = ring.active_gates().iter().map(|&g| g as f32).sum();
    let proportions: Vec<f32> = ring
        .active_gates()
        .iter()
        .map(|&g| g as f32 / total)
        .collect();

    assert!(
        (proportions[0] - 0.5).abs() < 0.01,
        "First adapter proportion should be ~0.5, got {}",
        proportions[0]
    );
    assert!(
        (proportions[1] - 0.3).abs() < 0.01,
        "Second adapter proportion should be ~0.3, got {}",
        proportions[1]
    );
    assert!(
        (proportions[2] - 0.2).abs() < 0.01,
        "Third adapter proportion should be ~0.2, got {}",
        proportions[2]
    );
}

#[test]
fn test_q15_zero_gate_identification() {
    // Adapter with gate=0 should be identifiable for skipping
    let decision = make_decision(&[0, 1, 2], &[32767, 0, 16384], 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    // Only indices 0 and 2 should be active (gates > 0)
    let active_count = ring.active_gates().iter().filter(|&&g| g > 0).count();
    assert_eq!(
        active_count, 2,
        "Should have 2 active adapters (gates > 0), got {}",
        active_count
    );

    // Verify gate values
    assert_eq!(ring.gates_q15[0], 32767, "First gate should be max");
    assert_eq!(ring.gates_q15[1], 0, "Second gate should be zero");
    assert_eq!(ring.gates_q15[2], 16384, "Third gate should be ~0.5");
}

#[test]
fn test_q15_negative_gate_handling() {
    // Negative gates should be preserved but typically skipped in processing
    // (per backend.rs: if gate_q15 <= 0 { continue; })
    let decision = make_decision(&[0, 1], &[32767, -16384], 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.k, 2);

    // Negative gate at index 1 should be preserved
    assert_eq!(
        ring.gates_q15[0], 32767,
        "First gate should be max positive"
    );
    assert_eq!(ring.gates_q15[1], -16384, "Second gate should be negative");

    // Count positive gates (what would be used in inference)
    let positive_count = ring.active_gates().iter().filter(|&&g| g > 0).count();
    assert_eq!(positive_count, 1, "Only 1 positive gate should be active");
}

#[test]
fn test_q15_encode_decode_precision_integration() {
    // Test encoding precision in integration context
    let values: [f32; 5] = [0.0, 0.5, 1.0, -1.0, 0.123456];
    for v in values {
        let clamped = v.clamp(-1.0, 1.0);
        let encoded = encode_q15(clamped);
        let decoded = decode_q15(encoded);
        assert!(
            (clamped - decoded).abs() < 1e-4,
            "Precision loss for {}: encoded={}, decoded={}",
            v,
            encoded,
            decoded
        );
    }
}

#[test]
fn test_q15_boundary_values_integration() {
    // Max positive (0.99997 * 32767.0 rounds to 32766 due to floating point precision)
    assert_eq!(encode_q15(0.99997), 32766);
    // Max negative
    assert_eq!(encode_q15(-1.0), -32767);
    // Zero
    assert_eq!(encode_q15(0.0), 0);

    // Test in ring context
    let decision = make_decision(&[0, 1, 2], &[32767, -32767, 0], 0.0);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.gates_q15[0], 32767, "Max positive in ring");
    assert_eq!(ring.gates_q15[1], -32767, "Max negative in ring");
    assert_eq!(ring.gates_q15[2], 0, "Zero in ring");
}

#[test]
fn test_q15_gate_normalization_integration() {
    // Gates should sum to ~1.0 after dequantization
    let gates_q15: Vec<i16> = vec![16384, 8192, 8191]; // ~0.5, ~0.25, ~0.25
    let decision = make_decision(&[0, 1, 2], &gates_q15, 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    let sum: f32 = ring.active_gates().iter().map(|&g| decode_q15(g)).sum();
    assert!(
        (sum - 1.0).abs() < 0.01,
        "Gate sum should be ~1.0, got {}",
        sum
    );
}

#[test]
fn test_q15_very_small_gates_integration() {
    // Very small but non-zero gates
    let decision = make_decision(&[0, 1], &[1, -1], 0.0);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    let small_positive = decode_q15(ring.gates_q15[0]);
    let small_negative = decode_q15(ring.gates_q15[1]);

    assert!(small_positive > 0.0, "Small positive should be > 0");
    assert!(
        small_positive < 0.0001,
        "Small positive should be < 0.0001, got {}",
        small_positive
    );

    assert!(small_negative < 0.0, "Small negative should be < 0");
    assert!(
        small_negative > -0.0001,
        "Small negative should be > -0.0001, got {}",
        small_negative
    );
}

#[test]
fn test_q15_alternating_sign_gates_integration() {
    // Mix of positive and negative (only positive used)
    let gates: [i16; 5] = [16384, -8192, 8192, -4096, 4096];
    let decision = make_decision(&[0, 1, 2, 3, 4], &gates, 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    let positive_sum: f32 = ring
        .active_gates()
        .iter()
        .filter(|&&g| g > 0)
        .map(|&g| decode_q15(g))
        .sum();

    // Sum of positive gates: 0.5 + 0.25 + 0.125 = 0.875
    assert!(
        (positive_sum - 0.875).abs() < 0.01,
        "Positive gate sum should be ~0.875, got {}",
        positive_sum
    );
}

#[test]
fn test_q15_equal_weight_distribution() {
    // K adapters with equal weights should each get 1/K weight
    let k = 4;
    let equal_gate = encode_q15(1.0 / k as f32); // Each gets 0.25
    let gates: Vec<i16> = vec![equal_gate; k];
    let indices: Vec<u16> = (0..k as u16).collect();

    let decision = make_decision(&indices, &gates, 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    // Verify all gates are approximately equal
    for (i, &gate) in ring.active_gates().iter().enumerate() {
        let weight = decode_q15(gate);
        assert!(
            (weight - 0.25).abs() < 0.01,
            "Adapter {} weight should be ~0.25, got {}",
            i,
            weight
        );
    }
}

#[test]
fn test_q15_dominant_adapter_weight() {
    // One dominant adapter with 80% weight
    let gates = [
        encode_q15(0.8),  // Dominant
        encode_q15(0.15), // Secondary
        encode_q15(0.05), // Minor
    ];

    let decision = make_decision(&[0, 1, 2], &gates, 0.3);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    let weights: Vec<f32> = ring.active_gates().iter().map(|&g| decode_q15(g)).collect();

    assert!(
        (weights[0] - 0.8).abs() < 0.01,
        "Dominant adapter should have ~0.8 weight, got {}",
        weights[0]
    );
    assert!(
        (weights[1] - 0.15).abs() < 0.01,
        "Secondary adapter should have ~0.15 weight, got {}",
        weights[1]
    );
    assert!(
        (weights[2] - 0.05).abs() < 0.01,
        "Minor adapter should have ~0.05 weight, got {}",
        weights[2]
    );
}

#[test]
fn test_q15_batch_gate_consistency() {
    // Batch of decisions should maintain consistent Q15 encoding
    let decisions = vec![
        make_decision(&[0], &[encode_q15(0.5)], 0.5),
        make_decision(&[1], &[encode_q15(0.75)], 0.4),
        make_decision(&[2], &[encode_q15(0.25)], 0.6),
    ];

    let rings = batch_decision_to_router_ring(&decisions, 100).unwrap();

    // Verify each ring maintains correct gate encoding
    assert!(
        (decode_q15(rings[0].gates_q15[0]) - 0.5).abs() < 0.01,
        "First ring gate should be ~0.5"
    );
    assert!(
        (decode_q15(rings[1].gates_q15[0]) - 0.75).abs() < 0.01,
        "Second ring gate should be ~0.75"
    );
    assert!(
        (decode_q15(rings[2].gates_q15[0]) - 0.25).abs() < 0.01,
        "Third ring gate should be ~0.25"
    );
}

#[test]
fn test_q15_max_k_with_varied_weights() {
    // Test K=8 with varied weights
    let gates: [i16; 8] = [
        encode_q15(0.25),
        encode_q15(0.2),
        encode_q15(0.15),
        encode_q15(0.12),
        encode_q15(0.1),
        encode_q15(0.08),
        encode_q15(0.06),
        encode_q15(0.04),
    ];

    let decision = make_decision(&[0, 1, 2, 3, 4, 5, 6, 7], &gates, 0.8);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring.k, 8);

    // Sum should be ~1.0
    let sum: f32 = ring.active_gates().iter().map(|&g| decode_q15(g)).sum();
    assert!(
        (sum - 1.0).abs() < 0.02,
        "K=8 gate sum should be ~1.0, got {}",
        sum
    );
}

#[test]
fn test_q15_deterministic_ring_creation() {
    // Same decision should produce identical ring every time
    let gates = [encode_q15(0.5), encode_q15(0.3), encode_q15(0.2)];
    let decision = make_decision(&[3, 1, 4], &gates, 0.5);

    let ring1 = decision_to_router_ring(&decision, 100).unwrap();
    let ring2 = decision_to_router_ring(&decision, 100).unwrap();
    let ring3 = decision_to_router_ring(&decision, 100).unwrap();

    assert_eq!(ring1.k, ring2.k);
    assert_eq!(ring2.k, ring3.k);
    assert_eq!(ring1.indices, ring2.indices);
    assert_eq!(ring2.indices, ring3.indices);
    assert_eq!(ring1.gates_q15, ring2.gates_q15);
    assert_eq!(ring2.gates_q15, ring3.gates_q15);
}

#[test]
fn test_q15_weight_ordering_preserved() {
    // Weight ordering should be preserved (not sorted)
    let gates = [
        encode_q15(0.1), // Smallest
        encode_q15(0.5), // Largest
        encode_q15(0.3), // Medium
        encode_q15(0.1), // Smallest (tied)
    ];

    let decision = make_decision(&[0, 1, 2, 3], &gates, 0.5);
    let ring = decision_to_router_ring(&decision, 100).unwrap();

    // Verify order matches input (not sorted by weight)
    let weights: Vec<f32> = ring.active_gates().iter().map(|&g| decode_q15(g)).collect();

    assert!((weights[0] - 0.1).abs() < 0.01, "First should be 0.1");
    assert!((weights[1] - 0.5).abs() < 0.01, "Second should be 0.5");
    assert!((weights[2] - 0.3).abs() < 0.01, "Third should be 0.3");
    assert!((weights[3] - 0.1).abs() < 0.01, "Fourth should be 0.1");
}

// ========================================================================
// End-to-End Router → MLX Backend Integration Tests
// ========================================================================
// These tests verify the complete pipeline from router decision through
// MLX backend execution, including determinism guarantees.

#[cfg(feature = "multi-backend")]
mod e2e_mlx_tests {
    use super::*;
    use adapteros_core::B3Hash;
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers};
    use adapteros_lora_mlx_ffi::{
        backend::MLXFFIBackend,
        lora::{LoRAAdapter, LoRAConfig},
        mock::{create_mock_adapter, create_mock_config},
        MLXFFIModel,
    };
    use adapteros_lora_worker::router_bridge::decision_to_router_ring;

    /// Create a test MLX backend with deterministic seeding
    fn create_test_backend_with_hash(manifest_hash: B3Hash) -> MLXFFIBackend {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::with_manifest_hash(model, manifest_hash)
            .expect("Failed to create backend with manifest hash")
    }

    /// Create a test MLX backend without deterministic seeding (for comparison)
    fn create_test_backend() -> MLXFFIBackend {
        let config = create_mock_config();
        let model = MLXFFIModel::new_null(config);
        MLXFFIBackend::new(model)
    }

    #[test]
    fn test_e2e_router_to_mlx_single_adapter() {
        // Test the complete flow: Decision → RouterRing → MLX execution
        let mut backend = create_test_backend();

        // Register a single adapter
        let adapter = create_mock_adapter("e2e-adapter-0", 4);
        backend.register_adapter(0, adapter).unwrap();

        // Create a router decision selecting this adapter
        let decision = make_decision(&[0], &[32767], 0.0); // Full weight on adapter 0

        // Convert to RouterRing using explicit bridge
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        // Prepare IO buffers
        let mut io = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; 32000], // Standard vocab size
            position: 0,
        };

        // Execute inference
        let result = backend.run_step(&ring, &mut io);
        assert!(result.is_ok(), "Inference should succeed: {:?}", result);

        // Verify output was produced
        assert!(
            io.output_logits.iter().any(|&x| x != 0.0),
            "Output logits should be non-zero after inference"
        );
        assert_eq!(io.position, 1, "Position should be incremented");
    }

    #[test]
    fn test_e2e_router_to_mlx_multi_adapter() {
        // Test multi-adapter routing through MLX
        let mut backend = create_test_backend();

        // Register multiple adapters
        for i in 0..4 {
            let adapter = create_mock_adapter(&format!("e2e-adapter-{}", i), 4);
            backend.register_adapter(i as u16, adapter).unwrap();
        }

        // Create a router decision selecting 3 adapters with different weights
        let decision = make_decision(
            &[0, 1, 2],
            &[16384, 8192, 8191], // ~0.5, ~0.25, ~0.25
            0.5,
        );

        let ring = decision_to_router_ring(&decision, 10).unwrap();

        let mut io = IoBuffers {
            input_ids: vec![42, 100, 200],
            output_logits: vec![0.0; 32000],
            position: 0,
        };

        let result = backend.run_step(&ring, &mut io);
        assert!(result.is_ok(), "Multi-adapter inference should succeed");
        assert!(io.output_logits.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_e2e_determinism_with_manifest_hash() {
        // Test that same manifest hash produces identical results
        let manifest_hash = B3Hash::hash(b"test-manifest-for-determinism");

        // Create two backends with same manifest hash
        let mut backend1 = create_test_backend_with_hash(manifest_hash);
        let mut backend2 = create_test_backend_with_hash(manifest_hash);

        // Register identical adapters
        let adapter1 = create_mock_adapter("determinism-test", 4);
        let adapter2 = create_mock_adapter("determinism-test", 4);
        backend1.register_adapter(0, adapter1).unwrap();
        backend2.register_adapter(0, adapter2).unwrap();

        // Same router decision
        let decision = make_decision(&[0], &[32767], 0.0);
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        // Same input
        let mut io1 = IoBuffers {
            input_ids: vec![1, 2, 3, 4, 5],
            output_logits: vec![0.0; 32000],
            position: 0,
        };
        let mut io2 = IoBuffers {
            input_ids: vec![1, 2, 3, 4, 5],
            output_logits: vec![0.0; 32000],
            position: 0,
        };

        // Run inference on both
        backend1.run_step(&ring, &mut io1).unwrap();
        backend2.run_step(&ring, &mut io2).unwrap();

        // Results should be identical (determinism guarantee)
        for i in 0..io1.output_logits.len() {
            assert!(
                (io1.output_logits[i] - io2.output_logits[i]).abs() < 1e-6,
                "Logit {} differs: {} vs {} (with same manifest hash)",
                i,
                io1.output_logits[i],
                io2.output_logits[i]
            );
        }
    }

    #[test]
    fn test_e2e_determinism_attestation() {
        let manifest_hash = B3Hash::hash(b"attestation-test-manifest");
        let backend = create_test_backend_with_hash(manifest_hash);

        // Get determinism attestation
        let report = backend.attest_determinism().unwrap();

        // With manifest hash, backend should attest as deterministic
        assert!(
            report.deterministic,
            "Backend with manifest hash should be deterministic"
        );
        assert!(
            report.metallib_hash.is_some(),
            "Should have manifest hash in attestation"
        );

        // Compare: backend without manifest hash
        let backend_unseeded = create_test_backend();
        let report_unseeded = backend_unseeded.attest_determinism().unwrap();

        assert!(
            !report_unseeded.deterministic,
            "Backend without manifest hash should NOT be deterministic"
        );
    }

    #[test]
    fn test_e2e_router_zero_adapters() {
        // Test that K=0 (no adapters selected) works correctly
        let mut backend = create_test_backend();

        // Register adapters but don't select any
        let adapter = create_mock_adapter("unused-adapter", 4);
        backend.register_adapter(0, adapter).unwrap();

        // Empty decision (K=0)
        let decision = make_decision(&[], &[], 0.0);
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        assert_eq!(ring.k, 0);

        let mut io = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; 32000],
            position: 0,
        };

        // Should succeed with base model output only
        let result = backend.run_step(&ring, &mut io);
        assert!(result.is_ok(), "K=0 should use base model: {:?}", result);
    }

    #[test]
    fn test_e2e_router_missing_adapter() {
        // Test graceful handling when router references non-existent adapter
        let mut backend = create_test_backend();

        // Register only adapter 0
        let adapter = create_mock_adapter("only-adapter", 4);
        backend.register_adapter(0, adapter).unwrap();

        // Decision references adapter 5 which doesn't exist
        let decision = make_decision(&[0, 5], &[16384, 16383], 0.5);
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        let mut io = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; 32000],
            position: 0,
        };

        // Should succeed but only use adapter 0
        let result = backend.run_step(&ring, &mut io);
        assert!(
            result.is_ok(),
            "Should handle missing adapter gracefully: {:?}",
            result
        );
    }

    #[test]
    fn test_e2e_router_batch_inference() {
        // Test batch of router decisions through MLX
        let mut backend = create_test_backend();

        // Register adapters
        for i in 0..4 {
            let adapter = create_mock_adapter(&format!("batch-adapter-{}", i), 4);
            backend.register_adapter(i as u16, adapter).unwrap();
        }

        // Create batch of decisions
        let decisions = vec![
            make_decision(&[0], &[32767], 0.3),
            make_decision(&[1, 2], &[16384, 16383], 0.5),
            make_decision(&[0, 1, 2, 3], &[8192, 8192, 8191, 8192], 0.8),
        ];

        // Convert all decisions
        let rings = batch_decision_to_router_ring(&decisions, 10).unwrap();

        // Run inference for each
        let mut results = Vec::new();
        for ring in &rings {
            let mut io = IoBuffers {
                input_ids: vec![42],
                output_logits: vec![0.0; 32000],
                position: 0,
            };
            backend.run_step(ring, &mut io).unwrap();
            results.push(io.output_logits[0]); // Just track first logit
        }

        assert_eq!(results.len(), 3);
        // Results should differ based on different adapter selections
        // (In stub mode they might be similar, but structure is tested)
    }

    #[test]
    fn test_e2e_adapter_hotswap_during_inference() {
        // Test hot-swapping adapters between inference calls
        let mut backend = create_test_backend();

        // Initial adapter
        let adapter_v1 = create_mock_adapter("hotswap-adapter-v1", 4);
        backend.register_adapter(0, adapter_v1).unwrap();

        let decision = make_decision(&[0], &[32767], 0.0);
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        // First inference
        let mut io1 = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; 32000],
            position: 0,
        };
        backend.run_step(&ring, &mut io1).unwrap();
        let first_result = io1.output_logits[0];

        // Hot-swap to new adapter
        backend.unload_adapter_runtime(0).unwrap();
        let adapter_v2 = create_mock_adapter("hotswap-adapter-v2", 8); // Different rank
        backend.load_adapter_runtime(0, adapter_v2).unwrap();

        // Second inference (same ring, different adapter)
        let mut io2 = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; 32000],
            position: 0,
        };
        backend.run_step(&ring, &mut io2).unwrap();
        let second_result = io2.output_logits[0];

        // Results should still be valid (hotswap worked)
        assert!(first_result.is_finite());
        assert!(second_result.is_finite());
    }

    #[test]
    fn test_e2e_q15_gate_propagation_to_backend() {
        // Verify Q15 gates are correctly propagated through the pipeline
        let mut backend = create_test_backend();

        // Register adapters
        for i in 0..3 {
            let adapter = create_mock_adapter(&format!("q15-test-{}", i), 4);
            backend.register_adapter(i as u16, adapter).unwrap();
        }

        // Create decision with specific Q15 gates
        let gates_q15: [i16; 3] = [
            encode_q15(0.6), // ~19660
            encode_q15(0.3), // ~9830
            encode_q15(0.1), // ~3277
        ];
        let decision = make_decision(&[0, 1, 2], &gates_q15, 0.5);
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        // Verify ring contains correct Q15 values
        assert_eq!(ring.k, 3);
        assert_eq!(ring.active_indices(), &[0, 1, 2]);

        // Gates should match (within Q15 encoding precision)
        for i in 0..3 {
            let expected = gates_q15[i];
            let actual = ring.gates_q15[i];
            assert_eq!(
                expected, actual,
                "Gate {} mismatch: expected {}, got {}",
                i, expected, actual
            );
        }

        // Run inference - should complete without error
        let mut io = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; 32000],
            position: 0,
        };
        backend.run_step(&ring, &mut io).unwrap();
    }

    #[test]
    fn test_e2e_negative_gates_skipped() {
        // Verify negative Q15 gates are skipped in backend processing
        let mut backend = create_test_backend();

        // Register adapters
        for i in 0..3 {
            let adapter = create_mock_adapter(&format!("neg-gate-{}", i), 4);
            backend.register_adapter(i as u16, adapter).unwrap();
        }

        // Create decision with one negative gate (should be skipped)
        let decision = make_decision(&[0, 1, 2], &[16384, -16384, 16383], 0.5);
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        assert_eq!(ring.k, 3); // All 3 in ring
        assert_eq!(ring.gates_q15[1], -16384); // Negative gate preserved

        let mut io = IoBuffers {
            input_ids: vec![1, 2, 3],
            output_logits: vec![0.0; 32000],
            position: 0,
        };

        // Should succeed - negative gate adapter is skipped internally
        let result = backend.run_step(&ring, &mut io);
        assert!(
            result.is_ok(),
            "Negative gates should be handled: {:?}",
            result
        );
    }

    #[test]
    fn test_e2e_sequential_inference_position_tracking() {
        // Test that position is correctly tracked across multiple inference steps
        let mut backend = create_test_backend();

        let adapter = create_mock_adapter("seq-adapter", 4);
        backend.register_adapter(0, adapter).unwrap();

        let decision = make_decision(&[0], &[32767], 0.0);
        let ring = decision_to_router_ring(&decision, 10).unwrap();

        let mut io = IoBuffers {
            input_ids: vec![1],
            output_logits: vec![0.0; 32000],
            position: 0,
        };

        // Run 5 sequential inference steps
        for expected_pos in 1..=5 {
            backend.run_step(&ring, &mut io).unwrap();
            assert_eq!(
                io.position, expected_pos,
                "Position should be {} after step {}",
                expected_pos, expected_pos
            );
        }
    }
}
