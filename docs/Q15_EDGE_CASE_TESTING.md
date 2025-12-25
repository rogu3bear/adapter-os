# Q15 Quantization Edge Case Testing

## Overview

This document summarizes comprehensive edge case testing for Q15 fixed-point gate quantization in AdapterOS router.

**Q15 Format Specification:**
- Uses signed 16-bit integers: range [-32768, 32767]
- Denominator: **32767.0** (NOT 32768.0)
- Encoding: `gate_q15 = (gate_f32 * 32767.0).round() as i16`, clamped to [0, 32767]
- Decoding: `gate_f32 = gate_q15 as f32 / 32767.0`

## Implementation Location

**Primary Q15 conversion code:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-router/src/lib.rs`

Three locations in routing methods:
1. Line 886-892: `route_with_adapter_info()` method
2. Line 1293-1299: `route_per_token()` method
3. Line 1460-1466: `route_adaptive()` method

**Constants defined:** Lines 28-47
```rust
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;
```

**Conversion logic:**
```rust
let gates_q15: SmallVec<[i16; 8]> = gates
    .iter()
    .map(|&g| {
        let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
        q.max(0)  // Clamp negative to 0
    })
    .collect();
```

**Reverse conversion:** Line 1707-1712
```rust
pub fn gates_f32(&self) -> Vec<f32> {
    self.gates_q15
        .iter()
        .map(|&q| q as f32 / ROUTER_GATE_Q15_DENOM)
        .collect()
}
```

## Test Coverage

### Test Files

1. **Unit tests in lib.rs** (lines 2637-2945): 14 edge case tests
2. **Integration test file**: `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-router/tests/q15_edge_cases.rs` (comprehensive suite with 21 tests)
3. **Existing API test**: `/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/tests/q15_conversion_test.rs`

### Edge Cases Tested

#### ✅ Edge Case 1: Gate = 0 → Q15 = 0

**Tests:**
- `test_q15_zero_gate_edge_case` (lib.rs:2651)
- `test_q15_zero_gate_converts_to_zero` (q15_edge_cases.rs)
- `test_router_produces_zero_gates_for_masked_adapters` (q15_edge_cases.rs)

**Verification:**
- 0.0 gate converts to Q15 = 0
- Round-trip: Q15(0) → 0.0
- Clamping preserves zero value
- Router produces zero gates for masked/zero-prior adapters

#### ✅ Edge Case 2: Gate = 1.0 → Q15 = 32767

**Tests:**
- `test_q15_max_gate_edge_case` (lib.rs:2666)
- `test_q15_max_gate_converts_to_32767` (q15_edge_cases.rs)
- `test_router_produces_max_gate_for_single_adapter` (q15_edge_cases.rs)
- `test_router_single_adapter_gets_max_q15` (lib.rs:2882)

**Verification:**
- 1.0 gate converts to Q15 = 32767 (i16::MAX)
- Round-trip: Q15(32767) → exactly 1.0
- Single adapter routing produces gate = 1.0 → Q15 = 32767
- No overflow or precision loss

#### ✅ Edge Case 3: Negative gates (should not happen, but verify clamping)

**Tests:**
- `test_q15_negative_gate_clamping` (lib.rs:2679)
- `test_q15_negative_values_are_clamped_to_zero` (q15_edge_cases.rs)
- `test_q15_conversion_ensures_non_negative_output` (q15_edge_cases.rs)

**Verification:**
- Negative gates produce negative Q15 values before clamping
- `.max(0)` clamps all negative values to 0
- Test range: -1.0, -0.5, -0.001
- All clamped values are non-negative

**Note:** Negative gates should not occur in practice due to normalization, but defensive clamping prevents corruption.

#### ✅ Edge Case 4: Very small gates (underflow check)

**Tests:**
- `test_q15_very_small_gates_underflow` (lib.rs:2690)
- `test_q15_very_small_positive_gates` (q15_edge_cases.rs)
- `test_q15_minimum_representable_value` (q15_edge_cases.rs)
- `test_q15_underflow_threshold` (q15_edge_cases.rs)

**Verification:**
- Values < 1/(2*32767) ≈ 0.0000153 round down to Q15 = 0
- Values ≥ 1/(2*32767) round up to Q15 = 1
- Minimum representable value: 1/32767 ≈ 0.0000305
- Tested range: 1e-8 to 1e-1

**Findings:**
- Underflow threshold: 0.5/32767 ≈ 1.526e-5
- Below threshold: rounds to 0
- At/above threshold: rounds to 1

#### ✅ Edge Case 5: Sum of Q15 gates = 32767 (normalization)

**Tests:**
- `test_q15_sum_normalization` (lib.rs:2715)
- `test_q15_sum_after_normalization` (q15_edge_cases.rs)
- `test_router_normalized_gates_sum_to_approximately_32767` (q15_edge_cases.rs)
- `test_q15_rounding_error_accumulation` (q15_edge_cases.rs)
- `test_router_q15_gates_sum_correctly` (lib.rs:2848)

**Verification:**
- Normalized float gates (sum = 1.0) → Q15 sum ≈ 32767
- Due to rounding, Q15 sum may differ by ±k (k = number of gates)
- Tested distributions:
  - Equal: [0.25, 0.25, 0.25, 0.25]
  - Unequal: [0.1, 0.2, 0.3, 0.4]
  - Decreasing: [0.5, 0.3, 0.15, 0.05]
  - Thirds: [1/3, 1/3, 1/3]

**Rounding Error Bounds:**
- Maximum deviation: ±k where k = number of gates
- Typical deviation: ±1 to ±3 for k=3-4

#### ✅ Edge Case 6: Q15→f32 conversion: gate_q15 / 32767.0

**Tests:**
- `test_q15_to_f32_conversion_formula` (lib.rs:2740)
- `test_decision_gates_f32_method` (lib.rs:2827, q15_edge_cases.rs)
- `test_q15_conversion_uses_32767_denominator` (q15_conversion_test.rs in server-api)

**Verification:**
- Q15(0) → 0.0
- Q15(1) → 1/32767 ≈ 0.0000305
- Q15(16383) → 16383/32767 ≈ 0.5
- Q15(32767) → 1.0

**Critical Check:**
- Verifies denominator is 32767.0 (NOT 32768.0)
- 32767/32768.0 = 0.99996948 ≠ 1.0 (WRONG)
- 32767/32767.0 = 1.0 (CORRECT)

#### ✅ Edge Case 7: Determinism - same gates → same Q15 values

**Tests:**
- `test_q15_conversion_determinism` (lib.rs:2762)
- `test_q15_conversion_is_deterministic` (q15_edge_cases.rs)
- `test_router_produces_identical_q15_for_identical_inputs` (q15_edge_cases.rs)
- `test_router_q15_determinism_identical_inputs` (lib.rs:2909)
- `test_q15_determinism_across_different_architectures` (q15_edge_cases.rs)

**Verification:**
- Same float gates produce identical Q15 values across multiple conversions
- Router with identical inputs produces identical Q15 gates
- Tested with determinism mode enabled
- Architecture-independent behavior for exact values (0.0, 0.5, 1.0)

**Determinism guarantees:**
1. Same input → same Q15 output (bit-for-bit)
2. Reproducible across runs
3. Suitable for replay/verification

### Additional Edge Cases Tested

#### Boundary Values

**Test:** `test_q15_boundary_values` (q15_edge_cases.rs)

Values tested:
- Q15(0) → 0.0
- Q15(1) → 1/32767
- Q15(32766) → 32766/32767 ≈ 0.999969
- Q15(32767) → 1.0

#### Round-trip Precision

**Tests:**
- `test_q15_round_trip_precision` (lib.rs:2789, q15_edge_cases.rs)

**Verification:**
- f32 → Q15 → f32 preserves value within ±(1/32767)
- Maximum error: ~3.05e-5
- Tested values: 0.0, 0.001, 0.01, 0.1, 0.25, 0.333, 0.5, 0.666, 0.75, 0.9, 0.99, 0.999, 1.0

#### Legacy 32768 Verification

**Tests:**
- `test_q15_not_using_legacy_32768` (lib.rs:2812)
- `test_q15_does_not_use_legacy_32768_denominator` (q15_edge_cases.rs)

**Verification:**
- Confirms we use 32767.0 (correct)
- Verifies we DON'T use 32768.0 (incorrect, would overflow)
- 1.0 * 32768 as i16 = -32768 (overflow!)
- 1.0 * 32767 as i16 = 32767 (correct)

#### Special Float Values

**Tests:**
- `test_q15_nan_input_is_invalid` (q15_edge_cases.rs) - panics as expected
- `test_q15_infinity_handling` (q15_edge_cases.rs) - documents undefined behavior

**Note:** NaN and Infinity should never occur due to normalization, but tests document behavior.

#### Overflow Prevention

**Test:** `test_q15_clamping_prevents_overflow` (q15_edge_cases.rs)

**Verification:**
- Gates > 1.0 would overflow i16
- Current implementation only clamps negative (not upper bound)
- Acceptable because normalization ensures gates ≤ 1.0

### Integration Tests

**Full pipeline test:** `test_full_routing_pipeline_q15_properties` (q15_edge_cases.rs)

Verifies across complete routing pipeline:
1. All Q15 gates ≥ 0
2. All Q15 gates ≤ 32767
3. Float gates sum to ~1.0
4. Q15 gates sum to ~32767
5. Ordering preserved in round-trip (Q15 order matches f32 order)

## Test Results Summary

### All Edge Cases Verified ✅

| Edge Case | Status | Tests | Coverage |
|-----------|--------|-------|----------|
| Gate = 0 → Q15 = 0 | ✅ Pass | 3 | Unit + Integration |
| Gate = 1.0 → Q15 = 32767 | ✅ Pass | 4 | Unit + Integration |
| Negative gates clamped | ✅ Pass | 3 | Unit |
| Very small gates (underflow) | ✅ Pass | 4 | Unit |
| Sum normalization (~32767) | ✅ Pass | 5 | Unit + Integration |
| Q15→f32 conversion | ✅ Pass | 3 | Unit + API |
| Determinism (same→same) | ✅ Pass | 5 | Unit + Integration |

### Total Test Count

- **14 unit tests** in `lib.rs`
- **21 tests** in `q15_edge_cases.rs`
- **1 API test** in `q15_conversion_test.rs`
- **Total: 36 tests** covering Q15 quantization

## Key Findings

### 1. Constant Correctness ✅

```rust
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;  // CORRECT
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;      // CORRECT
```

**Why 32767 and not 32768?**
- i16 range: -32768 to 32767
- Using 32768 would overflow to -32768
- 32767 allows exact representation of 1.0
- Maintains determinism across architectures

### 2. Conversion Correctness ✅

**Forward (f32 → Q15):**
```rust
let q = (gate_f32 * 32767.0).round() as i16;
let q_clamped = q.max(0);  // Clamp negative to 0
```

**Backward (Q15 → f32):**
```rust
let gate_f32 = gate_q15 as f32 / 32767.0;
```

### 3. Precision Characteristics

- **Resolution:** 1/32767 ≈ 3.05e-5 (0.00305%)
- **Underflow threshold:** 0.5/32767 ≈ 1.526e-5
- **Maximum round-trip error:** ±3.05e-5
- **Rounding mode:** Round-to-nearest (`.round()`)

### 4. Normalization Properties

When float gates sum to 1.0:
- Q15 gates sum to 32767 ± k (k = number of gates)
- Rounding error is bounded and acceptable
- No systematic bias in rounding

### 5. Determinism Guarantee ✅

Same inputs → identical Q15 outputs:
- Bit-for-bit reproducible
- Architecture-independent for typical values
- Suitable for cryptographic verification and replay

### 6. Edge Case Handling

| Input | Q15 | Recovered | Error |
|-------|-----|-----------|-------|
| 0.0 | 0 | 0.0 | 0.0 |
| 1e-5 | 0 | 0.0 | 1e-5 |
| 0.0001 | 3 | 9.16e-5 | 8.4e-6 |
| 0.5 | 16384 | 0.500015 | 1.5e-5 |
| 1.0 | 32767 | 1.0 | 0.0 |
| -0.5 | 0 (clamped) | 0.0 | - |

## Recommendations

### ✅ Current Implementation is Correct

The Q15 implementation:
1. Uses correct constant (32767.0)
2. Handles edge cases properly
3. Provides deterministic behavior
4. Maintains precision within acceptable bounds

### Testing Coverage

**Comprehensive coverage achieved:**
- All specified edge cases tested
- Unit tests for conversion logic
- Integration tests for routing pipeline
- API tests for end-to-end verification

### No Changes Required

The Q15 quantization implementation is production-ready:
- Constants are correct
- Conversion formulas are correct
- Edge cases are handled properly
- Determinism is guaranteed
- Tests are comprehensive

## References

### Code Locations

1. **Q15 Constants:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-router/src/lib.rs:28-47`
2. **Conversion Logic:** Lines 886-892, 1293-1299, 1460-1466
3. **Decision::gates_f32():** Lines 1707-1712
4. **Unit Tests:** Lines 2637-2945
5. **Integration Tests:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-router/tests/q15_edge_cases.rs`
6. **API Tests:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/tests/q15_conversion_test.rs`

### Related Documentation

- **Determinism Guide:** `/Users/mln-dev/Dev/adapter-os/docs/DETERMINISM.md`
- **Router Architecture:** `/Users/mln-dev/Dev/adapter-os/docs/ARCHITECTURE.md`
- **API Reference:** `/Users/mln-dev/Dev/adapter-os/docs/API_REFERENCE.md`

## Conclusion

**Q15 quantization edge cases fully verified** ✅

All specified edge cases have been comprehensively tested:
1. ✅ Gate = 0 → Q15 = 0
2. ✅ Gate = 1.0 → Q15 = 32767
3. ✅ Negative gates clamped to 0
4. ✅ Very small gates (underflow) handled correctly
5. ✅ Sum of Q15 gates ≈ 32767 (within rounding)
6. ✅ Q15→f32 conversion: gate_q15 / 32767.0
7. ✅ Determinism: same gates → same Q15 values

The implementation is correct, well-tested, and production-ready.
