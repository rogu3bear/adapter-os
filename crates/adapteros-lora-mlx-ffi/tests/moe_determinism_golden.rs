//! MoE Expert Selection Golden Tests
//!
//! Verifies deterministic top-k expert selection with:
//! - Small router_logits tensors with ties
//! - Golden expected expert IDs
//! - Golden expected normalized gates
//! - Stable ordering guarantees
//!
//! These tests use a pure Rust implementation of the selection algorithm
//! to verify determinism without requiring the full MLX runtime.

// ============================================================================
// TEST HELPER: Deterministic Top-K Selection
// ============================================================================

/// Helper function for testing deterministic selection without full MLX runtime.
///
/// Implements the same algorithm as MoE select_topk_experts:
/// 1. Apply softmax to convert logits to probabilities
/// 2. Sort by score descending, then by index ascending (tie-break)
/// 3. Select top-k and renormalize gates to sum to 1.0
///
/// # Tie-Breaking Rule
///
/// When multiple experts have equal scores, ties are broken deterministically:
/// - Primary sort: score descending (higher scores selected first)
/// - Secondary sort: expert_id ascending (lower IDs win on tie)
fn select_topk_deterministic(logits: &[f32], k: usize) -> (Vec<usize>, Vec<f32>) {
    // Apply softmax for numerical stability
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_logits: Vec<f32> = logits.iter().map(|x| (x - max_logit).exp()).collect();
    let sum_exp: f32 = exp_logits.iter().sum();
    let probs: Vec<f32> = exp_logits.iter().map(|x| x / sum_exp).collect();

    // Create (index, prob) pairs and sort with deterministic tie-breaking
    let mut indexed: Vec<(usize, f32)> = probs.into_iter().enumerate().collect();

    // Sort by score descending, then by index ascending (for tie-breaking)
    indexed.sort_by(|a, b| {
        match b.1.partial_cmp(&a.1) {
            Some(std::cmp::Ordering::Equal) => {
                // On tie, lower index wins (ascending)
                a.0.cmp(&b.0)
            }
            Some(ord) => ord,
            None => std::cmp::Ordering::Equal,
        }
    });

    // Take top k
    let top_k: Vec<(usize, f32)> = indexed.into_iter().take(k).collect();
    let indices: Vec<usize> = top_k.iter().map(|(i, _)| *i).collect();
    let scores: Vec<f32> = top_k.iter().map(|(_, s)| *s).collect();

    // Renormalize scores to sum to 1.0
    let score_sum: f32 = scores.iter().sum();
    let normalized_gates: Vec<f32> = scores.iter().map(|s| s / score_sum).collect();

    (indices, normalized_gates)
}

// ============================================================================
// GOLDEN FIXTURE TESTS: Tied Logits
// ============================================================================

/// Golden fixture: Small router logits tensor with ties
///
/// Input logits: [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]
/// After softmax: [~0.22, ~0.22, ~0.22, ~0.22, ~0.03, ~0.03, ~0.03, ~0.03]
/// Top-2 selection: experts 0, 1 (lowest IDs among highest scores)
#[test]
fn golden_moe_select_topk_with_ties() {
    // GOLDEN INPUT: 8 experts, first 4 tied at logit=1.0
    let logits = vec![1.0f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let k = 2;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    // GOLDEN EXPECTED OUTPUT
    // Experts 0-3 have equal softmax probability (highest)
    // With deterministic tie-breaking (ID ascending), must select 0, 1
    assert_eq!(
        indices,
        vec![0, 1],
        "GOLDEN: Must select lowest IDs (0, 1) when top experts are tied"
    );

    // Gates should be normalized to sum to 1.0
    let gate_sum: f32 = gates.iter().sum();
    assert!(
        (gate_sum - 1.0).abs() < 1e-5,
        "GOLDEN: gates must sum to 1.0, got {}",
        gate_sum
    );

    // With 2 experts having equal scores selected, each gate should be 0.5
    assert!(
        (gates[0] - 0.5).abs() < 1e-5,
        "GOLDEN: gate[0] = 0.5 (equal share), got {}",
        gates[0]
    );
    assert!(
        (gates[1] - 0.5).abs() < 1e-5,
        "GOLDEN: gate[1] = 0.5 (equal share), got {}",
        gates[1]
    );
}

/// Golden fixture: Mixed scores with partial ties
///
/// Input: [2.0, 1.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0]
/// Experts 0,2 tied highest; experts 1,3 tied second
#[test]
fn golden_moe_select_topk_partial_ties() {
    let logits = vec![2.0f32, 1.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let k = 3;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    // GOLDEN EXPECTED:
    // - Experts 0, 2 have highest equal softmax scores
    // - Expert 0 selected first (lower ID in tie)
    // - Expert 2 selected second (higher ID in tie)
    // - Expert 1 selected third (next highest score)
    assert_eq!(
        indices[0], 0,
        "GOLDEN: expert 0 first (tied highest, lower ID)"
    );
    assert_eq!(
        indices[1], 2,
        "GOLDEN: expert 2 second (tied highest, higher ID)"
    );
    assert_eq!(indices[2], 1, "GOLDEN: expert 1 third (next highest score)");

    // Gates should sum to 1.0
    let gate_sum: f32 = gates.iter().sum();
    assert!(
        (gate_sum - 1.0).abs() < 1e-5,
        "GOLDEN: gates must sum to 1.0"
    );

    // Experts 0 and 2 should have equal gates (tied scores)
    assert!(
        (gates[0] - gates[1]).abs() < 1e-5,
        "GOLDEN: tied experts should have equal gates"
    );
}

/// Golden fixture: All experts tied with uniform logits
#[test]
fn golden_moe_all_tied() {
    let logits = vec![1.0f32; 8]; // All equal
    let k = 4;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    // GOLDEN: Must select [0, 1, 2, 3] - lowest IDs when all tied
    assert_eq!(
        indices,
        vec![0, 1, 2, 3],
        "GOLDEN: must select lowest IDs [0,1,2,3] when all 8 experts tied"
    );

    // All gates should be 0.25 (uniform distribution after normalization)
    for (i, &gate) in gates.iter().enumerate() {
        assert!(
            (gate - 0.25).abs() < 1e-5,
            "GOLDEN: gate[{}] = 0.25 (uniform), got {}",
            i,
            gate
        );
    }
}

/// Golden fixture: Single dominant expert
#[test]
fn golden_moe_single_dominant() {
    // Expert 3 has much higher logit
    let logits = vec![0.0f32, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 0.0];
    let k = 2;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    // Expert 3 must be first (dominant)
    assert_eq!(indices[0], 3, "GOLDEN: dominant expert 3 must be first");

    // Expert 3's gate should be very close to 1.0
    assert!(
        gates[0] > 0.99,
        "GOLDEN: dominant expert gate > 0.99, got {}",
        gates[0]
    );

    // Second expert should be expert 0 (lowest ID among ties for second place)
    assert_eq!(
        indices[1], 0,
        "GOLDEN: second expert should be 0 (lowest ID among remaining)"
    );
}

// ============================================================================
// STABILITY TESTS
// ============================================================================

/// Stability test: Same input produces identical output across 100 runs
#[test]
fn test_select_topk_stability_tied_logits() {
    let logits = vec![1.0f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let k = 2;

    let (first_indices, first_gates) = select_topk_deterministic(&logits, k);

    // Verify stability across 100 runs
    for run in 0..100 {
        let (indices, gates) = select_topk_deterministic(&logits, k);

        assert_eq!(
            indices, first_indices,
            "Run {}: indices must be identical",
            run
        );

        for (i, (g1, g2)) in first_gates.iter().zip(gates.iter()).enumerate() {
            assert!(
                (g1 - g2).abs() < 1e-6,
                "Run {}: gate[{}] must be identical: {} vs {}",
                run,
                i,
                g1,
                g2
            );
        }
    }
}

/// Stability test: Partial ties produce stable ordering
#[test]
fn test_select_topk_stability_partial_ties() {
    let logits = vec![2.0f32, 1.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let k = 4;

    let (first_indices, _) = select_topk_deterministic(&logits, k);

    for run in 0..100 {
        let (indices, _) = select_topk_deterministic(&logits, k);
        assert_eq!(
            indices, first_indices,
            "Run {}: partial tie ordering must be stable",
            run
        );
    }
}

// ============================================================================
// GATE NORMALIZATION INVARIANTS
// ============================================================================

/// Test that gates always sum to 1.0 regardless of input distribution
#[test]
fn test_gate_normalization_invariant() {
    let test_cases = vec![
        vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], // monotonic increasing
        vec![8.0f32, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0], // monotonic decreasing
        vec![1.0f32, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], // uniform
        vec![100.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], // single dominant
        vec![0.001f32, 0.001, 0.001, 0.001, 0.001, 0.001, 0.001, 0.001], // very small
        vec![-1.0f32, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0], // negative
    ];

    for (i, logits) in test_cases.iter().enumerate() {
        let (_, gates) = select_topk_deterministic(logits, 3);

        let gate_sum: f32 = gates.iter().sum();
        assert!(
            (gate_sum - 1.0).abs() < 1e-5,
            "Test case {}: gates must sum to 1.0, got {}",
            i,
            gate_sum
        );

        // All gates must be non-negative
        for (j, &gate) in gates.iter().enumerate() {
            assert!(
                gate >= 0.0,
                "Test case {}: gate[{}] must be non-negative, got {}",
                i,
                j,
                gate
            );
        }
    }
}

/// Test that selected gates are in descending order (matching selection order)
#[test]
fn test_gate_ordering_matches_selection() {
    let logits = vec![1.0f32, 3.0, 2.0, 4.0, 0.5, 0.1, 0.2, 0.3];
    let k = 4;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    // Indices should be in order of decreasing score
    assert_eq!(indices[0], 3, "Highest scorer (logit=4.0) should be first");
    assert_eq!(indices[1], 1, "Second highest (logit=3.0) should be second");
    assert_eq!(indices[2], 2, "Third highest (logit=2.0) should be third");
    assert_eq!(indices[3], 0, "Fourth highest (logit=1.0) should be fourth");

    // Gates should also be in descending order
    for i in 0..(gates.len() - 1) {
        assert!(
            gates[i] >= gates[i + 1],
            "Gates should be in descending order: gate[{}]={} >= gate[{}]={}",
            i,
            gates[i],
            i + 1,
            gates[i + 1]
        );
    }
}

// ============================================================================
// EDGE CASES
// ============================================================================

/// Test k equals number of experts
#[test]
fn test_k_equals_num_experts() {
    let logits = vec![1.0f32, 2.0, 3.0, 4.0];
    let k = 4;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    assert_eq!(indices.len(), 4, "Should select all experts");
    assert_eq!(gates.len(), 4, "Should have 4 gates");

    // Should be in descending score order
    assert_eq!(indices, vec![3, 2, 1, 0]);
}

/// Test k=1 (single expert selection)
#[test]
fn test_k_equals_one() {
    // Tied for highest
    let logits = vec![5.0f32, 5.0, 1.0, 1.0];
    let k = 1;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    // Must select expert 0 (lowest ID among tied highest)
    assert_eq!(indices, vec![0], "Must select lowest ID on tie");
    assert!((gates[0] - 1.0).abs() < 1e-5, "Single expert gate = 1.0");
}

/// Test extreme logit values
#[test]
fn test_extreme_logits() {
    // Very large difference - should still work
    let logits = vec![1000.0f32, 0.0, 0.0, 0.0];
    let k = 2;

    let (indices, gates) = select_topk_deterministic(&logits, k);

    assert_eq!(indices[0], 0, "Expert with logit=1000 should be first");
    assert!(gates[0] > 0.99, "Dominant expert should have gate ~1.0");
}
