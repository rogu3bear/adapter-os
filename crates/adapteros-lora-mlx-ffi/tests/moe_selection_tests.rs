//! Tests for MoE (Mixture of Experts) deterministic selection semantics.
//!
//! These tests verify the algorithm specification for expert selection:
//! - Tie-breaking: sort by (score desc, expert_id asc)
//! - Normalization: gates sum to 1.0 within floating point tolerance
//!
//! The tests use a pure Rust reference implementation to verify the expected
//! behavior without requiring the full MLX runtime.

/// Reference implementation of deterministic top-k expert selection.
///
/// This implements the same algorithm documented in `moe.rs`:
/// - Apply softmax to convert logits to probabilities
/// - Sort by (score descending, expert_id ascending)
/// - Take top-k experts
/// - Renormalize gates to sum to 1.0
fn select_topk_experts_reference(logits: &[f32], k: usize) -> (Vec<usize>, Vec<f32>) {
    // Apply softmax
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_logits: Vec<f32> = logits.iter().map(|x| (x - max_logit).exp()).collect();
    let sum_exp: f32 = exp_logits.iter().sum();
    let probs: Vec<f32> = exp_logits.iter().map(|x| x / sum_exp).collect();

    // Create (index, prob) pairs
    let mut indexed: Vec<(usize, f32)> = probs.into_iter().enumerate().collect();

    // Sort by score descending, then by index ascending (deterministic tie-breaking)
    indexed.sort_by(|a, b| {
        match b.1.partial_cmp(&a.1) {
            Some(std::cmp::Ordering::Equal) => a.0.cmp(&b.0),
            Some(ord) => ord,
            None => std::cmp::Ordering::Equal,
        }
    });

    // Take top k
    let top_k: Vec<(usize, f32)> = indexed.into_iter().take(k).collect();
    let indices: Vec<usize> = top_k.iter().map(|(i, _)| *i).collect();
    let scores: Vec<f32> = top_k.iter().map(|(_, s)| *s).collect();

    // Renormalize to sum to 1.0
    let score_sum: f32 = scores.iter().sum();
    let normalized_gates: Vec<f32> = scores.iter().map(|s| s / score_sum).collect();

    (indices, normalized_gates)
}

/// Test that expert selection is stable when all experts have equal scores.
///
/// With equal logits, softmax produces equal probabilities.
/// Tie-breaking should consistently select lower IDs first.
#[test]
fn test_select_topk_experts_tie_break_is_stable() {
    // All equal logits -> all equal probabilities after softmax
    let equal_logits = vec![1.0f32; 8];

    // Run multiple times to verify stability
    for _ in 0..10 {
        let (indices, _gates) = select_topk_experts_reference(&equal_logits, 2);

        // With deterministic tie-breaking (lower ID first), should always get [0, 1]
        assert_eq!(
            indices,
            vec![0, 1],
            "Expected experts [0, 1] for equal scores, got {:?}",
            indices
        );
    }
}

/// Test tie-breaking with partial ties.
///
/// When some experts are tied for highest score, the one with lower ID wins.
#[test]
fn test_select_topk_experts_partial_tie_ordering() {
    // Experts 2 and 5 have highest scores (tied)
    // Experts 3 and 4 have second highest (tied)
    let partial_tie_logits = vec![0.0f32, 0.0, 2.0, 1.0, 1.0, 2.0, 0.0, 0.0];

    let (indices, _) = select_topk_experts_reference(&partial_tie_logits, 4);

    // Should be ordered: [2, 5, 3, 4]
    // - 2 comes before 5 (tied highest, lower ID wins)
    // - 3 comes before 4 (tied second, lower ID wins)
    assert_eq!(
        indices[0], 2,
        "First expert should be 2 (tied highest, lower ID)"
    );
    assert_eq!(
        indices[1], 5,
        "Second expert should be 5 (tied highest, higher ID)"
    );
    assert_eq!(
        indices[2], 3,
        "Third expert should be 3 (tied second, lower ID)"
    );
    assert_eq!(
        indices[3], 4,
        "Fourth expert should be 4 (tied second, higher ID)"
    );
}

/// Test that output is stable across multiple invocations.
#[test]
fn test_select_topk_experts_determinism() {
    let logits = vec![1.5f32, 2.3, 0.8, 3.1, 2.3, 1.2, 0.5, 2.9];

    let first_result = select_topk_experts_reference(&logits, 3);

    // Run 100 times and verify same result
    for i in 0..100 {
        let result = select_topk_experts_reference(&logits, 3);
        assert_eq!(
            result, first_result,
            "Result changed on iteration {}: {:?} vs {:?}",
            i, result, first_result
        );
    }
}

/// Test that gates are normalized to sum to 1.0.
#[test]
fn test_select_topk_experts_normalization_stable() {
    let test_cases = vec![
        vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], // monotonic
        vec![8.0f32, 1.0, 8.0, 1.0, 8.0, 1.0, 8.0, 1.0], // alternating
        vec![1.0f32, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], // uniform
        vec![100.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], // one dominant
        vec![-5.0f32, -3.0, -1.0, 0.0, 1.0, 3.0, 5.0, 10.0], // negative to positive
    ];

    for (i, logits) in test_cases.iter().enumerate() {
        let (_, gates) = select_topk_experts_reference(logits, 2);

        let gate_sum: f32 = gates.iter().sum();
        assert!(
            (gate_sum - 1.0).abs() < 1e-5,
            "Test case {}: Gates should sum to 1.0, got {} for logits {:?}",
            i,
            gate_sum,
            logits
        );

        // All gates should be non-negative
        for (j, &gate) in gates.iter().enumerate() {
            assert!(
                gate >= 0.0,
                "Test case {}: Gate {} should be non-negative, got {}",
                i,
                j,
                gate
            );
        }

        // All gates should be <= 1.0
        for (j, &gate) in gates.iter().enumerate() {
            assert!(
                gate <= 1.0 + 1e-6,
                "Test case {}: Gate {} should be <= 1.0, got {}",
                i,
                j,
                gate
            );
        }
    }
}

/// Test that larger logit differences result in more skewed gates.
#[test]
fn test_select_topk_experts_gate_distribution() {
    // Large difference - one expert should dominate
    let large_diff = vec![10.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let (indices, gates) = select_topk_experts_reference(&large_diff, 2);

    assert_eq!(indices[0], 0, "Expert 0 should be selected first");
    assert!(
        gates[0] > 0.99,
        "First gate should be > 0.99 for large logit difference, got {}",
        gates[0]
    );

    // Small difference - gates should be more balanced
    let small_diff = vec![1.0f32, 0.9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let (_, gates) = select_topk_experts_reference(&small_diff, 2);

    assert!(
        gates[0] < 0.7,
        "First gate should be < 0.7 for small logit difference, got {}",
        gates[0]
    );
    assert!(
        gates[1] > 0.3,
        "Second gate should be > 0.3 for small logit difference, got {}",
        gates[1]
    );
}

/// Test edge case: k equals number of experts.
#[test]
fn test_select_topk_experts_k_equals_n() {
    let logits = vec![1.0f32, 2.0, 3.0, 4.0];
    let (indices, gates) = select_topk_experts_reference(&logits, 4);

    // All experts should be selected, ordered by score descending
    assert_eq!(indices, vec![3, 2, 1, 0]);

    // Gates should still sum to 1.0
    let sum: f32 = gates.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5);
}

/// Test edge case: k = 1 (single expert selection).
#[test]
fn test_select_topk_experts_single_expert() {
    let logits = vec![1.0f32, 5.0, 3.0, 2.0];
    let (indices, gates) = select_topk_experts_reference(&logits, 1);

    assert_eq!(indices, vec![1], "Should select expert with highest score");
    assert!(
        (gates[0] - 1.0).abs() < 1e-6,
        "Single gate should be 1.0"
    );
}

/// Test numerical stability with very large logits.
#[test]
fn test_select_topk_experts_large_logits() {
    let large_logits = vec![100.0f32, 100.0, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0];
    let (indices, gates) = select_topk_experts_reference(&large_logits, 2);

    // Should select from the first 4 (all equal high values)
    // Tie-breaking: lower IDs first -> [0, 1]
    assert_eq!(indices, vec![0, 1]);

    // Gates should sum to 1.0
    let sum: f32 = gates.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5);

    // No NaN or Inf
    for &gate in &gates {
        assert!(gate.is_finite(), "Gate should be finite, got {}", gate);
    }
}

/// Test numerical stability with very negative logits.
#[test]
fn test_select_topk_experts_negative_logits() {
    let negative_logits = vec![-100.0f32, -50.0, -100.0, -100.0, -100.0, -100.0, -100.0, -100.0];
    let (indices, gates) = select_topk_experts_reference(&negative_logits, 2);

    // Expert 1 has highest (least negative) score
    assert_eq!(indices[0], 1);

    // Gates should be valid
    let sum: f32 = gates.iter().sum();
    assert!((sum - 1.0).abs() < 1e-5);

    for &gate in &gates {
        assert!(gate.is_finite(), "Gate should be finite");
        assert!(gate >= 0.0, "Gate should be non-negative");
    }
}
