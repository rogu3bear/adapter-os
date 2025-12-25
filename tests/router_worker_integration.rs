//! Integration test: Router → Worker data flow validation
//!
//! This test validates the complete flow from router decisions to worker execution:
//! 1. Router produces Decision with Q15 gates
//! 2. Decision is converted to RouterRing
//! 3. RouterRing gates are correctly converted back to f32 (divide by 32767)
//! 4. Adapter weights are applied with correct gate values
//!
//! Reference: Router-Worker Integration PRD-02

#![cfg(test)]

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::RouterRing;
use adapteros_lora_router::{Decision, ROUTER_GATE_Q15_DENOM};
use adapteros_lora_worker::router_bridge::{
    decision_to_router_ring, decision_to_router_ring_with_active_ids_and_strengths,
};
use smallvec::SmallVec;

/// Test that Q15 gates are correctly produced by router
#[test]
fn test_router_produces_q15_gates() {
    // Create test decision (simulating router output)
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2]),
        gates_q15: SmallVec::from_slice(&[16383, 8191, 4095]), // Q15 values
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    // Verify Q15 gates are in valid range
    for &gate in decision.gates_q15.iter() {
        assert!(
            gate >= -32767 && gate <= 32767,
            "Q15 gate must be in range [-32767, 32767], got {gate}"
        );
    }

    // Verify conversion to f32 uses correct denominator
    let gates_f32 = decision.gates_f32();
    assert_eq!(gates_f32.len(), 3);
    assert!((gates_f32[0] - (16383.0 / 32767.0)).abs() < 0.001);
    assert!((gates_f32[1] - (8191.0 / 32767.0)).abs() < 0.001);
    assert!((gates_f32[2] - (4095.0 / 32767.0)).abs() < 0.001);
}

/// Test Decision → RouterRing conversion preserves Q15 gates
#[test]
fn test_decision_to_router_ring_preserves_gates() -> Result<()> {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2]),
        gates_q15: SmallVec::from_slice(&[16383, 8191, 4095]),
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let ring = decision_to_router_ring(&decision, 100)?;

    // Verify RouterRing preserves Q15 gates exactly
    assert_eq!(ring.k, 3);
    assert_eq!(ring.active_gates(), &[16383, 8191, 4095]);
    assert_eq!(ring.active_indices(), &[0, 1, 2]);

    Ok(())
}

/// Test Q15 → f32 conversion uses correct denominator (32767)
#[test]
fn test_q15_to_f32_conversion() {
    let test_cases = vec![
        (32767, 1.0),      // Max Q15 → 1.0
        (16383, 0.5),      // Half max → 0.5
        (0, 0.0),          // Zero → 0.0
        (-16383, -0.5),    // Negative half → -0.5
        (-32767, -1.0),    // Min Q15 → -1.0
    ];

    for (q15_gate, expected_f32) in test_cases {
        let actual_f32 = q15_gate as f32 / ROUTER_GATE_Q15_DENOM;
        let error = (actual_f32 - expected_f32).abs();
        assert!(
            error < 0.001,
            "Q15 {q15_gate} → f32: expected {expected_f32}, got {actual_f32} (error: {error})"
        );
    }
}

/// Test RouterRing correctly stores gate order
#[test]
fn test_router_ring_preserves_decision_order() -> Result<()> {
    // Non-sorted indices to verify order preservation
    let decision = Decision {
        indices: SmallVec::from_slice(&[7, 3, 1, 5]),
        gates_q15: SmallVec::from_slice(&[1000, 2000, 3000, 4000]),
        entropy: 0.3,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let ring = decision_to_router_ring(&decision, 100)?;

    // Order must be preserved exactly (no sorting)
    assert_eq!(ring.active_indices(), &[7, 3, 1, 5]);
    assert_eq!(ring.active_gates(), &[1000, 2000, 3000, 4000]);

    Ok(())
}

/// Test adapter strength scaling is applied correctly to gates
#[test]
fn test_adapter_strength_scaling() -> Result<()> {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1]),
        gates_q15: SmallVec::from_slice(&[32767, 16383]),
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let active_adapter_ids = vec![100u16, 200u16];
    let strengths = vec![0.5, 1.0]; // First adapter at 50% strength

    let ring = decision_to_router_ring_with_active_ids_and_strengths(
        &decision,
        &active_adapter_ids,
        Some(&strengths),
        0,
    )?;

    // First gate should be scaled by 0.5
    let expected_scaled = (32767.0_f32 * 0.5).round() as i16;
    assert_eq!(ring.active_gates()[0], expected_scaled);

    // Second gate should be unchanged (strength = 1.0)
    assert_eq!(ring.active_gates()[1], 16383);

    Ok(())
}

/// Test end-to-end flow: Router → Decision → RouterRing → Backend
#[test]
fn test_e2e_router_to_worker_flow() -> Result<()> {
    // Step 1: Create router decision with Q15 gates
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2]),
        gates_q15: SmallVec::from_slice(&[20000, 10000, 2767]), // Q15 values
        entropy: 0.8,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    // Step 2: Convert to RouterRing (worker bridge)
    let ring = decision_to_router_ring(&decision, 100)?;

    // Step 3: Verify RouterRing has correct gates
    assert_eq!(ring.k, 3);
    assert_eq!(ring.active_gates(), &[20000, 10000, 2767]);

    // Step 4: Simulate backend converting Q15 → f32 for weight application
    let gate_0_f32 = ring.active_gates()[0] as f32 / ROUTER_GATE_Q15_DENOM;
    let gate_1_f32 = ring.active_gates()[1] as f32 / ROUTER_GATE_Q15_DENOM;
    let gate_2_f32 = ring.active_gates()[2] as f32 / ROUTER_GATE_Q15_DENOM;

    // Step 5: Verify f32 gates are in valid range [0, 1]
    assert!(gate_0_f32 >= 0.0 && gate_0_f32 <= 1.0);
    assert!(gate_1_f32 >= 0.0 && gate_1_f32 <= 1.0);
    assert!(gate_2_f32 >= 0.0 && gate_2_f32 <= 1.0);

    // Step 6: Verify approximate f32 values
    assert!((gate_0_f32 - 0.610).abs() < 0.01); // 20000/32767 ≈ 0.610
    assert!((gate_1_f32 - 0.305).abs() < 0.01); // 10000/32767 ≈ 0.305
    assert!((gate_2_f32 - 0.084).abs() < 0.01); // 2767/32767 ≈ 0.084

    Ok(())
}

/// Test negative Q15 gates (for signed gate support)
#[test]
fn test_negative_q15_gates() -> Result<()> {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1]),
        gates_q15: SmallVec::from_slice(&[-16383, 16383]),
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let ring = decision_to_router_ring(&decision, 100)?;

    // Verify negative gates are preserved
    assert_eq!(ring.active_gates()[0], -16383);
    assert_eq!(ring.active_gates()[1], 16383);

    // Verify f32 conversion
    let gate_0_f32 = ring.active_gates()[0] as f32 / ROUTER_GATE_Q15_DENOM;
    let gate_1_f32 = ring.active_gates()[1] as f32 / ROUTER_GATE_Q15_DENOM;

    assert!((gate_0_f32 - (-0.5)).abs() < 0.001);
    assert!((gate_1_f32 - 0.5).abs() < 0.001);

    Ok(())
}

/// Test boundary condition: K=0 (no adapters selected)
#[test]
fn test_empty_decision() -> Result<()> {
    let decision = Decision {
        indices: SmallVec::new(),
        gates_q15: SmallVec::new(),
        entropy: 0.0,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let ring = decision_to_router_ring(&decision, 100)?;

    assert_eq!(ring.k, 0);
    assert_eq!(ring.active_indices(), &[] as &[u16]);
    assert_eq!(ring.active_gates(), &[] as &[i16]);

    Ok(())
}

/// Test boundary condition: K=8 (maximum adapters)
#[test]
fn test_max_k_decision() -> Result<()> {
    let indices = [0, 1, 2, 3, 4, 5, 6, 7];
    let gates = [4096, 4096, 4096, 4096, 4096, 4096, 4096, 4095]; // Sum ≈ 32767

    let decision = Decision {
        indices: SmallVec::from_slice(&indices),
        gates_q15: SmallVec::from_slice(&gates),
        entropy: 1.0,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let ring = decision_to_router_ring(&decision, 100)?;

    assert_eq!(ring.k, 8);
    assert_eq!(ring.active_indices().len(), 8);
    assert_eq!(ring.active_gates().len(), 8);

    Ok(())
}

/// Test error handling: Out-of-bounds adapter index
#[test]
fn test_out_of_bounds_adapter_index() {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 200]), // 200 exceeds max_adapter_count
        gates_q15: SmallVec::from_slice(&[1000, 2000]),
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let result = decision_to_router_ring(&decision, 100);
    assert!(result.is_err());

    if let Err(AosError::Routing(msg)) = result {
        assert!(msg.contains("exceeds active adapter count"));
    } else {
        panic!("Expected Routing error for out-of-bounds index");
    }
}

/// Test Q15 denominator constant matches router implementation
#[test]
fn test_q15_denominator_constant() {
    // Critical: This constant must be exactly 32767, NOT 32768
    // See router documentation for why (determinism proof)
    assert_eq!(
        ROUTER_GATE_Q15_DENOM, 32767.0,
        "Q15 denominator must be 32767 for deterministic conversion"
    );

    // Verify max Q15 value converts to exactly 1.0
    let max_gate: i16 = 32767;
    let f32_gate = max_gate as f32 / ROUTER_GATE_Q15_DENOM;
    assert_eq!(f32_gate, 1.0, "Max Q15 (32767) must convert to 1.0");
}

/// Test that RouterRing zero-fills unused entries
#[test]
fn test_router_ring_zero_fill() -> Result<()> {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1]),
        gates_q15: SmallVec::from_slice(&[1000, 2000]),
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let ring = decision_to_router_ring(&decision, 100)?;

    // Verify active entries
    assert_eq!(ring.k, 2);
    assert_eq!(ring.active_gates(), &[1000, 2000]);

    // Verify unused entries are zero-filled
    assert_eq!(ring.indices[2..], [0; 6]);
    assert_eq!(ring.gates_q15[2..], [0; 6]);

    Ok(())
}
