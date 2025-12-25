//! Q15 Denominator Invariant Tests (Determinism Compliance)
//!
//! These tests ensure the Q15 quantization constants remain stable,
//! protecting against accidental changes that would break replay determinism.
//!
//! ## Critical Invariant
//!
//! The Q15 denominator MUST be 32767.0 (not 32768.0). This value is baked into:
//! - Replay logs
//! - Golden baselines
//! - Cross-platform determinism guarantees
//!
//! Changing this value breaks all existing replays.

use adapteros_lora_router::{ROUTER_GATE_Q15_DENOM, ROUTER_GATE_Q15_MAX};

// ============================================================================
// CRITICAL INVARIANT TESTS
// ============================================================================

/// Critical invariant: Q15 denominator must be exactly 32767.0
///
/// # Why 32767.0 and not 32768.0?
///
/// - i16 range is [-32768, 32767]
/// - Using 32767 allows exact 1.0 representation: 32767 / 32767.0 = 1.0
/// - Using 32768 would mean max representable is 32767/32768 ≈ 0.99997
/// - 32768 as i16 overflows to -32768, causing corruption
#[test]
fn test_q15_denominator_invariant_32767() {
    // CRITICAL INVARIANT: This value must NEVER change
    const EXPECTED_DENOM: f32 = 32767.0;

    assert_eq!(
        ROUTER_GATE_Q15_DENOM, EXPECTED_DENOM,
        "CRITICAL INVARIANT VIOLATION: Q15 denominator must be exactly {}. \
         Got {}. Changing this breaks replay determinism and PRD compliance. \
         If you intentionally need to change this, update all golden baselines \
         and document the migration path.",
        EXPECTED_DENOM, ROUTER_GATE_Q15_DENOM
    );
}

/// Critical invariant: Q15 max must equal denominator for exact 1.0 representation
#[test]
fn test_q15_max_matches_denominator_invariant() {
    assert_eq!(
        ROUTER_GATE_Q15_MAX as f32,
        ROUTER_GATE_Q15_DENOM,
        "INVARIANT: Q15_MAX ({}) as f32 must equal Q15_DENOM ({}) \
         for exact 1.0 representation",
        ROUTER_GATE_Q15_MAX,
        ROUTER_GATE_Q15_DENOM
    );
}

/// Critical invariant: Q15 max must be i16::MAX
#[test]
fn test_q15_max_is_i16_max() {
    assert_eq!(
        ROUTER_GATE_Q15_MAX,
        i16::MAX,
        "INVARIANT: Q15_MAX must be i16::MAX (32767)"
    );
}

// ============================================================================
// GOLDEN ENCODING TESTS
// ============================================================================

/// Golden fixture: Q15 encoding formula must produce these exact values
///
/// These golden values are derived from the formula:
///   q15 = (gate_f32 * 32767.0).round() as i16
///
/// If any of these fail, the encoding formula has changed.
#[test]
fn test_q15_encoding_formula_golden() {
    // Golden test cases: (input_f32, expected_q15)
    let golden_cases: Vec<(f32, i16)> = vec![
        (0.0, 0),           // Zero
        (1.0, 32767),       // Maximum (exact)
        (0.5, 16384),       // Half: 0.5 * 32767 = 16383.5 → rounds to 16384
        (0.25, 8192),       // Quarter: 0.25 * 32767 = 8191.75 → rounds to 8192
        (0.75, 24575),      // Three-quarters: 0.75 * 32767 = 24575.25 → rounds to 24575
        (0.1, 3277),        // 0.1 * 32767 = 3276.7 → rounds to 3277
        (0.01, 328),        // 0.01 * 32767 = 327.67 → rounds to 328
        (0.001, 33),        // 0.001 * 32767 = 32.767 → rounds to 33
        (0.333, 10911),     // 1/3 approx: 0.333 * 32767 = 10911.411 → rounds to 10911
        (0.666, 21823),     // 2/3 approx: 0.666 * 32767 = 21822.822 → rounds to 21823
    ];

    for (input, expected) in golden_cases {
        let computed = (input * ROUTER_GATE_Q15_DENOM).round() as i16;
        assert_eq!(
            computed, expected,
            "GOLDEN: Q15({}) must be {}, got {}. \
             Encoding formula has changed!",
            input, expected, computed
        );
    }
}

/// Golden fixture: Q15 decoding formula must produce these exact values
///
/// These golden values are derived from the formula:
///   gate_f32 = q15 as f32 / 32767.0
#[test]
fn test_q15_decoding_formula_golden() {
    // Golden test cases: (input_q15, expected_f32)
    let golden_cases: Vec<(i16, f32)> = vec![
        (0, 0.0),
        (32767, 1.0),
        (16384, 16384.0 / 32767.0),  // ~0.50001526
        (8192, 8192.0 / 32767.0),    // ~0.25000763
    ];

    for (input, expected) in golden_cases {
        let computed = input as f32 / ROUTER_GATE_Q15_DENOM;
        assert!(
            (computed - expected).abs() < 1e-7,
            "GOLDEN: Q15_DECODE({}) must be ~{}, got {}",
            input, expected, computed
        );
    }
}

// ============================================================================
// REGRESSION TESTS
// ============================================================================

/// Regression test: Ensure we don't accidentally use 32768 denominator
///
/// Using 32768 would:
/// 1. Make max representable value 32767/32768 ≈ 0.99997 (not 1.0)
/// 2. Not allow exact 1.0 representation (32768 exceeds i16::MAX)
#[test]
fn test_q15_not_using_32768_regression() {
    // With correct 32767 denominator, 1.0 encodes to exactly 32767
    let gate_max = 1.0f32;
    let q15_correct = (gate_max * 32767.0).round() as i16;

    assert_eq!(q15_correct, 32767, "Correct encoding of 1.0 is 32767");

    // Verify recovery is exact with 32767 denominator
    let recovered_correct = q15_correct as f32 / 32767.0;
    assert_eq!(recovered_correct, 1.0, "32767 denominator gives exact 1.0 recovery");

    // With 32768 denominator, recovery would NOT be exact 1.0
    let recovered_if_32768 = 32767_f32 / 32768.0;
    assert!(
        (recovered_if_32768 - 1.0).abs() > 1e-6,
        "32768 denominator cannot represent 1.0 exactly: {}",
        recovered_if_32768
    );

    // The actual max representable with 32768 denominator is ~0.99997
    let max_representable_32768 = 32767.0 / 32768.0;
    assert!(
        max_representable_32768 < 1.0,
        "32768 denominator max is {} < 1.0",
        max_representable_32768
    );
}

// ============================================================================
// STABILITY TESTS
// ============================================================================

/// Stability test: Encoding is deterministic across 1000 iterations
#[test]
fn test_q15_encoding_stability() {
    let test_gates = vec![0.0f32, 0.1, 0.25, 0.333, 0.5, 0.666, 0.75, 0.9, 1.0];

    // Compute expected values once
    let expected: Vec<i16> = test_gates
        .iter()
        .map(|&g| (g * ROUTER_GATE_Q15_DENOM).round() as i16)
        .collect();

    // Verify 1000 iterations produce identical results
    for iteration in 0..1000 {
        for (i, &gate) in test_gates.iter().enumerate() {
            let computed = (gate * ROUTER_GATE_Q15_DENOM).round() as i16;
            assert_eq!(
                computed, expected[i],
                "Iteration {}: Q15({}) must be stable. Expected {}, got {}",
                iteration, gate, expected[i], computed
            );
        }
    }
}

/// Test round-trip precision for all representable Q15 values
#[test]
fn test_q15_round_trip_all_values() {
    // Test a sample of Q15 values across the range
    let test_values: Vec<i16> = vec![
        0, 1, 100, 1000, 10000, 16383, 16384, 20000, 30000, 32766, 32767,
    ];

    for &q15_value in &test_values {
        let gate_f32 = q15_value as f32 / ROUTER_GATE_Q15_DENOM;
        let q15_back = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;

        assert_eq!(
            q15_value, q15_back,
            "Q15 round-trip failed: {} -> {} -> {}",
            q15_value, gate_f32, q15_back
        );
    }
}
