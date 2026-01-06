//! Tests for CHAT-PIN-02: Pinned Adapter Prior Boost
//!
//! Verifies that PINNED_BOOST constant is correctly defined and documented.
//! The boost is applied in the worker when constructing priors, but the
//! constant lives in the router crate for centralized configuration.
//!
//! Related tests:
//! - DB layer: adapteros-db/tests/chat_sessions_tests.rs (pinned adapter inheritance)
//! - Worker layer: applies PINNED_BOOST when building priors

use adapteros_lora_router::constants::PINNED_BOOST;

/// Verify PINNED_BOOST constant has the expected value of 0.3
///
/// This value was chosen to:
/// - Create preference, not exclusivity (pinned adapters are more likely to be selected)
/// - Allow non-pinned adapters to still win with sufficiently high feature scores
/// - Be deterministic (constant value, not RNG-based)
#[test]
fn test_pinned_boost_constant_value() {
    assert!(
        (PINNED_BOOST - 0.3).abs() < f32::EPSILON,
        "PINNED_BOOST should be exactly 0.3, got {}",
        PINNED_BOOST
    );
}

/// Verify PINNED_BOOST is a positive value that creates preference
#[test]
fn test_pinned_boost_is_positive() {
    const { assert!(PINNED_BOOST > 0.0) };
}

/// Verify PINNED_BOOST is not too large (would override routing decisions)
/// A boost > 1.0 would dominate baseline priors of 1.0
#[test]
fn test_pinned_boost_is_reasonable() {
    const { assert!(PINNED_BOOST < 1.0) };
}

/// Document the expected behavior of pinned boost in routing
///
/// When priors are constructed:
/// - Baseline prior for all adapters: 1.0
/// - Pinned adapter prior: 1.0 + PINNED_BOOST = 1.3
///
/// This means pinned adapters have 30% higher baseline probability,
/// but feature scores can still override this preference.
#[test]
fn test_pinned_boost_expected_prior_value() {
    let baseline_prior = 1.0f32;
    let boosted_prior = baseline_prior + PINNED_BOOST;

    assert!(
        (boosted_prior - 1.3).abs() < f32::EPSILON,
        "Boosted prior should be 1.3, got {}",
        boosted_prior
    );

    // Boosted is 30% higher than baseline
    let ratio = boosted_prior / baseline_prior;
    assert!(
        (ratio - 1.3).abs() < f32::EPSILON,
        "Boost should increase prior by 30%, got {}x",
        ratio
    );
}
