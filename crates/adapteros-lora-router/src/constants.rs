//! Router configuration constants
//!
//! These constants define the default values for router configuration.
//! The k_sparse value is 4 (matching the schema default).

pub use adapteros_core::defaults::{
    DEFAULT_COMPRESSION_RATIO, DEFAULT_ENTROPY_FLOOR, DEFAULT_GATE_QUANT_STR, DEFAULT_K_SPARSE,
    DEFAULT_OVERHEAD_BUDGET_PCT, DEFAULT_SAMPLE_TOKENS_FULL, MAX_K, PINNED_BOOST,
};

#[cfg(test)]
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
}
