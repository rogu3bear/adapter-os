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
use tracing::{debug, info, trace};

/// Result of a stop check containing the reason and token index
#[derive(Debug, Clone, Copy)]
pub struct StopDecision {
    /// The reason why generation should stop
    pub reason: StopReasonCode,
    /// The token index at which the stop was triggered
    pub token_index: u32,
    /// Tokens to trim from the already-emitted output (excludes current token)
    pub trim_tokens: usize,
}

/// Deterministic stop controller for autoregressive generation.
///
/// Evaluates stop conditions at each token generation step and returns
/// a stop decision if any condition is met. The order of evaluation is:
///
/// 1. BUDGET_MAX - Hard cap on generated tokens
/// 2. COMPLETION_CONFIDENT - EOS probability exceeds threshold
/// 3. REPETITION_GUARD - N-gram repetition detected
/// 4. STOP_SEQUENCE - Explicit stop sequence matched
/// 5. LENGTH - EOS token encountered
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
    /// Tokenized stop sequences to detect at the end of the stream
    stop_sequences_tokens: Vec<Vec<u32>>,
    /// Maximum stop sequence length (tokens)
    max_stop_sequence_len: usize,
    /// Sliding window size for history (max repetition window vs stop sequence length)
    history_window: usize,
}

impl StopController {
    /// Create a new StopController with the given policy
    pub fn new(policy: StopPolicySpec) -> Self {
        Self::new_with_stop_sequences(policy, Vec::new())
    }

    /// Create a new StopController with tokenized stop sequences
    pub fn new_with_stop_sequences(
        policy: StopPolicySpec,
        stop_sequences_tokens: Vec<Vec<u32>>,
    ) -> Self {
        let policy_digest = policy.digest();
        let repetition_window = policy.repetition_window as usize;
        let max_stop_sequence_len = stop_sequences_tokens
            .iter()
            .map(|seq| seq.len())
            .max()
            .unwrap_or(0);
        let history_window = repetition_window.max(max_stop_sequence_len);

        Self {
            policy,
            policy_digest,
            generated_count: 0,
            token_history: VecDeque::with_capacity(history_window),
            stop_sequences_tokens,
            max_stop_sequence_len,
            history_window,
        }
    }

    /// Create a StopController from optional policy, falling back to defaults
    pub fn from_policy_or_default(policy: Option<StopPolicySpec>, max_tokens: u32) -> Self {
        let policy = policy.unwrap_or_else(|| StopPolicySpec::new(max_tokens));
        Self::new(policy)
    }

    /// Create a StopController with tokenized stop sequences, falling back to defaults
    pub fn from_policy_or_default_with_stop_sequences(
        policy: Option<StopPolicySpec>,
        max_tokens: u32,
        stop_sequences_tokens: Vec<Vec<u32>>,
    ) -> Self {
        let policy = policy.unwrap_or_else(|| StopPolicySpec::new(max_tokens));
        Self::new_with_stop_sequences(policy, stop_sequences_tokens)
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

    /// Preload already-generated tokens (e.g., free tokens) into the controller state.
    pub fn preload_tokens(&mut self, tokens: &[u32]) {
        for &token in tokens {
            self.generated_count = self.generated_count.saturating_add(1);
            self.update_history(token);
        }
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
                target: "inference.stop",
                reason = %reason,
                token_index,
                budget = self.policy.output_max_tokens,
                generated_count = self.generated_count,
                "Stop condition: BUDGET_MAX"
            );
            let decision = StopDecision {
                reason,
                token_index,
                trim_tokens: 0,
            };
            info!(
                target: "inference.stop",
                reason = %decision.reason,
                token_index = decision.token_index,
                generated_count = self.generated_count,
                trim_tokens = decision.trim_tokens,
                "Stop controller triggered"
            );
            return Some(decision);
        }

        // 2. COMPLETION_CONFIDENT - High EOS probability
        if let Some(reason) = self.check_completion_confident(logits, eos_token_id) {
            let effective_eos = self.policy.eos_token_id.unwrap_or(eos_token_id) as usize;
            let eos_prob_q15 = if effective_eos < logits.len() {
                let p = self.compute_eos_probability(logits, effective_eos);
                (p * STOP_Q15_DENOM).round() as i16
            } else {
                0
            };
            debug!(
                target: "inference.stop",
                reason = %reason,
                token_index,
                eos_prob_q15,
                threshold_q15 = self.policy.completion_threshold_q15,
                "Stop condition: COMPLETION_CONFIDENT"
            );
            let decision = StopDecision {
                reason,
                token_index,
                trim_tokens: 0,
            };
            info!(
                target: "inference.stop",
                reason = %decision.reason,
                token_index = decision.token_index,
                generated_count = self.generated_count,
                trim_tokens = decision.trim_tokens,
                "Stop controller triggered"
            );
            return Some(decision);
        }

        // 3. REPETITION_GUARD - N-gram repetition detected
        if let Some(reason) = self.check_repetition_guard() {
            debug!(
                target: "inference.stop",
                reason = %reason,
                token_index,
                ngram_size = self.policy.repetition_ngram,
                max_count = self.policy.repetition_threshold,
                threshold = self.policy.repetition_threshold,
                "Stop condition: REPETITION_GUARD"
            );
            let decision = StopDecision {
                reason,
                token_index,
                trim_tokens: 0,
            };
            info!(
                target: "inference.stop",
                reason = %decision.reason,
                token_index = decision.token_index,
                generated_count = self.generated_count,
                trim_tokens = decision.trim_tokens,
                "Stop controller triggered"
            );
            return Some(decision);
        }

        // 4. STOP_SEQUENCE - explicit stop sequences matched
        if let Some(trim_tokens) = self.check_stop_sequences() {
            let sequence_len = trim_tokens + 1;
            debug!(
                target: "inference.stop",
                reason = %StopReasonCode::StopSequence,
                token_index,
                sequence_len,
                trim_tokens,
                "Stop condition: STOP_SEQUENCE"
            );
            let decision = StopDecision {
                reason: StopReasonCode::StopSequence,
                token_index,
                trim_tokens,
            };
            info!(
                target: "inference.stop",
                reason = %decision.reason,
                token_index = decision.token_index,
                generated_count = self.generated_count,
                trim_tokens = decision.trim_tokens,
                "Stop controller triggered"
            );
            return Some(decision);
        }

        // 5. LENGTH - EOS token encountered
        let effective_eos = self.policy.eos_token_id.unwrap_or(eos_token_id);
        if token == effective_eos {
            debug!(
                target: "inference.stop",
                reason = %StopReasonCode::Length,
                token_index,
                eos_token = effective_eos,
                "Stop condition: LENGTH (EOS)"
            );
            let decision = StopDecision {
                reason: StopReasonCode::Length,
                token_index,
                trim_tokens: 0,
            };
            info!(
                target: "inference.stop",
                reason = %decision.reason,
                token_index = decision.token_index,
                generated_count = self.generated_count,
                trim_tokens = decision.trim_tokens,
                "Stop controller triggered"
            );
            return Some(decision);
        }

        trace!(target: "inference.stop", token_index, token, "Continue generation");
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

    /// Check for n-gram repetition in the sliding window.
    ///
    /// Scans all n-gram sizes from `repetition_ngram` (minimum) to `window_size / 2`
    /// (maximum). For each size, counts occurrences of every n-gram in the window.
    /// If any n-gram appears more than `repetition_threshold` times, flags repetition.
    ///
    /// This algorithm is deterministic: given the same token sequence, it will
    /// always produce the same result. N-gram sizes are checked in ascending order
    /// (smallest first) for consistent early-exit behavior.
    fn check_repetition_guard(&self) -> Option<StopReasonCode> {
        use std::collections::HashMap;

        let min_ngram = self.policy.repetition_ngram as usize;
        let window_size = self.policy.repetition_window as usize;
        let threshold = self.policy.repetition_threshold as usize;

        // Extract the sliding window
        let history_len = self.token_history.len();
        let window_start = history_len.saturating_sub(window_size);
        let window: Vec<u32> = self
            .token_history
            .iter()
            .skip(window_start)
            .copied()
            .collect();

        let window_len = window.len();

        // Need enough tokens to form at least the smallest n-gram twice
        if window_len < min_ngram * 2 {
            return None;
        }

        // Maximum n-gram size is window_size / 2 (must fit at least 2 n-grams)
        let max_ngram = window_len / 2;

        // Check n-gram sizes from min to max (ascending order for determinism)
        for ngram_size in min_ngram..=max_ngram {
            // Count all n-grams of this size in the window
            let mut counts: HashMap<Vec<u32>, usize> = HashMap::new();

            for start in 0..=(window_len - ngram_size) {
                let ngram: Vec<u32> = window[start..start + ngram_size].to_vec();
                *counts.entry(ngram).or_insert(0) += 1;
            }

            // Check if any n-gram exceeds threshold (deterministic: max is well-defined)
            let max_count = counts.values().copied().max().unwrap_or(0);
            if max_count > threshold {
                trace!(
                    ngram_size,
                    max_count,
                    threshold,
                    "Repetition detected: n-gram count exceeds threshold"
                );
                return Some(StopReasonCode::RepetitionGuard);
            }
        }

        None
    }

    /// Check if any explicit stop sequence matches the tail of token history.
    fn check_stop_sequences(&self) -> Option<usize> {
        if self.stop_sequences_tokens.is_empty() || self.max_stop_sequence_len == 0 {
            return None;
        }

        let history_len = self.token_history.len();
        for sequence in &self.stop_sequences_tokens {
            let seq_len = sequence.len();
            if seq_len == 0 || history_len < seq_len {
                continue;
            }
            let start = history_len - seq_len;
            let matches = self
                .token_history
                .iter()
                .skip(start)
                .zip(sequence.iter())
                .all(|(a, b)| *a == *b);
            if matches {
                return Some(seq_len.saturating_sub(1));
            }
        }

        None
    }

    /// Update the token history sliding window
    fn update_history(&mut self, token: u32) {
        self.token_history.push_back(token);

        // Maintain window size
        while self.token_history.len() > self.history_window {
            self.token_history.pop_front();
        }
    }

    /// Reset the controller state for a new generation
    pub fn reset(&mut self) {
        self.generated_count = 0;
        self.token_history.clear();
    }

    /// Compute a BLAKE3 digest of the current sliding window state.
    ///
    /// The digest covers `generated_count` (LE) followed by each window token (LE).
    /// This is deterministic: the same token history always produces the same digest.
    /// Used to bind the stop window state into V7 receipts via `stop_window_digest_b3`.
    pub fn window_digest(&self) -> B3Hash {
        let mut buf = Vec::with_capacity(4 + self.token_history.len() * 4);
        buf.extend_from_slice(&self.generated_count.to_le_bytes());
        for &token in &self.token_history {
            buf.extend_from_slice(&token.to_le_bytes());
        }
        B3Hash::hash(&buf)
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
            repetition_threshold: 1,
            stop_sequences: Vec::new(),
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
        // Note: Cancelled and SystemError are not returned by StopController.check_stop()
        // but are valid stop reasons for external use (cancellation, hardware errors)
        let reasons = vec![
            StopReasonCode::Length,
            StopReasonCode::BudgetMax,
            StopReasonCode::CompletionConfident,
            StopReasonCode::RepetitionGuard,
            StopReasonCode::StopSequence,
            StopReasonCode::Cancelled,
            StopReasonCode::SystemError,
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
            repetition_threshold: 1,
            stop_sequences: Vec::new(),
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
            repetition_threshold: 1,
            stop_sequences: Vec::new(),
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

    #[test]
    fn test_cancelled_and_system_error_stop_codes() {
        // Patent 3535886.0002 Claim 6: Verify Cancelled and SystemError codes
        // are valid and can be used for receipt binding (even though they're
        // not returned by StopController.check_stop() directly).

        // Verify the codes are valid enum variants with correct string representation
        assert_eq!(StopReasonCode::Cancelled.to_string(), "CANCELLED");
        assert_eq!(StopReasonCode::SystemError.to_string(), "SYSTEM_ERROR");

        // Verify they can be serialized/deserialized for receipt persistence
        let cancelled_json = serde_json::to_string(&StopReasonCode::Cancelled).unwrap();
        let system_error_json = serde_json::to_string(&StopReasonCode::SystemError).unwrap();

        assert_eq!(cancelled_json, "\"CANCELLED\"");
        assert_eq!(system_error_json, "\"SYSTEM_ERROR\"");

        let parsed_cancelled: StopReasonCode = serde_json::from_str(&cancelled_json).unwrap();
        let parsed_system_error: StopReasonCode = serde_json::from_str(&system_error_json).unwrap();

        assert_eq!(parsed_cancelled, StopReasonCode::Cancelled);
        assert_eq!(parsed_system_error, StopReasonCode::SystemError);
    }

    #[test]
    fn test_preload_tokens_consumes_budget() {
        let policy = StopPolicySpec {
            output_max_tokens: 5,
            ..make_policy()
        };
        let mut controller = StopController::new(policy);
        let logits = vec![0.0; 151646];

        // Preload 4 tokens
        controller.preload_tokens(&[10, 20, 30, 40]);
        assert_eq!(controller.generated_count(), 4);

        // Next check_stop should allow 1 more token (index 4 < budget 5)
        let decision = controller.check_stop(50, 151645, &logits);
        assert!(
            decision.is_none(),
            "Should allow one more token after preload"
        );

        // The 6th token (index 5) should trigger BUDGET_MAX
        let decision = controller.check_stop(60, 151645, &logits);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().reason, StopReasonCode::BudgetMax);
        assert_eq!(decision.unwrap().token_index, 5);
    }

    #[test]
    fn test_completion_confident_nan_inf_logits() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(2),
            completion_threshold_q15: 24576, // ~0.75
            ..make_policy()
        };

        // Test NaN logits - should not panic
        {
            let mut controller = StopController::new(policy.clone());
            let mut logits = vec![0.0; 10];
            logits[0] = f32::NAN;
            logits[5] = f32::NAN;
            let decision = controller.check_stop(1, 2, &logits);
            // Should either return None or a valid decision, never panic
            if let Some(d) = decision {
                assert!(
                    d.reason == StopReasonCode::CompletionConfident
                        || d.reason == StopReasonCode::Length
                        || d.reason == StopReasonCode::BudgetMax
                );
            }
        }

        // Test Inf logits - should not panic
        {
            let mut controller = StopController::new(policy.clone());
            let mut logits = vec![0.0; 10];
            logits[0] = f32::INFINITY;
            logits[2] = f32::NEG_INFINITY;
            let decision = controller.check_stop(1, 2, &logits);
            if let Some(d) = decision {
                assert!(
                    d.reason == StopReasonCode::CompletionConfident
                        || d.reason == StopReasonCode::Length
                        || d.reason == StopReasonCode::BudgetMax
                );
            }
        }
    }

    #[test]
    fn test_completion_confident_all_zero_logits() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(2),
            completion_threshold_q15: 24576, // ~0.75
            ..make_policy()
        };
        let mut controller = StopController::new(policy);

        // All-zero logits → uniform softmax → probability = 1/vocab_size
        // For vocab_size=10, EOS prob = 0.1 → Q15 = ~3277 (well below 24576)
        let logits = vec![0.0; 10];
        let decision = controller.check_stop(1, 2, &logits);
        // Should not trigger COMPLETION_CONFIDENT since uniform prob is low
        assert!(
            decision.is_none(),
            "All-zero logits should give uniform distribution, too low for completion threshold"
        );
    }

    #[test]
    fn test_multiple_stop_sequences_first_match() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(999), // won't trigger
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 8,
            repetition_threshold: 100, // won't trigger
            stop_sequences: Vec::new(),
        };
        // Two stop sequences: [10, 20] and [20, 30, 40]
        let stop_seqs = vec![vec![10, 20], vec![20, 30, 40]];
        let mut controller = StopController::new_with_stop_sequences(policy, stop_seqs);
        let logits = vec![0.0; 1000];

        // Feed tokens: 5, 10, 20 → matches first stop sequence [10, 20]
        assert!(controller.check_stop(5, 999, &logits).is_none());
        assert!(controller.check_stop(10, 999, &logits).is_none());
        let decision = controller.check_stop(20, 999, &logits);
        assert!(decision.is_some());
        let d = decision.unwrap();
        assert_eq!(d.reason, StopReasonCode::StopSequence);
        // trim_tokens = seq_len - 1 = 2 - 1 = 1
        assert_eq!(d.trim_tokens, 1);
    }

    #[test]
    fn test_stop_sequence_no_match() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(999),
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 8,
            repetition_threshold: 100,
            stop_sequences: Vec::new(),
        };
        let stop_seqs = vec![vec![100, 200, 300]];
        let mut controller = StopController::new_with_stop_sequences(policy, stop_seqs);
        let logits = vec![0.0; 1000];

        // Feed tokens that don't match the stop sequence
        for tok in [1, 2, 3, 4, 5] {
            let decision = controller.check_stop(tok, 999, &logits);
            assert!(
                decision.is_none(),
                "Should not match stop sequence for token {tok}"
            );
        }
    }

    #[test]
    fn test_history_window_sized_for_stop_sequences() {
        // Stop sequence longer than repetition_window → history_window should be the larger value
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(999),
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 4, // small
            repetition_threshold: 100,
            stop_sequences: Vec::new(),
        };
        // Stop sequence of length 10 (larger than repetition_window of 4)
        let long_stop_seq = vec![vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]];
        let controller = StopController::new_with_stop_sequences(policy, long_stop_seq);
        assert_eq!(
            controller.history_window, 10,
            "history_window should use max of repetition_window and max stop sequence length"
        );
    }

    #[test]
    fn test_window_digest_deterministic() {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            ..make_policy()
        };

        // Create two controllers with the same token sequence
        let mut controller1 = StopController::new(policy.clone());
        let mut controller2 = StopController::new(policy);
        let logits = vec![0.0; 151646];

        let tokens = [10, 20, 30, 40, 50];
        for &tok in &tokens {
            controller1.check_stop(tok, 151645, &logits);
            controller2.check_stop(tok, 151645, &logits);
        }

        let digest1 = controller1.window_digest();
        let digest2 = controller2.window_digest();
        assert_eq!(
            digest1, digest2,
            "Same token sequence must produce identical window digest"
        );

        // Different sequence should produce different digest
        let mut controller3 = StopController::new(StopPolicySpec {
            output_max_tokens: 100,
            ..make_policy()
        });
        for &tok in &[10, 20, 30, 40, 99] {
            controller3.check_stop(tok, 151645, &logits);
        }
        let digest3 = controller3.window_digest();
        assert_ne!(
            digest1, digest3,
            "Different token sequence should produce different window digest"
        );
    }
}
