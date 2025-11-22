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

use adapteros_lora_kernel_api::RouterRing;
use adapteros_lora_router::Decision;
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
/// ```
/// use adapteros_lora_router::Decision;
/// use adapteros_lora_worker::router_bridge::decision_to_router_ring;
/// use smallvec::SmallVec;
///
/// let decision = Decision {
///     indices: SmallVec::from_slice(&[0, 1, 2]),
///     gates_q15: SmallVec::from_slice(&[16383, 8191, 4095]),
///     entropy: 0.5,
///     candidates: vec![],
/// };
///
/// let ring = decision_to_router_ring(&decision, 100);
/// assert_eq!(ring.k, 3);
/// assert_eq!(ring.active_indices(), &[0, 1, 2]);
/// assert_eq!(ring.active_gates(), &[16383, 8191, 4095]);
/// ```
pub fn decision_to_router_ring(decision: &Decision, max_adapter_count: u16) -> RouterRing {
    let k = decision.indices.len();

    debug!(
        k = %k,
        entropy = %decision.entropy,
        max_adapters = %max_adapter_count,
        "Converting Decision → RouterRing"
    );

    // Pre-condition: K ≤ 8 (SmallVec enforces capacity, but check explicitly)
    #[cfg(debug_assertions)]
    {
        if k > 8 {
            panic!(
                "router_bridge: Decision K > 8 (got {}), violates SmallVec<[_; 8]> invariant",
                k
            );
        }
    }

    #[cfg(not(debug_assertions))]
    {
        if k > 8 {
            error!(k = %k, "router_bridge: Decision K > 8, clamping to 8");
            // RouterRing::new will clamp to 8, but log for audit
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

    ring
}

/// Convert Decision to RouterRing without adapter count validation
///
/// **WARNING**: Skips bounds checking on adapter indices. Only use when:
/// - Adapter indices are known to be valid (e.g., from trusted source)
/// - Performance-critical path where validation is done elsewhere
///
/// For normal use, prefer `decision_to_router_ring()` with explicit max_adapter_count.
pub fn decision_to_router_ring_unchecked(decision: &Decision) -> RouterRing {
    decision_to_router_ring(decision, u16::MAX)
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
) -> Vec<RouterRing> {
    debug!(
        batch_size = %decisions.len(),
        max_adapters = %max_adapter_count,
        "Batch converting Decisions → RouterRings"
    );

    decisions
        .iter()
        .map(|d| decision_to_router_ring(d, max_adapter_count))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    /// Helper to create a test Decision
    fn make_decision(indices: &[u16], gates: &[i16], entropy: f32) -> Decision {
        use adapteros_lora_router::DecisionCandidate;

        Decision {
            indices: SmallVec::from_slice(indices),
            gates_q15: SmallVec::from_slice(gates),
            entropy,
            candidates: vec![],
        }
    }

    #[test]
    fn test_decision_to_router_ring_basic() {
        let decision = make_decision(&[0, 1, 2], &[16383, 8191, 4095], 0.5);
        let ring = decision_to_router_ring(&decision, 100);

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
        let ring = decision_to_router_ring(&decision, 100);

        assert_eq!(ring.k, 8);
        assert_eq!(ring.active_indices(), indices);
        assert_eq!(ring.active_gates(), gates);
    }

    #[test]
    fn test_decision_to_router_ring_empty() {
        // K=0 (no adapters selected)
        let decision = make_decision(&[], &[], 0.0);
        let ring = decision_to_router_ring(&decision, 100);

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
        let ring = decision_to_router_ring(&decision, 100);

        assert_eq!(ring.active_indices(), &[7, 3, 1, 5]);
        assert_eq!(ring.active_gates(), &[-1000, 500, 2000, -500]);
    }

    #[test]
    fn test_decision_to_router_ring_negative_gates() {
        // Test signed Q15 gates (negative values)
        let decision = make_decision(&[0, 1], &[-32767, -16383], 0.1);
        let ring = decision_to_router_ring(&decision, 100);

        assert_eq!(ring.active_gates(), &[-32767, -16383]);
    }

    #[test]
    #[should_panic(expected = "invalid adapter index")]
    #[cfg(debug_assertions)]
    fn test_decision_to_router_ring_out_of_bounds_debug() {
        // Debug builds should panic on out-of-bounds indices
        let decision = make_decision(&[0, 200], &[1000, 2000], 0.5);
        decision_to_router_ring(&decision, 100); // max_adapter=100, index 200 is invalid
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn test_decision_to_router_ring_out_of_bounds_release() {
        // Release builds should log error and zero-fill
        let decision = make_decision(&[0, 200], &[1000, 2000], 0.5);
        let ring = decision_to_router_ring(&decision, 100);

        // Verify zero-fill fallback
        assert_eq!(ring.k, 0);
        assert_eq!(ring.indices, [0; 8]);
        assert_eq!(ring.gates_q15, [0; 8]);
    }

    #[test]
    fn test_batch_decision_to_router_ring() {
        let decisions = vec![
            make_decision(&[0, 1], &[1000, 2000], 0.5),
            make_decision(&[2, 3, 4], &[3000, 4000, 5000], 0.6),
            make_decision(&[5], &[6000], 0.3),
        ];

        let rings = batch_decision_to_router_ring(&decisions, 100);

        assert_eq!(rings.len(), 3);
        assert_eq!(rings[0].k, 2);
        assert_eq!(rings[1].k, 3);
        assert_eq!(rings[2].k, 1);

        assert_eq!(rings[0].active_indices(), &[0, 1]);
        assert_eq!(rings[1].active_indices(), &[2, 3, 4]);
        assert_eq!(rings[2].active_indices(), &[5]);
    }

    #[test]
    fn test_decision_to_router_ring_unchecked() {
        let decision = make_decision(&[0, 1, 2], &[1000, 2000, 3000], 0.5);
        let ring = decision_to_router_ring_unchecked(&decision);

        assert_eq!(ring.k, 3);
        assert_eq!(ring.active_indices(), &[0, 1, 2]);
    }

    #[test]
    fn test_q15_range() {
        // Test full Q15 signed range: -32767 to +32767
        let decision = make_decision(
            &[0, 1, 2, 3],
            &[-32767, -16383, 16383, 32767],
            0.5,
        );
        let ring = decision_to_router_ring(&decision, 100);

        assert_eq!(ring.active_gates(), &[-32767, -16383, 16383, 32767]);
    }
}
