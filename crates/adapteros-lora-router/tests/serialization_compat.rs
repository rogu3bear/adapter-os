//! Routing Struct Serialization Compatibility Tests (PRD-03)
//!
//! These tests verify that routing structures maintain backward-compatible
//! serialization across versions. Any change to struct fields or serialization
//! format will cause these tests to fail.
//!
//! ## Purpose
//!
//! Routing decisions are persisted for:
//! - Replay determinism (exact reproduction of routing decisions)
//! - Audit trails (decision hashing for compliance)
//! - Cross-version compatibility (old logs readable by new code)
//!
//! ## Golden Baseline Format
//!
//! Golden baselines are stored as JSON strings embedded in the tests.
//! Changes to struct fields, field ordering, or serialization format
//! will cause test failures, alerting developers to potential breaking changes.

use adapteros_lora_router::{DecisionHash, RouterDeterminismConfig, RouterWeights};

// ============================================================================
// ROUTER DETERMINISM CONFIG SERIALIZATION
// ============================================================================

/// Golden baseline: RouterDeterminismConfig serialization
///
/// Tests that RouterDeterminismConfig serializes to a known JSON format.
/// Any changes to field names, types, or ordering will fail this test.
#[test]
fn test_router_determinism_config_serialization_compat() {
    let config = RouterDeterminismConfig {
        ieee754_deterministic: true,
        enable_decision_hashing: true,
    };

    let json = serde_json::to_string(&config).expect("serialization should succeed");

    // GOLDEN BASELINE - Version: 1.0.0
    // Format: {"field_a":value,"field_b":value} (alphabetical order by serde default)
    const GOLDEN_JSON: &str = r#"{"ieee754_deterministic":true,"enable_decision_hashing":true}"#;

    assert_eq!(
        json, GOLDEN_JSON,
        "RouterDeterminismConfig serialization changed!\n\
         Expected: {}\n\
         Got:      {}\n\
         If this change is intentional, update GOLDEN_JSON and document migration.",
        GOLDEN_JSON, json
    );

    // Verify round-trip deserialization
    let deserialized: RouterDeterminismConfig =
        serde_json::from_str(&json).expect("deserialization should succeed");
    assert_eq!(
        deserialized.ieee754_deterministic, config.ieee754_deterministic,
        "ieee754_deterministic field mismatch after round-trip"
    );
    assert_eq!(
        deserialized.enable_decision_hashing, config.enable_decision_hashing,
        "enable_decision_hashing field mismatch after round-trip"
    );
}

/// Test default config serialization
#[test]
fn test_router_determinism_config_default_serialization() {
    let config = RouterDeterminismConfig::default();

    // Default should have both fields true
    let json = serde_json::to_string(&config).expect("serialization should succeed");

    // GOLDEN: Default config
    const GOLDEN_DEFAULT: &str = r#"{"ieee754_deterministic":true,"enable_decision_hashing":true}"#;

    assert_eq!(
        json, GOLDEN_DEFAULT,
        "Default RouterDeterminismConfig serialization changed"
    );
}

// ============================================================================
// DECISION HASH SERIALIZATION
// ============================================================================

/// Golden baseline: DecisionHash serialization
///
/// DecisionHash is critical for audit trails and determinism verification.
/// Its serialization format must remain stable.
#[test]
fn test_decision_hash_serialization_compat() {
    let hash = DecisionHash {
        input_hash: "abc123def456".to_string(),
        output_hash: "789xyz000111".to_string(),
        reasoning_hash: Some("reasoning_hash_value".to_string()),
        combined_hash: "combined_hash_value".to_string(),
        tau: 1.0,
        eps: 0.02,
        k: 3,
    };

    let json = serde_json::to_string(&hash).expect("serialization should succeed");

    // GOLDEN BASELINE - Version: 1.0.0
    // Note: Field order follows struct definition order (serde default)
    const GOLDEN_JSON: &str = r#"{"input_hash":"abc123def456","output_hash":"789xyz000111","reasoning_hash":"reasoning_hash_value","combined_hash":"combined_hash_value","tau":1.0,"eps":0.02,"k":3}"#;

    assert_eq!(
        json, GOLDEN_JSON,
        "DecisionHash serialization changed!\n\
         Expected: {}\n\
         Got:      {}\n\
         This is a CRITICAL change affecting replay determinism.",
        GOLDEN_JSON, json
    );

    // Verify round-trip preserves all fields
    let deserialized: DecisionHash =
        serde_json::from_str(&json).expect("deserialization should succeed");
    assert_eq!(deserialized.input_hash, hash.input_hash);
    assert_eq!(deserialized.output_hash, hash.output_hash);
    assert_eq!(deserialized.reasoning_hash, hash.reasoning_hash);
    assert_eq!(deserialized.combined_hash, hash.combined_hash);
    assert_eq!(deserialized.tau, hash.tau);
    assert_eq!(deserialized.eps, hash.eps);
    assert_eq!(deserialized.k, hash.k);
}

/// Test DecisionHash field completeness
///
/// Ensures all expected fields are present in serialization.
/// Adding a new field will fail this test until updated.
#[test]
fn test_decision_hash_field_completeness() {
    let hash = DecisionHash {
        input_hash: "a".to_string(),
        output_hash: "b".to_string(),
        reasoning_hash: None,
        combined_hash: "c".to_string(),
        tau: 1.0,
        eps: 0.0,
        k: 1,
    };

    let json_value: serde_json::Value =
        serde_json::to_value(&hash).expect("to_value should succeed");

    // GOLDEN: Expected field names (alphabetical for checking)
    let expected_fields = vec![
        "combined_hash",
        "eps",
        "input_hash",
        "k",
        "output_hash",
        "reasoning_hash",
        "tau",
    ];

    let obj = json_value.as_object().expect("should be object");
    let mut actual_fields: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
    actual_fields.sort();

    assert_eq!(
        actual_fields, expected_fields,
        "DecisionHash field list changed!\n\
         Expected fields: {:?}\n\
         Actual fields:   {:?}",
        expected_fields, actual_fields
    );
}

// ============================================================================
// ROUTER WEIGHTS SERIALIZATION
// ============================================================================

/// Golden baseline: RouterWeights serialization
///
/// RouterWeights control feature importance and affect routing decisions.
/// Serialization stability ensures saved calibrations remain valid.
#[test]
fn test_router_weights_serialization_compat() {
    // Use specific values (not default) to test serialization
    let weights = RouterWeights {
        language_weight: 0.3,
        framework_weight: 0.25,
        symbol_hits_weight: 0.2,
        path_tokens_weight: 0.15,
        prompt_verb_weight: 0.1,
        orthogonal_weight: 0.05,
        diversity_weight: 0.03,
        similarity_penalty: 0.02,
    };

    let json = serde_json::to_string(&weights).expect("serialization should succeed");

    // GOLDEN BASELINE - Version: 1.0.0
    const GOLDEN_JSON: &str = r#"{"language_weight":0.3,"framework_weight":0.25,"symbol_hits_weight":0.2,"path_tokens_weight":0.15,"prompt_verb_weight":0.1,"orthogonal_weight":0.05,"diversity_weight":0.03,"similarity_penalty":0.02}"#;

    assert_eq!(
        json, GOLDEN_JSON,
        "RouterWeights serialization changed!\n\
         Expected: {}\n\
         Got:      {}\n\
         This affects saved calibrations.",
        GOLDEN_JSON, json
    );

    // Verify round-trip
    let deserialized: RouterWeights =
        serde_json::from_str(&json).expect("deserialization should succeed");
    assert_eq!(deserialized.language_weight, weights.language_weight);
    assert_eq!(deserialized.framework_weight, weights.framework_weight);
    assert_eq!(deserialized.symbol_hits_weight, weights.symbol_hits_weight);
    assert_eq!(deserialized.path_tokens_weight, weights.path_tokens_weight);
    assert_eq!(deserialized.prompt_verb_weight, weights.prompt_verb_weight);
    assert_eq!(deserialized.orthogonal_weight, weights.orthogonal_weight);
    assert_eq!(deserialized.diversity_weight, weights.diversity_weight);
    assert_eq!(deserialized.similarity_penalty, weights.similarity_penalty);
}

/// Test RouterWeights default serialization
#[test]
fn test_router_weights_default_serialization() {
    let weights = RouterWeights::default();
    let json = serde_json::to_string(&weights).expect("serialization should succeed");

    // Verify it can be deserialized
    let deserialized: RouterWeights =
        serde_json::from_str(&json).expect("deserialization should succeed");

    // Round-trip should produce identical JSON
    let json2 = serde_json::to_string(&deserialized).expect("re-serialization");
    assert_eq!(json, json2, "Round-trip serialization must be stable");
}

/// Test RouterWeights field completeness
#[test]
fn test_router_weights_field_completeness() {
    let weights = RouterWeights::default();
    let json_value: serde_json::Value =
        serde_json::to_value(&weights).expect("to_value should succeed");

    // GOLDEN: Expected field names
    let expected_fields = vec![
        "diversity_weight",
        "framework_weight",
        "language_weight",
        "orthogonal_weight",
        "path_tokens_weight",
        "prompt_verb_weight",
        "similarity_penalty",
        "symbol_hits_weight",
    ];

    let obj = json_value.as_object().expect("should be object");
    let mut actual_fields: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
    actual_fields.sort();

    assert_eq!(
        actual_fields, expected_fields,
        "RouterWeights field list changed!"
    );
}

// ============================================================================
// SERIALIZATION STABILITY TESTS
// ============================================================================

/// Test that serialization is deterministic across iterations
#[test]
fn test_serialization_stability_config() {
    let config = RouterDeterminismConfig {
        ieee754_deterministic: true,
        enable_decision_hashing: false,
    };

    let first_json = serde_json::to_string(&config).unwrap();

    for i in 0..100 {
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(
            json, first_json,
            "Iteration {}: serialization must be deterministic",
            i
        );
    }
}

/// Test that serialization is deterministic for DecisionHash
#[test]
fn test_serialization_stability_decision_hash() {
    let hash = DecisionHash {
        input_hash: "stable_input".to_string(),
        output_hash: "stable_output".to_string(),
        reasoning_hash: None,
        combined_hash: "stable_combined".to_string(),
        tau: 0.8,
        eps: 0.01,
        k: 4,
    };

    let first_json = serde_json::to_string(&hash).unwrap();

    for i in 0..100 {
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(
            json, first_json,
            "Iteration {}: DecisionHash serialization must be deterministic",
            i
        );
    }
}

/// Test that serialization is deterministic for RouterWeights
#[test]
fn test_serialization_stability_weights() {
    let weights = RouterWeights::default();

    let first_json = serde_json::to_string(&weights).unwrap();

    for i in 0..100 {
        let json = serde_json::to_string(&weights).unwrap();
        assert_eq!(
            json, first_json,
            "Iteration {}: RouterWeights serialization must be deterministic",
            i
        );
    }
}

// ============================================================================
// BACKWARD COMPATIBILITY TESTS
// ============================================================================

/// Test that old JSON can be deserialized by current code
///
/// This simulates loading a persisted config from an older version.
#[test]
fn test_backward_compat_router_determinism_config() {
    // Simulated "old" JSON that might be stored
    let old_json = r#"{"ieee754_deterministic":false,"enable_decision_hashing":true}"#;

    let config: RouterDeterminismConfig =
        serde_json::from_str(old_json).expect("should deserialize old format");

    assert!(!config.ieee754_deterministic);
    assert!(config.enable_decision_hashing);
}

/// Test that old DecisionHash JSON can be deserialized
#[test]
fn test_backward_compat_decision_hash() {
    let old_json = r#"{"input_hash":"old_input","output_hash":"old_output","combined_hash":"old_combined","tau":0.9,"eps":0.03,"k":2}"#;

    let hash: DecisionHash = serde_json::from_str(old_json).expect("should deserialize old format");

    assert_eq!(hash.input_hash, "old_input");
    assert_eq!(hash.k, 2);
}

/// Test that JSON with extra fields is handled gracefully
///
/// This tests forward compatibility - new code should handle
/// JSON that might have extra fields from a future version.
#[test]
fn test_forward_compat_extra_fields() {
    // JSON with an extra "future_field" that doesn't exist in current struct
    let future_json =
        r#"{"ieee754_deterministic":true,"enable_decision_hashing":true,"future_field":"ignored"}"#;

    // Should deserialize successfully, ignoring the unknown field
    // Note: This requires #[serde(deny_unknown_fields)] NOT being set
    let result: Result<RouterDeterminismConfig, _> = serde_json::from_str(future_json);

    // If deny_unknown_fields is enabled, this will fail and should be documented
    match result {
        Ok(config) => {
            assert!(config.ieee754_deterministic);
            assert!(config.enable_decision_hashing);
        }
        Err(e) => {
            // Document that unknown fields are NOT allowed
            panic!(
                "Unknown fields rejected. If this is intentional, update test. Error: {}",
                e
            );
        }
    }
}

// ============================================================================
// FLOATING POINT SERIALIZATION PRECISION
// ============================================================================

/// Test that floating point values serialize with sufficient precision
#[test]
fn test_float_serialization_precision() {
    let weights = RouterWeights {
        language_weight: 0.27272728,
        framework_weight: 0.22727273,
        symbol_hits_weight: 0.18181819,
        path_tokens_weight: 0.13636364,
        prompt_verb_weight: 0.09090909,
        orthogonal_weight: 0.04545455,
        diversity_weight: 0.02727273,
        similarity_penalty: 0.01818182,
    };

    let json = serde_json::to_string(&weights).unwrap();
    let deserialized: RouterWeights = serde_json::from_str(&json).unwrap();

    // Verify precision is preserved (within f32 epsilon)
    assert!(
        (deserialized.language_weight - weights.language_weight).abs() < f32::EPSILON,
        "language_weight precision lost"
    );
    assert!(
        (deserialized.framework_weight - weights.framework_weight).abs() < f32::EPSILON,
        "framework_weight precision lost"
    );
}
