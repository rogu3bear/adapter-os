//! Router Decision → RouterRing conversion bridge (PRD-02)
//!
//! This module provides pure conversion functions from router Decision
//! to the canonical RouterRing format used by fused kernels.
//!
//! **Design principles:**
//! - Worker crate owns conversion logic (not router crate)
//! - Preserves router decision order exactly
//! - Enforces invariants with runtime checks (panic in debug, log in release)
//! - Zero-copy when possible
//!
//! **References:**
//! - PRD-02: Router-Kernel Ring Buffer Unification
//! - Router Decision: adapteros-lora-router/src/lib.rs:1010-1032
//! - Canonical RouterRing: adapteros-lora-kernel-api/src/lib.rs:8-159

use adapteros_core::AosError;
use adapteros_lora_kernel_api::RouterRing;
use adapteros_lora_router::{Decision, ROUTER_GATE_Q15_MAX};
use tracing::debug;

/// Convert router Decision to canonical RouterRing format
///
/// **Invariants enforced:**
/// - K ≤ 8 (enforced by RouterRing constructor)
/// - indices.len() == gates_q15.len() (enforced by Decision structure)
/// - Decision order preserved exactly
///
/// **Violation policy:**
/// - Debug builds: `panic!` on invariant violation
/// - Release builds: `error!` log + safe fallback (zero-filled ring)
///
/// # Arguments
/// * `decision` - Router decision with indices and Q15 gates
/// * `max_adapter_count` - Total registered adapters (for bounds checking)
///
/// # Returns
/// RouterRing with K active entries, unused entries zero-filled
///
/// # Examples
/// ```ignore
/// # use adapteros_lora_router::Decision;
/// # use adapteros_lora_worker::router_bridge::decision_to_router_ring;
/// # use smallvec::SmallVec;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let decision = Decision {
///     indices: SmallVec::from_slice(&[0, 1, 2]),
///     gates_q15: SmallVec::from_slice(&[16383, 8191, 4095]),
///     entropy: 0.5,
///     candidates: vec![],
///     decision_hash: None,
/// };
///
/// let ring = decision_to_router_ring(&decision, 100)?;
/// assert_eq!(ring.k, 3);
/// assert_eq!(ring.active_indices(), &[0, 1, 2]);
/// assert_eq!(ring.active_gates(), &[16383, 8191, 4095]);
/// # Ok(())
/// # }
/// ```
pub fn decision_to_router_ring(
    decision: &Decision,
    max_adapter_count: u16,
) -> Result<RouterRing, adapteros_core::AosError> {
    let k = decision.indices.len();

    debug!(
        k = %k,
        entropy = %decision.entropy,
        max_adapters = %max_adapter_count,
        "Converting Decision → RouterRing"
    );

    // Pre-condition: K ≤ 8 (SmallVec enforces capacity, but check explicitly)
    if k > 8 {
        return Err(adapteros_core::AosError::Routing(format!(
            "Decision K > 8 (got {}), violates SmallVec<[_; 8]> invariant",
            k
        )));
    }

    // Bounds check all indices before constructing the ring
    for (pos, idx) in decision.indices.iter().enumerate() {
        if *idx >= max_adapter_count {
            return Err(adapteros_core::AosError::Routing(format!(
                "Router decision index {} at position {} exceeds active adapter count {}",
                idx, pos, max_adapter_count
            )));
        }
    }

    // Create RouterRing with K active entries
    let mut ring = RouterRing::new(k);

    // Pre-condition: indices and gates have matching lengths (Decision guarantees this)
    debug_assert_eq!(
        decision.indices.len(),
        decision.gates_q15.len(),
        "Decision invariant violated: mismatched indices/gates lengths"
    );

    // Convert SmallVec slices to RouterRing (with bounds checking)
    ring.set_with_max_adapter(
        decision.indices.as_slice(),
        decision.gates_q15.as_slice(),
        max_adapter_count,
    );

    // Post-condition: verify K matches
    debug_assert_eq!(
        ring.k, k,
        "RouterRing K mismatch after conversion (expected {}, got {})",
        k, ring.k
    );

    Ok(ring)
}

/// Convert Decision to RouterRing using a fixed active adapter ID list (hashed IDs)
///
/// This enforces that router-selected indices must map to the current active set.
/// Returns an error if a decision index is out of range for the active set.
pub fn decision_to_router_ring_with_active_ids(
    decision: &Decision,
    active_adapter_ids: &[u16],
    position: usize,
) -> Result<RouterRing, AosError> {
    decision_to_router_ring_with_active_ids_and_strengths(
        decision,
        active_adapter_ids,
        None,
        position,
    )
}

/// Convert Decision to RouterRing and apply per-adapter strength scaling
pub fn decision_to_router_ring_with_active_ids_and_strengths(
    decision: &Decision,
    active_adapter_ids: &[u16],
    strengths: Option<&[f32]>,
    position: usize,
) -> Result<RouterRing, AosError> {
    let mapped_indices: Vec<u16> = decision
        .indices
        .iter()
        .map(|idx| {
            active_adapter_ids
                .get(*idx as usize)
                .copied()
                .ok_or_else(|| {
                    AosError::Routing(format!(
                        "Router decision index {} not in active set (len={})",
                        idx,
                        active_adapter_ids.len()
                    ))
                })
        })
        .collect::<Result<_, _>>()?;

    let mut ring = RouterRing::new(mapped_indices.len());
    ring.position = position;
    ring.set_with_max_adapter(
        mapped_indices.as_slice(),
        decision.gates_q15.as_slice(),
        u16::MAX,
    );

    if let Some(strengths) = strengths {
        if strengths.len() != active_adapter_ids.len() {
            return Err(AosError::Routing(format!(
                "Strengths length {} does not match active adapters {}",
                strengths.len(),
                active_adapter_ids.len()
            )));
        }

        for (slot, active_idx) in decision.indices.iter().enumerate() {
            let strength = strengths
                .get(*active_idx as usize)
                .copied()
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let scaled = (ring.gates_q15[slot] as f32 * strength).round() as i16;
            ring.gates_q15[slot] = scaled.clamp(-ROUTER_GATE_Q15_MAX, ROUTER_GATE_Q15_MAX);
        }
    }

    Ok(ring)
}

/// Batch convert multiple Decisions to RouterRings
///
/// # Arguments
/// * `decisions` - Slice of router decisions (e.g., one per token in batch)
/// * `max_adapter_count` - Total registered adapters
///
/// # Returns
/// Vec of RouterRings, one per Decision, in original order
pub fn batch_decision_to_router_ring(
    decisions: &[Decision],
    max_adapter_count: u16,
) -> Result<Vec<RouterRing>, adapteros_core::AosError> {
    debug!(
        batch_size = %decisions.len(),
        max_adapters = %max_adapter_count,
        "Batch converting Decisions → RouterRings"
    );

    decisions
        .iter()
        .map(|d| decision_to_router_ring(d, max_adapter_count))
        .collect::<Result<Vec<_>, _>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    /// Helper to create a test Decision
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

    #[test]
    fn test_decision_to_router_ring_basic() {
        let decision = make_decision(&[0, 1, 2], &[16383, 8191, 4095], 0.5);
        let ring = decision_to_router_ring(&decision, 100).unwrap();

        assert_eq!(ring.k, 3);
        assert_eq!(ring.active_indices(), &[0, 1, 2]);
        assert_eq!(ring.active_gates(), &[16383, 8191, 4095]);
        assert_eq!(ring.position, 0);

        // Verify zero-fill for unused entries
        assert_eq!(ring.indices[3..], [0; 5]);
        assert_eq!(ring.gates_q15[3..], [0; 5]);
    }

    #[test]
    fn test_decision_to_router_ring_max_k() {
        // Test with K=8 (maximum)
        let indices = [0, 1, 2, 3, 4, 5, 6, 7];
        let gates = [1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000];
        let decision = make_decision(&indices, &gates, 0.8);
        let ring = decision_to_router_ring(&decision, 100).unwrap();

        assert_eq!(ring.k, 8);
        assert_eq!(ring.active_indices(), indices);
        assert_eq!(ring.active_gates(), gates);
    }

    #[test]
    fn test_decision_to_router_ring_empty() {
        // K=0 (no adapters selected)
        let decision = make_decision(&[], &[], 0.0);
        let ring = decision_to_router_ring(&decision, 100).unwrap();

        assert_eq!(ring.k, 0);
        assert_eq!(ring.active_indices(), &[] as &[u16]);
        assert_eq!(ring.active_gates(), &[] as &[i16]);
        assert_eq!(ring.indices, [0u16; 8]); // All zero-filled
        assert_eq!(ring.gates_q15, [0i16; 8]);
    }

    #[test]
    fn test_decision_to_router_ring_preserves_order() {
        // Verify that Decision order is preserved exactly
        let decision = make_decision(&[7, 3, 1, 5], &[-1000, 500, 2000, -500], 0.3);
        let ring = decision_to_router_ring(&decision, 100).unwrap();

        assert_eq!(ring.active_indices(), &[7, 3, 1, 5]);
        assert_eq!(ring.active_gates(), &[-1000, 500, 2000, -500]);
    }

    #[test]
    fn test_decision_to_router_ring_negative_gates() {
        // Test signed Q15 gates (negative values)
        let decision = make_decision(&[0, 1], &[-32767, -16383], 0.1);
        let ring = decision_to_router_ring(&decision, 100).unwrap();

        assert_eq!(ring.active_gates(), &[-32767, -16383]);
    }

    #[test]
    fn test_decision_to_router_ring_out_of_bounds_errors() {
        // Out-of-bounds indices must return an error (no silent zero-fill)
        let decision = make_decision(&[0, 200], &[1000, 2000], 0.5);
        let err = decision_to_router_ring(&decision, 100).unwrap_err();
        if let AosError::Routing(msg) = err {
            assert!(
                msg.contains("exceeds active adapter count"),
                "unexpected message: {msg}"
            );
        } else {
            panic!("expected routing error, got {:?}", err);
        }
    }

    #[test]
    fn test_batch_decision_to_router_ring() {
        let decisions = vec![
            make_decision(&[0, 1], &[1000, 2000], 0.5),
            make_decision(&[2, 3, 4], &[3000, 4000, 5000], 0.6),
            make_decision(&[5], &[6000], 0.3),
        ];

        let rings = batch_decision_to_router_ring(&decisions, 100).unwrap();

        assert_eq!(rings.len(), 3);
        assert_eq!(rings[0].k, 2);
        assert_eq!(rings[1].k, 3);
        assert_eq!(rings[2].k, 1);

        assert_eq!(rings[0].active_indices(), &[0, 1]);
        assert_eq!(rings[1].active_indices(), &[2, 3, 4]);
        assert_eq!(rings[2].active_indices(), &[5]);
    }

    #[test]
    fn test_q15_range() {
        // Test full Q15 signed range: -32767 to +32767
        let decision = make_decision(&[0, 1, 2, 3], &[-32767, -16383, 16383, 32767], 0.5);
        let ring = decision_to_router_ring(&decision, 100).unwrap();

        assert_eq!(ring.active_gates(), &[-32767, -16383, 16383, 32767]);
    }

    #[test]
    fn test_decision_to_router_ring_with_active_ids_maps_hashes() {
        let decision = make_decision(&[0, 1], &[111, 222], 0.4);
        let active_ids = [42u16, 99u16];

        let ring = decision_to_router_ring_with_active_ids(&decision, &active_ids, 7).unwrap();

        assert_eq!(ring.active_indices(), &[42, 99]);
        assert_eq!(ring.active_gates(), &[111, 222]);
        assert_eq!(ring.position, 7);
    }

    #[test]
    fn test_decision_to_router_ring_with_active_ids_errors_on_missing() {
        let decision = make_decision(&[0, 1], &[100, 200], 0.2);
        let active_ids = [7u16];

        let result = decision_to_router_ring_with_active_ids(&decision, &active_ids, 0);
        assert!(matches!(result, Err(AosError::Routing(_))));
    }
}
