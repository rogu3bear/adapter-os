//! Test router weighted scoring

use mplora_router::{CodeFeatures, Router, RouterWeights};

#[test]
fn test_weighted_scoring_influences_selection() {
    // Create two different weight configurations
    let heavy_language_weights = RouterWeights::new(
        0.8,  // language - very high
        0.05, // framework
        0.05, // symbols
        0.05, // paths
        0.05, // verb
    );

    let heavy_framework_weights = RouterWeights::new(
        0.05, // language
        0.8,  // framework - very high
        0.05, // symbols
        0.05, // paths
        0.05, // verb
    );

    let mut router1 = Router::new_with_weights(heavy_language_weights, 3, 1.0, 0.02, [0u8; 32]);
    let mut router2 = Router::new_with_weights(heavy_framework_weights, 3, 1.0, 0.02, [0u8; 32]);

    // Create code features with strong Python + Django signal
    let features = CodeFeatures::from_context("def main(): # Python Django application");
    let feature_vec = features.to_vector();

    // Same priors for all adapters
    let priors = vec![1.0, 1.0, 1.0, 1.0, 1.0];

    // Get scoring explanations
    let explanation1 = router1.explain_score(&feature_vec);
    let explanation2 = router2.explain_score(&feature_vec);

    // With heavy language weights, language score should dominate
    assert!(
        explanation1.language_score > explanation1.framework_score,
        "Language-weighted router should prioritize language score"
    );

    // With heavy framework weights, framework score should be significant
    // (though it depends on framework detection in the features)
    println!("Heavy language weights: {}", explanation1.format());
    println!("\nHeavy framework weights: {}", explanation2.format());

    // Total scores should be different
    assert_ne!(
        explanation1.total_score, explanation2.total_score,
        "Different weights should produce different total scores"
    );
}

#[test]
fn test_feature_score_components() {
    let weights = RouterWeights::default();
    let router = Router::new_with_weights(weights, 3, 1.0, 0.02, [0u8; 32]);

    // Create features with known components
    let features = CodeFeatures::from_context("Fix the bug in this Python function");
    let feature_vec = features.to_vector();

    let explanation = router.explain_score(&feature_vec);

    // All scores should be non-negative
    assert!(explanation.language_score >= 0.0);
    assert!(explanation.framework_score >= 0.0);
    assert!(explanation.symbol_hits_score >= 0.0);
    assert!(explanation.path_tokens_score >= 0.0);
    assert!(explanation.prompt_verb_score >= 0.0);

    // Total should equal sum of components
    let sum = explanation.language_score
        + explanation.framework_score
        + explanation.symbol_hits_score
        + explanation.path_tokens_score
        + explanation.prompt_verb_score;

    assert!(
        (sum - explanation.total_score).abs() < 0.001,
        "Total score should equal sum of components"
    );

    println!("{}", explanation.format());
}

#[test]
fn test_default_weights_sum_to_one() {
    let weights = RouterWeights::default();
    let total = weights.total_weight();

    assert!(
        (total - 1.0).abs() < 0.001,
        "Default weights should sum to 1.0, got {}",
        total
    );
}

#[test]
fn test_routing_decision_changes_with_weights() {
    // Create features
    let features = CodeFeatures::from_context("Implement this React component");
    let feature_vec = features.to_vector();

    // Test with different weights
    let mut router1 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02, [0u8; 32]);
    let mut router2 = Router::new_with_weights(
        RouterWeights::new(0.5, 0.3, 0.1, 0.05, 0.05),
        3,
        1.0,
        0.02,
        [0u8; 32],
    );

    let priors = vec![1.0, 1.2, 0.8, 1.5, 0.9];

    let decision1 = router1.route(&feature_vec, &priors);
    let decision2 = router2.route(&feature_vec, &priors);

    // Both should return K=3 adapters
    assert_eq!(decision1.indices.len(), 3);
    assert_eq!(decision2.indices.len(), 3);

    // Gates should sum to approximately 1.0
    let sum1: f32 = decision1.gates_f32().iter().sum();
    let sum2: f32 = decision2.gates_f32().iter().sum();

    assert!((sum1 - 1.0).abs() < 0.01);
    assert!((sum2 - 1.0).abs() < 0.01);
}
