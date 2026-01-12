//! Deterministic jitter utilities for retry and rate limiting.
//!
//! This module provides jitter functions that respect the global determinism
//! configuration. When determinism is enabled, jitter is computed using the
//! configured seed; otherwise, it uses wall-clock-derived randomness.
//!
//! # Determinism Guarantee
//!
//! When [`DeterminismConfig::fixed_seed`] is set, all jitter operations produce
//! reproducible values. This enables replay of retry sequences during debugging.
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_core::jitter::compute_jitter_delay;
//!
//! let base_delay_ms = 100;
//! let jitter_factor = 0.2; // ±20% jitter
//! let jittered_delay = compute_jitter_delay(base_delay_ms, jitter_factor);
//! ```

use crate::seed::get_deterministic_rng;

/// Compute a delay with jitter applied, respecting determinism configuration.
///
/// Uses the global determinism config to produce reproducible jitter when
/// a fixed seed is configured, or random jitter otherwise.
///
/// # Arguments
/// * `base_delay_ms` - The base delay in milliseconds
/// * `jitter_factor` - The jitter range as a fraction (0.0 to 1.0)
///   A factor of 0.2 means ±20% of base_delay
///
/// # Returns
/// The jittered delay, guaranteed to be at least 1ms.
///
/// # Example
/// ```ignore
/// let delay = compute_jitter_delay(100, 0.2);
/// // Returns a value between 80 and 120 (approximately)
/// ```
pub fn compute_jitter_delay(base_delay_ms: u64, jitter_factor: f64) -> u64 {
    if jitter_factor <= 0.0 || base_delay_ms == 0 {
        return base_delay_ms.max(1);
    }

    let mut rng = get_deterministic_rng();
    let jitter_range = base_delay_ms as f64 * jitter_factor;

    // Generate a value in [-1.0, 1.0) range
    // fastrand::f64() returns [0.0, 1.0), so transform to [-1.0, 1.0)
    let random_factor = (rng.f64() - 0.5) * 2.0;
    let jitter = random_factor * jitter_range;

    ((base_delay_ms as f64 + jitter).max(1.0)) as u64
}

/// Compute exponential backoff delay with jitter.
///
/// Combines exponential backoff with jitter for thundering herd prevention.
///
/// # Arguments
/// * `base_delay_ms` - The initial delay in milliseconds
/// * `attempt` - The attempt number (1-indexed)
/// * `max_delay_ms` - Maximum delay cap
/// * `jitter_factor` - Jitter range as a fraction (0.0 to 1.0)
///
/// # Returns
/// The computed delay with exponential backoff and jitter applied.
pub fn compute_backoff_with_jitter(
    base_delay_ms: u64,
    attempt: u32,
    max_delay_ms: u64,
    jitter_factor: f64,
) -> u64 {
    // Calculate exponential backoff: base * 2^(attempt-1)
    let exponent = attempt.saturating_sub(1);
    let multiplier = 2u64.saturating_pow(exponent);
    let delay = base_delay_ms.saturating_mul(multiplier).min(max_delay_ms);

    compute_jitter_delay(delay, jitter_factor)
}

/// Check a probability condition deterministically.
///
/// Returns true with approximately `probability` chance, using the deterministic
/// RNG when configured.
///
/// # Arguments
/// * `probability` - Probability of returning true (0.0 to 1.0)
///
/// # Example
/// ```ignore
/// // Returns true approximately 5% of the time
/// if check_probability(0.05) {
///     do_something();
/// }
/// ```
pub fn check_probability(probability: f32) -> bool {
    if probability <= 0.0 {
        return false;
    }
    if probability >= 1.0 {
        return true;
    }

    let mut rng = get_deterministic_rng();
    rng.f32() < probability
}

/// Compute a deterministic probability check based on a stable identifier.
///
/// This function produces the same result for the same identifier, making it
/// suitable for consistent sampling decisions that must be reproducible.
///
/// # Arguments
/// * `identifier` - A stable identifier (e.g., request ID, signal ID)
/// * `probability` - Target probability (0.0 to 1.0)
///
/// # Returns
/// True if the identifier's hash falls within the probability threshold.
///
/// # Example
/// ```ignore
/// // Consistent 5% sampling based on signal ID
/// if check_probability_by_id(signal_id.as_bytes(), 0.05) {
///     discover_contact();
/// }
/// ```
pub fn check_probability_by_id(identifier: &[u8], probability: f32) -> bool {
    if probability <= 0.0 {
        return false;
    }
    if probability >= 1.0 {
        return true;
    }

    // Use BLAKE3 hash of identifier for consistent, uniform distribution
    let hash = blake3::hash(identifier);
    let bytes = hash.as_bytes();

    // Use first byte for probability check (256 buckets)
    // This gives ~0.4% granularity which is sufficient for most use cases
    let threshold = (probability * 256.0) as u8;
    bytes[0] < threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::{with_determinism_config, DeterminismConfig};

    #[test]
    fn test_compute_jitter_delay_zero_factor() {
        let delay = compute_jitter_delay(100, 0.0);
        assert_eq!(delay, 100);
    }

    #[test]
    fn test_compute_jitter_delay_zero_base() {
        let delay = compute_jitter_delay(0, 0.2);
        assert_eq!(delay, 1); // Minimum is 1ms
    }

    #[test]
    fn test_compute_jitter_delay_range() {
        // With 20% jitter on 100ms base, expect values in [80, 120]
        for _ in 0..100 {
            let delay = compute_jitter_delay(100, 0.2);
            assert!((80..=120).contains(&delay), "delay {} out of range", delay);
        }
    }

    #[test]
    fn test_compute_jitter_delay_deterministic() {
        let delay1 = with_determinism_config(DeterminismConfig::fully_deterministic(), || {
            compute_jitter_delay(100, 0.2)
        });
        let delay2 = with_determinism_config(DeterminismConfig::fully_deterministic(), || {
            compute_jitter_delay(100, 0.2)
        });

        assert_eq!(
            delay1, delay2,
            "Deterministic jitter should be reproducible"
        );
    }

    #[test]
    fn test_compute_backoff_with_jitter() {
        let base = 100;
        let max = 1000;
        let jitter = 0.0001; // Minimal jitter for predictable tests

        // Attempt 1: 100 * 2^0 = 100
        let delay1 = compute_backoff_with_jitter(base, 1, max, jitter);
        assert!((99..=101).contains(&delay1));

        // Attempt 3: 100 * 2^2 = 400
        let delay3 = compute_backoff_with_jitter(base, 3, max, jitter);
        assert!((399..=401).contains(&delay3));

        // Attempt 10: capped at max
        let delay10 = compute_backoff_with_jitter(base, 10, max, jitter);
        assert!(delay10 <= 1001);
    }

    #[test]
    fn test_check_probability_bounds() {
        // Zero probability always returns false
        for _ in 0..100 {
            assert!(!check_probability(0.0));
        }

        // 100% probability always returns true
        for _ in 0..100 {
            assert!(check_probability(1.0));
        }
    }

    #[test]
    fn test_check_probability_by_id_consistent() {
        let id = b"test-signal-12345";

        // Same ID should produce same result
        let result1 = check_probability_by_id(id, 0.5);
        let result2 = check_probability_by_id(id, 0.5);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_check_probability_by_id_distribution() {
        // With 50% probability, roughly half of distinct IDs should pass
        let mut passed = 0;
        for i in 0..1000 {
            let id = format!("id-{}", i);
            if check_probability_by_id(id.as_bytes(), 0.5) {
                passed += 1;
            }
        }

        // Allow 10% tolerance for statistical variance
        assert!(
            (400..=600).contains(&passed),
            "Expected ~500 to pass, got {}",
            passed
        );
    }

    #[test]
    fn test_never_returns_zero_delay() {
        // Even with maximum jitter, delay should never be 0
        for _ in 0..100 {
            let delay = compute_jitter_delay(1, 1.0);
            assert!(delay >= 1, "Delay must be at least 1ms");
        }
    }
}
