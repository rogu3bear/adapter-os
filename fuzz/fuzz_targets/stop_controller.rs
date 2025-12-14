#![no_main]

use adapteros_api_types::inference::{StopPolicySpec, StopReasonCode, STOP_Q15_DENOM};
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

// Import from worker crate
use adapteros_lora_worker::stop_controller::StopController;

/// Fuzz the stop controller decision logic
///
/// Tests:
/// - Budget enforcement with various token counts
/// - EOS probability detection with different logit distributions
/// - Repetition detection with various n-gram patterns
/// - EOS token detection
/// - Determinism: same inputs produce same outputs
/// - Edge cases: empty logits, extreme values, boundary conditions
fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // Generate policy parameters
    let output_max_tokens = match u.int_in_range::<u32>(1..=1000) {
        Ok(v) => v,
        Err(_) => return,
    };

    let eos_token_id = match u.int_in_range::<u32>(0..=50000) {
        Ok(v) => v,
        Err(_) => 151645, // Qwen default
    };

    let completion_threshold_q15 = match u.int_in_range::<i16>(0..=STOP_Q15_DENOM as i16) {
        Ok(v) => v,
        Err(_) => 24576, // ~0.75
    };

    let repetition_ngram = match u.int_in_range::<u32>(2..=8) {
        Ok(v) => v,
        Err(_) => 3,
    };

    let repetition_window = match u.int_in_range::<u32>(8..=128) {
        Ok(v) => v,
        Err(_) => 32,
    };

    let policy = StopPolicySpec {
        output_max_tokens,
        eos_token_id: Some(eos_token_id),
        completion_threshold_q15,
        repetition_ngram: repetition_ngram.try_into().unwrap_or(3),
        repetition_window: repetition_window.try_into().unwrap_or(32),
    };

    // Create two controllers with same policy for determinism check
    let mut controller1 = StopController::new(policy.clone());
    let mut controller2 = StopController::new(policy.clone());

    // Check policy digest is deterministic
    assert_eq!(controller1.policy_digest(), controller2.policy_digest());

    // Generate vocab size
    let vocab_size = match u.int_in_range::<usize>(1000..=50000) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Make sure EOS token is within vocab
    let eos_token_id = (eos_token_id as usize).min(vocab_size - 1) as u32;

    // Generate sequence of tokens
    let seq_len = match u.int_in_range::<usize>(1..=50) {
        Ok(v) => v,
        Err(_) => return,
    };

    let mut decisions1 = Vec::new();
    let mut decisions2 = Vec::new();

    for _ in 0..seq_len {
        // Generate token
        let token = match u.int_in_range::<u32>(0..=(vocab_size - 1) as u32) {
            Ok(v) => v,
            Err(_) => break,
        };

        // Generate logits with various distributions
        let mut logits = vec![0.0f32; vocab_size];

        let logit_mode = u.int_in_range::<u8>(0..=3).unwrap_or(0);
        match logit_mode {
            0 => {
                // Uniform logits
                for logit in &mut logits {
                    *logit = 0.0;
                }
            }
            1 => {
                // High EOS probability
                logits[eos_token_id as usize] = 10.0;
            }
            2 => {
                // Random logits
                for logit in &mut logits {
                    let raw: i16 = u.arbitrary().unwrap_or(0);
                    *logit = (raw as f32) / 1000.0;
                }
            }
            _ => {
                // Sparse logits
                let num_nonzero = u.int_in_range::<usize>(1..=100).unwrap_or(10);
                for _ in 0..num_nonzero {
                    let idx = u.int_in_range::<usize>(0..=(vocab_size - 1)).unwrap_or(0);
                    let val: i16 = u.arbitrary().unwrap_or(0);
                    logits[idx] = (val as f32) / 100.0;
                }
            }
        }

        // Check both controllers (determinism test)
        let decision1 = controller1.check_stop(token, eos_token_id, &logits);
        let decision2 = controller2.check_stop(token, eos_token_id, &logits);

        // Decisions must match
        match (decision1, decision2) {
            (Some(d1), Some(d2)) => {
                assert_eq!(d1.reason, d2.reason, "Stop reasons must be deterministic");
                assert_eq!(
                    d1.token_index, d2.token_index,
                    "Token indices must be deterministic"
                );
                decisions1.push(d1);
                decisions2.push(d2);
                break; // Stop generation
            }
            (None, None) => {
                // Continue
            }
            _ => {
                panic!("Controllers produced different stop decisions!");
            }
        }
    }

    // Verify decisions are identical
    assert_eq!(
        decisions1.len(),
        decisions2.len(),
        "Decision counts must match"
    );

    // Verify generated counts match
    assert_eq!(
        controller1.generated_count(),
        controller2.generated_count(),
        "Generated counts must match"
    );

    // Test reset
    controller1.reset();
    assert_eq!(controller1.generated_count(), 0);

    // Test budget enforcement specifically
    let budget_policy = StopPolicySpec {
        output_max_tokens: 5,
        eos_token_id: Some(999999),
        completion_threshold_q15: STOP_Q15_DENOM as i16, // Never trigger
        repetition_ngram: 100,
        repetition_window: 200,
    };
    let mut budget_controller = StopController::new(budget_policy);
    let dummy_logits = vec![0.0; 1000];

    for i in 0..10 {
        let decision = budget_controller.check_stop(1, 999999, &dummy_logits);
        if i < 5 {
            assert!(
                decision.is_none(),
                "Should not stop before budget at token {}",
                i
            );
        } else {
            assert!(decision.is_some(), "Should stop at budget at token {}", i);
            if let Some(d) = decision {
                assert_eq!(d.reason, StopReasonCode::BudgetMax);
                break;
            }
        }
    }

    // Test EOS token detection
    let eos_policy = StopPolicySpec::new(100);
    let mut eos_controller = StopController::new(eos_policy);
    let decision = eos_controller.check_stop(151645, 151645, &vec![0.0; 152000]);
    assert!(decision.is_some());
    if let Some(d) = decision {
        assert_eq!(d.reason, StopReasonCode::Length);
    }
});
