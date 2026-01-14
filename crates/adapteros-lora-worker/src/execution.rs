//! Inference Pipeline Execution
//!
//! This module provides execution context and result types for inference pipelines
//! with deterministic routing. It integrates with the canonical receipt computation
//! in `adapteros_core::receipt_digest` and the `TraceSink` trait for persistence.
//!
//! ## Design Principles
//!
//! - **No duplicate canonical algorithms**: Uses `adapteros_core::receipt_digest` for
//!   all hash computations to ensure parity with production and CLI verification.
//! - **Interface-focused**: Provides `StepExecutor` trait for abstracting inference
//!   backends while maintaining determinism invariants.
//! - **Receipt schema aware**: Supports V1-V5 receipt schemas via `ReceiptDigestInput`.
//!
//! ## Integration Points
//!
//! - `adapteros_core::receipt_digest`: Canonical hash algorithms
//! - `adapteros_db::inference_trace::TraceSink`: Per-token recording and finalization
//! - `adapteros_lora_router::Decision`: Routing decisions with Q15 gates
//!
//! ## Determinism Guarantees
//!
//! - HKDF-SHA256 seed derivation with BLAKE3 global seed
//! - Q15 gate quantization (denominator 32767.0)
//! - Score DESC, index ASC tie-breaking
//! - Kahan summation for softmax stability
//! - Per-token decision hashing for run_head chain

use adapteros_core::receipt_digest::{
    self, encode_adapter_ids, encode_allowed_mask, encode_gates_q15, ReceiptDigestInput,
    RECEIPT_SCHEMA_V4,
};
use adapteros_core::{B3Hash, Result};
use adapteros_lora_router::policy_mask::PolicyMask;
use adapteros_lora_router::Decision;
use smallvec::SmallVec;
use std::time::Instant;

/// Execution context for a single inference run.
///
/// Maintains the routing digest accumulator and tracks execution state
/// across token positions. Uses canonical hash functions from `receipt_digest`.
#[derive(Debug)]
pub struct ExecutionContext {
    /// Context digest (BLAKE3 of prompt + metadata)
    context_digest: B3Hash,
    /// Running hash of all routing decisions (run_head chain)
    run_head: B3Hash,
    /// Current token position
    position: u32,
    /// Start time for latency tracking
    start_time: Instant,
    /// Input token count (for attributed token calculation)
    pub input_token_count: u32,
    /// Cached token count from prefix KV cache hit
    pub cached_token_count: u32,
    /// Backend identifier for decision hashing
    backend_id: Option<String>,
    /// Kernel version for decision hashing
    kernel_version_id: Option<String>,
}

impl ExecutionContext {
    /// Create a new execution context.
    ///
    /// # Arguments
    /// * `context_digest` - BLAKE3 digest of the prompt and request metadata
    /// * `input_token_count` - Number of input tokens before generation
    pub fn new(context_digest: B3Hash, input_token_count: u32) -> Self {
        Self {
            context_digest,
            run_head: B3Hash::zero(),
            position: 0,
            start_time: Instant::now(),
            input_token_count,
            cached_token_count: 0,
            backend_id: None,
            kernel_version_id: None,
        }
    }

    /// Set the cached token count from prefix KV cache hit.
    pub fn with_cached_tokens(mut self, count: u32) -> Self {
        self.cached_token_count = count;
        self
    }

    /// Set backend identity for decision hashing.
    pub fn with_backend(mut self, backend_id: String, kernel_version_id: Option<String>) -> Self {
        self.backend_id = Some(backend_id);
        self.kernel_version_id = kernel_version_id;
        self
    }

    /// Get the context digest.
    pub fn context_digest(&self) -> &B3Hash {
        &self.context_digest
    }

    /// Get the current run_head hash (accumulated routing decisions).
    pub fn run_head(&self) -> &B3Hash {
        &self.run_head
    }

    /// Get current token position.
    pub fn position(&self) -> u32 {
        self.position
    }

    /// Compute attributed tokens (input tokens minus cached tokens).
    ///
    /// This follows Patent 3535886.0002: attributed = logical - cached.
    pub fn attributed_tokens(&self) -> u32 {
        self.input_token_count
            .saturating_sub(self.cached_token_count)
    }

    /// Get elapsed time since execution started.
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Commit a routing decision to the run_head chain.
    ///
    /// Uses canonical hash functions from `adapteros_core::receipt_digest`.
    ///
    /// # Arguments
    /// * `adapter_ids` - Real adapter IDs (not indices) for this decision
    /// * `decision` - The routing decision with adapter indices and Q15 gates
    /// * `policy_mask` - The policy mask that was applied
    ///
    /// # Returns
    /// The decision hash for this token position
    pub fn commit_decision(
        &mut self,
        adapter_ids: &[String],
        decision: &Decision,
        policy_mask: &PolicyMask,
    ) -> B3Hash {
        let token_index = self.position;
        self.position += 1;

        // Use canonical encoding from receipt_digest
        let adapter_blob = encode_adapter_ids(adapter_ids);
        let gates_blob = encode_gates_q15(decision.gates_q15.as_slice());
        let allowed_blob = encode_allowed_mask(&policy_mask.allowed);

        // Use serde for policy overrides to match production (via API types)
        let overrides_json = decision.policy_overrides_applied.as_ref().map(|o| {
            // Manual JSON to match the structure expected by production
            // The router's PolicyOverrideFlags matches the API type's field names
            format!(
                "{{\"allow_list\":{},\"deny_list\":{},\"trust_state\":{}}}",
                o.allow_list, o.deny_list, o.trust_state
            )
        });

        // Use canonical hash function
        let decision_hash = receipt_digest::hash_token_decision(
            self.context_digest.as_bytes(),
            token_index,
            &adapter_blob,
            &gates_blob,
            decision.policy_mask_digest_b3.map(|h| *h.as_bytes()),
            Some(&allowed_blob),
            overrides_json.as_deref(),
            self.backend_id.as_deref(),
            self.kernel_version_id.as_deref(),
        );

        // Use canonical chain update
        self.run_head =
            receipt_digest::update_run_head(&self.run_head, token_index, &decision_hash);

        decision_hash
    }
}

/// Committed routing decision for a single token position.
#[derive(Debug, Clone)]
pub struct CommittedDecision {
    /// Token position
    pub token_index: u32,
    /// Real adapter IDs (not indices)
    pub adapter_ids: Vec<String>,
    /// Selected adapter indices (positional)
    pub adapter_indices: SmallVec<[u16; 8]>,
    /// Quantized gates (Q15 format)
    pub gates_q15: SmallVec<[i16; 8]>,
    /// Shannon entropy of the gate distribution
    pub entropy: f32,
    /// BLAKE3 hash of this decision
    pub decision_hash: B3Hash,
    /// Policy mask digest that was applied
    pub policy_mask_digest: Option<B3Hash>,
}

impl CommittedDecision {
    /// Create from a router Decision with position context.
    pub fn from_decision(
        adapter_ids: Vec<String>,
        decision: &Decision,
        token_index: u32,
        decision_hash: B3Hash,
    ) -> Self {
        Self {
            token_index,
            adapter_ids,
            adapter_indices: decision.indices.clone(),
            gates_q15: decision.gates_q15.clone(),
            entropy: decision.entropy,
            decision_hash,
            policy_mask_digest: decision.policy_mask_digest_b3,
        }
    }

    /// Get the dominant adapter (highest gate value).
    pub fn dominant_adapter(&self) -> Option<&str> {
        self.adapter_ids.first().map(|s| s.as_str())
    }

    /// Get gate for a specific adapter ID.
    pub fn gate_for(&self, adapter_id: &str) -> Option<i16> {
        self.adapter_ids
            .iter()
            .position(|id| id == adapter_id)
            .map(|pos| self.gates_q15[pos])
    }
}

/// Result of pipeline execution containing output and receipt.
///
/// Uses `ReceiptDigestInput` from `receipt_digest` for schema-versioned
/// receipt computation.
#[derive(Debug)]
pub struct ExecutionResult {
    /// Generated output tokens
    pub output_tokens: Vec<u32>,
    /// All committed routing decisions
    pub decisions: Vec<CommittedDecision>,
    /// Final run_head hash (Merkle root of decision chain)
    pub run_head_hash: B3Hash,
    /// Output digest (BLAKE3 of output tokens)
    pub output_digest: B3Hash,
    /// Receipt digest (BLAKE3 of receipt fields) - schema version determines fields
    pub receipt_digest: B3Hash,
    /// Schema version used for receipt
    pub receipt_schema_version: u8,
    /// Execution latency in milliseconds
    pub latency_ms: u64,
    /// Logical prompt tokens
    pub logical_prompt_tokens: u32,
    /// Tokens satisfied by prefix cache
    pub prefix_cached_token_count: u32,
    /// Billed input tokens (logical - cached)
    pub billed_input_tokens: u32,
    /// Logical output tokens
    pub logical_output_tokens: u32,
    /// Stop reason (if terminated by stop controller)
    pub stop_reason: Option<StopReason>,
    /// Full receipt input for verification/re-computation
    pub receipt_input: ReceiptDigestInput,
}

impl ExecutionResult {
    /// Create a new execution result with V4 schema (production default).
    ///
    /// Uses canonical `ReceiptDigestInput` and `compute_receipt_digest` from
    /// `adapteros_core::receipt_digest`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        output_tokens: Vec<u32>,
        decisions: Vec<CommittedDecision>,
        run_head_hash: B3Hash,
        context_digest: &B3Hash,
        logical_prompt_tokens: u32,
        prefix_cached_token_count: u32,
        stop_reason: Option<StopReason>,
        latency_ms: u64,
        stop_fields: Option<StopFields>,
        kv_fields: Option<KvFields>,
        prefix_cache_fields: Option<PrefixCacheFields>,
        model_cache_identity: Option<B3Hash>,
    ) -> Self {
        let output_digest = receipt_digest::compute_output_digest(&output_tokens);
        let logical_output_tokens = output_tokens.len() as u32;
        let billed_input_tokens = logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
        let billed_output_tokens = logical_output_tokens;

        // Build ReceiptDigestInput with all V4 fields
        let mut input = ReceiptDigestInput::new(
            *context_digest.as_bytes(),
            *run_head_hash.as_bytes(),
            *output_digest.as_bytes(),
            logical_prompt_tokens,
            prefix_cached_token_count,
            billed_input_tokens,
            logical_output_tokens,
            billed_output_tokens,
        );

        // Add stop controller fields
        if let Some(stop) = &stop_fields {
            input = input.with_stop_controller(
                stop.stop_reason_code.clone(),
                stop.stop_reason_token_index,
                stop.stop_policy_digest_b3.map(|h| *h.as_bytes()),
            );
        }

        // Add KV quota fields
        if let Some(kv) = &kv_fields {
            input = input.with_kv_quota(
                kv.tenant_kv_quota_bytes,
                kv.tenant_kv_bytes_used,
                kv.kv_evictions,
                kv.kv_residency_policy_id.clone(),
                kv.kv_quota_enforced,
            );
        }

        // Add prefix cache fields
        if let Some(prefix) = &prefix_cache_fields {
            input = input.with_prefix_cache(
                prefix.prefix_kv_key_b3.map(|h| *h.as_bytes()),
                prefix.prefix_cache_hit,
                prefix.prefix_kv_bytes,
            );
        }

        // Add model cache identity
        if let Some(mci) = model_cache_identity {
            input = input.with_model_cache_identity(Some(*mci.as_bytes()));
        }

        // Compute V4 receipt digest using canonical function
        let receipt_digest = receipt_digest::compute_receipt_digest(&input, RECEIPT_SCHEMA_V4)
            .unwrap_or_else(|| {
                // Fallback to manual V1 if V4 fails (shouldn't happen)
                B3Hash::hash_multi(&[
                    context_digest.as_bytes(),
                    run_head_hash.as_bytes(),
                    output_digest.as_bytes(),
                    &logical_prompt_tokens.to_le_bytes(),
                    &prefix_cached_token_count.to_le_bytes(),
                    &billed_input_tokens.to_le_bytes(),
                    &logical_output_tokens.to_le_bytes(),
                    &billed_output_tokens.to_le_bytes(),
                ])
            });

        Self {
            output_tokens,
            decisions,
            run_head_hash,
            output_digest,
            receipt_digest,
            receipt_schema_version: RECEIPT_SCHEMA_V4,
            latency_ms,
            logical_prompt_tokens,
            prefix_cached_token_count,
            billed_input_tokens,
            logical_output_tokens,
            stop_reason,
            receipt_input: input,
        }
    }

    /// Verify the decision chain integrity.
    ///
    /// Recomputes run_head from decisions using canonical `update_run_head`.
    pub fn verify_integrity(&self) -> bool {
        let mut run_head = B3Hash::zero();
        for decision in &self.decisions {
            run_head = receipt_digest::update_run_head(
                &run_head,
                decision.token_index,
                &decision.decision_hash,
            );
        }
        run_head == self.run_head_hash
    }

    /// Verify receipt digest matches stored input.
    ///
    /// Re-computes receipt digest from `receipt_input` and compares.
    pub fn verify_receipt(&self) -> bool {
        let recomputed = receipt_digest::compute_receipt_digest(
            &self.receipt_input,
            self.receipt_schema_version,
        );
        recomputed
            .map(|d| d == self.receipt_digest)
            .unwrap_or(false)
    }
}

/// Stop controller fields for V4+ receipts.
#[derive(Debug, Clone, Default)]
pub struct StopFields {
    pub stop_reason_code: Option<String>,
    pub stop_reason_token_index: Option<u32>,
    pub stop_policy_digest_b3: Option<B3Hash>,
}

/// KV quota/residency fields for V4+ receipts.
#[derive(Debug, Clone, Default)]
pub struct KvFields {
    pub tenant_kv_quota_bytes: u64,
    pub tenant_kv_bytes_used: u64,
    pub kv_evictions: u32,
    pub kv_residency_policy_id: Option<String>,
    pub kv_quota_enforced: bool,
}

/// Prefix KV cache fields for V4+ receipts.
#[derive(Debug, Clone, Default)]
pub struct PrefixCacheFields {
    pub prefix_kv_key_b3: Option<B3Hash>,
    pub prefix_cache_hit: bool,
    pub prefix_kv_bytes: u64,
}

/// Stop reason for pipeline termination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Maximum tokens reached
    MaxTokens,
    /// EOS token generated
    EosToken,
    /// Stop sequence matched
    StopSequence,
    /// Budget exceeded
    BudgetMax,
    /// High EOS probability (completion confident)
    CompletionConfident,
    /// Repetition detected
    RepetitionGuard,
    /// Maximum context length reached
    MaxContextLength,
    /// System error
    Error,
}

impl StopReason {
    /// Convert from API StopReasonCode.
    pub fn from_code(code: adapteros_api_types::inference::StopReasonCode) -> Self {
        use adapteros_api_types::inference::StopReasonCode;
        match code {
            StopReasonCode::Length => StopReason::EosToken,
            StopReasonCode::BudgetMax => StopReason::BudgetMax,
            StopReasonCode::CompletionConfident => StopReason::CompletionConfident,
            StopReasonCode::RepetitionGuard => StopReason::RepetitionGuard,
            StopReasonCode::StopSequence => StopReason::StopSequence,
            StopReasonCode::Cancelled => StopReason::Error,
            StopReasonCode::SystemError => StopReason::Error,
        }
    }

    /// Convert to string representation for receipt.
    pub fn as_str(&self) -> &'static str {
        match self {
            StopReason::MaxTokens => "MAX_TOKENS",
            StopReason::EosToken => "EOS",
            StopReason::StopSequence => "STOP_SEQUENCE",
            StopReason::BudgetMax => "BUDGET_MAX",
            StopReason::CompletionConfident => "COMPLETION_CONFIDENT",
            StopReason::RepetitionGuard => "REPETITION_GUARD",
            StopReason::MaxContextLength => "MAX_CONTEXT_LENGTH",
            StopReason::Error => "ERROR",
        }
    }

    /// Convert to StopFields for receipt computation.
    pub fn to_stop_fields(&self, token_index: u32, policy_digest: Option<B3Hash>) -> StopFields {
        StopFields {
            stop_reason_code: Some(self.as_str().to_string()),
            stop_reason_token_index: Some(token_index),
            stop_policy_digest_b3: policy_digest,
        }
    }
}

// =============================================================================
// Pipeline Executor
// =============================================================================

/// Configuration for pipeline execution.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Maximum context length (prompt + generated)
    pub max_context_length: u32,
    /// EOS token ID
    pub eos_token_id: u32,
    /// Enable decision hashing for receipts
    pub enable_decision_hashing: bool,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            max_context_length: 32768,
            eos_token_id: 151645, // Qwen2.5 EOS
            enable_decision_hashing: true,
        }
    }
}

/// Output from a single step of execution.
#[derive(Debug)]
pub struct StepOutput {
    /// Real adapter IDs selected for this step
    pub adapter_ids: Vec<String>,
    /// Routing decision with indices and Q15 gates
    pub decision: Decision,
    /// Policy mask applied
    pub policy_mask: PolicyMask,
    /// Generated token (None during prefill)
    pub generated_token: Option<u32>,
}

/// Trait for pipeline step execution.
///
/// Implementors provide the actual model inference and routing logic.
/// The trait requires passing real adapter IDs to ensure receipt hashes match production.
pub trait StepExecutor: Send {
    /// Execute a single pipeline step.
    ///
    /// # Arguments
    /// * `ctx` - Execution context with run_head state
    /// * `input_tokens` - Current token sequence
    /// * `position` - Current position in the sequence
    ///
    /// # Returns
    /// StepOutput containing adapter IDs, decision, and optionally generated token
    fn execute_step(
        &mut self,
        ctx: &mut ExecutionContext,
        input_tokens: &[u32],
        position: u32,
    ) -> Result<StepOutput>;

    /// Check if generation should stop.
    ///
    /// # Arguments
    /// * `token` - The just-generated token
    /// * `position` - Current position
    /// * `config` - Execution configuration (for EOS check)
    ///
    /// # Returns
    /// Stop reason if termination is required
    fn check_stop(
        &mut self,
        token: u32,
        position: u32,
        config: &ExecutionConfig,
    ) -> Option<StopReason>;
}

/// Execute the complete inference pipeline.
///
/// This is the main entry point for inference execution. It:
/// 1. Processes input tokens (prefill)
/// 2. Generates output tokens (decode)
/// 3. Commits routing decisions to the run_head chain using canonical hashes
/// 4. Produces a verifiable V4 receipt
///
/// # Arguments
/// * `executor` - Step executor providing model inference
/// * `ctx` - Execution context with initial state
/// * `input_tokens` - Prompt tokens
/// * `config` - Execution configuration
///
/// # Returns
/// ExecutionResult with output tokens and cryptographic receipt
pub fn execute_pipeline<E: StepExecutor>(
    executor: &mut E,
    ctx: &mut ExecutionContext,
    input_tokens: &[u32],
    config: &ExecutionConfig,
) -> Result<ExecutionResult> {
    let mut current_tokens = input_tokens.to_vec();
    let mut output_tokens = Vec::with_capacity(config.max_tokens as usize);
    let mut decisions = Vec::with_capacity(config.max_tokens as usize);
    let mut stop_reason = None;
    let mut stop_token_index = None;

    // Generation loop
    for step in 0..config.max_tokens {
        // Check context length limit
        if current_tokens.len() >= config.max_context_length as usize {
            stop_reason = Some(StopReason::MaxContextLength);
            stop_token_index = Some(step);
            break;
        }

        // Execute step (routing + generation)
        let output = executor.execute_step(ctx, &current_tokens, step)?;

        // Commit decision to run_head chain using real adapter IDs
        let decision_hash =
            ctx.commit_decision(&output.adapter_ids, &output.decision, &output.policy_mask);
        let committed = CommittedDecision::from_decision(
            output.adapter_ids,
            &output.decision,
            step,
            decision_hash,
        );
        decisions.push(committed);

        // Handle generated token
        if let Some(token) = output.generated_token {
            // Check stop conditions
            if let Some(reason) = executor.check_stop(token, step, config) {
                stop_reason = Some(reason);
                stop_token_index = Some(step);
                // Don't include EOS token in output
                if reason != StopReason::EosToken {
                    output_tokens.push(token);
                    current_tokens.push(token);
                }
                break;
            }

            output_tokens.push(token);
            current_tokens.push(token);
        }
        // Prefill steps (generated_token = None) just continue
    }

    // If we exhausted max_tokens without another stop reason
    if stop_reason.is_none() && output_tokens.len() >= config.max_tokens as usize {
        stop_reason = Some(StopReason::MaxTokens);
        stop_token_index = Some(config.max_tokens - 1);
    }

    // Build stop fields for receipt
    let stop_fields = stop_reason.map(|r| r.to_stop_fields(stop_token_index.unwrap_or(0), None));

    // Build result with V4 receipt
    let result = ExecutionResult::new(
        output_tokens,
        decisions,
        ctx.run_head().clone(),
        ctx.context_digest(),
        ctx.input_token_count,
        ctx.cached_token_count,
        stop_reason,
        ctx.elapsed_ms(),
        stop_fields,
        None, // KV fields - caller can provide via builder pattern
        None, // Prefix cache fields
        None, // Model cache identity
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Parity test**: Verifies that our decision hashing matches production.
    ///
    /// This test uses the exact same inputs as would be passed to `inference_trace.rs`
    /// and verifies we produce identical hashes. If this fails, receipts won't verify.
    #[test]
    fn test_decision_hash_parity_with_production() {
        let context_digest = [0x01u8; 32];
        let token_index = 5u32;
        let adapter_ids = vec!["lora-rust".to_string(), "lora-python".to_string()];
        let gates_q15: &[i16] = &[16384, 16383];

        // Encode using canonical functions (same as both execution.rs and inference_trace.rs)
        let adapter_blob = encode_adapter_ids(&adapter_ids);
        let gates_blob = encode_gates_q15(gates_q15);

        // Hash using canonical function
        let our_hash = receipt_digest::hash_token_decision(
            &context_digest,
            token_index,
            &adapter_blob,
            &gates_blob,
            None, // policy_mask_digest
            None, // allowed_mask_blob
            None, // policy_overrides_json
            None, // backend_id
            None, // kernel_version_id
        );

        // Manually compute what production would compute (SqlTraceSink::hash_decision)
        let production_hash = B3Hash::hash_multi(&[
            &context_digest[..],
            &token_index.to_le_bytes(),
            &(adapter_blob.len() as u32).to_le_bytes(),
            &adapter_blob,
            &(gates_blob.len() as u32).to_le_bytes(),
            &gates_blob,
            // policy_bytes empty
            &0u32.to_le_bytes(),
            &[],
            // allowed_bytes empty
            &0u32.to_le_bytes(),
            &[],
            // overrides_bytes empty
            &0u32.to_le_bytes(),
            &[],
            // backend_bytes empty
            &0u32.to_le_bytes(),
            &[],
            // kernel_bytes empty
            &0u32.to_le_bytes(),
            &[],
        ]);

        assert_eq!(
            our_hash,
            production_hash,
            "Decision hash must match production algorithm.\n\
             Ours: {}\n\
             Prod: {}",
            our_hash.to_hex(),
            production_hash.to_hex()
        );
    }

    /// Verifies adapter_blob encoding matches production.
    #[test]
    fn test_adapter_blob_encoding_parity() {
        let ids = vec!["adapter-a".to_string(), "adapter-b".to_string()];

        // Our encoding via canonical function
        let our_blob = encode_adapter_ids(&ids);

        // Production encoding (SqlTraceSink::encode_adapter_ids)
        let mut prod_blob = Vec::with_capacity(4 + ids.iter().map(|s| s.len() + 4).sum::<usize>());
        prod_blob.extend_from_slice(&(ids.len() as u32).to_le_bytes());
        for id in &ids {
            let bytes = id.as_bytes();
            prod_blob.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            prod_blob.extend_from_slice(bytes);
        }

        assert_eq!(
            our_blob, prod_blob,
            "Adapter blob encoding must match production"
        );
    }

    /// Verifies gates_blob encoding matches production.
    #[test]
    fn test_gates_blob_encoding_parity() {
        let gates: &[i16] = &[16384, 8192, -100];

        // Our encoding via canonical function
        let our_blob = encode_gates_q15(gates);

        // Production encoding (SqlTraceSink::encode_gates_q15)
        let mut prod_blob = Vec::with_capacity(4 + gates.len() * 2);
        prod_blob.extend_from_slice(&(gates.len() as u32).to_le_bytes());
        for g in gates {
            prod_blob.extend_from_slice(&g.to_le_bytes());
        }

        assert_eq!(
            our_blob, prod_blob,
            "Gates blob encoding must match production"
        );
    }

    #[test]
    fn test_execution_context_attributed_tokens() {
        let ctx = ExecutionContext::new(B3Hash::hash(b"test"), 100).with_cached_tokens(30);
        assert_eq!(ctx.attributed_tokens(), 70);
    }

    #[test]
    fn test_execution_context_commit_decision() {
        let mut ctx = ExecutionContext::new(B3Hash::hash(b"test"), 100);

        let decision = Decision {
            indices: SmallVec::from_slice(&[0, 1]),
            gates_q15: SmallVec::from_slice(&[16384, 16383]),
            entropy: 1.0,
            candidates: Vec::new(),
            decision_hash: None,
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
        };
        let adapter_ids = vec!["adapter-a".to_string(), "adapter-b".to_string()];
        let mask = PolicyMask::allow_all(&adapter_ids, None);

        let hash1 = ctx.commit_decision(&adapter_ids, &decision, &mask);
        assert_eq!(ctx.position(), 1);
        assert_ne!(ctx.run_head(), &B3Hash::zero());

        let hash2 = ctx.commit_decision(&adapter_ids, &decision, &mask);
        assert_eq!(ctx.position(), 2);
        assert_ne!(hash1, hash2); // Different positions = different hashes
    }

    #[test]
    fn test_run_head_uses_canonical_function() {
        // Verify our run_head matches the canonical implementation
        let decision_hash = B3Hash::hash(b"decision");
        let prev = B3Hash::zero();
        let token_index = 0u32;

        let our_result = receipt_digest::update_run_head(&prev, token_index, &decision_hash);

        // Canonical algorithm: hash_multi([prev, decision_hash, token_index LE bytes])
        let canonical = B3Hash::hash_multi(&[
            prev.as_bytes(),
            decision_hash.as_bytes(),
            &token_index.to_le_bytes(),
        ]);

        assert_eq!(our_result, canonical);
    }

    #[test]
    fn test_stop_reason_to_stop_fields() {
        let reason = StopReason::EosToken;
        let fields = reason.to_stop_fields(42, None);

        assert_eq!(fields.stop_reason_code, Some("EOS".to_string()));
        assert_eq!(fields.stop_reason_token_index, Some(42));
        assert!(fields.stop_policy_digest_b3.is_none());
    }

    #[test]
    fn test_committed_decision_accessors() {
        let decision = CommittedDecision {
            token_index: 0,
            adapter_ids: vec!["lora-rust".to_string(), "lora-python".to_string()],
            adapter_indices: SmallVec::from_slice(&[2, 5]),
            gates_q15: SmallVec::from_slice(&[16384, 16383]),
            entropy: 1.0,
            decision_hash: B3Hash::hash(b"test"),
            policy_mask_digest: None,
        };

        assert_eq!(decision.dominant_adapter(), Some("lora-rust"));
        assert_eq!(decision.gate_for("lora-rust"), Some(16384));
        assert_eq!(decision.gate_for("lora-python"), Some(16383));
        assert_eq!(decision.gate_for("nonexistent"), None);
    }

    #[test]
    fn test_execution_result_verify_integrity() {
        // Create a minimal result and verify chain integrity
        let ctx_digest = B3Hash::hash(b"context");
        let decision_hash = B3Hash::hash(b"decision0");

        // Manually compute run_head for one decision
        let run_head = receipt_digest::update_run_head(&B3Hash::zero(), 0, &decision_hash);

        let decisions = vec![CommittedDecision {
            token_index: 0,
            adapter_ids: vec!["test".to_string()],
            adapter_indices: SmallVec::from_slice(&[0]),
            gates_q15: SmallVec::from_slice(&[32767]),
            entropy: 0.0,
            decision_hash,
            policy_mask_digest: None,
        }];

        let result = ExecutionResult::new(
            vec![1234],
            decisions,
            run_head,
            &ctx_digest,
            10,
            0,
            None,
            100,
            None,
            None,
            None,
            None,
        );

        assert!(result.verify_integrity());
    }
}
