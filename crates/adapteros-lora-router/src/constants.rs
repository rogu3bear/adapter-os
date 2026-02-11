//! Router configuration constants
//!
//! These constants define the default values for router configuration.
//! The k_sparse value is 4 (matching the schema default).

pub use adapteros_core::defaults::{
    DEFAULT_COMPRESSION_RATIO, DEFAULT_ENTROPY_FLOOR, DEFAULT_GATE_QUANT_STR, DEFAULT_K_SPARSE,
    DEFAULT_OVERHEAD_BUDGET_PCT, DEFAULT_SAMPLE_TOKENS_FULL, MAX_K, PINNED_BOOST,
};

// =============================================================================
// Scoring Multipliers
// =============================================================================

/// Multiplier for language affinity boost when adapter supports detected language.
/// Applied in `compute_adapter_feature_score` to increase score for matching languages.
pub const LANGUAGE_AFFINITY_MULTIPLIER: f32 = 2.0;

/// Multiplier for framework specialization boost.
/// Applied when adapter has framework specialization and frameworks are detected.
pub const FRAMEWORK_SPECIALIZATION_MULTIPLIER: f32 = 1.5;

// =============================================================================
// Adapter Tier Scoring Boosts
// =============================================================================

/// Score boost for tier_0 (highest priority) adapters.
pub const TIER_0_BOOST: f32 = 0.3;

/// Score boost for tier_1 adapters.
pub const TIER_1_BOOST: f32 = 0.2;

/// Score boost for tier_2 adapters.
pub const TIER_2_BOOST: f32 = 0.1;

// =============================================================================
// LoRA Tier Scoring Boosts
// =============================================================================

/// Score boost for "max" LoRA tier adapters.
pub const LORA_TIER_MAX_BOOST: f32 = 0.12;

/// Score boost for "standard" LoRA tier adapters.
pub const LORA_TIER_STANDARD_BOOST: f32 = 0.06;

/// Score boost for "micro" LoRA tier adapters (no boost).
pub const LORA_TIER_MICRO_BOOST: f32 = 0.0;

// =============================================================================
// Tie-Breaking and Numerical Precision
// =============================================================================

/// Relative epsilon for tie detection in score comparisons.
/// Scores within this relative tolerance (plus f32::EPSILON floor) are treated as ties.
/// This catches practical ties caused by floating-point drift.
pub const TIE_BREAK_RELATIVE_EPSILON: f32 = 1e-6;

// =============================================================================
// Adapter Limits
// =============================================================================

/// Maximum number of adapters supported in orthogonal constraint tracking.
/// Adapter indices >= this value are silently ignored in activation vectors.
pub const MAX_ADAPTERS: usize = 256;

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;
    use adapteros_core::defaults as core_defaults;

    #[test]
    fn router_defaults_match_core_defaults() {
        assert_eq!(DEFAULT_K_SPARSE, core_defaults::DEFAULT_K_SPARSE);
        assert_eq!(DEFAULT_ENTROPY_FLOOR, core_defaults::DEFAULT_ENTROPY_FLOOR);
        assert_eq!(
            DEFAULT_GATE_QUANT_STR,
            core_defaults::DEFAULT_GATE_QUANT_STR
        );
        assert_eq!(
            DEFAULT_SAMPLE_TOKENS_FULL,
            core_defaults::DEFAULT_SAMPLE_TOKENS_FULL
        );
        assert_eq!(
            DEFAULT_OVERHEAD_BUDGET_PCT,
            core_defaults::DEFAULT_OVERHEAD_BUDGET_PCT
        );
        assert_eq!(
            DEFAULT_COMPRESSION_RATIO,
            core_defaults::DEFAULT_COMPRESSION_RATIO
        );
        assert_eq!(MAX_K, core_defaults::MAX_K);
        assert_eq!(PINNED_BOOST, core_defaults::PINNED_BOOST);
    }

    #[test]
    fn tier_boosts_are_ordered_correctly() {
        // Higher tiers should have higher boosts
        assert!(
            TIER_0_BOOST > TIER_1_BOOST,
            "Tier 0 boost ({}) should be greater than Tier 1 boost ({})",
            TIER_0_BOOST,
            TIER_1_BOOST
        );
        assert!(
            TIER_1_BOOST > TIER_2_BOOST,
            "Tier 1 boost ({}) should be greater than Tier 2 boost ({})",
            TIER_1_BOOST,
            TIER_2_BOOST
        );
        assert!(
            TIER_2_BOOST > 0.0,
            "Tier 2 boost ({}) should be positive",
            TIER_2_BOOST
        );
    }

    #[test]
    fn lora_tier_boosts_are_ordered_correctly() {
        // Higher capacity LoRA should have higher boosts
        assert!(
            LORA_TIER_MAX_BOOST > LORA_TIER_STANDARD_BOOST,
            "Max LoRA boost ({}) should be greater than Standard ({})",
            LORA_TIER_MAX_BOOST,
            LORA_TIER_STANDARD_BOOST
        );
        assert!(
            LORA_TIER_STANDARD_BOOST > LORA_TIER_MICRO_BOOST,
            "Standard LoRA boost ({}) should be greater than Micro ({})",
            LORA_TIER_STANDARD_BOOST,
            LORA_TIER_MICRO_BOOST
        );
        assert!(
            LORA_TIER_MICRO_BOOST >= 0.0,
            "Micro LoRA boost ({}) should be non-negative",
            LORA_TIER_MICRO_BOOST
        );
    }

    #[test]
    fn scoring_multipliers_are_positive() {
        assert!(
            LANGUAGE_AFFINITY_MULTIPLIER > 0.0,
            "Language affinity multiplier should be positive"
        );
        assert!(
            FRAMEWORK_SPECIALIZATION_MULTIPLIER > 0.0,
            "Framework specialization multiplier should be positive"
        );
    }

    #[test]
    fn tie_break_epsilon_is_small_but_positive() {
        assert!(
            TIE_BREAK_RELATIVE_EPSILON > 0.0,
            "Tie break epsilon should be positive"
        );
        assert!(
            TIE_BREAK_RELATIVE_EPSILON < 1e-3,
            "Tie break epsilon should be small (< 1e-3)"
        );
    }
}
