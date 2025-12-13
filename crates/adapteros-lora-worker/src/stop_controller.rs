//! Deterministic Stop Controller for Inference Generation
//!
//! Implements PRD: Hard Deterministic Stop Controller with Enumerated Stop Reasons.
//!
//! This module provides a deterministic stop controller that:
//! - Enforces token budgets
//! - Detects confident completion via EOS probability
//! - Guards against repetition loops
//! - Emits reason codes committed to receipts
//!
//! # Determinism Guarantees
//!
//! - No RNG: Stop decisions use only current token, history, and quantized thresholds
//! - No time dependence: No timestamps or timeouts in stop logic
//! - Quantized comparisons: All probability comparisons use Q15 to avoid float drift
//! - Sorted n-gram detection: Deterministic ordering for repetition detection

use adapteros_api_types::inference::{StopPolicySpec, StopReasonCode, STOP_Q15_DENOM};
use adapteros_core::B3Hash;
use std::collections::VecDeque;
use tracing::{debug, trace};

/// Result of a stop check containing the reason and token index
#[derive(Debug, Clone, Copy)]
pub struct StopDecision {
    /// The reason why generation should stop
    pub reason: StopReasonCode,
    /// The token index at which the stop was triggered
    pub token_index: u32,
}

/// Deterministic stop controller for autoregressive generation.
///
/// Evaluates stop conditions at each token generation step and returns
/// a stop decision if any condition is met. The order of evaluation is:
///
/// 1. BUDGET_MAX - Hard cap on generated tokens
/// 2. COMPLETION_CONFIDENT - EOS probability exceeds threshold
/// 3. REPETITION_GUARD - N-gram repetition detected
/// 4. LENGTH - EOS token encountered
///
/// # Example
///
/// ```ignore
/// use adapteros_lora_worker::stop_controller::StopController;
/// use adapteros_api_types::inference::StopPolicySpec;
///
/// let policy = StopPolicySpec::new(100);
/// let mut controller = StopController::new(policy);
///
/// // During generation loop:
/// if let Some(decision) = controller.check_stop(next_token, eos_token_id, &logits) {
///     // Stop generation and record decision.reason
/// }
/// ```
#[derive(Debug)]
pub struct StopController {
    /// The stop policy configuration
    policy: StopPolicySpec,
    /// BLAKE3 digest of the policy for audit
    policy_digest: B3Hash,
    /// Count of tokens generated so far
    generated_count: u32,
    /// Sliding window of recent tokens for repetition detection
    token_history: VecDeque<u32>,
}

impl StopController {
    /// Create a new StopController with the given policy
    pub fn new(policy: StopPolicySpec) -> Self {
        let policy_digest = policy.digest();
        let window_size = policy.repetition_window as usize;

        Self {
            policy,
            policy_digest,
            generated_count: 0,
            token_history: VecDeque::with_capacity(window_size),
        }
    }

    /// Create a StopController from optional policy, falling back to defaults
    pub fn from_policy_or_default(policy: Option<StopPolicySpec>, max_tokens: u32) -> Self {
        let policy = policy.unwrap_or_else(|| StopPolicySpec::new(max_tokens));
        Self::new(policy)
    }

    /// Get the BLAKE3 digest of the policy specification
    pub fn policy_digest(&self) -> &B3Hash {
        &self.policy_digest
    }

    /// Get the underlying policy specification
    pub fn policy(&self) -> &StopPolicySpec {
        &self.policy
    }

    /// Get the count of tokens generated so far
    pub fn generated_count(&self) -> u32 {
        self.generated_count
    }

    /// Check if generation should stop.
    ///
    /// This method is **deterministic**: given the same inputs, it will always
    /// produce the same output. No randomness or time-dependence is involved.
    ///
    /// # Arguments
    ///
    /// * `token` - The token just generated
    /// * `eos_token_id` - The end-of-sequence token ID for this model
    /// * `logits` - The full vocabulary logits (for EOS probability extraction)
    ///
    /// # Returns
    ///
    /// * `Some(StopDecision)` if generation should stop, with the reason
    /// * `None` if generation should continue
    pub fn check_stop(
        &mut self,
        token: u32,
        eos_token_id: u32,
        logits: &[f32],
    ) -> Option<StopDecision> {
        let token_index = self.generated_count;

        // Update state first
        self.generated_count += 1;
        self.update_history(token);

        // Check conditions in priority order (most restrictive first)

        // 1. BUDGET_MAX - Hard budget cap
        if let Some(reason) = self.check_budget_max(token_index) {
            debug!(
                token_index,
                budget = self.policy.output_max_tokens,
                "Stop: BUDGET_MAX"
            );
            return Some(StopDecision {
                reason,
                token_index,
            });
        }

        // 2. COMPLETION_CONFIDENT - High EOS probability
        if let Some(reason) = self.check_completion_confident(logits, eos_token_id) {
            debug!(
                token_index,
                threshold_q15 = self.policy.completion_threshold_q15,
                "Stop: COMPLETION_CONFIDENT"
            );
            return Some(StopDecision {
                reason,
                token_index,
            });
        }

        // 3. REPETITION_GUARD - N-gram repetition detected
        if let Some(reason) = self.check_repetition_guard() {
            debug!(
                token_index,
                ngram = self.policy.repetition_ngram,
                window = self.policy.repetition_window,
                "Stop: REPETITION_GUARD"
            );
            return Some(StopDecision {
                reason,
                token_index,
            });
        }

        // 4. LENGTH - EOS token encountered
        let effective_eos = self.policy.eos_token_id.unwrap_or(eos_token_id);
        if token == effective_eos {
            debug!(token_index, eos_token = effective_eos, "Stop: LENGTH (EOS)");
            return Some(StopDecision {
                reason: StopReasonCode::Length,
                token_index,
            });
        }

        trace!(token_index, token, "Continue generation");
        None
    }

    /// Check if hard budget cap is exceeded
    fn check_budget_max(&self, token_index: u32) -> Option<StopReasonCode> {
        // Check if we've reached the budget BEFORE generating this token
        // (token_index is 0-based, so we check if token_index >= output_max_tokens)
        if token_index >= self.policy.output_max_tokens {
            return Some(StopReasonCode::BudgetMax);
        }
        None
    }

    /// Check if EOS probability exceeds the completion threshold
    fn check_completion_confident(
        &self,
        logits: &[f32],
        eos_token_id: u32,
    ) -> Option<StopReasonCode> {
        let effective_eos = self.policy.eos_token_id.unwrap_or(eos_token_id) as usize;

        // Skip if EOS token is out of vocabulary range
        if effective_eos >= logits.len() {
            return None;
        }

        // Compute softmax probability for EOS token
        let eos_prob = self.compute_eos_probability(logits, effective_eos);

        // Quantize to Q15 for deterministic comparison
        let eos_prob_q15 = (eos_prob * STOP_Q15_DENOM).round() as i16;

        if eos_prob_q15 >= self.policy.completion_threshold_q15 {
            trace!(
                eos_prob,
                eos_prob_q15,
                threshold = self.policy.completion_threshold_q15,
                "EOS probability exceeded threshold"
            );
            return Some(StopReasonCode::CompletionConfident);
        }

        None
    }

    /// Compute softmax probability for the EOS token
    ///
    /// Uses numerically stable softmax computation.
    fn compute_eos_probability(&self, logits: &[f32], eos_idx: usize) -> f32 {
        // Find max logit for numerical stability
        let max_logit = logits
            .iter()
            .copied()
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0);

        // Compute sum of exp(logit - max) for normalization
        let sum_exp: f64 = logits.iter().map(|&l| ((l - max_logit) as f64).exp()).sum();

        // Compute EOS probability
        let eos_exp = ((logits[eos_idx] - max_logit) as f64).exp();
        (eos_exp / sum_exp) as f32
    }

    /// Check for n-gram repetition in the sliding window
    fn check_repetition_guard(&self) -> Option<StopReasonCode> {
        let ngram_size = self.policy.repetition_ngram as usize;

        // Need at least 2 * ngram_size tokens to detect a repeated n-gram
        if self.token_history.len() < ngram_size * 2 {
            return None;
        }

        // Get the last n-gram (the one we're checking for repetition)
        let history_len = self.token_history.len();
        let last_ngram: Vec<u32> = self
            .token_history
            .iter()
            .skip(history_len - ngram_size)
            .copied()
            .collect();

        // Check if this n-gram appears earlier in the window
        // We check all positions except the last one (which is the n-gram itself)
        for start_pos in 0..=(history_len - ngram_size * 2) {
            let candidate: Vec<u32> = self
                .token_history
                .iter()
                .skip(start_pos)
                .take(ngram_size)
                .copied()
                .collect();

            if candidate == last_ngram {
                trace!(
                    ngram = ?last_ngram,
                    first_pos = start_pos,
                    second_pos = history_len - ngram_size,
                    "Repetition detected"
                );
                return Some(StopReasonCode::RepetitionGuard);
            }
        }

        None
    }

    /// Update the token history sliding window
    fn update_history(&mut self, token: u32) {
        let window_size = self.policy.repetition_window as usize;

        self.token_history.push_back(token);

        // Maintain window size
        while self.token_history.len() > window_size {
            self.token_history.pop_front();
        }
    }

    /// Reset the controller state for a new generation
    pub fn reset(&mut self) {
        self.generated_count = 0;
        self.token_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy() -> StopPolicySpec {
        StopPolicySpec {
            output_max_tokens: 10,
            eos_token_id: Some(151645),
            completion_threshold_q15: 24576, // ~0.75
            repetition_ngram: 3,
            repetition_window: 32,
        }
    }

    fn make_logits_with_eos_prob(vocab_size: usize, eos_idx: usize, eos_logit: f32) -> Vec<f32> {
        let mut logits = vec![0.0; vocab_size];
        logits[eos_idx] = eos_logit;
        logits
    }

    #[test]
    fn test_stop_reason_enum_is_exhaustive_and_always_present() {
        // This test verifies that every stop path returns a valid StopReasonCode
        let policy = make_policy();
        let mut controller = StopController::new(policy);

        // Test BUDGET_MAX
        for _ in 0..10 {
            controller.check_stop(1, 151645, &vec![0.0; 151646]);
        }
        let decision = controller.check_stop(1, 151645, &vec![0.0; 151646]);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().reason, StopReasonCode::BudgetMax);

        // Test LENGTH (EOS token)
        let mut controller = StopController::new(make_policy());
        let decision = controller.check_stop(151645, 151645, &vec![0.0; 151646]);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().reason, StopReasonCode::Length);

        // All stop reasons should be valid enum variants
        let reasons = vec![
            StopReasonCode::Length,
            StopReasonCode::BudgetMax,
            StopReasonCode::CompletionConfident,
            StopReasonCode::RepetitionGuard,
        ];
        for reason in reasons {
            // Verify serialization works
            let s = reason.to_string();
            assert!(!s.is_empty());
            // Verify parsing works
            let parsed: StopReasonCode = s.parse().unwrap();
            assert_eq!(parsed, reason);
        }
    }

    #[test]
    fn test_stop_controller_enforces_budget_max() {
        let policy = StopPolicySpec {
            output_max_tokens: 5,
            ..make_policy()
        };
        let mut controller = StopController::new(policy);
        let logits = vec![0.0; 151646];

        // Generate 5 tokens (indices 0-4)
        for i in 0..5 {
            let decision = controller.check_stop(i, 151645, &logits);
            assert!(decision.is_none(), "Should not stop at token {}", i);
        }

        // 6th token (index 5) should trigger BUDGET_MAX
        let decision = controller.check_stop(5, 151645, &logits);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().reason, StopReasonCode::BudgetMax);
        assert_eq!(decision.unwrap().token_index, 5);
    }

    #[test]
    fn test_stop_controller_triggers_completion_confident() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(10),
            completion_threshold_q15: 24576, // ~0.75
            ..make_policy()
        };
        let mut controller = StopController::new(policy);

        // Create logits where EOS (token 10) has very high probability
        // High logit value should result in high probability after softmax
        let mut logits = vec![0.0; 100];
        logits[10] = 10.0; // Very high logit for EOS token

        // This should trigger COMPLETION_CONFIDENT
        let decision = controller.check_stop(1, 10, &logits);
        assert!(decision.is_some());
        assert_eq!(
            decision.unwrap().reason,
            StopReasonCode::CompletionConfident
        );
    }

    #[test]
    fn test_stop_controller_triggers_repetition_guard_deterministically() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(999),
            completion_threshold_q15: 32767, // Very high threshold (won't trigger)
            repetition_ngram: 3,
            repetition_window: 32,
        };
        let mut controller = StopController::new(policy.clone());
        let logits = vec![0.0; 1000];

        // Generate a pattern: [1, 2, 3, 4, 5, 1, 2, 3]
        // The 3-gram [1, 2, 3] repeats
        let tokens = vec![1, 2, 3, 4, 5, 1, 2, 3];
        let mut stop_decision = None;

        for token in tokens {
            let decision = controller.check_stop(token, 999, &logits);
            if decision.is_some() {
                stop_decision = decision;
                break;
            }
        }

        assert!(stop_decision.is_some());
        let first_decision = stop_decision.unwrap();
        assert_eq!(first_decision.reason, StopReasonCode::RepetitionGuard);
        let first_token_index = first_decision.token_index;

        // Run again with same sequence - should be deterministic
        let mut controller2 = StopController::new(policy);
        let tokens = vec![1, 2, 3, 4, 5, 1, 2, 3];
        let mut stop_decision2 = None;

        for token in tokens {
            let decision = controller2.check_stop(token, 999, &logits);
            if decision.is_some() {
                stop_decision2 = decision;
                break;
            }
        }

        assert!(stop_decision2.is_some());
        let second_decision = stop_decision2.unwrap();
        assert_eq!(second_decision.reason, first_decision.reason);
        assert_eq!(second_decision.token_index, first_token_index);
    }

    #[test]
    fn test_stop_controller_length_on_eos() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(42),
            completion_threshold_q15: 32767, // Won't trigger
            repetition_ngram: 3,
            repetition_window: 32,
        };
        let mut controller = StopController::new(policy);
        let logits = vec![0.0; 100];

        // Non-EOS tokens should continue
        assert!(controller.check_stop(1, 42, &logits).is_none());
        assert!(controller.check_stop(2, 42, &logits).is_none());

        // EOS token should trigger LENGTH
        let decision = controller.check_stop(42, 42, &logits);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().reason, StopReasonCode::Length);
    }

    #[test]
    fn test_policy_digest_is_deterministic() {
        let policy1 = make_policy();
        let policy2 = make_policy();

        let digest1 = policy1.digest();
        let digest2 = policy2.digest();

        assert_eq!(digest1, digest2);

        // Different policy should have different digest
        let different_policy = StopPolicySpec {
            output_max_tokens: 999,
            ..make_policy()
        };
        let digest3 = different_policy.digest();
        assert_ne!(digest1, digest3);
    }

    #[test]
    fn test_stop_controller_reset() {
        let policy = make_policy();
        let mut controller = StopController::new(policy);
        let logits = vec![0.0; 151646];

        // Generate some tokens
        for _ in 0..5 {
            controller.check_stop(1, 151645, &logits);
        }
        assert_eq!(controller.generated_count(), 5);

        // Reset
        controller.reset();
        assert_eq!(controller.generated_count(), 0);
    }
}
