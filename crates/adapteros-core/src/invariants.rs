//! Invariant validation for determinism-critical operations.
//!
//! This module provides explicit validation functions for the 5 critical invariants
//! that must hold for deterministic inference:
//!
//! 1. **HKDF Seed**: 32 bytes, algorithm version 2
//! 2. **Q15 Denominator**: 32767.0 for gate quantization
//! 3. **Router Tie-Breaking**: Score DESC, Index ASC
//! 4. **Dual-Write Consistency**: SQL and KV must match
//! 5. **Backend Capability**: Selected backend must match hardware
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_core::invariants::{validate_seed_bytes, HKDF_OUTPUT_LENGTH};
//!
//! let seed = derive_seed(&global, "label");
//! validate_seed_bytes(&seed)?; // Validates length is 32
//! ```
//!
//! # Audit Logging
//!
//! When `AOS_DEBUG_DETERMINISM=1` is set, invariant checks emit structured traces.

use crate::{AosError, Result};

// Re-export seed constants for validation
pub use crate::seed::HKDF_OUTPUT_LENGTH;
pub use crate::version::HKDF_ALGORITHM_VERSION;

/// Validates that a seed buffer meets HKDF invariant requirements.
///
/// # Requirements
/// - Seed must be exactly [`HKDF_OUTPUT_LENGTH`] (32) bytes
/// - Empty seeds are rejected
///
/// # Errors
/// Returns `AosError::Validation` if the seed length is incorrect.
///
/// # Example
/// ```rust,ignore
/// use adapteros_core::invariants::validate_seed_bytes;
///
/// let seed = [0u8; 32];
/// validate_seed_bytes(&seed)?; // Ok
///
/// let short_seed = [0u8; 16];
/// validate_seed_bytes(&short_seed)?; // Err: length mismatch
/// ```
#[inline]
pub fn validate_seed_bytes(seed: &[u8]) -> Result<()> {
    if seed.is_empty() {
        return Err(AosError::Validation(
            "Seed buffer cannot be empty (HKDF invariant)".to_string(),
        ));
    }

    if seed.len() != HKDF_OUTPUT_LENGTH {
        return Err(AosError::Validation(format!(
            "Seed length {} != required {} (HKDF invariant violated)",
            seed.len(),
            HKDF_OUTPUT_LENGTH
        )));
    }

    // Audit log if determinism debugging enabled
    if std::env::var("AOS_DEBUG_DETERMINISM").ok().map_or(false, |v| matches!(v.as_str(), "1" | "true" | "yes")) {
        let checksum = &seed[..4];
        tracing::debug!(
            seed_len = seed.len(),
            seed_checksum = %format!("{:02x}{:02x}{:02x}{:02x}", checksum[0], checksum[1], checksum[2], checksum[3]),
            "HKDF seed invariant validated"
        );
    }

    Ok(())
}

/// Validates that a seed buffer is at least the expected length, with warning for shorter seeds.
///
/// This is a softer check for contexts where truncation is acceptable but should be logged.
/// Prefer [`validate_seed_bytes`] for strict validation.
#[inline]
pub fn validate_seed_bytes_soft(seed: &[u8], context: &str) -> Result<()> {
    if seed.is_empty() {
        return Err(AosError::Validation(
            "Seed buffer cannot be empty".to_string(),
        ));
    }

    if seed.len() < HKDF_OUTPUT_LENGTH {
        tracing::warn!(
            seed_len = seed.len(),
            expected_len = HKDF_OUTPUT_LENGTH,
            context = context,
            "Seed shorter than HKDF_OUTPUT_LENGTH, will be extended"
        );
    }

    Ok(())
}

/// Compile-time assertion that Q15 denominator is 32767.0
///
/// This constant is used across the codebase for gate quantization.
/// Changing it would break determinism proofs and replay verification.
pub const Q15_GATE_DENOMINATOR: f32 = 32767.0;

// Compile-time assertion for Q15 denominator (determinism-critical)
// Note: We can't import from adapteros-lora-router here due to dependency order,
// but we validate our own constant matches the expected bit pattern.
const _: () = assert!(
    Q15_GATE_DENOMINATOR.to_bits() == 32767.0_f32.to_bits(),
    "Q15 denominator must be 32767.0 for determinism"
);

/// Decodes a Q15-quantized gate value to f32.
///
/// Uses the invariant denominator (32767.0) to ensure consistent reconstruction.
#[inline]
pub fn decode_q15_gate(gate_q15: i16) -> f32 {
    gate_q15 as f32 / Q15_GATE_DENOMINATOR
}

/// Encodes an f32 gate value to Q15 format.
///
/// Uses the invariant denominator (32767.0) to ensure consistent quantization.
#[inline]
pub fn encode_q15_gate(gate: f32) -> i16 {
    let scaled = (gate * Q15_GATE_DENOMINATOR).round() as i32;
    scaled.clamp(0, Q15_GATE_DENOMINATOR as i32) as i16
}

/// Canonical comparison for deterministic adapter sorting.
///
/// Implements the invariant: Score DESC, then Index ASC for tie-breaking.
/// Uses IEEE 754 total ordering to handle NaN deterministically.
///
/// # Usage
/// ```rust,ignore
/// scores.sort_by(canonical_score_comparator);
/// ```
#[inline]
pub fn canonical_score_comparator(a: &(usize, f32), b: &(usize, f32)) -> std::cmp::Ordering {
    // Primary: Score DESC (higher scores first)
    // Uses total_cmp for IEEE 754 total ordering (handles NaN deterministically)
    let score_cmp = b.1.total_cmp(&a.1);

    if score_cmp == std::cmp::Ordering::Equal {
        // Tie-break: Index ASC (lower indices first for determinism)
        a.0.cmp(&b.0)
    } else {
        score_cmp
    }
}

/// Sorts adapter scores using the canonical deterministic ordering.
///
/// # Invariant
/// - Primary sort: Score DESC (higher scores first)
/// - Tie-break: Index ASC (lower indices first)
///
/// This function MUST be used for all adapter sorting to maintain determinism.
#[inline]
pub fn canonical_adapter_sort(scores: &mut [(usize, f32)]) {
    scores.sort_by(canonical_score_comparator);

    // Verify ordering is correct (O(n) after O(n log n) sort — always worth it)
    for i in 1..scores.len() {
        let prev = &scores[i - 1];
        let curr = &scores[i];
        // Previous score should be >= current score
        assert!(
            prev.1 >= curr.1 || prev.1.is_nan(),
            "canonical_adapter_sort: scores not DESC at index {}: {} > {}",
            i,
            curr.1,
            prev.1
        );
        // If scores are equal, previous index should be < current index
        if (prev.1 - curr.1).abs() < f32::EPSILON {
            assert!(
                prev.0 < curr.0,
                "canonical_adapter_sort: tie-break failed at index {}: idx {} >= {}",
                i,
                prev.0,
                curr.0
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_seed_bytes_correct_length() {
        let seed = [0u8; 32];
        assert!(validate_seed_bytes(&seed).is_ok());
    }

    #[test]
    fn test_validate_seed_bytes_empty() {
        let seed: [u8; 0] = [];
        let result = validate_seed_bytes(&seed);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_validate_seed_bytes_wrong_length() {
        let short = [0u8; 16];
        let result = validate_seed_bytes(&short);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("16"));

        let long = [0u8; 64];
        let result = validate_seed_bytes(&long);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("64"));
    }

    #[test]
    fn test_q15_encode_decode_roundtrip() {
        let original = 0.5f32;
        let encoded = encode_q15_gate(original);
        let decoded = decode_q15_gate(encoded);
        // Should be within Q15 precision
        assert!((original - decoded).abs() < 0.0001);
    }

    #[test]
    fn test_q15_denominator_is_32767() {
        assert_eq!(Q15_GATE_DENOMINATOR, 32767.0);
    }

    #[test]
    fn test_canonical_sort_score_descending() {
        let mut scores = vec![(0, 0.3), (1, 0.7), (2, 0.5)];
        canonical_adapter_sort(&mut scores);
        assert_eq!(scores, vec![(1, 0.7), (2, 0.5), (0, 0.3)]);
    }

    #[test]
    fn test_canonical_sort_tiebreak_ascending() {
        let mut scores = vec![(2, 0.5), (0, 0.5), (1, 0.5)];
        canonical_adapter_sort(&mut scores);
        // Equal scores should sort by index ASC
        assert_eq!(scores, vec![(0, 0.5), (1, 0.5), (2, 0.5)]);
    }

    #[test]
    fn test_canonical_sort_mixed() {
        let mut scores = vec![(0, 0.3), (1, 0.7), (2, 0.7), (3, 0.5)];
        canonical_adapter_sort(&mut scores);
        // 0.7 DESC, then index ASC for ties, then 0.5, then 0.3
        assert_eq!(scores, vec![(1, 0.7), (2, 0.7), (3, 0.5), (0, 0.3)]);
    }
}
