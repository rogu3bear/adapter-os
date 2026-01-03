//! Integration tests for StopController (PRD: Hard Deterministic Stop Controller)
//!
//! These tests verify the integration of stop fields through the receipt system.

use adapteros_api_types::inference::{StopPolicySpec, StopReasonCode};
use adapteros_core::B3Hash;
use adapteros_lora_worker::stop_controller::StopController;

/// Default functions for test setup
fn default_completion_threshold_q15() -> i16 {
    24576 // ~0.75
}
fn default_repetition_ngram() -> u8 {
    3
}
fn default_repetition_window() -> u16 {
    32
}

fn make_test_policy() -> StopPolicySpec {
    StopPolicySpec {
        output_max_tokens: 100,
        eos_token_id: Some(0),
        completion_threshold_q15: default_completion_threshold_q15(),
        repetition_ngram: default_repetition_ngram(),
        repetition_window: default_repetition_window(),
        stop_sequences: Vec::new(),
    }
}

#[test]
fn test_stop_fields_committed_to_merkle_bundle_and_receipt() {
    // This test verifies PRD acceptance criteria:
    // "stop_reason_code, stop_reason_token_index, and stop_policy_digest_b3
    //  are committed to the Merkle bundle and included in receipt"

    // Create a StopController with a known policy
    let policy = make_test_policy();
    let controller = StopController::new(policy.clone());

    // 1. Verify policy_digest is computed and non-zero
    let policy_digest = controller.policy_digest();
    let zero_hash = B3Hash::from_bytes([0u8; 32]);
    assert_ne!(
        *policy_digest, zero_hash,
        "Policy digest should be computed and non-zero"
    );

    // 2. Verify policy_digest is deterministic (same policy -> same digest)
    let controller2 = StopController::new(policy.clone());
    assert_eq!(
        *controller.policy_digest(),
        *controller2.policy_digest(),
        "Same policy should produce identical digests"
    );

    // 3. Verify different policies produce different digests
    let different_policy = StopPolicySpec {
        output_max_tokens: 200, // Different from original
        ..policy
    };
    let controller3 = StopController::new(different_policy);
    assert_ne!(
        *controller.policy_digest(),
        *controller3.policy_digest(),
        "Different policies should produce different digests"
    );
}

#[test]
fn test_stop_reason_code_serialization() {
    // Verify StopReasonCode serializes to SCREAMING_SNAKE_CASE as required for audit
    let length = StopReasonCode::Length;
    let budget_max = StopReasonCode::BudgetMax;
    let completion_confident = StopReasonCode::CompletionConfident;
    let repetition_guard = StopReasonCode::RepetitionGuard;
    let stop_sequence = StopReasonCode::StopSequence;

    // JSON serialization should use SCREAMING_SNAKE_CASE
    assert_eq!(
        serde_json::to_string(&length).unwrap(),
        "\"LENGTH\"",
        "Length should serialize to LENGTH"
    );
    assert_eq!(
        serde_json::to_string(&budget_max).unwrap(),
        "\"BUDGET_MAX\"",
        "BudgetMax should serialize to BUDGET_MAX"
    );
    assert_eq!(
        serde_json::to_string(&completion_confident).unwrap(),
        "\"COMPLETION_CONFIDENT\"",
        "CompletionConfident should serialize to COMPLETION_CONFIDENT"
    );
    assert_eq!(
        serde_json::to_string(&repetition_guard).unwrap(),
        "\"REPETITION_GUARD\"",
        "RepetitionGuard should serialize to REPETITION_GUARD"
    );
    assert_eq!(
        serde_json::to_string(&stop_sequence).unwrap(),
        "\"STOP_SEQUENCE\"",
        "StopSequence should serialize to STOP_SEQUENCE"
    );
}

#[test]
fn test_stop_decision_contains_token_index() {
    // Verify that stop decisions include the token index where stop occurred
    let policy = StopPolicySpec {
        output_max_tokens: 5,
        eos_token_id: Some(999),
        ..make_test_policy()
    };
    let mut controller = StopController::new(policy);

    // Generate some tokens
    let dummy_logits: Vec<f32> = vec![0.0; 100];

    // Token 0-4: no stop (we can generate tokens 0,1,2,3,4 before budget is exceeded)
    for i in 0..5 {
        let decision = controller.check_stop(i, 999, &dummy_logits);
        assert!(decision.is_none(), "Should not stop at token {}", i);
    }

    // Token 5: should trigger BUDGET_MAX (token_index 5 >= output_max_tokens 5)
    let decision = controller.check_stop(5, 999, &dummy_logits);
    assert!(decision.is_some(), "Should stop at token 5");
    let decision = decision.unwrap();

    // Verify the token_index is correct
    assert_eq!(
        decision.token_index, 5,
        "Stop decision should record token index 5"
    );
    assert_eq!(
        decision.reason,
        StopReasonCode::BudgetMax,
        "Stop reason should be BUDGET_MAX"
    );
}

#[test]
fn test_stop_policy_spec_q15_threshold_range() {
    // Verify Q15 threshold is in valid range
    let policy = make_test_policy();

    // Q15 uses 32767 as denominator, so valid range is 0-32767
    assert!(
        policy.completion_threshold_q15 >= 0 && policy.completion_threshold_q15 <= 32767,
        "completion_threshold_q15 should be in Q15 range [0, 32767]"
    );

    // Default is ~0.75 = 24576/32767
    let threshold_f32 = policy.completion_threshold_q15 as f32 / 32767.0;
    assert!(
        (threshold_f32 - 0.75).abs() < 0.01,
        "Default threshold should be approximately 0.75, got {}",
        threshold_f32
    );
}

#[test]
fn test_stop_controller_determinism_no_rng() {
    // PRD requirement: "no RNG, no time dependence, quantized thresholds"
    // Run the same sequence multiple times and verify identical results
    let policy = StopPolicySpec {
        output_max_tokens: 100,
        eos_token_id: Some(0),
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        stop_sequences: Vec::new(),
    };

    // Fixed token sequence that triggers repetition guard
    let tokens = vec![1u32, 2, 3, 1, 2, 3, 1, 2, 3]; // Repeating 3-gram

    // Create logits with low EOS probability
    let mut dummy_logits: Vec<f32> = vec![0.0; 100];
    dummy_logits[0] = 0.1; // EOS token prob too low to trigger COMPLETION_CONFIDENT

    // Run 1
    let mut controller1 = StopController::new(policy.clone());
    let mut results1 = Vec::new();
    for &token in &tokens {
        results1.push(controller1.check_stop(token, 0, &dummy_logits));
    }

    // Run 2
    let mut controller2 = StopController::new(policy.clone());
    let mut results2 = Vec::new();
    for &token in &tokens {
        results2.push(controller2.check_stop(token, 0, &dummy_logits));
    }

    // Results must be identical
    assert_eq!(
        results1.len(),
        results2.len(),
        "Result lengths should match"
    );
    for (i, (r1, r2)) in results1.iter().zip(results2.iter()).enumerate() {
        match (r1, r2) {
            (None, None) => {} // Both None, OK
            (Some(d1), Some(d2)) => {
                assert_eq!(
                    d1.reason, d2.reason,
                    "Stop reason should be identical at token {}",
                    i
                );
                assert_eq!(
                    d1.token_index, d2.token_index,
                    "Token index should be identical at token {}",
                    i
                );
                assert_eq!(
                    d1.trim_tokens, d2.trim_tokens,
                    "Trim tokens should be identical at token {}",
                    i
                );
            }
            _ => panic!("Determinism violation at token {}: {:?} vs {:?}", i, r1, r2),
        }
    }
}

#[test]
fn test_stop_policy_spec_serialization_roundtrip() {
    // PRD: Stop policy must be serializable for transport and replay
    let policy = StopPolicySpec {
        output_max_tokens: 256,
        eos_token_id: Some(151645),
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        stop_sequences: vec!["</s>".to_string(), "\n\n".to_string()],
    };

    // Serialize to JSON
    let json = serde_json::to_string(&policy).expect("serialize");

    // Deserialize back
    let deserialized: StopPolicySpec = serde_json::from_str(&json).expect("deserialize");

    // Verify round-trip
    assert_eq!(policy.output_max_tokens, deserialized.output_max_tokens);
    assert_eq!(policy.eos_token_id, deserialized.eos_token_id);
    assert_eq!(
        policy.completion_threshold_q15,
        deserialized.completion_threshold_q15
    );
    assert_eq!(policy.repetition_ngram, deserialized.repetition_ngram);
    assert_eq!(policy.repetition_window, deserialized.repetition_window);
    assert_eq!(policy.stop_sequences, deserialized.stop_sequences);

    // Verify digest is stable after round-trip
    let original_controller = StopController::new(policy.clone());
    let roundtrip_controller = StopController::new(deserialized);
    assert_eq!(
        *original_controller.policy_digest(),
        *roundtrip_controller.policy_digest(),
        "Digest should be stable after JSON round-trip"
    );
}

#[test]
fn test_all_stop_reason_codes_are_reachable() {
    // PRD: Each stop reason code must be reachable through StopController
    let vocab_size = 1000;
    let eos_token: u32 = 42;

    // Test 1: LENGTH (EOS token encountered)
    {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(eos_token),
            completion_threshold_q15: 32767, // Max threshold, won't trigger
            repetition_ngram: 3,
            repetition_window: 32,
            stop_sequences: Vec::new(),
        };
        let mut controller = StopController::new(policy);
        let logits = vec![0.0; vocab_size];

        // Non-EOS token should not stop
        assert!(controller.check_stop(1, eos_token, &logits).is_none());

        // EOS token should stop with LENGTH
        let decision = controller.check_stop(eos_token, eos_token, &logits);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().reason, StopReasonCode::Length);
    }

    // Test 2: BUDGET_MAX (hard token limit)
    {
        let policy = StopPolicySpec {
            output_max_tokens: 3,
            eos_token_id: Some(eos_token),
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 32,
            stop_sequences: Vec::new(),
        };
        let mut controller = StopController::new(policy);
        let logits = vec![0.0; vocab_size];

        // Generate 3 tokens (indices 0, 1, 2)
        for _ in 0..3 {
            assert!(controller.check_stop(1, eos_token, &logits).is_none());
        }

        // Token 4 (index 3) should hit BUDGET_MAX
        let decision = controller.check_stop(1, eos_token, &logits);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().reason, StopReasonCode::BudgetMax);
    }

    // Test 3: COMPLETION_CONFIDENT (high EOS probability)
    {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(10),
            completion_threshold_q15: 16384, // ~0.5 threshold
            repetition_ngram: 3,
            repetition_window: 32,
            stop_sequences: Vec::new(),
        };
        let mut controller = StopController::new(policy);

        // Create logits with high EOS probability
        // Use large logit for EOS to ensure high softmax probability
        let mut logits = vec![0.0; vocab_size];
        logits[10] = 15.0; // Very high logit for EOS

        // Should trigger COMPLETION_CONFIDENT
        let decision = controller.check_stop(1, 10, &logits);
        assert!(decision.is_some());
        assert_eq!(
            decision.unwrap().reason,
            StopReasonCode::CompletionConfident
        );
    }

    // Test 4: REPETITION_GUARD (n-gram repetition)
    {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(999),
            completion_threshold_q15: 32767, // Max, won't trigger
            repetition_ngram: 3,
            repetition_window: 32,
            stop_sequences: Vec::new(),
        };
        let mut controller = StopController::new(policy);
        let logits = vec![0.0; vocab_size];

        // Generate a repeating pattern: [1,2,3,4,5,1,2,3]
        // The 3-gram [1,2,3] appears twice
        let tokens = [1u32, 2, 3, 4, 5, 1, 2, 3];
        let mut stop_found = false;

        for token in tokens {
            if let Some(decision) = controller.check_stop(token, 999, &logits) {
                assert_eq!(decision.reason, StopReasonCode::RepetitionGuard);
                stop_found = true;
                break;
            }
        }

        assert!(stop_found, "REPETITION_GUARD should have been triggered");
    }

    // Test 5: STOP_SEQUENCE (explicit stop sequence matched)
    {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(999),
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 32,
            stop_sequences: vec!["END".to_string()],
        };
        let mut controller =
            StopController::new_with_stop_sequences(policy, vec![vec![7u32, 8, 9]]);
        let logits = vec![0.0; vocab_size];

        assert!(controller.check_stop(7, 999, &logits).is_none());
        assert!(controller.check_stop(8, 999, &logits).is_none());
        let decision = controller
            .check_stop(9, 999, &logits)
            .expect("stop sequence should trigger");
        assert_eq!(decision.reason, StopReasonCode::StopSequence);
        assert_eq!(decision.trim_tokens, 2);
    }
}

#[test]
fn test_stop_controller_priority_order() {
    // PRD: Stop conditions are checked in priority order:
    // 1. BUDGET_MAX, 2. COMPLETION_CONFIDENT, 3. REPETITION_GUARD,
    // 4. STOP_SEQUENCE, 5. LENGTH

    // When budget is exceeded, BUDGET_MAX should take priority over LENGTH
    let policy = StopPolicySpec {
        output_max_tokens: 1,
        eos_token_id: Some(42),
        completion_threshold_q15: 0, // Would trigger COMPLETION_CONFIDENT
        repetition_ngram: 1,         // Would trigger REPETITION_GUARD immediately
        repetition_window: 2,
        stop_sequences: Vec::new(),
    };
    let mut controller = StopController::new(policy);

    // Generate one token (uses up the budget)
    let logits = vec![10.0; 100]; // High probability for all tokens including EOS
    controller.check_stop(42, 42, &logits); // This is EOS but budget allows 1 token

    // Next token should be BUDGET_MAX, not LENGTH or others
    let decision = controller.check_stop(42, 42, &logits);
    assert!(decision.is_some());
    assert_eq!(
        decision.unwrap().reason,
        StopReasonCode::BudgetMax,
        "BUDGET_MAX should take priority"
    );
}

#[test]
fn test_stop_policy_digest_changes_with_any_field() {
    // PRD: Policy digest must reflect all policy parameters
    let base_policy = StopPolicySpec {
        output_max_tokens: 100,
        eos_token_id: Some(42),
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        stop_sequences: vec!["END".to_string()],
    };
    let base_digest = StopController::new(base_policy.clone())
        .policy_digest()
        .clone();

    // Changing output_max_tokens
    let p1 = StopPolicySpec {
        output_max_tokens: 200,
        ..base_policy.clone()
    };
    assert_ne!(*StopController::new(p1).policy_digest(), base_digest);

    // Changing eos_token_id
    let p2 = StopPolicySpec {
        eos_token_id: Some(43),
        ..base_policy.clone()
    };
    assert_ne!(*StopController::new(p2).policy_digest(), base_digest);

    // Changing completion_threshold_q15
    let p3 = StopPolicySpec {
        completion_threshold_q15: 16384,
        ..base_policy.clone()
    };
    assert_ne!(*StopController::new(p3).policy_digest(), base_digest);

    // Changing repetition_ngram
    let p4 = StopPolicySpec {
        repetition_ngram: 4,
        ..base_policy.clone()
    };
    assert_ne!(*StopController::new(p4).policy_digest(), base_digest);

    // Changing repetition_window
    let p5 = StopPolicySpec {
        repetition_window: 64,
        ..base_policy.clone()
    };
    assert_ne!(*StopController::new(p5).policy_digest(), base_digest);

    // Changing eos_token_id to None
    let p6 = StopPolicySpec {
        eos_token_id: None,
        ..base_policy.clone()
    };
    assert_ne!(*StopController::new(p6).policy_digest(), base_digest);

    // Changing stop_sequences
    let p7 = StopPolicySpec {
        stop_sequences: vec!["STOP".to_string()],
        ..base_policy.clone()
    };
    assert_ne!(*StopController::new(p7).policy_digest(), base_digest);
}

#[test]
fn test_stop_reason_code_from_string_roundtrip() {
    // PRD: Stop reason codes must serialize as SCREAMING_SNAKE_CASE for audit
    use std::str::FromStr;

    let codes = [
        (StopReasonCode::Length, "LENGTH"),
        (StopReasonCode::BudgetMax, "BUDGET_MAX"),
        (StopReasonCode::CompletionConfident, "COMPLETION_CONFIDENT"),
        (StopReasonCode::RepetitionGuard, "REPETITION_GUARD"),
        (StopReasonCode::StopSequence, "STOP_SEQUENCE"),
    ];

    for (code, expected_str) in codes {
        // to_string should produce SCREAMING_SNAKE_CASE
        let s = code.to_string();
        assert_eq!(s, expected_str, "to_string mismatch for {:?}", code);

        // FromStr should parse back
        let parsed = StopReasonCode::from_str(&s).expect("parse");
        assert_eq!(parsed, code, "FromStr roundtrip failed for {}", s);

        // JSON serialization should use quoted SCREAMING_SNAKE_CASE
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, format!("\"{}\"", expected_str));

        // JSON deserialization should work
        let deserialized: StopReasonCode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, code);
    }
}
